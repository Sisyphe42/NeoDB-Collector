//! Persistent historical preview-result contracts. No serializer or runtime dependency.
use crate::{
    access::{Endpoint, PortFuture},
    catalog::{CatalogItem, CatalogSearchPage, validate_catalog_item},
    preview::{Comparison, ComparisonStatus, SinkValue, compare_mapped_field},
    preview_jobs::{MAX_PREVIEW_OBSERVATIONS, PreviewResolutionInput, valid_id, validate_page},
    preview_records::{PreviewRecord, PreviewRecordProvenance, PreviewResolutionProof},
};
use collector_domain::{
    ApiError,
    mapping::MappingProfile,
    observation::{Observation, ScalarValue},
};

pub const MAX_PREVIEW_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_PREVIEW_RESULTS_TOTAL_BYTES: usize = 32 * 1024 * 1024;

pub struct PreviewResultWrite {
    job_id: String,
    ordinal: u32,
    attempt: u32,
    committed_at: i64,
    record: PreviewRecord,
}

impl PreviewResultWrite {
    pub fn new(
        job_id: String,
        ordinal: u32,
        attempt: u32,
        committed_at: i64,
        record: PreviewRecord,
    ) -> Result<Self, ApiError> {
        if !valid_id(&job_id)
            || ordinal as usize >= MAX_PREVIEW_OBSERVATIONS
            || attempt == 0
            || committed_at < 0
        {
            return Err(invalid_result());
        }
        Ok(Self {
            job_id,
            ordinal,
            attempt,
            committed_at,
            record,
        })
    }

    pub fn job_id(&self) -> &str {
        &self.job_id
    }
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
    pub fn committed_at(&self) -> i64 {
        self.committed_at
    }
    pub fn record(&self) -> &PreviewRecord {
        &self.record
    }
    pub fn binding_proof(&self) -> &Endpoint {
        self.record.binding_proof()
    }
    pub fn profile_proof(&self) -> &MappingProfile {
        self.record.profile_proof()
    }
    pub fn observation_proof(&self) -> &Observation {
        self.record.observation_proof()
    }
    pub fn matches_resolution_input(&self, input: &PreviewResolutionInput) -> bool {
        match (self.record.resolution_proof(), input) {
            (PreviewResolutionProof::Native, PreviewResolutionInput::Native) => true,
            (PreviewResolutionProof::Search(proof), PreviewResolutionInput::Search(saved)) => {
                proof == saved
            }
            (PreviewResolutionProof::Manual(proof), PreviewResolutionInput::Manual(saved)) => {
                proof == &saved.detail_path()
            }
            _ => false,
        }
    }
}

pub trait PreviewResultRepository: Send + Sync {
    fn store(&self, write: PreviewResultWrite) -> PortFuture<'_, bool>;
    fn results(
        &self,
        job_id: String,
        attempt: u32,
        offset: u32,
        limit: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>>;
    fn complete(&self, job_id: String, attempt: u32, now: i64) -> PortFuture<'_, bool>;
}

pub struct StoredPreviewResult {
    ordinal: u32,
    attempt: u32,
    committed_at: i64,
    source_connector_id: String,
    source_record_id: String,
    source_fingerprint: String,
    profile_id: String,
    profile_version: u32,
    base_url: String,
    account: String,
    outcome: StoredPreviewOutcome,
}

pub struct StoredPreviewMetadata {
    pub ordinal: u32,
    pub attempt: u32,
    pub committed_at: i64,
    pub source_connector_id: String,
    pub source_record_id: String,
    pub source_fingerprint: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub base_url: String,
    pub account: String,
}

pub enum StoredPreviewOutcome {
    Compared {
        item: CatalogItem,
        observed_at: i64,
        comparison: Comparison,
        provenance: PreviewRecordProvenance,
    },
    NeedsReview {
        item: CatalogItem,
        reason: String,
    },
    SearchCandidates(CatalogSearchPage),
    Unsupported,
    RateLimited,
    Unavailable,
}

impl StoredPreviewResult {
    pub fn from_stored(
        metadata: StoredPreviewMetadata,
        outcome: StoredPreviewOutcome,
    ) -> Result<Self, ApiError> {
        if metadata.ordinal as usize >= MAX_PREVIEW_OBSERVATIONS
            || metadata.attempt == 0
            || metadata.committed_at < 0
            || !bounded(&metadata.source_connector_id, 128)
            || !bounded(&metadata.source_record_id, 128)
            || !bounded(&metadata.source_fingerprint, 128)
            || !valid_id(&metadata.profile_id)
            || metadata.profile_version == 0
            || !bounded(&metadata.base_url, 4096)
            || !bounded(&metadata.account, 256)
        {
            return Err(invalid_stored());
        }
        validate_outcome(&metadata, &outcome)?;
        Ok(Self {
            ordinal: metadata.ordinal,
            attempt: metadata.attempt,
            committed_at: metadata.committed_at,
            source_connector_id: metadata.source_connector_id,
            source_record_id: metadata.source_record_id,
            source_fingerprint: metadata.source_fingerprint,
            profile_id: metadata.profile_id,
            profile_version: metadata.profile_version,
            base_url: metadata.base_url,
            account: metadata.account,
            outcome,
        })
    }

    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
    pub fn committed_at(&self) -> i64 {
        self.committed_at
    }
    pub fn source_connector_id(&self) -> &str {
        &self.source_connector_id
    }
    pub fn source_record_id(&self) -> &str {
        &self.source_record_id
    }
    pub fn source_fingerprint(&self) -> &str {
        &self.source_fingerprint
    }
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    pub fn profile_version(&self) -> u32 {
        self.profile_version
    }
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn outcome(&self) -> &StoredPreviewOutcome {
        &self.outcome
    }
}

pub fn validate_result_page(limit: u32) -> Result<(), ApiError> {
    validate_page(limit)
}

/// Replays the existing evaluator only to verify that historical source
/// suggestions and rule order still agree with the immutable job inputs. It
/// does not recreate a snapshot, resolve identity or grant write authority.
pub fn validate_historical_against_inputs(
    profile: &MappingProfile,
    observation: &Observation,
    result: &StoredPreviewResult,
) -> Result<(), ApiError> {
    let StoredPreviewOutcome::Compared { comparison, .. } = result.outcome() else {
        return Ok(());
    };
    let mapped = crate::mapping::evaluate(profile, observation).map_err(|_| invalid_stored())?;
    if mapped.len() != comparison.fields.len() {
        return Err(invalid_stored());
    }
    let suggested_shelf = mapped.iter().any(|field| {
        field.target == "shelf_type"
            && field.status == collector_domain::mapping::FieldStatus::Accepted
            && field.value.is_some()
    });
    if comparison.fields.iter().enumerate().any(|(index, field)| {
        comparison.fields[..index]
            .iter()
            .any(|other| other.field == field.field && other.sink != field.sink)
    }) {
        return Err(invalid_stored());
    }
    let existing_shelf = comparison.fields.iter().any(|field| {
        matches!(
            (&field.field[..], &field.sink),
            ("shelf_type", SinkValue::Known(_))
                | ("rating_grade", SinkValue::Known(_) | SinkValue::Empty)
        )
    });
    let shelf_ready = existing_shelf || suggested_shelf;
    let shelf_sink = comparison
        .fields
        .iter()
        .find(|field| field.field == "shelf_type")
        .map(|field| &field.sink);
    let rating_sink = comparison
        .fields
        .iter()
        .find(|field| field.field == "rating_grade")
        .map(|field| &field.sink);
    if matches!(shelf_sink, Some(SinkValue::Empty))
        && matches!(rating_sink, Some(SinkValue::Known(_) | SinkValue::Empty))
        || matches!(shelf_sink, Some(SinkValue::Known(_)))
            && matches!(rating_sink, Some(SinkValue::Unknown))
    {
        return Err(invalid_stored());
    }
    for (mapped, field) in mapped.into_iter().zip(&comparison.fields) {
        if compare_mapped_field(mapped, field.sink.clone(), shelf_ready) != *field {
            return Err(invalid_stored());
        }
    }
    Ok(())
}

fn validate_outcome(
    metadata: &StoredPreviewMetadata,
    outcome: &StoredPreviewOutcome,
) -> Result<(), ApiError> {
    match outcome {
        StoredPreviewOutcome::Compared {
            item,
            observed_at,
            comparison,
            ..
        } => {
            if *observed_at < 0 || *observed_at > metadata.committed_at {
                return Err(invalid_stored());
            }
            validate_item(&metadata.base_url, item)?;
            if comparison.profile_id != metadata.profile_id
                || comparison.profile_version != metadata.profile_version
                || comparison.fields.len() > 128
                || comparison.fields.iter().any(|field| !valid_field(field))
                || comparison.fields.iter().enumerate().any(|(index, field)| {
                    comparison.fields[..index]
                        .iter()
                        .any(|other| other.rule_id == field.rule_id)
                })
            {
                return Err(invalid_stored());
            }
        }
        StoredPreviewOutcome::NeedsReview { item, reason } => {
            validate_item(&metadata.base_url, item)?;
            if !matches!(
                reason.as_str(),
                "item_granularity_mismatch" | "item_identity_conflict" | "item_identity_missing"
            ) {
                return Err(invalid_stored());
            }
        }
        StoredPreviewOutcome::SearchCandidates(page) => {
            if page.candidates.len() > 20
                || page.candidates.iter().any(|candidate| {
                    candidate.title.is_empty()
                        || candidate.title.len() > 1024
                        || candidate.title.chars().any(char::is_control)
                        || candidate.title.trim() != candidate.title
                        || validate_item(&metadata.base_url, &candidate.item).is_err()
                })
                || page
                    .candidates
                    .iter()
                    .enumerate()
                    .any(|(index, candidate)| {
                        page.candidates[..index]
                            .iter()
                            .any(|other| other.item.uuid == candidate.item.uuid)
                    })
            {
                return Err(invalid_stored());
            }
        }
        StoredPreviewOutcome::Unsupported
        | StoredPreviewOutcome::RateLimited
        | StoredPreviewOutcome::Unavailable => {}
    }
    Ok(())
}

fn valid_field(field: &crate::preview::ComparedField) -> bool {
    if !valid_id(&field.rule_id) || !matches!(field.field.as_str(), "rating_grade" | "shelf_type") {
        return false;
    }
    let suggestion_valid = field
        .suggestion
        .as_ref()
        .is_none_or(|v| valid_suggestion(&field.field, v));
    let sink_valid = match &field.sink {
        SinkValue::Known(value) => valid_sink(&field.field, value),
        SinkValue::Empty | SinkValue::Unknown => true,
    };
    let reason_valid = match field.status {
        ComparisonStatus::Accepted => {
            field.reason == "sink_empty_accepts_suggestion"
                && field.suggestion.is_some()
                && matches!(field.sink, SinkValue::Empty)
        }
        ComparisonStatus::NeedsReview => match field.reason.as_str() {
            "sink_conflict_needs_review" => {
                matches!((&field.suggestion, &field.sink), (Some(source), SinkValue::Known(sink)) if source != sink)
            }
            "sink_state_unknown" | "rating_requires_shelf" => {
                field.suggestion.is_some() && matches!(field.sink, SinkValue::Unknown)
            }
            _ => false,
        },
        ComparisonStatus::Skipped => {
            if field.reason == "already_equal" {
                matches!((&field.suggestion, &field.sink), (Some(source), SinkValue::Known(sink)) if source == sink)
            } else {
                matches!(
                    field.reason.as_str(),
                    "rule_disabled"
                        | "rule_skipped"
                        | "source_missing"
                        | "enum_unmapped"
                        | "condition_false"
                ) && field.suggestion.is_none()
            }
        }
        ComparisonStatus::Rejected => {
            matches!(
                field.reason.as_str(),
                "source_type_invalid"
                    | "source_range_invalid"
                    | "mapped_value_invalid"
                    | "mapped_value_missing"
            ) && field.suggestion.is_none()
        }
    };
    suggestion_valid && sink_valid && reason_valid
}

fn valid_suggestion(field: &str, value: &ScalarValue) -> bool {
    match (field, value) {
        ("rating_grade", ScalarValue::Number(value)) => {
            value.is_finite() && (0.0..=10.0).contains(value)
        }
        ("shelf_type", ScalarValue::Text(value)) => matches!(
            value.as_str(),
            "wishlist" | "progress" | "complete" | "dropped"
        ),
        _ => false,
    }
}

fn valid_sink(field: &str, value: &ScalarValue) -> bool {
    match (field, value) {
        ("rating_grade", ScalarValue::Number(value)) => {
            value.is_finite() && (1.0..=10.0).contains(value) && value.fract() == 0.0
        }
        ("shelf_type", ScalarValue::Text(value)) => matches!(
            value.as_str(),
            "wishlist" | "progress" | "complete" | "dropped"
        ),
        _ => false,
    }
}

fn validate_item(base: &str, item: &CatalogItem) -> Result<(), ApiError> {
    validate_catalog_item(base, item).map_err(|_| invalid_stored())
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn invalid_result() -> ApiError {
    ApiError::new(
        "preview_result_invalid",
        "Preview result metadata is invalid.",
    )
}

fn invalid_stored() -> ApiError {
    ApiError::new(
        "database_unavailable",
        "Stored preview result data is invalid.",
    )
}

#[cfg(test)]
mod tests;
