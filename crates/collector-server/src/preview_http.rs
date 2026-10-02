//! Authenticated historical preview inspection and local job management.
use crate::{
    access::{self, AccessState, HttpError},
    resolution_control::TokioResolutionControl,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use collector_core::{
    access::Endpoint,
    catalog::{CatalogItem, CatalogSearchPage},
    catalog_resolver::ResolutionControl,
    preview::{Comparison, ComparisonStatus, SinkValue},
    preview_jobs::{PreviewJob, PreviewJobStatus},
    preview_records::PreviewRecordProvenance,
    preview_results::{StoredPreviewOutcome, StoredPreviewResult},
};
use collector_domain::{ApiError, preview::*};
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MANAGEMENT_BUDGET: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn routes() -> Router<AccessState> {
    Router::new()
        .route("/api/v1/previews/{id}", get(job).delete(delete_preview))
        .route("/api/v1/previews/{id}/results", get(results))
        .route("/api/v1/previews/{id}/cancel", post(cancel_preview))
        .layer(DefaultBodyLimit::max(4096))
}

pub(crate) async fn authorize(
    state: &AccessState,
    headers: &HeaderMap,
    scope: &str,
    mutation: bool,
) -> Result<(), HttpError> {
    if headers.contains_key(header::AUTHORIZATION) {
        access::bearer(state, headers).await?;
        if !collector_core::access::PAIRING_SCOPES.contains(&scope) {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "scope_required",
                "The required import scope is missing.",
            ));
        }
    } else {
        access::admin(state, headers, mutation).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResultsQuery {
    attempt: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default = "default_limit")]
    limit: u32,
}

fn default_limit() -> u32 {
    50
}

#[utoipa::path(get, path="/api/v1/previews/{id}", params(("id"=String, Path)),
    responses((status=200, body=PreviewJobView), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=404, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn job(
    State(state): State<AccessState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
) -> Result<Response, HttpError> {
    authorize(&state, &headers, "imports.read", false).await?;
    let id = path.map_err(|_| request_invalid())?.0;
    validate_id(&id)?;
    let jobs = state.preview_jobs.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let stored = control
        .run(MANAGEMENT_BUDGET, jobs.load(id.clone()))
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(not_found)?;
    if stored.id() != id {
        return Err(unavailable());
    }
    ensure_retained(&state, &stored)?;
    let endpoint = control
        .run(MANAGEMENT_BUDGET, state.service.repository.endpoint())
        .await
        .map_err(|_| unavailable())?;
    ensure_retained(&state, &stored)?;
    json_response(&job_view(&stored, endpoint.as_ref()))
}

#[utoipa::path(get, path="/api/v1/previews/{id}/results", params(
        ("id"=String, Path),
        ("attempt"=u32, Query, minimum=1, maximum=4294967295_u64),
        ("offset"=Option<u32>, Query, minimum=0, maximum=4294967295_u64),
        ("limit"=Option<u32>, Query, minimum=1, maximum=100)
    ), responses((status=200, body=PreviewResultPage), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=404, body=ApiError), (status=409, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn results(
    State(state): State<AccessState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ResultsQuery>, QueryRejection>,
) -> Result<Response, HttpError> {
    authorize(&state, &headers, "imports.read", false).await?;
    let id = path.map_err(|_| request_invalid())?.0;
    validate_id(&id)?;
    let query = query.map_err(|_| request_invalid())?.0;
    if query.attempt == 0 || query.limit == 0 || query.limit > 100 {
        return Err(request_invalid());
    }
    let jobs = state.preview_jobs.as_ref().ok_or_else(unavailable)?;
    let result_port = state.preview_results.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let before = control
        .run(MANAGEMENT_BUDGET, jobs.load(id.clone()))
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(not_found)?;
    if before.id() != id {
        return Err(unavailable());
    }
    ensure_retained(&state, &before)?;
    if before.attempt() != query.attempt {
        return Err(attempt_conflict());
    }
    let rows = control
        .run(
            MANAGEMENT_BUDGET,
            result_port.results(id.clone(), query.attempt, query.offset, query.limit),
        )
        .await
        .map_err(|_| unavailable())?;
    let after = control
        .run(MANAGEMENT_BUDGET, jobs.load(id.clone()))
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(not_found)?;
    if after.id() != id {
        return Err(unavailable());
    }
    ensure_retained(&state, &after)?;
    if after.attempt() != query.attempt {
        return Err(attempt_conflict());
    }
    if !same_job(&before, &after) || !valid_rows(&after, query.offset, query.limit, &rows) {
        return Err(unavailable());
    }
    let page = PreviewResultPage {
        job_id: id,
        attempt: query.attempt,
        offset: query.offset,
        limit: query.limit,
        historical: true,
        executable: false,
        items: rows.iter().map(result_view).collect(),
    };
    json_response(&page)
}

#[utoipa::path(post, path="/api/v1/previews/{id}/cancel", params(("id"=String, Path)),
    request_body=PreviewCancelRequest,
    responses((status=200, body=PreviewMutationResult), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=413, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn cancel_preview(
    State(state): State<AccessState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<PreviewCancelRequest>, JsonRejection>,
) -> Result<Response, HttpError> {
    authorize(&state, &headers, "imports.submit", true).await?;
    let id = path.map_err(|_| request_invalid())?.0;
    validate_id(&id)?;
    let request = access::json(body)?;
    if request.expected_attempt == 0 {
        return Err(request_invalid());
    }
    let management = state.preview_management.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let applied = control
        .run(
            MANAGEMENT_BUDGET,
            management.cancel(&id, request.expected_attempt),
        )
        .await
        .map_err(|_| cancel_failed())?;
    json_response(&PreviewMutationResult { applied })
}

#[utoipa::path(delete, path="/api/v1/previews/{id}", params(("id"=String, Path)),
    responses((status=200, body=PreviewMutationResult), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn delete_preview(
    State(state): State<AccessState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
) -> Result<Response, HttpError> {
    authorize(&state, &headers, "imports.submit", true).await?;
    let id = path.map_err(|_| request_invalid())?.0;
    validate_id(&id)?;
    let management = state.preview_management.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let applied = control
        .run(MANAGEMENT_BUDGET, management.delete(&id))
        .await
        .map_err(|_| delete_failed())?;
    json_response(&PreviewMutationResult { applied })
}

fn ensure_retained(state: &AccessState, job: &PreviewJob) -> Result<(), HttpError> {
    let now = state.service.clock.now();
    if now < 0 {
        return Err(unavailable());
    }
    if job.expires_at() <= now {
        return Err(not_found());
    }
    Ok(())
}

fn same_job(left: &PreviewJob, right: &PreviewJob) -> bool {
    left.id() == right.id()
        && left.attempt() == right.attempt()
        && left.profile_id() == right.profile_id()
        && left.profile_version() == right.profile_version()
        && left.created_at() == right.created_at()
        && left.expires_at() == right.expires_at()
        && left.observation_count() == right.observation_count()
        && left.base_url() == right.base_url()
        && left.account() == right.account()
}

fn valid_rows(job: &PreviewJob, offset: u32, limit: u32, rows: &[StoredPreviewResult]) -> bool {
    if rows.len() > limit as usize {
        return false;
    }
    rows.iter().enumerate().all(|(index, row)| {
        row.ordinal() >= offset
            && row.ordinal() < job.observation_count()
            && (index == 0 || rows[index - 1].ordinal() < row.ordinal())
            && row.attempt() == job.attempt()
            && row.committed_at() >= job.created_at()
            && row.committed_at() <= job.updated_at()
            && row.committed_at() < job.expires_at()
            && row.profile_id() == job.profile_id()
            && row.profile_version() == job.profile_version()
            && row.base_url() == job.base_url()
            && row.account() == job.account()
    })
}

fn job_view(job: &PreviewJob, endpoint: Option<&Endpoint>) -> PreviewJobView {
    PreviewJobView {
        id: job.id().into(),
        status: match job.status() {
            PreviewJobStatus::Queued => PreviewJobStatusView::Queued,
            PreviewJobStatus::Running => PreviewJobStatusView::Running,
            PreviewJobStatus::Interrupted => PreviewJobStatusView::Interrupted,
            PreviewJobStatus::Failed => PreviewJobStatusView::Failed,
            PreviewJobStatus::Cancelled => PreviewJobStatusView::Cancelled,
            PreviewJobStatus::Completed => PreviewJobStatusView::Completed,
        },
        attempt: job.attempt(),
        created_at: job.created_at(),
        updated_at: job.updated_at(),
        expires_at: job.expires_at(),
        observation_count: job.observation_count(),
        profile_id: job.profile_id().into(),
        profile_version: job.profile_version(),
        base_url: job.base_url().into(),
        account: job.account().into(),
        binding_state: match endpoint {
            None => PreviewBindingState::Disconnected,
            Some(endpoint) if job.is_bound_to(endpoint) => PreviewBindingState::Current,
            Some(_) => PreviewBindingState::Changed,
        },
        historical: true,
        executable: false,
    }
}

fn result_view(row: &StoredPreviewResult) -> PreviewResultView {
    PreviewResultView {
        ordinal: row.ordinal(),
        attempt: row.attempt(),
        committed_at: row.committed_at(),
        source_connector_id: row.source_connector_id().into(),
        source_record_id: row.source_record_id().into(),
        source_fingerprint: row.source_fingerprint().into(),
        profile_id: row.profile_id().into(),
        profile_version: row.profile_version(),
        base_url: row.base_url().into(),
        account: row.account().into(),
        historical: true,
        executable: false,
        outcome: outcome_view(row.outcome()),
    }
}

fn outcome_view(outcome: &StoredPreviewOutcome) -> PreviewOutcomeView {
    match outcome {
        StoredPreviewOutcome::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } => PreviewOutcomeView::Compared {
            item: item_view(item),
            observed_at: *observed_at,
            comparison: comparison_view(comparison),
            provenance: match provenance {
                PreviewRecordProvenance::Automatic => PreviewProvenanceView::Automatic,
                PreviewRecordProvenance::Manual => PreviewProvenanceView::Manual,
            },
        },
        StoredPreviewOutcome::NeedsReview { item, reason } => PreviewOutcomeView::NeedsReview {
            item: item_view(item),
            reason: reason.clone(),
        },
        StoredPreviewOutcome::SearchCandidates(page) => search_view(page),
        StoredPreviewOutcome::Unsupported => PreviewOutcomeView::Unsupported,
        StoredPreviewOutcome::RateLimited => PreviewOutcomeView::RateLimited,
        StoredPreviewOutcome::Unavailable => PreviewOutcomeView::Unavailable,
    }
}

fn search_view(page: &CatalogSearchPage) -> PreviewOutcomeView {
    PreviewOutcomeView::SearchCandidates {
        candidates: page
            .candidates
            .iter()
            .map(|candidate| PreviewSearchCandidateView {
                title: candidate.title.clone(),
                item: item_view(&candidate.item),
            })
            .collect(),
        more_available: page.more_available,
    }
}

fn item_view(item: &CatalogItem) -> PreviewCatalogItemView {
    PreviewCatalogItemView {
        uuid: item.uuid.clone(),
        canonical_url: item.canonical_url.clone(),
        detail_url: item.detail_url.clone(),
        media_type: item.media_type.clone(),
        model: item.model.clone(),
        parent_uuid: item.parent_uuid.clone(),
        external_ids: item
            .external_ids
            .iter()
            .map(|(scheme, value)| PreviewExternalIdClaim {
                scheme: scheme.clone(),
                value: value.clone(),
            })
            .collect(),
    }
}

fn comparison_view(comparison: &Comparison) -> PreviewComparisonView {
    PreviewComparisonView {
        profile_id: comparison.profile_id.clone(),
        profile_version: comparison.profile_version,
        fields: comparison
            .fields
            .iter()
            .map(|field| PreviewComparedFieldView {
                rule_id: field.rule_id.clone(),
                field: field.field.clone(),
                suggestion: field.suggestion.clone(),
                sink: match &field.sink {
                    SinkValue::Known(value) => PreviewSinkView::Known {
                        value: value.clone(),
                    },
                    SinkValue::Empty => PreviewSinkView::Empty,
                    SinkValue::Unknown => PreviewSinkView::Unknown,
                },
                status: match field.status {
                    ComparisonStatus::Accepted => PreviewComparisonStatusView::Accepted,
                    ComparisonStatus::Skipped => PreviewComparisonStatusView::Skipped,
                    ComparisonStatus::Rejected => PreviewComparisonStatusView::Rejected,
                    ComparisonStatus::NeedsReview => PreviewComparisonStatusView::NeedsReview,
                },
                reason: field.reason.clone(),
            })
            .collect(),
    }
}

fn json_response(value: &impl serde::Serialize) -> Result<Response, HttpError> {
    let bytes = serde_json::to_vec(value).map_err(|_| unavailable())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(unavailable());
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .map_err(|_| unavailable())
}

pub(crate) fn validate_id(id: &str) -> Result<(), HttpError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
    {
        return Err(request_invalid());
    }
    Ok(())
}

fn request_invalid() -> HttpError {
    HttpError::new(
        StatusCode::BAD_REQUEST,
        "preview_request_invalid",
        "Preview inspection request is invalid.",
    )
}

fn not_found() -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "preview_job_not_found",
        "Preview job was not found or has expired.",
    )
}

fn attempt_conflict() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "preview_attempt_changed",
        "The requested preview attempt is no longer current.",
    )
}

fn unavailable() -> HttpError {
    HttpError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "preview_unavailable",
        "Preview inspection is temporarily unavailable.",
    )
}

fn cancel_failed() -> HttpError {
    HttpError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "preview_cancel_failed",
        "Preview cancellation could not be completed; query job status.",
    )
}

fn delete_failed() -> HttpError {
    HttpError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "preview_delete_failed",
        "Preview deletion could not be completed; query job status.",
    )
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use collector_core::{
        catalog::{CatalogSearchCandidate, CatalogSearchPage},
        preview::ComparedField,
        preview_results::StoredPreviewMetadata,
    };
    use collector_domain::observation::ScalarValue;

    fn item() -> CatalogItem {
        CatalogItem {
            uuid: "Resolved123".into(),
            canonical_url: "https://neodb.example/tv/Resolved123".into(),
            detail_url: "https://neodb.example/api/tv/Resolved123".into(),
            media_type: "tv".into(),
            model: "TVShow".into(),
            parent_uuid: None,
            external_ids: vec![
                ("imdb:title".into(), "tt1234567".into()),
                ("imdb:title".into(), "tt7654321".into()),
            ],
        }
    }

    fn stored(outcome: StoredPreviewOutcome) -> StoredPreviewResult {
        StoredPreviewResult::from_stored(
            StoredPreviewMetadata {
                ordinal: 0,
                attempt: 1,
                committed_at: 3,
                source_connector_id: "imdb:ratings_csv".into(),
                source_record_id: "tt1234567".into(),
                source_fingerprint: "fingerprint".into(),
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                base_url: "https://neodb.example".into(),
                account: "alice".into(),
            },
            outcome,
        )
        .unwrap()
    }

    #[test]
    fn every_historical_outcome_and_evidence_variant_is_projected() {
        let compared = stored(StoredPreviewOutcome::Compared {
            item: item(),
            observed_at: 2,
            comparison: Comparison {
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                fields: vec![
                    ComparedField {
                        rule_id: "shelf".into(),
                        field: "shelf_type".into(),
                        suggestion: Some(ScalarValue::Text("complete".into())),
                        sink: SinkValue::Empty,
                        status: ComparisonStatus::Accepted,
                        reason: "sink_empty_accepts_suggestion".into(),
                    },
                    ComparedField {
                        rule_id: "rating".into(),
                        field: "rating_grade".into(),
                        suggestion: Some(ScalarValue::Number(8.0)),
                        sink: SinkValue::Unknown,
                        status: ComparisonStatus::NeedsReview,
                        reason: "sink_state_unknown".into(),
                    },
                ],
            },
            provenance: PreviewRecordProvenance::Automatic,
        });
        let value = serde_json::to_value(result_view(&compared)).unwrap();
        assert_eq!(value["outcome"]["kind"], "compared");
        assert_eq!(value["outcome"]["provenance"], "automatic");
        assert_eq!(
            value["outcome"]["comparison"]["fields"][0]["sink"]["kind"],
            "empty"
        );
        assert_eq!(
            value["outcome"]["comparison"]["fields"][1]["sink"]["kind"],
            "unknown"
        );
        assert_eq!(
            value["outcome"]["item"]["externalIds"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let known = stored(StoredPreviewOutcome::Compared {
            item: item(),
            observed_at: 2,
            comparison: Comparison {
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                fields: vec![ComparedField {
                    rule_id: "shelf".into(),
                    field: "shelf_type".into(),
                    suggestion: Some(ScalarValue::Text("complete".into())),
                    sink: SinkValue::Known(ScalarValue::Text("complete".into())),
                    status: ComparisonStatus::Skipped,
                    reason: "already_equal".into(),
                }],
            },
            provenance: PreviewRecordProvenance::Manual,
        });
        let value = serde_json::to_value(result_view(&known)).unwrap();
        assert_eq!(value["outcome"]["provenance"], "manual");
        assert_eq!(
            value["outcome"]["comparison"]["fields"][0]["sink"]["kind"],
            "known"
        );

        let outcomes = [
            StoredPreviewOutcome::NeedsReview {
                item: item(),
                reason: "item_identity_conflict".into(),
            },
            StoredPreviewOutcome::SearchCandidates(CatalogSearchPage {
                candidates: vec![CatalogSearchCandidate {
                    title: "Candidate".into(),
                    item: item(),
                }],
                more_available: true,
            }),
            StoredPreviewOutcome::Unsupported,
            StoredPreviewOutcome::RateLimited,
            StoredPreviewOutcome::Unavailable,
        ];
        let kinds: Vec<_> = outcomes
            .into_iter()
            .map(|outcome| {
                serde_json::to_value(result_view(&stored(outcome))).unwrap()["outcome"]["kind"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "needs_review",
                "search_candidates",
                "unsupported",
                "rate_limited",
                "unavailable"
            ]
        );
    }

    #[tokio::test]
    async fn encoded_response_cap_accepts_exact_boundary_and_rejects_plus_one() {
        use axum::{body::to_bytes, response::IntoResponse};

        let exact = "x".repeat(MAX_RESPONSE_BYTES - 2);
        let response = match json_response(&exact) {
            Ok(response) => response,
            Err(_) => panic!("an exactly 8 MiB response must be accepted"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
                .await
                .unwrap()
                .len(),
            MAX_RESPONSE_BYTES
        );

        let oversized = "x".repeat(MAX_RESPONSE_BYTES - 1);
        let response = json_response(&oversized).unwrap_err().into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error: ApiError = serde_json::from_slice(&body).unwrap();
        assert_eq!(error.code, "preview_unavailable");
    }
}
