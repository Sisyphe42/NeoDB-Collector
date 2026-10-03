//! Preview inspection and local management wire contracts.
//!
//! Historical views are non-executable review evidence and contain no credential
//! reference. Cancel/delete requests manage only local operational preview jobs.
use crate::observation::ScalarValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewCancelRequest {
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub expected_attempt: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewMutationResult {
    pub applied: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewImportSelection {
    #[schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_.:-]+$"))]
    #[schema(min_length = 1, max_length = 128, pattern = r"^[A-Za-z0-9_.:-]+$")]
    pub profile_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub profile_version: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewImdbResolutionRequest {
    #[schemars(length(max = 10485760))]
    #[schema(max_length = 10485760)]
    pub csv: String,
    #[schemars(length(max = 10000))]
    #[schema(max_items = 10000)]
    pub resolution_inputs: Vec<PreviewResolutionChoice>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewResolutionChoice {
    Search {
        #[schemars(range(min = 0, max = 9999))]
        #[schema(minimum = 0, maximum = 9999)]
        ordinal: u32,
        #[serde(rename = "sourceRecordId")]
        #[schemars(length(min = 9, max = 18), regex(pattern = r"^tt[0-9]{7,16}$"))]
        #[schema(min_length = 9, max_length = 18, pattern = r"^tt[0-9]{7,16}$")]
        source_record_id: String,
        #[schemars(length(min = 1, max = 512))]
        #[schema(min_length = 1, max_length = 512)]
        query: String,
    },
    Manual {
        #[schemars(range(min = 0, max = 9999))]
        #[schema(minimum = 0, maximum = 9999)]
        ordinal: u32,
        #[serde(rename = "sourceRecordId")]
        #[schemars(length(min = 9, max = 18), regex(pattern = r"^tt[0-9]{7,16}$"))]
        #[schema(min_length = 9, max_length = 18, pattern = r"^tt[0-9]{7,16}$")]
        source_record_id: String,
        item: PreviewCatalogReferenceRequest,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewCatalogReferenceRequest {
    #[schemars(length(min = 1, max = 22), regex(pattern = r"^[A-Za-z0-9]+$"))]
    #[schema(min_length = 1, max_length = 22, pattern = r"^[A-Za-z0-9]+$")]
    pub uuid: String,
    #[schemars(length(min = 1, max = 64), regex(pattern = r"^[A-Za-z0-9_.:-]+$"))]
    #[schema(min_length = 1, max_length = 64, pattern = r"^[A-Za-z0-9_.:-]+$")]
    pub media_type: String,
    #[schemars(length(min = 1, max = 64), regex(pattern = r"^[A-Za-z0-9_.:-]+$"))]
    #[schema(min_length = 1, max_length = 64, pattern = r"^[A-Za-z0-9_.:-]+$")]
    pub model: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewResumeRequest {
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub expected_attempt: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewAccepted {
    #[schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_.:-]+$"))]
    #[schema(min_length = 1, max_length = 128, pattern = r"^[A-Za-z0-9_.:-]+$")]
    pub job_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub attempt: u32,
    #[schemars(extend("const" = false))]
    #[schema(schema_with = non_executable_schema)]
    pub executable: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewImportAccepted {
    pub preview: PreviewAccepted,
    #[schemars(range(min = 0, max = 10000))]
    #[schema(minimum = 0, maximum = 10000)]
    pub observation_count: u32,
    #[schemars(range(min = 0, max = 10000))]
    #[schema(minimum = 0, maximum = 10000)]
    pub skipped_row_count: u32,
    #[schemars(range(min = 0, max = 10485760))]
    #[schema(minimum = 0, maximum = 10485760)]
    pub skipped_column_count: u32,
    #[schemars(range(min = 0, max = 10000))]
    #[schema(minimum = 0, maximum = 10000)]
    pub duplicate_row_count: u32,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewAdmissionFailure {
    #[schemars(length(min = 1, max = 128), regex(pattern = r"^[A-Za-z0-9_.:-]+$"))]
    #[schema(min_length = 1, max_length = 128, pattern = r"^[A-Za-z0-9_.:-]+$")]
    pub job_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub attempt: u32,
    pub error: crate::ApiError,
}

fn historical_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Boolean)
        .enum_values(Some([true]))
        .build()
}

fn non_executable_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Boolean)
        .enum_values(Some([false]))
        .build()
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreviewJobStatusView {
    Queued,
    Running,
    Interrupted,
    Failed,
    Cancelled,
    Completed,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreviewBindingState {
    Current,
    Changed,
    Disconnected,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewJobView {
    pub id: String,
    pub status: PreviewJobStatusView,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub attempt: u32,
    #[schemars(range(min = 0))]
    #[schema(minimum = 0)]
    pub created_at: i64,
    #[schemars(range(min = 0))]
    #[schema(minimum = 0)]
    pub updated_at: i64,
    #[schemars(range(min = 0))]
    #[schema(minimum = 0)]
    pub expires_at: i64,
    #[schemars(range(min = 0, max = 10000))]
    #[schema(minimum = 0, maximum = 10000)]
    pub observation_count: u32,
    pub profile_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub profile_version: u32,
    pub base_url: String,
    pub account: String,
    pub binding_state: PreviewBindingState,
    #[schemars(extend("const" = true))]
    #[schema(schema_with = historical_schema)]
    pub historical: bool,
    #[schemars(extend("const" = false))]
    #[schema(schema_with = non_executable_schema)]
    pub executable: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewResultPage {
    pub job_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub attempt: u32,
    #[schemars(range(min = 0, max = 4294967295_u64))]
    #[schema(minimum = 0, maximum = 4294967295_u64)]
    pub offset: u32,
    #[schemars(range(min = 1, max = 100))]
    #[schema(minimum = 1, maximum = 100)]
    pub limit: u32,
    #[schemars(extend("const" = true))]
    #[schema(schema_with = historical_schema)]
    pub historical: bool,
    #[schemars(extend("const" = false))]
    #[schema(schema_with = non_executable_schema)]
    pub executable: bool,
    #[schemars(length(max = 100))]
    #[schema(max_items = 100)]
    pub items: Vec<PreviewResultView>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewResultView {
    #[schemars(range(min = 0, max = 9999))]
    #[schema(minimum = 0, maximum = 9999)]
    pub ordinal: u32,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub attempt: u32,
    #[schemars(range(min = 0))]
    #[schema(minimum = 0)]
    pub committed_at: i64,
    pub source_connector_id: String,
    pub source_record_id: String,
    pub source_fingerprint: String,
    pub profile_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub profile_version: u32,
    pub base_url: String,
    pub account: String,
    #[schemars(extend("const" = true))]
    #[schema(schema_with = historical_schema)]
    pub historical: bool,
    #[schemars(extend("const" = false))]
    #[schema(schema_with = non_executable_schema)]
    pub executable: bool,
    pub outcome: PreviewOutcomeView,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewOutcomeView {
    Compared {
        item: PreviewCatalogItemView,
        #[serde(rename = "observedAt")]
        #[schemars(range(min = 0))]
        #[schema(minimum = 0)]
        observed_at: i64,
        comparison: PreviewComparisonView,
        provenance: PreviewProvenanceView,
    },
    NeedsReview {
        item: PreviewCatalogItemView,
        reason: String,
    },
    SearchCandidates {
        candidates: Vec<PreviewSearchCandidateView>,
        #[serde(rename = "moreAvailable")]
        more_available: bool,
    },
    Unsupported,
    RateLimited,
    Unavailable,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreviewProvenanceView {
    Automatic,
    Manual,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewCatalogItemView {
    pub uuid: String,
    pub canonical_url: String,
    pub detail_url: String,
    pub media_type: String,
    pub model: String,
    pub parent_uuid: Option<String>,
    /// A list preserves distinct, potentially contradictory identity claims.
    pub external_ids: Vec<PreviewExternalIdClaim>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewExternalIdClaim {
    pub scheme: String,
    pub value: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewSearchCandidateView {
    pub title: String,
    pub item: PreviewCatalogItemView,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewComparisonView {
    pub profile_id: String,
    #[schemars(range(min = 1, max = 4294967295_u64))]
    #[schema(minimum = 1, maximum = 4294967295_u64)]
    pub profile_version: u32,
    pub fields: Vec<PreviewComparedFieldView>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewComparedFieldView {
    pub rule_id: String,
    pub field: String,
    pub suggestion: Option<ScalarValue>,
    pub sink: PreviewSinkView,
    pub status: PreviewComparisonStatusView,
    pub reason: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewSinkView {
    Known { value: ScalarValue },
    Empty,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreviewComparisonStatusView {
    Accepted,
    Skipped,
    Rejected,
    NeedsReview,
}
