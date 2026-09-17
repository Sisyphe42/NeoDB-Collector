use super::*;
use collector_domain::observation::ObservedItem;
use std::collections::BTreeMap;

fn observation() -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        observed_at: "2026-09-17T00:00:00Z".into(),
        idempotency_key: "fixture".into(),
        item: ObservedItem {
            media_type: "movie".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::new(),
            title_hint: None,
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(8.0))]),
    }
}
fn result(op: Operation, source: ScalarValue, target: &str) -> MappedField {
    let mut profile = imdb_default();
    profile.rules.truncate(1);
    profile.rules[0].operation = op;
    profile.rules[0].target = target.into();
    let mut obs = observation();
    obs.state.insert("rating".into(), source);
    evaluate(&profile, &obs).unwrap().remove(0)
}
#[test]
fn default_is_rating_only_and_completion_is_explicit_opt_in() {
    let mut profile = imdb_default();
    let mapped = evaluate(&profile, &observation()).unwrap();
    assert!(matches!(mapped[0].value, Some(ScalarValue::Number(8.0))));
    assert!(mapped[1].status == FieldStatus::Skipped && mapped[1].value.is_none());
    assert!(!profile.enabled);
    profile.rules[1].enabled = true;
    let mapped = evaluate(&profile, &observation()).unwrap();
    assert!(matches!(&mapped[1].value, Some(ScalarValue::Text(s)) if s == "complete"));
    assert_eq!(mapped.len(), 2); // No watched-date inference.
}
#[test]
fn all_operations_have_deterministic_typed_outputs() {
    for (op, input, expected) in [
        (
            Operation::Direct {
                from: "rating".into(),
            },
            8.0,
            8.0,
        ),
        (
            Operation::Clamp {
                from: "rating".into(),
                min: 0.0,
                max: 10.0,
            },
            20.0,
            10.0,
        ),
        (
            Operation::NumericScale {
                from: "rating".into(),
                input_min: 0.0,
                input_max: 5.0,
                output_min: 0.0,
                output_max: 10.0,
            },
            4.0,
            8.0,
        ),
    ] {
        let field = result(op, ScalarValue::Number(input), "rating_grade");
        assert!(field.status == FieldStatus::Accepted);
        assert!(matches!(field.value, Some(ScalarValue::Number(n)) if n == expected));
    }
    let mapped = result(
        Operation::EnumMap {
            from: "rating".into(),
            values: BTreeMap::from([("done".into(), ScalarValue::Text("complete".into()))]),
        },
        ScalarValue::Text("done".into()),
        "shelf_type",
    );
    assert!(mapped.status == FieldStatus::Accepted);
    let skipped = result(Operation::Skip, ScalarValue::Number(8.0), "rating_grade");
    assert!(skipped.status == FieldStatus::Skipped && skipped.value.is_none());
    for predicate in [
        Predicate::Equals {
            value: ScalarValue::Number(8.0),
        },
        Predicate::GreaterThan { value: 7.0 },
    ] {
        assert!(
            result(
                Operation::Condition {
                    from: "rating".into(),
                    test: predicate,
                    value: ScalarValue::Text("complete".into())
                },
                ScalarValue::Number(8.0),
                "shelf_type"
            )
            .status
                == FieldStatus::Accepted
        );
    }
}
#[test]
fn missing_wrong_type_unmapped_and_false_condition_do_not_invent_values() {
    let mut obs = observation();
    obs.state.clear();
    assert_eq!(
        evaluate(&imdb_default(), &obs).unwrap()[0].reason,
        "source_missing"
    );
    let field = result(
        Operation::Direct {
            from: "rating".into(),
        },
        ScalarValue::Text("8".into()),
        "rating_grade",
    );
    assert!(field.status == FieldStatus::Rejected && field.value.is_none());
    let field = result(
        Operation::Clamp {
            from: "rating".into(),
            min: 0.0,
            max: 10.0,
        },
        ScalarValue::Boolean(true),
        "rating_grade",
    );
    assert_eq!(field.reason, "source_type_invalid");
    let field = result(
        Operation::Condition {
            from: "rating".into(),
            test: Predicate::GreaterThan { value: 9.0 },
            value: ScalarValue::Text("complete".into()),
        },
        ScalarValue::Number(8.0),
        "shelf_type",
    );
    assert_eq!(field.reason, "condition_false");
    let field = result(
        Operation::EnumMap {
            from: "rating".into(),
            values: BTreeMap::from([("known".into(), ScalarValue::Number(1.0))]),
        },
        ScalarValue::Text("unknown".into()),
        "rating_grade",
    );
    assert_eq!(field.reason, "enum_unmapped");
}
#[test]
fn rejects_versions_activation_wrong_sources_targets_and_duplicate_rules() {
    let mut profile = imdb_default();
    profile.version = 0;
    assert_eq!(
        validate(&profile).unwrap_err().code,
        "mapping_version_invalid"
    );
    profile = imdb_default();
    profile.enabled = true;
    assert_eq!(
        validate(&profile).unwrap_err().code,
        "connector_not_activated"
    );
    profile = imdb_default();
    profile.rules[0].target = "watched_date".into();
    assert_eq!(
        validate(&profile).unwrap_err().code,
        "mapping_target_unsupported"
    );
    profile = imdb_default();
    profile.rules.push(profile.rules[0].clone());
    assert_eq!(validate(&profile).unwrap_err().code, "mapping_rule_invalid");
    profile.rules.last_mut().unwrap().id = "another".into();
    assert_eq!(
        validate(&profile).unwrap_err().code,
        "mapping_target_duplicate"
    );
    let mut obs = observation();
    obs.schema_version = 2;
    assert_eq!(
        evaluate(&imdb_default(), &obs).err().unwrap().code,
        "observation_schema_unsupported"
    );
    obs.schema_version = 1;
    obs.source_connector_id = "other".into();
    assert_eq!(
        evaluate(&imdb_default(), &obs).err().unwrap().code,
        "mapping_source_mismatch"
    );
}
#[test]
fn rejects_nonfinite_arithmetic_invalid_ranges_and_out_of_range_outputs() {
    for op in [
        Operation::Clamp {
            from: "rating".into(),
            min: 10.0,
            max: 0.0,
        },
        Operation::Clamp {
            from: "rating".into(),
            min: 0.0,
            max: f64::NAN,
        },
        Operation::NumericScale {
            from: "rating".into(),
            input_min: -f64::MAX,
            input_max: f64::MAX,
            output_min: 0.0,
            output_max: 10.0,
        },
    ] {
        let mut p = imdb_default();
        p.rules[0].operation = op;
        assert!(validate(&p).is_err());
    }
    let field = result(
        Operation::Direct {
            from: "rating".into(),
        },
        ScalarValue::Number(11.0),
        "rating_grade",
    );
    assert!(field.status == FieldStatus::Rejected && field.value.is_none());
    let mut obs = observation();
    obs.state
        .insert("rating".into(), ScalarValue::Number(f64::INFINITY));
    assert_eq!(
        evaluate(&imdb_default(), &obs).err().unwrap().code,
        "observation_state_invalid"
    );
}
