//! Local access HTTP boundary. No secret is accepted in a path or query.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use collector_core::access::{AccessRecord, AccessService};
use collector_domain::{ApiError, access::*};
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;

const COOKIE: &str = "collector_session";

#[derive(Clone)]
pub struct AccessState {
    pub service: Arc<AccessService>,
    pub oauth: Option<Arc<collector_core::oauth::OAuthService>>,
    pub mappings: Option<Arc<dyn collector_core::mapping::MappingRepository>>,
    pub preview_jobs: Option<Arc<dyn collector_core::preview_jobs::PreviewJobRepository>>,
    pub preview_results: Option<Arc<dyn collector_core::preview_results::PreviewResultRepository>>,
    pub preview_management: Option<Arc<dyn crate::preview_runtime::PreviewManagement>>,
    pub preview_submission: Option<Arc<dyn crate::preview_runtime::PreviewSubmissionRuntime>>,
    limiter: Arc<Mutex<(i64, u32)>>,
}
impl AccessState {
    pub fn new(service: Arc<AccessService>) -> Self {
        Self {
            service,
            oauth: None,
            mappings: None,
            preview_jobs: None,
            preview_results: None,
            preview_management: None,
            preview_submission: None,
            limiter: Arc::new(Mutex::new((0, 0))),
        }
    }
    pub(crate) fn rate_limit(&self) -> Result<(), HttpError> {
        let now = self.service.clock.now();
        let mut window = self.limiter.lock().map_err(|_| HttpError::internal())?;
        if now < window.0 || now.saturating_sub(window.0) >= 60 {
            *window = (now, 0);
        }
        if window.1 >= 60 {
            return Err(HttpError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "auth_rate_limited",
                "Too many access requests; retry in one minute.",
            ));
        }
        window.1 += 1;
        Ok(())
    }
}

pub(crate) struct HttpError(StatusCode, ApiError);
impl HttpError {
    pub(crate) fn from_parts(status: StatusCode, error: ApiError) -> Self {
        Self(status, error)
    }
    pub(crate) fn new(status: StatusCode, code: &str, message: &str) -> Self {
        Self(status, ApiError::new(code, message))
    }
    fn internal() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "access_unavailable",
            "Local access is unavailable.",
        )
    }
}
impl From<ApiError> for HttpError {
    fn from(error: ApiError) -> Self {
        let status = match error.code.as_str() {
            "authentication_required" | "challenge_invalid" => StatusCode::UNAUTHORIZED,
            "pairing_scope_invalid" => StatusCode::BAD_REQUEST,
            "session_required" | "oauth_cancel_forbidden" => StatusCode::FORBIDDEN,
            "neodb_base_url_invalid" | "oauth_callback_invalid" => StatusCode::BAD_REQUEST,
            "oauth_in_progress" => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self(status, error)
    }
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}
pub(crate) fn json<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, HttpError> {
    body.map(|Json(value)| value).map_err(|error| {
        HttpError::new(
            if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                error.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            "request_invalid",
            "Expected a valid JSON request within the size limit.",
        )
    })
}
fn origin(headers: &HeaderMap) -> Result<(), HttpError> {
    let expected = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| format!("http://{h}"));
    if expected.is_none()
        || headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != expected.as_deref()
    {
        return Err(HttpError::new(
            StatusCode::FORBIDDEN,
            "origin_not_allowed",
            "A same-origin browser request is required.",
        ));
    }
    Ok(())
}
fn session_token(headers: &HeaderMap) -> Result<&str, HttpError> {
    let mut values = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|v| v.trim().split_once('='))
        .filter(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value);
    let token = values.next().ok_or_else(|| {
        HttpError::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Open the one-time WebUI link printed by Core.",
        )
    })?;
    if values.next().is_some() {
        return Err(HttpError::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Ambiguous session cookie.",
        ));
    }
    Ok(token)
}
pub(crate) async fn admin(
    state: &AccessState,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<AccessRecord, HttpError> {
    // An invalid Bearer header must never fall back to a privileged cookie.
    if headers.contains_key(header::AUTHORIZATION) {
        return Err(HttpError::new(
            StatusCode::FORBIDDEN,
            "session_required",
            "This operation requires a WebUI session.",
        ));
    }
    let token = session_token(headers)?;
    let record = state.service.session(token).await?;
    if mutation {
        origin(headers)?;
        let expected = state.service.csrf(token);
        let actual = headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let equal = actual.len() == expected.len()
            && bool::from(actual.as_bytes().ct_eq(expected.as_bytes()));
        if !equal {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "csrf_invalid",
                "The session CSRF token is required.",
            ));
        }
    }
    Ok(record)
}

#[utoipa::path(post, path="/api/v1/session/bootstrap", request_body=BootstrapExchange,
    responses((status=200, body=SessionView), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=413, body=ApiError), (status=429, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn bootstrap(
    State(state): State<AccessState>,
    headers: HeaderMap,
    body: Result<Json<BootstrapExchange>, JsonRejection>,
) -> Result<Response, HttpError> {
    origin(&headers)?;
    state.rate_limit()?;
    let (token, view) = state.service.open_session(&json(body)?.key).await?;
    let cookie = format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        collector_core::access::SESSION_TTL
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(view)).into_response())
}
#[utoipa::path(get, path="/api/v1/session", responses((status=200, body=SessionView), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn session(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<SessionView>, HttpError> {
    let record = admin(&state, &headers, false).await?;
    Ok(Json(SessionView {
        csrf_token: state.service.csrf(session_token(&headers)?),
        expires_at: record.expires_at,
    }))
}
#[utoipa::path(delete, path="/api/v1/session", responses((status=200, body=ActionResult), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn logout(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let record = admin(&state, &headers, true).await?;
    state
        .service
        .repository
        .revoke(record.id, "session".into())
        .await?;
    Ok((
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0"),
        )],
        Json(ActionResult { ok: true }),
    )
        .into_response())
}
#[utoipa::path(post, path="/api/v1/pairings", request_body=PairingRequest,
    responses((status=200, body=PairingLink), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=413, body=ApiError), (status=429, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn create_pairing(
    State(state): State<AccessState>,
    headers: HeaderMap,
    body: Result<Json<PairingRequest>, JsonRejection>,
) -> Result<Json<PairingLink>, HttpError> {
    admin(&state, &headers, true).await?;
    state.rate_limit()?;
    Ok(Json(state.service.create_pairing(json(body)?).await?))
}
#[utoipa::path(post, path="/api/v1/pairings/exchange", request_body=PairingExchange,
    responses((status=200, body=PairingToken), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=413, body=ApiError), (status=429, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn exchange_pairing(
    State(state): State<AccessState>,
    body: Result<Json<PairingExchange>, JsonRejection>,
) -> Result<Json<PairingToken>, HttpError> {
    state.rate_limit()?;
    Ok(Json(state.service.pair(json(body)?).await?))
}
#[utoipa::path(get, path="/api/v1/clients", responses((status=200, body=ClientList), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn clients(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<ClientList>, HttpError> {
    admin(&state, &headers, false).await?;
    Ok(Json(ClientList {
        clients: state.service.repository.clients().await?,
    }))
}
#[utoipa::path(delete, path="/api/v1/clients/{id}", params(("id"=String, Path, description="Opaque client ID")),
    responses((status=200, body=ActionResult), (status=400, body=ApiError), (status=401, body=ApiError), (status=403, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn revoke(
    State(state): State<AccessState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ActionResult>, HttpError> {
    admin(&state, &headers, true).await?;
    collector_core::access::validate_key(&id).map_err(|_| {
        HttpError::new(
            StatusCode::BAD_REQUEST,
            "client_id_invalid",
            "Invalid client ID.",
        )
    })?;
    state.service.repository.revoke(id, "client".into()).await?;
    Ok(Json(ActionResult { ok: true }))
}
pub(crate) fn routes(state: AccessState) -> Router {
    Router::new()
        .route("/api/v1/session/bootstrap", post(bootstrap))
        .route("/api/v1/session", get(session).delete(logout))
        .route("/api/v1/pairings", post(create_pairing))
        .route("/api/v1/pairings/exchange", post(exchange_pairing))
        .route("/api/v1/clients", get(clients))
        .route("/api/v1/client", get(client_identity))
        .route("/api/v1/clients/{id}", delete(revoke))
        .route("/api/v1/settings/neodb", get(crate::oauth::status))
        .route(
            "/api/v1/oauth/neodb",
            post(crate::oauth::start).delete(crate::oauth::cancel),
        )
        .route("/api/v1/oauth/neodb/callback", get(crate::oauth::callback))
        .layer(DefaultBodyLimit::max(4096))
        .merge(crate::mapping_http::routes())
        .merge(crate::preview_http::routes())
        .merge(crate::preview_submission_http::routes())
        .with_state(state)
}

#[utoipa::path(get, path="/api/v1/client",
    responses((status=200, body=PairedClient), (status=401, body=ApiError), (status=503, body=ApiError)))]
pub(crate) async fn client_identity(
    State(state): State<AccessState>,
    headers: HeaderMap,
) -> Result<Json<PairedClient>, HttpError> {
    let record = bearer(&state, &headers).await?;
    Ok(Json(PairedClient {
        id: record.id,
        name: record.name,
        kind: record.client_kind,
        revoked: false,
    }))
}

pub(crate) async fn bearer(
    state: &AccessState,
    headers: &HeaderMap,
) -> Result<AccessRecord, HttpError> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let token = values
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|_| values.next().is_none())
        .ok_or_else(|| {
            HttpError::new(
                StatusCode::UNAUTHORIZED,
                "authentication_required",
                "A paired client Bearer token is required.",
            )
        })?;
    Ok(state.service.bearer(token).await?)
}
