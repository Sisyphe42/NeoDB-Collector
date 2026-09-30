use crate::{Storage, database_error};
use collector_core::{
    access::PortFuture,
    mapping::{MappingRepository, validate, validate_revision},
};
use collector_domain::{
    ApiError,
    mapping::{MAX_PROFILE_BYTES, MappingProfile},
};
use sqlx::Row;

impl MappingRepository for Storage {
    fn load(&self, id: String, version: Option<u32>) -> PortFuture<'_, Option<MappingProfile>> {
        Box::pin(async move {
            let row = sqlx::query("SELECT version, profile_json FROM mapping_profiles WHERE id=? AND (? IS NULL OR version=?) ORDER BY version DESC LIMIT 1")
                .bind(&id).bind(version).bind(version).fetch_optional(&self.pool).await.map_err(|_| database_error())?;
            let Some(row) = row else {
                return Ok(None);
            };
            let json: String = row.try_get("profile_json").map_err(|_| database_error())?;
            let saved_version: u32 = row.try_get("version").map_err(|_| database_error())?;
            let profile: MappingProfile =
                serde_json::from_str(&json).map_err(|_| database_error())?;
            // Fail closed on incompatible/corrupt stored configurations.
            validate(&profile).map_err(|_| database_error())?;
            if profile.id != id || profile.version != saved_version {
                return Err(database_error());
            }
            Ok(Some(profile))
        })
    }

    fn save(&self, profile: MappingProfile, expected_version: u32) -> PortFuture<'_, ()> {
        Box::pin(async move {
            validate_revision(&profile, expected_version)?;
            let json = serde_json::to_string(&profile).map_err(|_| database_error())?;
            if json.len() > MAX_PROFILE_BYTES {
                return Err(ApiError::new(
                    "mapping_document_too_large",
                    "Mapping document exceeds 64 KiB.",
                ));
            }
            // A single SQLite write statement serializes competing saves. No
            // read-then-write race or overwrite of historical versions.
            let result = sqlx::query("INSERT INTO mapping_profiles(id, version, profile_json) SELECT ?, ?, ? WHERE COALESCE((SELECT MAX(version) FROM mapping_profiles WHERE id=?), 0)=?")
                .bind(&profile.id).bind(profile.version).bind(json).bind(&profile.id).bind(expected_version)
                .execute(&self.pool).await.map_err(|_| database_error())?;
            if result.rows_affected() != 1 {
                return Err(ApiError::new(
                    "mapping_version_conflict",
                    "Mapping profile has changed; reload before saving.",
                ));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collector_core::mapping::imdb_default;

    #[tokio::test]
    async fn creates_revises_and_reopens_immutable_disabled_versions() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(dir.path()).await.unwrap();
        let first = imdb_default();
        assert!(store.load(first.id.clone(), None).await.unwrap().is_none());
        store.save(first.clone(), 0).await.unwrap();
        let mut second = first.clone();
        second.version = 2;
        second.rules[1].enabled = true;
        store.save(second.clone(), 1).await.unwrap();
        store.close().await;
        let store = Storage::open(dir.path()).await.unwrap();
        assert!(
            store
                .load(first.id.clone(), Some(1))
                .await
                .unwrap()
                .unwrap()
                == first
        );
        assert!(store.load(first.id.clone(), None).await.unwrap().unwrap() == second);
        assert!(store.load(first.id, Some(3)).await.unwrap().is_none());
        store.close().await;
    }

    #[tokio::test]
    async fn concurrent_creation_and_update_have_exactly_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(dir.path()).await.unwrap();
        for expected in [0, 1] {
            let mut profile = imdb_default();
            profile.version = expected + 1;
            let (a, b) = tokio::join!(
                store.save(profile.clone(), expected),
                store.save(profile.clone(), expected)
            );
            assert_ne!(a.is_ok(), b.is_ok());
            assert_eq!(
                a.err().or(b.err()).unwrap().code,
                "mapping_version_conflict"
            );
            assert!(store.load(profile.id.clone(), None).await.unwrap().unwrap() == profile);
        }
        store.close().await;
    }

    #[tokio::test]
    async fn rejects_invalid_stale_enabled_and_failed_writes_without_changing_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(dir.path()).await.unwrap();
        let original = imdb_default();
        store.save(original.clone(), 0).await.unwrap();
        assert_eq!(
            store.save(original.clone(), 0).await.unwrap_err().code,
            "mapping_version_conflict"
        );
        for (version, expected, enabled, code) in [
            (3, 1, false, "mapping_version_invalid"),
            (2, 1, true, "connector_not_activated"),
            (u32::MAX, u32::MAX, false, "mapping_version_invalid"),
        ] {
            let mut invalid = original.clone();
            invalid.version = version;
            invalid.enabled = enabled;
            assert_eq!(store.save(invalid, expected).await.unwrap_err().code, code);
        }
        sqlx::raw_sql("CREATE TRIGGER fail_mapping BEFORE INSERT ON mapping_profiles BEGIN SELECT RAISE(ABORT, 'test'); END;")
            .execute(&store.pool).await.unwrap();
        let mut second = original.clone();
        second.version = 2;
        assert_eq!(
            store.save(second, 1).await.unwrap_err().code,
            "database_unavailable"
        );
        assert!(
            store
                .load(original.id.clone(), None)
                .await
                .unwrap()
                .unwrap()
                == original
        );
        store.close().await;
    }

    #[tokio::test]
    async fn rejects_oversized_normalized_profiles_before_insert() {
        use collector_domain::{mapping::Operation, observation::ScalarValue};
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(dir.path()).await.unwrap();
        let mut profile = imdb_default();
        profile.rules[0].operation = Operation::EnumMap {
            from: "status".into(),
            values: (0..20)
                .map(|i| {
                    (
                        format!("{i:02}{}", "x".repeat(4000)),
                        ScalarValue::Number(1.0),
                    )
                })
                .collect(),
        };
        assert_eq!(
            store.save(profile.clone(), 0).await.unwrap_err().code,
            "mapping_document_too_large"
        );
        assert!(store.load(profile.id, None).await.unwrap().is_none());
        store.close().await;
    }

    #[tokio::test]
    async fn upgrades_access_schema_without_losing_existing_account_and_rejects_corruption() {
        use collector_core::access::{AccessRepository, Endpoint};
        let dir = tempfile::tempdir().unwrap();
        let store = Storage::open(dir.path()).await.unwrap();
        store
            .set_endpoint(Some(Endpoint {
                base_url: "https://neodb.social".into(),
                account: "fixture".into(),
                credential_ref: "opaque-reference".into(),
            }))
            .await
            .unwrap();
        sqlx::raw_sql("DROP TABLE preview_results; DROP TABLE preview_observations; DROP TABLE preview_jobs; DROP TABLE mapping_profiles; DELETE FROM _sqlx_migrations WHERE version>=3;")
            .execute(&store.pool)
            .await
            .unwrap();
        store.close().await;
        let store = Storage::open(dir.path()).await.unwrap();
        assert_eq!(store.endpoint().await.unwrap().unwrap().account, "fixture");
        let profile = imdb_default();
        store.save(profile.clone(), 0).await.unwrap();
        sqlx::query("UPDATE mapping_profiles SET profile_json='{}'")
            .execute(&store.pool)
            .await
            .unwrap();
        assert_eq!(
            store.load(profile.id, None).await.err().unwrap().code,
            "database_unavailable"
        );
        store.close().await;
    }
}
