use crate::observation::ScalarValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

pub const MAX_PROFILE_BYTES: usize = 65_536;

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MappingProfile {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub source_connector_id: String,
    pub sink_connector_id: String,
    pub enabled: bool,
    pub rules: Vec<MappingRule>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MappingRule {
    pub id: String,
    pub enabled: bool,
    pub target: String,
    pub operation: Operation,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Direct {
        from: String,
    },
    NumericScale {
        from: String,
        input_min: f64,
        input_max: f64,
        output_min: f64,
        output_max: f64,
    },
    Clamp {
        from: String,
        min: f64,
        max: f64,
    },
    EnumMap {
        from: String,
        values: BTreeMap<String, ScalarValue>,
    },
    Condition {
        from: String,
        test: Predicate,
        value: ScalarValue,
    },
    Skip,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Predicate {
    Exists,
    Equals { value: ScalarValue },
    GreaterThan { value: f64 },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldStatus {
    Accepted,
    Skipped,
    Rejected,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MappedField {
    pub rule_id: String,
    pub target: String,
    pub status: FieldStatus,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ScalarValue>,
}
