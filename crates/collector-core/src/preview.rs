//! Account-bound personal-state snapshots and pure preview comparison.
//!
//! A snapshot is historical evidence from one completed read. Its binding checks
//! are point-in-time freshness checks, not an atomic authorization for later work.
use crate::{
    access::{AccessRepository, Clock, Endpoint, SecretStore},
    catalog::CatalogItem,
    mapping,
    marks::{MarkRead, NeoDbMarkTransport},
    oauth::NeoDbOAuthTransport,
};
use collector_domain::{
    ApiError,
    mapping::{FieldStatus, MappedField, MappingProfile},
    observation::{Observation, ScalarValue},
};
use std::sync::Arc;

pub struct AccountMarkSnapshot {
    base_url: String,
    account: String,
    observed_at: i64,
    item: CatalogItem,
    state: SnapshotState,
    // Deliberately private: credential references are operational secret metadata.
    binding: Endpoint,
}

enum SnapshotState {
    Present {
        shelf_type: String,
        rating_grade: Option<u8>,
    },
    MissingShelf,
}

impl AccountMarkSnapshot {
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn account(&self) -> &str {
        &self.account
    }

    pub fn observed_at(&self) -> i64 {
        self.observed_at
    }

    pub fn item(&self) -> &CatalogItem {
        &self.item
    }
}

pub struct PreviewSnapshotService {
    repository: Arc<dyn AccessRepository>,
    secrets: Arc<dyn SecretStore>,
    clock: Arc<dyn Clock>,
    identity: Arc<dyn NeoDbOAuthTransport>,
    marks: Arc<dyn NeoDbMarkTransport>,
}

impl PreviewSnapshotService {
    pub fn new(
        repository: Arc<dyn AccessRepository>,
        secrets: Arc<dyn SecretStore>,
        clock: Arc<dyn Clock>,
        identity: Arc<dyn NeoDbOAuthTransport>,
        marks: Arc<dyn NeoDbMarkTransport>,
    ) -> Self {
        Self {
            repository,
            secrets,
            clock,
            identity,
            marks,
        }
    }

    pub async fn read(
        &self,
        expected: Endpoint,
        resolved_item: CatalogItem,
    ) -> Result<AccountMarkSnapshot, ApiError> {
        self.ensure_expected_binding(&expected).await?;
        let token = self
            .secrets
            .get(expected.credential_ref.clone())
            .await
            .map_err(|_| credential_unavailable())?;
        self.ensure_expected_binding(&expected).await?;

        let mark = self
            .marks
            .read_mark(
                expected.base_url.clone(),
                &token,
                expected.account.clone(),
                &resolved_item,
            )
            .await?;

        let state = match mark {
            MarkRead::Present(mark) => {
                if mark.owner != expected.account {
                    return Err(account_changed());
                }
                SnapshotState::Present {
                    shelf_type: mark.shelf_type,
                    rating_grade: mark.rating_grade,
                }
            }
            MarkRead::MissingShelf => {
                // Do not send a second request after learning that this credential
                // generation is stale. MissingShelf itself has no response owner.
                self.ensure_expected_binding(&expected).await?;
                let owner = self
                    .identity
                    .identity(expected.base_url.clone(), &token)
                    .await?;
                if owner.username != expected.account {
                    return Err(account_changed());
                }
                SnapshotState::MissingShelf
            }
        };

        self.ensure_expected_binding(&expected).await?;
        Ok(AccountMarkSnapshot {
            base_url: expected.base_url.clone(),
            account: expected.account.clone(),
            observed_at: self.clock.now(),
            item: resolved_item,
            state,
            binding: expected,
        })
    }

    /// Checks only whether the snapshot's exact endpoint/account/credential
    /// generation is current at this instant. It does not authorize future I/O.
    pub async fn ensure_current_binding(
        &self,
        snapshot: &AccountMarkSnapshot,
    ) -> Result<(), ApiError> {
        self.ensure_expected_binding(&snapshot.binding).await
    }

    /// Checks the complete expected endpoint/account/credential generation at
    /// this instant without reading the credential or mutating connection state.
    pub async fn ensure_expected_binding(&self, expected: &Endpoint) -> Result<(), ApiError> {
        let current = self
            .repository
            .endpoint()
            .await
            .map_err(|_| binding_unavailable())?;
        let Some(current) = current else {
            return Err(disconnected());
        };
        if current.base_url != expected.base_url
            || current.account != expected.account
            || current.credential_ref != expected.credential_ref
        {
            return Err(binding_changed());
        }
        Ok(())
    }
}

fn binding_unavailable() -> ApiError {
    ApiError::new(
        "preview_binding_unavailable",
        "The current NeoDB connection could not be checked.",
    )
}

fn credential_unavailable() -> ApiError {
    ApiError::new(
        "preview_credential_unavailable",
        "The current NeoDB credential could not be read.",
    )
}

fn disconnected() -> ApiError {
    ApiError::new(
        "preview_account_disconnected",
        "The NeoDB account is no longer connected.",
    )
}

fn binding_changed() -> ApiError {
    ApiError::new(
        "preview_binding_changed",
        "The NeoDB account connection changed; start a fresh preview.",
    )
}

fn account_changed() -> ApiError {
    ApiError::new(
        "preview_account_changed",
        "The NeoDB credential belongs to a different account.",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonStatus {
    Accepted,
    Skipped,
    Rejected,
    NeedsReview,
}

#[derive(Clone, PartialEq)]
pub enum SinkValue {
    Known(ScalarValue),
    Empty,
    Unknown,
}

#[derive(Clone, PartialEq)]
pub struct ComparedField {
    pub rule_id: String,
    pub field: String,
    /// Present only when the mapping evaluator produced a legitimate suggestion.
    pub suggestion: Option<ScalarValue>,
    pub sink: SinkValue,
    pub status: ComparisonStatus,
    pub reason: String,
}

#[derive(Clone, PartialEq)]
pub struct Comparison {
    pub profile_id: String,
    pub profile_version: u32,
    pub fields: Vec<ComparedField>,
}

/// Compares deterministic mapping suggestions with one normalized point-in-time
/// snapshot. Prior observation-to-item identity resolution remains the caller's
/// responsibility; this function has no execution path.
pub fn compare(
    profile: &MappingProfile,
    observation: &Observation,
    snapshot: &AccountMarkSnapshot,
) -> Result<Comparison, ApiError> {
    let mapped = mapping::evaluate(profile, observation)?;
    // Evaluate readiness across all rules so rule order cannot change the result.
    let suggested_shelf = mapped.iter().any(|field| {
        field.target == "shelf_type"
            && field.status == FieldStatus::Accepted
            && field.value.is_some()
    });
    let existing_shelf = matches!(&snapshot.state, SnapshotState::Present { .. });
    let shelf_ready = existing_shelf || suggested_shelf;

    let fields = mapped
        .into_iter()
        .map(|field| {
            let sink = snapshot.sink(&field.target);
            compare_mapped_field(field, sink, shelf_ready)
        })
        .collect();

    Ok(Comparison {
        profile_id: profile.id.clone(),
        profile_version: profile.version,
        fields,
    })
}

pub(crate) fn compare_mapped_field(
    field: MappedField,
    sink: SinkValue,
    shelf_ready: bool,
) -> ComparedField {
    let suggestion = field.value;
    let (status, reason) = match field.status {
        FieldStatus::Skipped => (ComparisonStatus::Skipped, field.reason),
        FieldStatus::Rejected => (ComparisonStatus::Rejected, field.reason),
        FieldStatus::Accepted if field.target == "rating_grade" && !shelf_ready => (
            ComparisonStatus::NeedsReview,
            "rating_requires_shelf".into(),
        ),
        FieldStatus::Accepted => match (&suggestion, &sink) {
            (Some(source), SinkValue::Known(current)) if source == current => {
                (ComparisonStatus::Skipped, "already_equal".into())
            }
            (Some(_), SinkValue::Known(_)) => (
                ComparisonStatus::NeedsReview,
                "sink_conflict_needs_review".into(),
            ),
            (Some(_), SinkValue::Empty) => (
                ComparisonStatus::Accepted,
                "sink_empty_accepts_suggestion".into(),
            ),
            (Some(_), SinkValue::Unknown) => {
                (ComparisonStatus::NeedsReview, "sink_state_unknown".into())
            }
            (None, _) => (ComparisonStatus::Rejected, "mapped_value_missing".into()),
        },
    };
    ComparedField {
        rule_id: field.rule_id,
        field: field.target,
        suggestion,
        sink,
        status,
        reason,
    }
}

impl AccountMarkSnapshot {
    fn sink(&self, target: &str) -> SinkValue {
        match (&self.state, target) {
            (SnapshotState::Present { shelf_type, .. }, "shelf_type") => {
                SinkValue::Known(ScalarValue::Text(shelf_type.clone()))
            }
            (
                SnapshotState::Present {
                    rating_grade: Some(rating),
                    ..
                },
                "rating_grade",
            ) => SinkValue::Known(ScalarValue::Number(f64::from(*rating))),
            (
                SnapshotState::Present {
                    rating_grade: None, ..
                },
                "rating_grade",
            ) => SinkValue::Empty,
            (SnapshotState::MissingShelf, "shelf_type") => SinkValue::Empty,
            (SnapshotState::MissingShelf, "rating_grade") => SinkValue::Unknown,
            _ => SinkValue::Unknown,
        }
    }
}

#[cfg(test)]
mod tests;
