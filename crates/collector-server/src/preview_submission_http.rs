//! Authenticated bounded IMDb CSV submission and explicit preview resume.

use crate::{
    access::{AccessState, HttpError},
    preview_http,
    resolution_control::TokioResolutionControl,
};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::{
        Path, Query, Request, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use collector_connectors::imdb::{self, ImportError};
use collector_core::{
    catalog::CatalogReference,
    catalog_resolver::ResolutionControl,
    preview_jobs::{MAX_PREVIEW_OBSERVATIONS, PreviewJobStatus, PreviewSubmission},
};
use collector_domain::{ApiError, preview::*};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const ADMISSION_BUDGET: Duration = Duration::from_secs(10);
const JSON_BODY_LIMIT: usize = 4096;

enum ImportBody {
    Csv(Bytes),
    Structured(PreviewImdbResolutionRequest),
}

pub(crate) fn routes() -> Router<AccessState> {
    Router::new()
        .route("/api/v1/imports/imdb/ratings", post(import_imdb_ratings))
        .route("/api/v1/previews/{id}/resume", post(resume_preview))
}

#[utoipa::path(post, path="/api/v1/imports/imdb/ratings", params(
        ("profileId"=String, Query, min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_.:-]+$"),
        ("profileVersion"=u32, Query, minimum=1, maximum=4294967295_u64)
    ), request_body(content((String="text/csv"), (PreviewImdbResolutionRequest="application/json"))),
    responses((status=202, body=PreviewImportAccepted), (status=400, body=ApiError),
        (status=401, body=ApiError), (status=403, body=ApiError),
        (status=404, body=ApiError), (status=409, body=ApiError),
        (status=413, body=ApiError), (status=415, body=ApiError),
        (status=429, body=ApiError), (status=503, body=PreviewAdmissionFailure)))]
pub(crate) async fn import_imdb_ratings(
    State(state): State<AccessState>,
    headers: HeaderMap,
    query: Result<Query<PreviewImportSelection>, QueryRejection>,
    request: Request,
) -> Result<Response, HttpError> {
    preview_http::authorize(&state, &headers, "imports.submit", true).await?;
    let structured = import_content_type(&headers)?;
    let selection = query.map_err(|_| request_invalid())?.0;
    crate::mapping_http::valid_id(&selection.profile_id)?;
    if selection.profile_version == 0 {
        return Err(request_invalid());
    }
    let submission_runtime = state.preview_submission.as_ref().ok_or_else(unavailable)?;
    let mappings = state.mappings.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let deadline = ADMISSION_BUDGET;

    let endpoint = control
        .run(deadline, state.service.repository.endpoint())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(not_connected)?;
    let profile = control
        .run(
            deadline,
            mappings.load(
                selection.profile_id.clone(),
                Some(selection.profile_version),
            ),
        )
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(profile_not_found)?;
    if profile.id != selection.profile_id
        || profile.version != selection.profile_version
        || profile.enabled
        || profile.source_connector_id != imdb::CONNECTOR_ID
    {
        return Err(profile_conflict());
    }

    let bytes = control
        .run(
            deadline,
            Box::pin(read_body(request.into_body(), imdb::MAX_BYTES)),
        )
        .await
        .map_err(body_error)?;
    checkpoint(&control, deadline).await?;
    let body = if structured {
        ImportBody::Structured(serde_json::from_slice(&bytes).map_err(|_| request_invalid())?)
    } else {
        ImportBody::Csv(bytes)
    };
    let now = state.service.clock.now();
    if now < 0 {
        return Err(unavailable());
    }
    let (csv, resolution_inputs) = match &body {
        ImportBody::Csv(csv) => (csv.as_ref(), None),
        ImportBody::Structured(body) => {
            (body.csv.as_bytes(), Some(body.resolution_inputs.as_slice()))
        }
    };
    let batch = imdb::parse_ratings(csv, &now.to_string()).map_err(import_error)?;
    checkpoint(&control, deadline).await?;

    let observation_count = bounded_count(batch.observations.len())?;
    let skipped_row_count = bounded_count(batch.skipped_rows.len())?;
    let skipped_column_count = bounded_count(batch.skipped_columns.len())?;
    let duplicate_row_count = bounded_count(batch.duplicate_rows)?;
    let job_id = state.service.tokens.random().map_err(|_| unavailable())?;
    let expires_at = submission_runtime
        .retention()
        .expires_at(now)
        .map_err(|_| unavailable())?;
    let mut submission = PreviewSubmission::new(
        job_id.clone(),
        endpoint,
        profile,
        batch.observations,
        now,
        expires_at,
    )
    .map_err(submission_invalid)?;
    if let Some(inputs) = resolution_inputs {
        submission = with_resolution_inputs(submission, inputs)?;
    }
    checkpoint(&control, deadline).await?;
    let observer = submission_runtime
        .submit(submission)
        .map_err(pre_admission_error)?;
    match control.run(deadline, Box::pin(observer.wait())).await {
        Ok(receipt) => accepted_response(&PreviewImportAccepted {
            preview: accepted(receipt.job_id, receipt.attempt),
            observation_count,
            skipped_row_count,
            skipped_column_count,
            duplicate_row_count,
        }),
        Err(error) => admission_failure_response(
            job_id,
            1,
            crate::preview_runtime::sanitize_submit_error(error),
        ),
    }
}

#[utoipa::path(post, path="/api/v1/previews/{id}/resume", params(
        ("id"=String, Path, min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_.:-]+$")
    ),
    request_body=PreviewResumeRequest,
    responses((status=202, body=PreviewAccepted), (status=400, body=ApiError),
        (status=401, body=ApiError), (status=403, body=ApiError),
        (status=404, body=ApiError), (status=409, body=ApiError),
        (status=413, body=ApiError), (status=429, body=ApiError),
        (status=503, body=PreviewAdmissionFailure)))]
pub(crate) async fn resume_preview(
    State(state): State<AccessState>,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    request: Request,
) -> Result<Response, HttpError> {
    preview_http::authorize(&state, &headers, "imports.submit", true).await?;
    let id = path.map_err(|_| request_invalid())?.0;
    preview_http::validate_id(&id)?;
    ensure_json_content_type(&headers)?;
    let submission_runtime = state.preview_submission.as_ref().ok_or_else(unavailable)?;
    let jobs = state.preview_jobs.as_ref().ok_or_else(unavailable)?;
    let control = TokioResolutionControl::new(CancellationToken::new());
    let deadline = ADMISSION_BUDGET;
    let bytes = control
        .run(
            deadline,
            Box::pin(read_body(request.into_body(), JSON_BODY_LIMIT)),
        )
        .await
        .map_err(body_error)?;
    let body: PreviewResumeRequest =
        serde_json::from_slice(&bytes).map_err(|_| request_invalid())?;
    if body.expected_attempt == 0 {
        return Err(request_invalid());
    }
    let job = control
        .run(deadline, jobs.load(id.clone()))
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(job_not_found)?;
    if job.id() != id {
        return Err(unavailable());
    }
    let now = state.service.clock.now();
    if now < 0 {
        return Err(unavailable());
    }
    if job.expires_at() <= now {
        return Err(job_not_found());
    }
    if job.attempt() != body.expected_attempt {
        return Err(attempt_changed());
    }
    if !matches!(
        job.status(),
        PreviewJobStatus::Interrupted | PreviewJobStatus::Failed | PreviewJobStatus::Cancelled
    ) {
        return Err(resume_conflict());
    }
    let next_attempt = body
        .expected_attempt
        .checked_add(1)
        .ok_or_else(attempt_exhausted)?;
    checkpoint(&control, deadline).await?;
    let observer = submission_runtime
        .resume(&job)
        .map_err(pre_admission_error)?;
    match control.run(deadline, Box::pin(observer.wait())).await {
        Ok(receipt) => accepted_response(&accepted(receipt.job_id, receipt.attempt)),
        Err(error) => admission_failure_response(
            id,
            next_attempt,
            crate::preview_runtime::sanitize_resume_error(error),
        ),
    }
}

async fn read_body(body: Body, limit: usize) -> Result<Bytes, ApiError> {
    match to_bytes(body, limit).await {
        Ok(bytes) => Ok(bytes),
        Err(error) if error_chain_contains_length_limit(&error) => Err(ApiError::new(
            "request_too_large",
            "Request body exceeds the size limit.",
        )),
        Err(_) => Err(ApiError::new(
            "request_invalid",
            "Request body could not be read.",
        )),
    }
}

fn error_chain_contains_length_limit(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(value) = current {
        if value.is::<http_body_util::LengthLimitError>() {
            return true;
        }
        current = value.source();
    }
    false
}

async fn checkpoint(control: &TokioResolutionControl, deadline: Duration) -> Result<(), HttpError> {
    control
        .run(deadline, Box::pin(async { Ok(()) }))
        .await
        .map_err(|_| unavailable())
}

fn accepted(job_id: String, attempt: u32) -> PreviewAccepted {
    PreviewAccepted {
        job_id,
        attempt,
        executable: false,
    }
}

fn accepted_response(value: &impl serde::Serialize) -> Result<Response, HttpError> {
    Ok((StatusCode::ACCEPTED, axum::Json(value)).into_response())
}

fn admission_failure_response(
    job_id: String,
    attempt: u32,
    error: ApiError,
) -> Result<Response, HttpError> {
    let status = failure_status(&error);
    Ok((
        status,
        axum::Json(PreviewAdmissionFailure {
            job_id,
            attempt,
            error,
        }),
    )
        .into_response())
}

fn bounded_count(value: usize) -> Result<u32, HttpError> {
    u32::try_from(value).map_err(|_| unavailable())
}

fn import_content_type(headers: &HeaderMap) -> Result<bool, HttpError> {
    let mut values = headers.get_all(header::CONTENT_TYPE).iter();
    let value = values
        .next()
        .and_then(|value| value.to_str().ok())
        .filter(|_| values.next().is_none())
        .ok_or_else(unsupported_media_type)?;
    let mut parts = value.split(';').map(str::trim);
    if !parts
        .next()
        .is_some_and(|value| value.eq_ignore_ascii_case("text/csv"))
    {
        if value.eq_ignore_ascii_case("application/json") {
            return Ok(true);
        }
        return Err(unsupported_media_type());
    }
    match (parts.next(), parts.next()) {
        (None, None) => Ok(false),
        (Some(charset), None)
            if charset.split_once('=').is_some_and(|(name, value)| {
                name.trim().eq_ignore_ascii_case("charset")
                    && value.trim().eq_ignore_ascii_case("utf-8")
            }) =>
        {
            Ok(false)
        }
        _ => Err(unsupported_media_type()),
    }
}

fn with_resolution_inputs(
    submission: PreviewSubmission,
    inputs: &[PreviewResolutionChoice],
) -> Result<PreviewSubmission, HttpError> {
    if inputs.len() > MAX_PREVIEW_OBSERVATIONS || inputs.len() > submission.observations().len() {
        return Err(request_invalid());
    }
    let mut searches = Vec::new();
    let mut manuals = Vec::new();
    for input in inputs {
        let (ordinal, source_record_id) = match input {
            PreviewResolutionChoice::Search {
                ordinal,
                source_record_id,
                ..
            }
            | PreviewResolutionChoice::Manual {
                ordinal,
                source_record_id,
                ..
            } => (*ordinal, source_record_id),
        };
        if submission
            .observations()
            .get(ordinal as usize)
            .is_none_or(|observation| observation.source_record_id != *source_record_id)
        {
            return Err(request_invalid());
        }
        match input {
            PreviewResolutionChoice::Search { ordinal, query, .. } => {
                searches.push((*ordinal, query.clone()));
            }
            PreviewResolutionChoice::Manual { ordinal, item, .. } => {
                let reference = CatalogReference::from_stored(
                    item.uuid.clone(),
                    item.media_type.clone(),
                    item.model.clone(),
                )
                .map_err(submission_invalid)?;
                manuals.push((*ordinal, reference));
            }
        }
    }
    submission
        .with_search_queries(searches)
        .and_then(|submission| submission.with_manual_selections(manuals))
        .map_err(submission_invalid)
}

fn ensure_json_content_type(headers: &HeaderMap) -> Result<(), HttpError> {
    let mut values = headers.get_all(header::CONTENT_TYPE).iter();
    if values
        .next()
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
        && values.next().is_none()
    {
        Ok(())
    } else {
        Err(request_invalid())
    }
}

fn import_error(error: ImportError) -> HttpError {
    let status = if matches!(error.code, "csv_too_large" | "csv_too_many_rows") {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    let mut api = ApiError::new(error.code, "IMDb CSV input is invalid.");
    api.field = error.field.map(str::to_owned);
    HttpError::from_parts(status, api)
}

fn submission_invalid(error: ApiError) -> HttpError {
    HttpError::from_parts(StatusCode::BAD_REQUEST, error)
}

fn pre_admission_error(error: ApiError) -> HttpError {
    let (status, safe) = match error.code.as_str() {
        "preview_runtime_busy" => (
            StatusCode::TOO_MANY_REQUESTS,
            ApiError::new(
                "preview_runtime_busy",
                "The preview runtime admission queue is full.",
            ),
        ),
        "preview_runtime_duplicate" => (
            StatusCode::CONFLICT,
            ApiError::new(
                "preview_runtime_duplicate",
                "This preview job is already admitted locally.",
            ),
        ),
        "preview_runtime_job_invalid" => (
            StatusCode::CONFLICT,
            ApiError::new(
                "preview_runtime_job_invalid",
                "Preview job metadata is not eligible for runtime admission.",
            ),
        ),
        "preview_runtime_job_expired" => (
            StatusCode::CONFLICT,
            ApiError::new(
                "preview_runtime_job_expired",
                "The preview job has expired and cannot be admitted.",
            ),
        ),
        "preview_attempt_exhausted" => (
            StatusCode::CONFLICT,
            ApiError::new(
                "preview_attempt_exhausted",
                "The preview attempt generation is exhausted.",
            ),
        ),
        "preview_runtime_closed" => (
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                "preview_runtime_closed",
                "The preview runtime is shutting down.",
            ),
        ),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                "preview_unavailable",
                "Preview submission is temporarily unavailable.",
            ),
        ),
    };
    HttpError::from_parts(status, safe)
}

fn failure_status(error: &ApiError) -> StatusCode {
    match error.code.as_str() {
        "preview_binding_changed"
        | "preview_profile_changed"
        | "preview_transition_invalid"
        | "preview_attempt_exhausted" => StatusCode::CONFLICT,
        "preview_observation_invalid" | "preview_job_invalid" => StatusCode::BAD_REQUEST,
        "preview_observation_too_large" | "preview_submission_too_large" => {
            StatusCode::PAYLOAD_TOO_LARGE
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn body_error(error: ApiError) -> HttpError {
    if error.code == "request_too_large" {
        HttpError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request_invalid",
            "Request body exceeds the size limit.",
        )
    } else if error.code == "request_invalid" {
        request_invalid()
    } else {
        unavailable()
    }
}

fn request_invalid() -> HttpError {
    HttpError::new(
        StatusCode::BAD_REQUEST,
        "request_invalid",
        "The request is invalid.",
    )
}

fn unsupported_media_type() -> HttpError {
    HttpError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "request_invalid",
        "IMDb ratings imports require UTF-8 text/csv or application/json.",
    )
}

fn unavailable() -> HttpError {
    HttpError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "preview_unavailable",
        "Preview submission is temporarily unavailable.",
    )
}

fn not_connected() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "neodb_not_connected",
        "Connect a NeoDB account before importing ratings.",
    )
}

fn profile_not_found() -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "mapping_profile_not_found",
        "The selected mapping profile revision was not found.",
    )
}

fn profile_conflict() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "preview_profile_changed",
        "The selected mapping profile is not eligible for this import.",
    )
}

fn job_not_found() -> HttpError {
    HttpError::new(
        StatusCode::NOT_FOUND,
        "preview_job_not_found",
        "The preview job was not found or has expired.",
    )
}

fn attempt_changed() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "preview_attempt_changed",
        "The requested preview attempt is no longer current.",
    )
}

fn resume_conflict() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "preview_runtime_job_invalid",
        "Only an interrupted, failed, or cancelled preview attempt can be resumed.",
    )
}

fn attempt_exhausted() -> HttpError {
    HttpError::new(
        StatusCode::CONFLICT,
        "preview_attempt_exhausted",
        "The preview attempt generation is exhausted.",
    )
}
