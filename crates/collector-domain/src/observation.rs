//! Direction-neutral, normalized connector observations; no raw source payloads.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(untagged)]
pub enum ScalarValue {
    Number(f64),
    Boolean(bool),
    Text(String),
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedItem {
    pub media_type: String,
    pub canonical_url: String,
    pub external_ids: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_hint: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub schema_version: u32,
    pub source_connector_id: String,
    pub source_record_id: String,
    pub observed_at: String,
    pub idempotency_key: String,
    pub item: ObservedItem,
    pub state: BTreeMap<String, ScalarValue>,
}
