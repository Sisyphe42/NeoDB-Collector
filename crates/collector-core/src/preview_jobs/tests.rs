use super::*;
use crate::{access::Endpoint, mapping::imdb_default};
use collector_domain::observation::ObservedItem;
use std::collections::BTreeMap;

fn endpoint() -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: "neodb-token:opaque".into(),
    }
}

fn observation() -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        observed_at: "2026-09-30T12:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{}", "a".repeat(64)),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::from([("imdb:title".into(), "tt1234567".into())]),
            title_hint: Some("Line one\nLine two".into()),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(8.0))]),
    }
}

#[test]
fn valid_submission_accepts_empty_or_normalized_multiline_title_batches() {
    for observations in [vec![], vec![observation()]] {
        let submission = PreviewSubmission::new(
            "job:1".into(),
            endpoint(),
            imdb_default(),
            observations,
            100,
            200,
        )
        .unwrap();
        assert_eq!(submission.id(), "job:1");
        assert_eq!(submission.created_at(), 100);
        assert_eq!(submission.expires_at(), 200);
    }
}

#[test]
fn observation_validator_rejects_every_unsupported_or_denormalized_field() {
    let mut cases = Vec::new();
    let mut value = observation();
    value.schema_version = 2;
    cases.push(value);
    let mut value = observation();
    value.source_connector_id = "other".into();
    cases.push(value);
    let mut value = observation();
    value.source_record_id = "tt12".into();
    cases.push(value);
    let mut value = observation();
    value.observed_at = "bad\nmetadata".into();
    cases.push(value);
    let mut value = observation();
    value.idempotency_key = format!("imdb:ratings_csv:{}", "A".repeat(64));
    cases.push(value);
    let mut value = observation();
    value.item.media_type = "book".into();
    cases.push(value);
    let mut value = observation();
    value.item.canonical_url = "https://evil.example/title/tt1234567/".into();
    cases.push(value);
    let mut value = observation();
    value.item.external_ids.insert("tmdb:tv".into(), "1".into());
    cases.push(value);
    let mut value = observation();
    value.item.title_hint = Some(" padded ".into());
    cases.push(value);
    let mut value = observation();
    value
        .state
        .insert("raw".into(), ScalarValue::Text("payload".into()));
    cases.push(value);
    let mut value = observation();
    value
        .state
        .insert("rating".into(), ScalarValue::Number(8.5));
    cases.push(value);
    let mut value = observation();
    value
        .state
        .insert("rating".into(), ScalarValue::Number(0.0));
    cases.push(value);

    for value in cases {
        assert_eq!(
            validate_imdb_observation(&value).unwrap_err().code,
            "preview_observation_invalid"
        );
    }
}

#[test]
fn submission_reuses_mapping_validation_and_enforces_count() {
    let mut profile = imdb_default();
    profile.enabled = true;
    assert_eq!(
        PreviewSubmission::new("job".into(), endpoint(), profile, vec![], 1, 2)
            .err()
            .unwrap()
            .code,
        "connector_not_activated"
    );
    assert_eq!(
        PreviewSubmission::new(
            "job".into(),
            endpoint(),
            imdb_default(),
            vec![observation(); MAX_PREVIEW_OBSERVATIONS + 1],
            1,
            2,
        )
        .err()
        .unwrap()
        .code,
        "preview_submission_too_large"
    );
}

#[test]
fn job_and_transition_metadata_fail_closed() {
    assert_eq!(
        PreviewJob::from_stored(
            PreviewJobIdentity {
                id: "bad id".into(),
                profile_id: "profile".into(),
                profile_version: 1,
                observation_count: 0,
            },
            PreviewJobLifecycle {
                status: PreviewJobStatus::Queued,
                attempt: 1,
                created_at: 1,
                updated_at: 1,
                expires_at: 2,
            },
            endpoint(),
        )
        .err()
        .unwrap()
        .code,
        "database_unavailable"
    );
    assert_eq!(
        validate_transition(0, 1).unwrap_err().code,
        "preview_transition_invalid"
    );
    assert_eq!(
        validate_transition(1, -1).unwrap_err().code,
        "preview_transition_invalid"
    );
    assert_eq!(validate_page(0).unwrap_err().code, "preview_page_invalid");
    assert_eq!(
        validate_page(MAX_PREVIEW_PAGE + 1).unwrap_err().code,
        "preview_page_invalid"
    );
    validate_page(MAX_PREVIEW_PAGE).unwrap();

    let job = PreviewJob::from_stored(
        PreviewJobIdentity {
            id: "job:binding".into(),
            profile_id: "profile".into(),
            profile_version: 1,
            observation_count: 0,
        },
        PreviewJobLifecycle {
            status: PreviewJobStatus::Queued,
            attempt: 1,
            created_at: 1,
            updated_at: 1,
            expires_at: 2,
        },
        endpoint(),
    )
    .unwrap();
    assert!(job.is_bound_to(&endpoint()));
    let mut changed = endpoint();
    changed.credential_ref = "neodb-token:new-generation".into();
    assert!(!job.is_bound_to(&changed));
}

#[test]
fn lifecycle_values_round_trip_including_result_completion() {
    for status in [
        PreviewJobStatus::Queued,
        PreviewJobStatus::Running,
        PreviewJobStatus::Interrupted,
        PreviewJobStatus::Failed,
        PreviewJobStatus::Cancelled,
        PreviewJobStatus::Completed,
    ] {
        assert_eq!(PreviewJobStatus::parse(status.as_str()).unwrap(), status);
    }
}
