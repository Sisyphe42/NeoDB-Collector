use super::*;
use crate::{mapping::imdb_default, preview::ComparedField};
use collector_domain::mapping::{MappingRule, Operation};
use collector_domain::observation::{ObservedItem, ScalarValue};
use std::collections::BTreeMap;

fn metadata() -> StoredPreviewMetadata {
    StoredPreviewMetadata {
        ordinal: 0,
        attempt: 1,
        committed_at: 120,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        source_fingerprint: format!("imdb:ratings_csv:{:0>64}", "1234567"),
        profile_id: "imdb_ratings".into(),
        profile_version: 1,
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
    }
}

fn item() -> CatalogItem {
    CatalogItem {
        uuid: "Resolved123".into(),
        canonical_url: "https://neodb.example/tv/Resolved123".into(),
        detail_url: "https://neodb.example/api/tv/Resolved123".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        parent_uuid: None,
        external_ids: vec![("imdb:title".into(), "tt1234567".into())],
    }
}

fn observation() -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        observed_at: "2026-10-01T00:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{:0>64}", "1234567"),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::from([("imdb:title".into(), "tt1234567".into())]),
            title_hint: None,
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(8.0))]),
    }
}

fn compared(fields: Vec<ComparedField>) -> StoredPreviewResult {
    StoredPreviewResult::from_stored(
        metadata(),
        StoredPreviewOutcome::Compared {
            item: item(),
            observed_at: 110,
            comparison: Comparison {
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                fields,
            },
            provenance: PreviewRecordProvenance::Automatic,
        },
    )
    .unwrap()
}

#[test]
fn result_position_and_page_bounds_are_strict() {
    assert_eq!(
        validate_result_page(0).unwrap_err().code,
        "preview_page_invalid"
    );
    assert_eq!(
        validate_result_page(101).unwrap_err().code,
        "preview_page_invalid"
    );
}

#[test]
fn historical_comparison_rejects_duplicate_rules_and_impossible_sink_state() {
    let rating = ComparedField {
        rule_id: "rating".into(),
        field: "rating_grade".into(),
        suggestion: Some(ScalarValue::Number(8.0)),
        sink: SinkValue::Known(ScalarValue::Number(8.0)),
        status: ComparisonStatus::Skipped,
        reason: "already_equal".into(),
    };
    let duplicate = StoredPreviewResult::from_stored(
        metadata(),
        StoredPreviewOutcome::Compared {
            item: item(),
            observed_at: 110,
            comparison: Comparison {
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                fields: vec![rating.clone(), rating.clone()],
            },
            provenance: PreviewRecordProvenance::Automatic,
        },
    );
    assert!(duplicate.is_err());

    let result = compared(vec![
        rating,
        ComparedField {
            rule_id: "rating_implies_complete".into(),
            field: "shelf_type".into(),
            suggestion: Some(ScalarValue::Text("complete".into())),
            sink: SinkValue::Empty,
            status: ComparisonStatus::Accepted,
            reason: "sink_empty_accepts_suggestion".into(),
        },
    ]);
    assert!(validate_historical_against_inputs(&imdb_default(), &observation(), &result).is_err());

    let mut duplicate_target = imdb_default();
    duplicate_target.rules[1].target = "rating_grade".into();
    assert!(
        validate_historical_against_inputs(&duplicate_target, &observation(), &result).is_err()
    );

    let mut profile = imdb_default();
    profile.rules.push(MappingRule {
        id: "disabled_rating".into(),
        enabled: false,
        target: "rating_grade".into(),
        operation: Operation::Skip,
    });
    let inconsistent = compared(vec![
        ComparedField {
            rule_id: "rating".into(),
            field: "rating_grade".into(),
            suggestion: Some(ScalarValue::Number(8.0)),
            sink: SinkValue::Known(ScalarValue::Number(8.0)),
            status: ComparisonStatus::Skipped,
            reason: "already_equal".into(),
        },
        ComparedField {
            rule_id: "rating_implies_complete".into(),
            field: "shelf_type".into(),
            suggestion: Some(ScalarValue::Text("complete".into())),
            sink: SinkValue::Known(ScalarValue::Text("complete".into())),
            status: ComparisonStatus::Skipped,
            reason: "already_equal".into(),
        },
        ComparedField {
            rule_id: "disabled_rating".into(),
            field: "rating_grade".into(),
            suggestion: None,
            sink: SinkValue::Unknown,
            status: ComparisonStatus::Skipped,
            reason: "rule_disabled".into(),
        },
    ]);
    assert!(validate_historical_against_inputs(&profile, &observation(), &inconsistent).is_err());
}
