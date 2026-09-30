//! Single-record preview orchestration. Results are historical evidence only.
use crate::{
    access::Endpoint,
    catalog::{CatalogItem, CatalogSearchPage, NeoDbCatalogSearchTransport, NeoDbCatalogTransport},
    catalog_resolver::{
        CatalogIdentity, RESOLUTION_BUDGET, Resolution, ResolutionControl, resolve_manual,
        resolve_native, resolve_with_search, validate_catalog_search_query,
    },
    mapping,
    preview::{AccountMarkSnapshot, Comparison, PreviewSnapshotService, compare},
    preview_jobs::validate_imdb_observation,
};
use collector_domain::{ApiError, mapping::MappingProfile, observation::Observation};
use std::sync::Arc;

pub enum PreviewResolutionMode<'a> {
    Native,
    Search { query: &'a str },
    Manual { reference: &'a str },
}

pub struct PreviewRecordRequest<'a> {
    pub expected: &'a Endpoint,
    pub profile: &'a MappingProfile,
    pub observation: &'a Observation,
    pub identity: &'a CatalogIdentity,
    pub mode: PreviewResolutionMode<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewRecordProvenance {
    Automatic,
    Manual,
}

pub enum PreviewRecordOutcome {
    Compared(Box<ComparedPreviewRecord>),
    NeedsReview {
        item: CatalogItem,
        reason: &'static str,
    },
    SearchCandidates(CatalogSearchPage),
    Unsupported,
    RateLimited,
    Unavailable,
}

pub struct ComparedPreviewRecord {
    pub snapshot: AccountMarkSnapshot,
    pub comparison: Comparison,
    pub provenance: PreviewRecordProvenance,
}

pub struct PreviewRecord {
    source_connector_id: String,
    source_record_id: String,
    source_fingerprint: String,
    profile_id: String,
    profile_version: u32,
    binding: Endpoint,
    outcome: PreviewRecordOutcome,
    proof: PreviewRecordProof,
}

struct PreviewRecordProof {
    profile: MappingProfile,
    observation: Observation,
}

impl PreviewRecord {
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

    pub fn outcome(&self) -> &PreviewRecordOutcome {
        &self.outcome
    }

    pub(crate) fn binding_proof(&self) -> &Endpoint {
        &self.binding
    }

    pub(crate) fn profile_proof(&self) -> &MappingProfile {
        &self.proof.profile
    }

    pub(crate) fn observation_proof(&self) -> &Observation {
        &self.proof.observation
    }
}

pub struct PreviewRecordService {
    snapshots: Arc<PreviewSnapshotService>,
}

impl PreviewRecordService {
    pub fn new(snapshots: Arc<PreviewSnapshotService>) -> Self {
        Self { snapshots }
    }

    pub async fn preview<T, C>(
        &self,
        transport: &T,
        control: &C,
        request: PreviewRecordRequest<'_>,
    ) -> Result<PreviewRecord, ApiError>
    where
        T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport,
        C: ResolutionControl,
    {
        validate_request(&request)?;
        let deadline = control
            .now()
            .checked_add(RESOLUTION_BUDGET)
            .ok_or_else(resolution_timeout)?;
        control
            .run(
                deadline,
                Box::pin(async move {
                    self.snapshots
                        .ensure_expected_binding(request.expected)
                        .await?;

                    let resolution = match request.mode {
                        PreviewResolutionMode::Native => {
                            resolve_native(
                                transport,
                                control,
                                &request.expected.base_url,
                                request.identity,
                            )
                            .await?
                        }
                        PreviewResolutionMode::Search { query } => {
                            resolve_with_search(
                                transport,
                                control,
                                &request.expected.base_url,
                                request.identity,
                                query,
                            )
                            .await?
                        }
                        PreviewResolutionMode::Manual { reference } => {
                            resolve_manual(
                                transport,
                                control,
                                &request.expected.base_url,
                                request.identity,
                                reference,
                            )
                            .await?
                        }
                    };

                    self.snapshots
                        .ensure_expected_binding(request.expected)
                        .await?;

                    let outcome = match resolution {
                        Resolution::Matched(item) => {
                            self.compared(
                                request.expected,
                                request.profile,
                                request.observation,
                                item,
                                PreviewRecordProvenance::Automatic,
                            )
                            .await?
                        }
                        Resolution::ManuallySelected(item) => {
                            self.compared(
                                request.expected,
                                request.profile,
                                request.observation,
                                item,
                                PreviewRecordProvenance::Manual,
                            )
                            .await?
                        }
                        Resolution::NeedsReview { item, reason } => {
                            PreviewRecordOutcome::NeedsReview { item, reason }
                        }
                        Resolution::SearchCandidates(page) => {
                            PreviewRecordOutcome::SearchCandidates(page)
                        }
                        Resolution::Unsupported => PreviewRecordOutcome::Unsupported,
                        Resolution::RateLimited => PreviewRecordOutcome::RateLimited,
                        Resolution::Unavailable => PreviewRecordOutcome::Unavailable,
                    };

                    self.snapshots
                        .ensure_expected_binding(request.expected)
                        .await?;
                    Ok(PreviewRecord {
                        source_connector_id: request.observation.source_connector_id.clone(),
                        source_record_id: request.observation.source_record_id.clone(),
                        source_fingerprint: request.observation.idempotency_key.clone(),
                        profile_id: request.profile.id.clone(),
                        profile_version: request.profile.version,
                        binding: request.expected.clone(),
                        outcome,
                        proof: PreviewRecordProof {
                            profile: request.profile.clone(),
                            observation: request.observation.clone(),
                        },
                    })
                }),
            )
            .await
    }

    async fn compared(
        &self,
        expected: &Endpoint,
        profile: &MappingProfile,
        observation: &Observation,
        item: CatalogItem,
        provenance: PreviewRecordProvenance,
    ) -> Result<PreviewRecordOutcome, ApiError> {
        let snapshot = self.snapshots.read(expected.clone(), item).await?;
        let comparison = compare(profile, observation, &snapshot)?;
        Ok(PreviewRecordOutcome::Compared(Box::new(
            ComparedPreviewRecord {
                snapshot,
                comparison,
                provenance,
            },
        )))
    }
}

fn validate_request(request: &PreviewRecordRequest<'_>) -> Result<(), ApiError> {
    validate_imdb_observation(request.observation)?;
    // Evaluation validates the exact disabled profile/source pairing before I/O.
    mapping::evaluate(request.profile, request.observation)?;

    let observed_id = request
        .observation
        .item
        .external_ids
        .get(&request.identity.scheme);
    if request.identity.model.is_empty()
        || request.identity.source_url != request.observation.item.canonical_url
        || request.identity.media_type != request.observation.item.media_type
        || request.identity.external_id != request.observation.source_record_id
        || observed_id != Some(&request.identity.external_id)
    {
        return Err(ApiError::new(
            "catalog_identity_invalid",
            "Expected a validated source identity.",
        ));
    }

    match request.mode {
        PreviewResolutionMode::Search { query } => {
            validate_catalog_search_query(query)?;
        }
        PreviewResolutionMode::Manual { reference } if reference.trim().is_empty() => {
            return Err(ApiError::new(
                "neodb_catalog_reference_invalid",
                "Expected a supported NeoDB catalog detail reference.",
            ));
        }
        PreviewResolutionMode::Native | PreviewResolutionMode::Manual { .. } => {}
    }
    Ok(())
}

fn resolution_timeout() -> ApiError {
    ApiError::new(
        "catalog_resolution_timeout",
        "Catalog resolution exceeded its time budget.",
    )
}

#[cfg(test)]
mod tests;
