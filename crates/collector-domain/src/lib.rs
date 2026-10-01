//! Cross-process values. No network, filesystem or runtime dependencies.
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub mod access;
pub mod mapping;
pub mod observation;
pub mod preview;

pub const API_VERSION: &str = "v1";
pub const SCHEMA_VERSION: u32 = 1;
pub const MEDIA_TYPES: &[&str] = &[
    "movie",
    "tv",
    "game",
    "book",
    "album",
    "performance",
    "podcast",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field: None,
            retryable: false,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorageStatus {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDocument {
    pub status: HealthStatus,
    pub core_version: String,
    pub api_version: String,
    #[schemars(range(min = 0, max = 4294967295_u64))]
    #[schema(minimum = 0, maximum = 4294967295_u64)]
    pub schema_version: u32,
    pub storage: StorageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDocument {
    #[schemars(range(min = 0, max = 4294967295_u64))]
    #[schema(minimum = 0, maximum = 4294967295_u64)]
    pub schema_version: u32,
    pub connector_ids: Vec<String>,
    pub media_types: Vec<String>,
    pub routes: BTreeMap<String, String>,
}
