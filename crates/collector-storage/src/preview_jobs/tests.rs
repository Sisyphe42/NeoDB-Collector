use super::*;
use collector_core::{
    access::{AccessRecord, AccessRepository},
    mapping::{MappingRepository, imdb_default},
    preview_jobs::PreviewJobRepository,
};
use collector_domain::observation::{ObservedItem, ScalarValue};
use std::collections::BTreeMap;

fn binding(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn observation(id: &str, rating: u8) -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: id.into(),
        observed_at: "2026-09-30T12:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{:0>64}", id.trim_start_matches("tt")),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: format!("https://www.imdb.com/title/{id}/"),
            external_ids: BTreeMap::from([("imdb:title".into(), id.into())]),
            title_hint: Some(format!("Title {id}\nsecond line")),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(f64::from(rating)))]),
    }
}

async fn setup() -> (tempfile::TempDir, Storage, MappingProfile) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    storage
        .set_endpoint(Some(binding("credential-a")))
        .await
        .unwrap();
    let profile = imdb_default();
    storage.save(profile.clone(), 0).await.unwrap();
    (dir, storage, profile)
}

async fn load_job_view(storage: &Storage, id: &str) -> Option<PreviewJob> {
    PreviewJobRepository::load(storage, id.to_owned())
        .await
        .unwrap()
}

fn submission(
    id: &str,
    profile: MappingProfile,
    observations: Vec<Observation>,
    expires_at: i64,
) -> PreviewSubmission {
    PreviewSubmission::new(
        id.into(),
        binding("credential-a"),
        profile,
        observations,
        100,
        expires_at,
    )
    .unwrap()
}

#[tokio::test]
async fn submission_pages_and_reopen_preserve_only_normalized_ordered_rows() {
    let (dir, storage, profile) = setup().await;
    let values = vec![
        observation("tt1234567", 8),
        observation("tt1234568", 9),
        observation("tt1234569", 10),
    ];
    let job = storage
        .submit(submission("job:ordered", profile, values.clone(), 1_000))
        .await
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Queued);
    assert_eq!(job.attempt(), 1);
    assert_eq!(job.observation_count(), 3);
    assert_eq!(job.base_url(), "https://neodb.example");
    assert_eq!(job.account(), "alice");
    assert!(
        storage
            .observations("job:ordered".into(), 1, 2)
            .await
            .unwrap()
            == values[1..]
    );
    storage.close().await;

    let reopened = Storage::open(dir.path()).await.unwrap();
    let job = load_job_view(&reopened, "job:ordered").await.unwrap();
    assert_eq!(job.profile_id(), "imdb_ratings");
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.expires_at(), 1_000);
    assert!(
        reopened
            .observations("job:ordered".into(), 0, 100)
            .await
            .unwrap()
            == values
    );
    reopened.close().await;
}

#[tokio::test]
async fn concurrent_claim_has_one_winner_and_exact_expiry_is_not_live() {
    let (_dir, storage, profile) = setup().await;
    storage
        .submit(submission("job:claim", profile.clone(), vec![], 200))
        .await
        .unwrap();
    let (one, two) = tokio::join!(
        storage.claim("job:claim".into(), 1, 150),
        storage.claim("job:claim".into(), 1, 150)
    );
    assert_ne!(one.unwrap(), two.unwrap());
    assert_eq!(
        load_job_view(&storage, "job:claim").await.unwrap().status(),
        PreviewJobStatus::Running
    );

    storage
        .submit(submission("job:expired", profile, vec![], 200))
        .await
        .unwrap();
    assert!(!storage.claim("job:expired".into(), 1, 200).await.unwrap());
    assert!(storage.cancel("job:expired".into(), 1, 200).await.unwrap());
    assert_eq!(
        storage.resume("job:expired".into(), 1, 200).await.unwrap(),
        None
    );
    storage.close().await;
}

#[tokio::test]
async fn interruption_resume_and_attempt_guards_keep_rows_and_fixed_expiry() {
    let (_dir, storage, profile) = setup().await;
    let values = vec![observation("tt1234567", 8), observation("tt1234568", 9)];
    storage
        .submit(submission(
            "job:resume",
            profile.clone(),
            values.clone(),
            1_000,
        ))
        .await
        .unwrap();
    assert!(storage.claim("job:resume".into(), 1, 110).await.unwrap());
    assert_eq!(storage.interrupt_running(120).await.unwrap(), 1);
    let mut newer = profile;
    newer.version = 2;
    newer.rules[1].enabled = true;
    storage.save(newer, 1).await.unwrap();
    assert_eq!(
        storage.resume("job:resume".into(), 1, 130).await.unwrap(),
        Some(2)
    );
    let job = load_job_view(&storage, "job:resume").await.unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Queued);
    assert_eq!(job.attempt(), 2);
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.expires_at(), 1_000);
    assert!(
        storage
            .observations("job:resume".into(), 0, 100)
            .await
            .unwrap()
            == values
    );
    assert!(!storage.cancel("job:resume".into(), 1, 140).await.unwrap());
    assert!(!storage.fail("job:resume".into(), 1, 140).await.unwrap());
    assert!(storage.claim("job:resume".into(), 2, 140).await.unwrap());
    assert!(storage.fail("job:resume".into(), 2, 150).await.unwrap());
    assert!(!storage.fail("job:resume".into(), 2, 151).await.unwrap());
    assert_eq!(
        storage.resume("job:resume".into(), 2, 160).await.unwrap(),
        Some(3)
    );
    storage.close().await;
}

#[tokio::test]
async fn startup_recovery_interrupts_queued_and_running_even_after_clock_regression() {
    let (_dir, storage, profile) = setup().await;
    storage
        .submit(submission("job:queued", profile.clone(), vec![], 1_000))
        .await
        .unwrap();
    storage
        .submit(submission("job:running", profile, vec![], 1_000))
        .await
        .unwrap();
    assert!(storage.claim("job:running".into(), 1, 150).await.unwrap());
    assert_eq!(storage.interrupt_running(50).await.unwrap(), 2);
    for id in ["job:queued", "job:running"] {
        let value = load_job_view(&storage, id).await.unwrap();
        assert_eq!(value.status(), PreviewJobStatus::Interrupted);
        assert_eq!(
            value.updated_at(),
            if id == "job:running" { 150 } else { 100 }
        );
        assert_eq!(storage.resume(id.into(), 1, 151).await.unwrap(), Some(2));
    }
    storage.close().await;
}

#[tokio::test]
async fn cancel_is_attempt_guarded_idempotent_and_rejects_clock_regression() {
    let (_dir, storage, profile) = setup().await;
    storage
        .submit(submission("job:cancel", profile, vec![], 1_000))
        .await
        .unwrap();
    assert!(storage.claim("job:cancel".into(), 1, 120).await.unwrap());
    assert!(!storage.cancel("job:cancel".into(), 1, 119).await.unwrap());
    assert!(storage.cancel("job:cancel".into(), 1, 130).await.unwrap());
    assert!(storage.cancel("job:cancel".into(), 1, 130).await.unwrap());
    assert_eq!(
        storage.resume("job:cancel".into(), 1, 140).await.unwrap(),
        Some(2)
    );
    assert!(!storage.cancel("job:cancel".into(), 1, 150).await.unwrap());
    sqlx::query(
        "UPDATE preview_jobs SET status='cancelled',attempt=4294967295 WHERE id='job:cancel'",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        storage
            .resume("job:cancel".into(), u32::MAX, 160)
            .await
            .unwrap_err()
            .code,
        "preview_attempt_exhausted"
    );
    let unchanged = load_job_view(&storage, "job:cancel").await.unwrap();
    assert_eq!(unchanged.attempt(), u32::MAX);
    assert_eq!(unchanged.status(), PreviewJobStatus::Cancelled);
    storage.close().await;
}

#[tokio::test]
async fn binding_profile_and_corruption_fail_closed_for_submit_claim_and_resume() {
    let (_dir, storage, profile) = setup().await;
    let forged = {
        let mut value = profile.clone();
        value.rules[0].enabled = false;
        value
    };
    assert_eq!(
        storage
            .submit(submission("job:forged", forged, vec![], 1_000))
            .await
            .err()
            .unwrap()
            .code,
        "preview_profile_changed"
    );
    storage
        .set_endpoint(Some(binding("credential-b")))
        .await
        .unwrap();
    assert_eq!(
        storage
            .submit(submission(
                "job:wrong-binding",
                profile.clone(),
                vec![],
                1_000
            ))
            .await
            .err()
            .unwrap()
            .code,
        "preview_binding_changed"
    );
    storage
        .set_endpoint(Some(binding("credential-a")))
        .await
        .unwrap();
    storage
        .submit(submission(
            "job:stale-binding",
            profile.clone(),
            vec![],
            1_000,
        ))
        .await
        .unwrap();
    assert!(
        storage
            .cancel("job:stale-binding".into(), 1, 110)
            .await
            .unwrap()
    );
    storage
        .set_endpoint(Some(binding("credential-b")))
        .await
        .unwrap();
    assert_eq!(
        storage
            .resume("job:stale-binding".into(), 1, 120)
            .await
            .unwrap_err()
            .code,
        "preview_binding_changed"
    );
    storage
        .set_endpoint(Some(binding("credential-a")))
        .await
        .unwrap();
    storage
        .submit(submission("job:corrupt", profile, vec![], 1_000))
        .await
        .unwrap();
    assert!(storage.cancel("job:corrupt".into(), 1, 110).await.unwrap());
    sqlx::query(
        "UPDATE mapping_profiles SET profile_json='{}' WHERE id='imdb_ratings' AND version=1",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        storage
            .claim("job:corrupt".into(), 1, 120)
            .await
            .err()
            .unwrap()
            .code,
        "database_unavailable"
    );
    assert_eq!(
        storage
            .resume("job:corrupt".into(), 1, 120)
            .await
            .err()
            .unwrap()
            .code,
        "database_unavailable"
    );
    storage.close().await;
}

#[tokio::test]
async fn summed_normalized_observation_bytes_are_bounded_before_insert() {
    let (_dir, storage, profile) = setup().await;
    let observations = (0..10_000)
        .map(|index| {
            let mut value = observation(&format!("tt{index:07}"), 8);
            value.item.title_hint = Some("x".repeat(3_400));
            value
        })
        .collect();
    assert_eq!(
        storage
            .submit(submission(
                "job:too-many-bytes",
                profile,
                observations,
                1_000
            ))
            .await
            .err()
            .unwrap()
            .code,
        "preview_submission_too_large"
    );
    assert!(
        load_job_view(&storage, "job:too-many-bytes")
            .await
            .is_none()
    );
    storage.close().await;
}

#[tokio::test]
async fn json_escaping_cannot_bypass_per_observation_byte_limit() {
    let (_dir, storage, profile) = setup().await;
    let mut value = observation("tt1234567", 8);
    value.item.title_hint = Some(format!("A{}Z", "\0".repeat(4_094)));
    let submission = submission("job:escaped", profile, vec![value], 1_000);
    assert_eq!(
        storage.submit(submission).await.err().unwrap().code,
        "preview_observation_too_large"
    );
    assert!(load_job_view(&storage, "job:escaped").await.is_none());
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM preview_observations")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    storage.close().await;
}

#[tokio::test]
async fn failed_row_insert_rolls_back_job_and_all_observations() {
    let (_dir, storage, profile) = setup().await;
    sqlx::raw_sql("CREATE TRIGGER fail_second_preview_row BEFORE INSERT ON preview_observations WHEN NEW.ordinal=1 BEGIN SELECT RAISE(ABORT, 'test'); END;")
        .execute(&storage.pool).await.unwrap();
    assert_eq!(
        storage
            .submit(submission(
                "job:rollback",
                profile,
                vec![observation("tt1234567", 8), observation("tt1234568", 9)],
                1_000,
            ))
            .await
            .err()
            .unwrap()
            .code,
        "database_unavailable"
    );
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM preview_jobs")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM preview_observations")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!((jobs, rows), (0, 0));
    storage.close().await;
}

#[tokio::test]
async fn corrupt_or_holey_observations_never_return_partial_pages() {
    for corruption in ["json", "missing", "hole"] {
        let (_dir, storage, profile) = setup().await;
        storage
            .submit(submission(
                "job:rows",
                profile,
                vec![observation("tt1234567", 8), observation("tt1234568", 9)],
                1_000,
            ))
            .await
            .unwrap();
        match corruption {
            "json" => {
                sqlx::query(
                    "UPDATE preview_observations SET observation_json='{}' WHERE ordinal=1",
                )
                .execute(&storage.pool)
                .await
                .unwrap();
            }
            "missing" => {
                sqlx::query("DELETE FROM preview_observations WHERE ordinal=1")
                    .execute(&storage.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query("UPDATE preview_observations SET ordinal=5 WHERE ordinal=1")
                    .execute(&storage.pool)
                    .await
                    .unwrap();
            }
        }
        assert_eq!(
            storage
                .observations("job:rows".into(), 0, 100)
                .await
                .err()
                .unwrap()
                .code,
            "database_unavailable"
        );
        storage.close().await;
    }
}

#[tokio::test]
async fn delete_and_expiry_are_atomic_idempotent_and_keep_configuration() {
    let (_dir, storage, profile) = setup().await;
    storage
        .insert(AccessRecord {
            id: "client-survives".into(),
            hash: "hash-survives".into(),
            kind: "client".into(),
            audience: "http://127.0.0.1:19423".into(),
            name: "CLI".into(),
            client_kind: "cli".into(),
            expires_at: i64::MAX,
        })
        .await
        .unwrap();
    for (id, expiry) in [("job:delete", 500), ("job:expiry", 200)] {
        storage
            .submit(submission(
                id,
                profile.clone(),
                vec![observation("tt1234567", 8)],
                expiry,
            ))
            .await
            .unwrap();
    }
    assert!(storage.delete("job:delete".into()).await.unwrap());
    assert!(!storage.delete("job:delete".into()).await.unwrap());
    assert!(!storage.fail("job:delete".into(), 1, 150).await.unwrap());
    assert_eq!(storage.cleanup_expired(200).await.unwrap(), 1);
    assert!(load_job_view(&storage, "job:expiry").await.is_none());
    assert!(storage.endpoint().await.unwrap().is_some());
    assert_eq!(storage.clients().await.unwrap().len(), 1);
    assert!(
        MappingRepository::load(&storage, profile.id, Some(1))
            .await
            .unwrap()
            .is_some()
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM preview_observations")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    storage.close().await;
}

#[tokio::test]
async fn concurrent_reads_and_delete_observe_whole_job_or_absence() {
    let (_dir, storage, profile) = setup().await;
    for index in 0..20 {
        let id = format!("job:race:{index}");
        storage
            .submit(submission(
                &id,
                profile.clone(),
                vec![observation("tt1234567", 8), observation("tt1234568", 9)],
                1_000,
            ))
            .await
            .unwrap();
        let (loaded, page, deleted) = tokio::join!(
            PreviewJobRepository::load(&storage, id.clone()),
            storage.observations(id.clone(), 0, 100),
            storage.delete(id)
        );
        let loaded = loaded.unwrap();
        let page = page.unwrap();
        assert!(loaded.is_none() || loaded.unwrap().observation_count() == 2);
        assert!(page.is_empty() || page.len() == 2);
        assert!(deleted.unwrap());
    }
    storage.close().await;
}

#[tokio::test]
async fn audit_schema_has_no_raw_or_token_columns() {
    let (_dir, storage, _profile) = setup().await;
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('preview_jobs') UNION ALL SELECT name FROM pragma_table_info('preview_observations')",
    )
    .fetch_all(&storage.pool)
    .await
    .unwrap();
    assert!(!columns.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("raw")
            || lower.contains("token")
            || lower.contains("reason")
            || lower.contains("message")
    }));
    storage.close().await;
}
