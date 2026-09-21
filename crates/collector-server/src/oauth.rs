use crate::access::{AccessState, HttpError, admin, json};
use axum::{
    Json,
    extract::{Query, RawQuery, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::Redirect,
};
use collector_domain::{ApiError, access::*};
use serde::Deserialize;
use std::sync::Arc;

fn service(state: &AccessState) -> Result<Arc<collector_core::oauth::OAuthService>, HttpError> {
    state.oauth.clone().ok_or_else(|| {
        HttpError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_unavailable",
            "NeoDB authorization is unavailable.",
        )
    })
}

#[utoipa::path(get, path="/api/v1/settings/neodb",
    responses((status=200, body=NeoDbStatus), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn status(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<NeoDbStatus>, HttpError> {
    admin(&state, &headers, false).await?;
    let view = service(&state)?.status().await?;
    Ok(Json(NeoDbStatus {
        connected: view.connected,
        base_url: view.base_url,
        account: view.account,
        pending: view.pending,
    }))
}

#[utoipa::path(post, path="/api/v1/oauth/neodb", request_body=NeoDbStartRequest,
    responses((status=200, body=NeoDbAuthorization), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=409, body=ApiError), (status=413, body=ApiError), (status=429, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn start(
    State(state): State<AccessState>,
    headers: HeaderMap,
    body: Result<Json<NeoDbStartRequest>, JsonRejection>,
) -> Result<Json<NeoDbAuthorization>, HttpError> {
    let session = admin(&state, &headers, true).await?;
    state.rate_limit()?;
    let callback = format!("{}/api/v1/oauth/neodb/callback", state.service.audience);
    let view = service(&state)?
        .start(&session, json(body)?.base_url, callback)
        .await?;
    Ok(Json(NeoDbAuthorization {
        authorization_url: view.authorization_url,
        expires_at: view.expires_at,
    }))
}

#[utoipa::path(delete, path="/api/v1/oauth/neodb",
    responses((status=200, body=ActionResult), (status=401, body=ApiError), (status=403, body=ApiError), (status=409, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn cancel(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<ActionResult>, HttpError> {
    let session = admin(&state, &headers, true).await?;
    service(&state)?.cancel(&session).await?;
    Ok(Json(ActionResult { ok: true }))
}

#[derive(Deserialize)]
struct Callback {
    state: String,
    code: Option<String>,
    error: Option<String>,
}

#[utoipa::path(get, path="/api/v1/oauth/neodb/callback",
    params(("state"=String, Query), ("code"=Option<String>, Query), ("error"=Option<String>, Query)),
    responses((status=303, description="Redirect to a clean local page; inspect authenticated status")))]
pub(crate) async fn callback(
    State(state): State<AccessState>,
    RawQuery(raw): RawQuery,
) -> Redirect {
    // Never expose query/parser/upstream errors or forward provider-controlled redirects.
    let result = async {
        state.rate_limit().map_err(|_| ())?;
        let raw = raw.filter(|q| q.len() <= 4096).ok_or(())?;
        let Query(query) =
            Query::<Callback>::try_from_uri(&format!("/?{raw}").parse().map_err(|_| ())?)
                .map_err(|_| ())?;
        if query.state.len() > 128 {
            return Err(());
        }
        let code = if query.error.is_some() {
            String::new()
        } else {
            query.code.unwrap_or_default()
        };
        service(&state)
            .map_err(|_| ())?
            .complete(&query.state, code.into())
            .await
            .map_err(|_| ())?;
        Ok::<(), ()>(())
    }
    .await;
    Redirect::to(if result.is_ok() {
        "/#oauth=complete"
    } else {
        "/#oauth=failed"
    })
}
