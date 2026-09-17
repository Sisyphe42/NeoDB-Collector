//! Authenticated profile configuration, never activation or media execution.
use crate::{
    access::{self, AccessState, HttpError},
    mapping_profiles,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    routing::{get, post},
};
use collector_domain::{
    ApiError,
    mapping::{MappingDocument, MappingProfile, SaveMappingProfile},
};

pub(crate) fn routes() -> Router<AccessState> {
    Router::new()
        .route(
            "/api/v1/mapping-profiles/validate",
            post(validate)
                .get(load_named_validate)
                .put(save_named_validate),
        )
        .route("/api/v1/mapping-profiles/{id}", get(load).put(save))
        // A JSON string envelope can expand the 64 KiB document up to 6x.
        .layer(DefaultBodyLimit::max(512 * 1024))
}

// Preserve the resource contract for the valid ID "validate", despite the
// static POST action at that same path.
async fn load_named_validate(
    state: State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<MappingProfile>, HttpError> {
    load(state, headers, Path("validate".into())).await
}
async fn save_named_validate(
    state: State<AccessState>,
    headers: HeaderMap,
    body: Result<Json<SaveMappingProfile>, JsonRejection>,
) -> Result<Json<MappingProfile>, HttpError> {
    save(state, headers, Path("validate".into()), body).await
}

async fn authorize(
    state: &AccessState,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(), HttpError> {
    if headers.contains_key(header::AUTHORIZATION) {
        access::bearer(state, headers).await?;
        let scope = if mutation {
            "mapping.write"
        } else {
            "mapping.read"
        };
        if !collector_core::access::PAIRING_SCOPES.contains(&scope) {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "scope_required",
                "Mapping scope is required.",
            ));
        }
    } else {
        access::admin(state, headers, mutation).await?;
    }
    Ok(())
}

fn mapping_error(error: ApiError) -> HttpError {
    let status = match error.code.as_str() {
        "mapping_version_conflict" => StatusCode::CONFLICT,
        "mapping_document_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "database_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    HttpError::from_parts(status, error)
}

fn valid_id(id: &str) -> Result<(), HttpError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
    {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "mapping_identifier_invalid",
            "Invalid profile ID.",
        ));
    }
    Ok(())
}

#[utoipa::path(post, path="/api/v1/mapping-profiles/validate", request_body=MappingDocument,
    responses((status=200, body=MappingProfile), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=413, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn validate(
    State(state): State<AccessState>,
    headers: HeaderMap,
    body: Result<Json<MappingDocument>, JsonRejection>,
) -> Result<Json<MappingProfile>, HttpError> {
    authorize(&state, &headers, false).await?;
    let body = access::json(body)?;
    Ok(Json(
        mapping_profiles::decode(&body.document, body.format).map_err(mapping_error)?,
    ))
}

#[utoipa::path(get, path="/api/v1/mapping-profiles/{id}", params(("id" = String, Path)),
    responses((status=200, body=MappingProfile), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=404, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn load(
    State(state): State<AccessState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<MappingProfile>, HttpError> {
    authorize(&state, &headers, false).await?;
    valid_id(&id)?;
    let repository = state.mappings.as_ref().ok_or_else(unavailable)?;
    let profile = repository
        .load(id, None)
        .await
        .map_err(mapping_error)?
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::NOT_FOUND,
                "mapping_profile_not_found",
                "Mapping profile not found.",
            )
        })?;
    Ok(Json(profile))
}

#[utoipa::path(put, path="/api/v1/mapping-profiles/{id}", params(("id" = String, Path)), request_body=SaveMappingProfile,
    responses((status=200, body=MappingProfile), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=409, body=ApiError), (status=413, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn save(
    State(state): State<AccessState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Result<Json<SaveMappingProfile>, JsonRejection>,
) -> Result<Json<MappingProfile>, HttpError> {
    authorize(&state, &headers, true).await?;
    valid_id(&id)?;
    let body = access::json(body)?;
    let profile = mapping_profiles::decode(&body.document, body.format).map_err(mapping_error)?;
    if profile.id != id {
        return Err(HttpError::new(
            StatusCode::BAD_REQUEST,
            "mapping_identifier_mismatch",
            "Path and profile ID must agree.",
        ));
    }
    collector_core::mapping::validate_revision(&profile, body.expected_version)
        .map_err(mapping_error)?;
    state
        .mappings
        .as_ref()
        .ok_or_else(unavailable)?
        .save(profile.clone(), body.expected_version)
        .await
        .map_err(mapping_error)?;
    Ok(Json(profile))
}

fn unavailable() -> HttpError {
    HttpError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "mapping_unavailable",
        "Mapping storage is unavailable.",
    )
}
