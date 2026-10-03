//! Internal resumable-preview job contracts. No HTTP, runtime or persistence code.
use crate::{
    access::{Endpoint, PortFuture},
    catalog::valid_imdb_title_id,
    catalog_resolver::validate_catalog_search_query,
    mapping,
};
use collector_domain::{
    ApiError,
    mapping::MappingProfile,
    observation::{Observation, ScalarValue},
};

pub const MAX_PREVIEW_OBSERVATIONS: usize = 10_000;
pub const MAX_PREVIEW_OBSERVATION_BYTES: usize = 16 * 1024;
pub const MAX_PREVIEW_TOTAL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PREVIEW_PAGE: u32 = 100;
pub const DEFAULT_PREVIEW_RETENTION_DAYS: u32 = 30;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewRetention {
    days: u32,
}

impl PreviewRetention {
    pub fn new(days: u32) -> Result<Self, ApiError> {
        if days == 0 {
            return Err(invalid_retention());
        }
        Ok(Self { days })
    }

    pub fn days(self) -> u32 {
        self.days
    }

    pub fn expires_at(self, created_at: i64) -> Result<i64, ApiError> {
        if created_at < 0 {
            return Err(invalid_retention());
        }
        let seconds = i64::from(self.days)
            .checked_mul(SECONDS_PER_DAY)
            .ok_or_else(invalid_retention)?;
        created_at
            .checked_add(seconds)
            .ok_or_else(invalid_retention)
    }
}

impl Default for PreviewRetention {
    fn default() -> Self {
        Self {
            days: DEFAULT_PREVIEW_RETENTION_DAYS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewJobStatus {
    Queued,
    Running,
    Interrupted,
    Failed,
    Cancelled,
    Completed,
}

impl PreviewJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ApiError> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "interrupted" => Ok(Self::Interrupted),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "completed" => Ok(Self::Completed),
            _ => Err(invalid_stored()),
        }
    }
}

pub struct PreviewJob {
    id: String,
    profile_id: String,
    profile_version: u32,
    status: PreviewJobStatus,
    attempt: u32,
    created_at: i64,
    updated_at: i64,
    expires_at: i64,
    observation_count: u32,
    // Private operational binding; especially never Debug/serialize credential_ref.
    binding: Endpoint,
}

pub struct PreviewJobIdentity {
    pub id: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub observation_count: u32,
}

pub struct PreviewJobLifecycle {
    pub status: PreviewJobStatus,
    pub attempt: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: i64,
}

impl PreviewJob {
    pub fn from_stored(
        identity: PreviewJobIdentity,
        lifecycle: PreviewJobLifecycle,
        binding: Endpoint,
    ) -> Result<Self, ApiError> {
        if !valid_id(&identity.id)
            || !valid_id(&identity.profile_id)
            || identity.profile_version == 0
            || lifecycle.attempt == 0
            || lifecycle.created_at < 0
            || lifecycle.updated_at < lifecycle.created_at
            || lifecycle.expires_at <= lifecycle.created_at
            || identity.observation_count as usize > MAX_PREVIEW_OBSERVATIONS
            || !valid_binding(&binding)
        {
            return Err(invalid_stored());
        }
        Ok(Self {
            id: identity.id,
            profile_id: identity.profile_id,
            profile_version: identity.profile_version,
            status: lifecycle.status,
            attempt: lifecycle.attempt,
            created_at: lifecycle.created_at,
            updated_at: lifecycle.updated_at,
            expires_at: lifecycle.expires_at,
            observation_count: identity.observation_count,
            binding,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    pub fn profile_version(&self) -> u32 {
        self.profile_version
    }
    pub fn status(&self) -> PreviewJobStatus {
        self.status
    }
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub fn observation_count(&self) -> u32 {
        self.observation_count
    }
    pub fn base_url(&self) -> &str {
        &self.binding.base_url
    }
    pub fn account(&self) -> &str {
        &self.binding.account
    }
    pub fn is_bound_to(&self, endpoint: &Endpoint) -> bool {
        self.binding.base_url == endpoint.base_url
            && self.binding.account == endpoint.account
            && self.binding.credential_ref == endpoint.credential_ref
    }
}

pub struct PreviewSubmission {
    id: String,
    binding: Endpoint,
    profile: MappingProfile,
    observations: Vec<Observation>,
    search_queries: Vec<(u32, String)>,
    created_at: i64,
    expires_at: i64,
}

impl PreviewSubmission {
    pub fn new(
        id: String,
        binding: Endpoint,
        profile: MappingProfile,
        observations: Vec<Observation>,
        created_at: i64,
        expires_at: i64,
    ) -> Result<Self, ApiError> {
        if !valid_id(&id) || !valid_binding(&binding) || created_at < 0 || expires_at <= created_at
        {
            return Err(ApiError::new(
                "preview_job_invalid",
                "Preview job metadata is invalid.",
            ));
        }
        mapping::validate(&profile)?;
        if observations.len() > MAX_PREVIEW_OBSERVATIONS {
            return Err(ApiError::new(
                "preview_submission_too_large",
                "Preview submission exceeds the normalized record limit.",
            ));
        }
        for observation in &observations {
            validate_imdb_observation(observation)?;
            // Enforce source/profile compatibility with the production evaluator.
            mapping::evaluate(&profile, observation)?;
        }
        Ok(Self {
            id,
            binding,
            profile,
            observations,
            search_queries: Vec::new(),
            created_at,
            expires_at,
        })
    }

    pub fn with_search_queries(
        mut self,
        search_queries: Vec<(u32, String)>,
    ) -> Result<Self, ApiError> {
        if search_queries.len() > self.observations.len() {
            return Err(invalid_search_plan());
        }
        let mut normalized = Vec::with_capacity(search_queries.len());
        for (ordinal, query) in search_queries {
            if ordinal as usize >= self.observations.len() {
                return Err(invalid_search_plan());
            }
            let query = validate_catalog_search_query(&query)?.to_owned();
            normalized.push((ordinal, query));
        }
        normalized.sort_unstable_by_key(|(ordinal, _)| *ordinal);
        if normalized.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(invalid_search_plan());
        }
        self.search_queries = normalized;
        Ok(self)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn binding(&self) -> &Endpoint {
        &self.binding
    }
    pub fn profile(&self) -> &MappingProfile {
        &self.profile
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    pub fn search_query(&self, ordinal: u32) -> Option<&str> {
        self.search_queries
            .binary_search_by_key(&ordinal, |(saved, _)| *saved)
            .ok()
            .map(|index| self.search_queries[index].1.as_str())
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

pub trait PreviewJobRepository: Send + Sync {
    fn submit(&self, submission: PreviewSubmission) -> PortFuture<'_, PreviewJob>;
    fn load(&self, id: String) -> PortFuture<'_, Option<PreviewJob>>;
    fn observations(&self, id: String, offset: u32, limit: u32)
    -> PortFuture<'_, Vec<Observation>>;
    fn search_query(&self, id: String, ordinal: u32) -> PortFuture<'_, Option<String>>;
    fn claim(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool>;
    fn cancel(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool>;
    fn interrupt_running(&self, now: i64) -> PortFuture<'_, u64>;
    fn fail(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool>;
    fn resume(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, Option<u32>>;
    fn delete(&self, id: String) -> PortFuture<'_, bool>;
    fn cleanup_expired(&self, now: i64) -> PortFuture<'_, u64>;
}

pub fn validate_page(limit: u32) -> Result<(), ApiError> {
    if limit == 0 || limit > MAX_PREVIEW_PAGE {
        return Err(ApiError::new(
            "preview_page_invalid",
            "Preview page size must be between 1 and 100.",
        ));
    }
    Ok(())
}

pub fn validate_transition(attempt: u32, now: i64) -> Result<(), ApiError> {
    if attempt == 0 || now < 0 {
        return Err(ApiError::new(
            "preview_transition_invalid",
            "Preview transition metadata is invalid.",
        ));
    }
    Ok(())
}

pub fn validate_imdb_observation(observation: &Observation) -> Result<(), ApiError> {
    let invalid = || {
        ApiError::new(
            "preview_observation_invalid",
            "Observation is outside the supported normalized preview subset.",
        )
    };
    if observation.schema_version != 1
        || observation.source_connector_id != "imdb:ratings_csv"
        || !valid_imdb_title_id(&observation.source_record_id)
        || !bounded_metadata(&observation.observed_at, 128)
        || !valid_idempotency_key(&observation.idempotency_key)
        || !matches!(observation.item.media_type.as_str(), "movie" | "tv")
        || observation.item.canonical_url
            != format!(
                "https://www.imdb.com/title/{}/",
                observation.source_record_id
            )
        || observation.item.external_ids.len() != 1
        || observation
            .item
            .external_ids
            .get("imdb:title")
            .is_none_or(|value| value != &observation.source_record_id)
        || observation
            .item
            .title_hint
            .as_ref()
            .is_some_and(|title| title.is_empty() || title.len() > 4096 || title.trim() != title)
        || observation.state.len() != 1
    {
        return Err(invalid());
    }
    match observation.state.get("rating") {
        Some(ScalarValue::Number(value))
            if value.is_finite() && *value >= 1.0 && *value <= 10.0 && value.fract() == 0.0 =>
        {
            Ok(())
        }
        _ => Err(invalid()),
    }
}

fn valid_idempotency_key(value: &str) -> bool {
    value.strip_prefix("imdb:ratings_csv:").is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

fn bounded_metadata(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_binding(binding: &Endpoint) -> bool {
    bounded_metadata(&binding.base_url, 4096)
        && bounded_metadata(&binding.account, 256)
        && bounded_metadata(&binding.credential_ref, 256)
}

fn invalid_stored() -> ApiError {
    ApiError::new(
        "database_unavailable",
        "Stored preview job data is invalid.",
    )
}

fn invalid_retention() -> ApiError {
    ApiError::new(
        "preview_retention_invalid",
        "Preview retention must be a positive whole number of days with a valid expiry.",
    )
}

fn invalid_search_plan() -> ApiError {
    ApiError::new(
        "preview_search_plan_invalid",
        "Preview search inputs must identify distinct submitted records.",
    )
}

#[cfg(test)]
mod tests;
