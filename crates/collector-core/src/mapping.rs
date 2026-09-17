//! Declarative, deterministic suggestions. No writer, clock, network or parser.
use collector_domain::{
    ApiError,
    mapping::*,
    observation::{Observation, ScalarValue},
};
use std::collections::HashSet;

/// Immutable revisions: expected_version=0 creates version 1. A save must compare
/// the latest revision atomically, never overwrite a previously saved revision.
pub trait MappingRepository: Send + Sync {
    fn load(
        &self,
        id: String,
        version: Option<u32>,
    ) -> crate::access::PortFuture<'_, Option<MappingProfile>>;
    fn save(
        &self,
        profile: MappingProfile,
        expected_version: u32,
    ) -> crate::access::PortFuture<'_, ()>;
}

pub fn validate_revision(profile: &MappingProfile, expected_version: u32) -> Result<(), ApiError> {
    validate(profile)?;
    if expected_version.checked_add(1) != Some(profile.version) {
        return Err(error("mapping_version_invalid", "version"));
    }
    Ok(())
}

fn error(code: &str, field: &str) -> ApiError {
    let mut value = ApiError::new(code, "Mapping configuration or observation is invalid.");
    value.field = Some(field.into());
    value
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
fn scalar(value: &ScalarValue) -> bool {
    match value {
        ScalarValue::Number(n) => n.is_finite(),
        ScalarValue::Text(s) => s.len() <= 4096,
        ScalarValue::Boolean(_) => true,
    }
}
fn target_value(target: &str, value: &ScalarValue) -> bool {
    match (target, value) {
        ("rating_grade", ScalarValue::Number(n)) => n.is_finite() && (0.0..=10.0).contains(n),
        ("shelf_type", ScalarValue::Text(s)) => {
            ["wishlist", "progress", "complete", "dropped"].contains(&s.as_str())
        }
        _ => false,
    }
}

/// The preview milestone supports IMDb-compatible scalar targets only.
/// Enabling profiles/other sink capabilities belongs to later reviewed slices.
pub fn validate(profile: &MappingProfile) -> Result<(), ApiError> {
    if profile.schema_version != 1 || profile.version == 0 {
        return Err(error("mapping_version_invalid", "version"));
    }
    if !name(&profile.id) || !name(&profile.source_connector_id) {
        return Err(error("mapping_identifier_invalid", "id"));
    }
    if profile.sink_connector_id != "neodb" {
        return Err(error("mapping_sink_unsupported", "sinkConnectorId"));
    }
    if profile.enabled {
        return Err(error("connector_not_activated", "enabled"));
    }
    if profile.rules.is_empty() || profile.rules.len() > 128 {
        return Err(error("mapping_rules_invalid", "rules"));
    }
    let mut ids = HashSet::new();
    let mut targets = HashSet::new();
    for (index, rule) in profile.rules.iter().enumerate() {
        let path = format!("rules.{index}");
        if !name(&rule.id) || !ids.insert(&rule.id) {
            return Err(error("mapping_rule_invalid", &path));
        }
        if !["rating_grade", "shelf_type"].contains(&rule.target.as_str()) {
            return Err(error("mapping_target_unsupported", &path));
        }
        if rule.enabled && !targets.insert(&rule.target) {
            return Err(error("mapping_target_duplicate", &path));
        }
        let from = match &rule.operation {
            Operation::Direct { from }
            | Operation::NumericScale { from, .. }
            | Operation::Clamp { from, .. }
            | Operation::EnumMap { from, .. }
            | Operation::Condition { from, .. } => Some(from),
            Operation::Skip => None,
        };
        if from.is_some_and(|f| !name(f)) {
            return Err(error("mapping_source_invalid", &path));
        }
        let valid = match &rule.operation {
            Operation::NumericScale {
                input_min,
                input_max,
                output_min,
                output_max,
                ..
            } => {
                [input_min, input_max, output_min, output_max]
                    .iter()
                    .all(|n| n.is_finite())
                    && input_min < input_max
                    && (input_max - input_min).is_finite()
                    && output_min <= output_max
                    && rule.target == "rating_grade"
                    && *output_min >= 0.0
                    && *output_max <= 10.0
            }
            Operation::Clamp { min, max, .. } => {
                min.is_finite()
                    && max.is_finite()
                    && min <= max
                    && rule.target == "rating_grade"
                    && *min >= 0.0
                    && *max <= 10.0
            }
            Operation::EnumMap { values, .. } => {
                !values.is_empty()
                    && values.len() <= 128
                    && values
                        .iter()
                        .all(|(k, v)| k.len() <= 4096 && scalar(v) && target_value(&rule.target, v))
            }
            Operation::Condition { test, value, .. } => {
                scalar(value)
                    && target_value(&rule.target, value)
                    && match test {
                        Predicate::Exists => true,
                        Predicate::Equals { value } => scalar(value),
                        Predicate::GreaterThan { value } => value.is_finite(),
                    }
            }
            _ => true,
        };
        if !valid {
            return Err(error("mapping_arguments_invalid", &path));
        }
    }
    Ok(())
}

pub fn imdb_default() -> MappingProfile {
    MappingProfile {
        schema_version: 1,
        id: "imdb_ratings".into(),
        version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        sink_connector_id: "neodb".into(),
        enabled: false,
        rules: vec![
            MappingRule {
                id: "rating".into(),
                enabled: true,
                target: "rating_grade".into(),
                operation: Operation::Clamp {
                    from: "rating".into(),
                    min: 0.0,
                    max: 10.0,
                },
            },
            MappingRule {
                id: "rating_implies_complete".into(),
                enabled: false,
                target: "shelf_type".into(),
                operation: Operation::Condition {
                    from: "rating".into(),
                    test: Predicate::Exists,
                    value: ScalarValue::Text("complete".into()),
                },
            },
        ],
    }
}

pub fn evaluate(
    profile: &MappingProfile,
    observation: &Observation,
) -> Result<Vec<MappedField>, ApiError> {
    validate(profile)?;
    if observation.schema_version != 1 {
        return Err(error("observation_schema_unsupported", "schemaVersion"));
    }
    if profile.source_connector_id != observation.source_connector_id {
        return Err(error("mapping_source_mismatch", "sourceConnectorId"));
    }
    if observation.state.len() > 128
        || observation
            .state
            .iter()
            .any(|(k, v)| !name(k) || !scalar(v))
    {
        return Err(error("observation_state_invalid", "state"));
    }
    Ok(profile
        .rules
        .iter()
        .map(|rule| {
            let (status, reason, value) = if !rule.enabled {
                (FieldStatus::Skipped, "rule_disabled", None)
            } else {
                match apply(&rule.operation, observation) {
                    Ok(Some(value)) if target_value(&rule.target, &value) => {
                        (FieldStatus::Accepted, "mapped", Some(value))
                    }
                    Ok(Some(_)) => (FieldStatus::Rejected, "mapped_value_invalid", None),
                    Ok(None) => (FieldStatus::Skipped, "rule_skipped", None),
                    Err(reason) => (
                        if reason == "source_missing"
                            || reason == "enum_unmapped"
                            || reason == "condition_false"
                        {
                            FieldStatus::Skipped
                        } else {
                            FieldStatus::Rejected
                        },
                        reason,
                        None,
                    ),
                }
            };
            MappedField {
                rule_id: rule.id.clone(),
                target: rule.target.clone(),
                status,
                reason: reason.into(),
                value,
            }
        })
        .collect())
}

fn apply(op: &Operation, observation: &Observation) -> Result<Option<ScalarValue>, &'static str> {
    let get = |from: &str| observation.state.get(from).ok_or("source_missing");
    let number = |from: &str| match get(from)? {
        ScalarValue::Number(n) => Ok(*n),
        _ => Err("source_type_invalid"),
    };
    Ok(Some(match op {
        Operation::Direct { from } => get(from)?.clone(),
        Operation::Clamp { from, min, max } => ScalarValue::Number(number(from)?.clamp(*min, *max)),
        Operation::NumericScale {
            from,
            input_min,
            input_max,
            output_min,
            output_max,
        } => {
            let n = number(from)?;
            if n < *input_min || n > *input_max {
                return Err("source_range_invalid");
            }
            let scaled =
                (n - input_min) / (input_max - input_min) * (output_max - output_min) + output_min;
            if !scaled.is_finite() {
                return Err("mapped_value_invalid");
            }
            ScalarValue::Number(scaled)
        }
        Operation::EnumMap { from, values } => match get(from)? {
            ScalarValue::Text(s) => values.get(s).cloned().ok_or("enum_unmapped")?,
            _ => return Err("source_type_invalid"),
        },
        Operation::Condition { from, test, value } => {
            let matches = match test {
                Predicate::Exists => observation.state.contains_key(from),
                Predicate::Equals { value } => get(from)? == value,
                Predicate::GreaterThan { value } => number(from)? > *value,
            };
            if !matches {
                return Err("condition_false");
            }
            value.clone()
        }
        Operation::Skip => return Ok(None),
    }))
}

#[cfg(test)]
mod tests;
