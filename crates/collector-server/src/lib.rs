use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use collector_core::StorageHealth;
use collector_domain::{
    ApiError, CapabilityDocument, HealthDocument, HealthStatus, MEDIA_TYPES, SCHEMA_VERSION,
    access::*,
    mapping::{MappingDocument, MappingProfile, SaveMappingProfile},
};
use std::{collections::BTreeMap, future::Future, net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;
use utoipa::OpenApi;

pub mod access;
#[cfg(test)]
mod access_tests;
mod mapping_http;
#[cfg(test)]
mod mapping_http_tests;
pub mod mapping_profiles;
mod oauth;
#[cfg(test)]
mod oauth_tests;

pub const HEALTH_ROUTE: &str = "/api/v1/health";
pub const CAPABILITIES_ROUTE: &str = "/api/v1/capabilities";
pub const SCHEMA_ROUTE: &str = "/api/v1/schema";

#[derive(Clone)]
struct AppState {
    storage: Arc<dyn StorageHealth>,
    assets: PathBuf,
}

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(ApiError::new(code, message))).into_response()
}

#[utoipa::path(get, path = "/api/v1/health", responses(
    (status = 200, body = HealthDocument), (status = 503, body = HealthDocument), (status = 403, body = ApiError)
))]
async fn health(State(state): State<AppState>) -> Response {
    let value = collector_core::health(state.storage.as_ref(), env!("CARGO_PKG_VERSION")).await;
    let status = if value.status == HealthStatus::Ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(value)).into_response()
}

#[utoipa::path(get, path = "/api/v1/capabilities", responses((status = 200, body = CapabilityDocument), (status = 403, body = ApiError)))]
async fn capabilities() -> Json<CapabilityDocument> {
    Json(CapabilityDocument {
        schema_version: SCHEMA_VERSION,
        connector_ids: Vec::new(),
        media_types: MEDIA_TYPES.iter().map(|s| (*s).into()).collect(),
        routes: BTreeMap::from([
            ("health".into(), HEALTH_ROUTE.into()),
            ("capabilities".into(), CAPABILITIES_ROUTE.into()),
            ("schema".into(), SCHEMA_ROUTE.into()),
            ("session".into(), "/api/v1/session".into()),
            (
                "sessionBootstrap".into(),
                "/api/v1/session/bootstrap".into(),
            ),
            ("pairings".into(), "/api/v1/pairings".into()),
            ("pairingExchange".into(), "/api/v1/pairings/exchange".into()),
            ("clients".into(), "/api/v1/clients".into()),
            ("clientIdentity".into(), "/api/v1/client".into()),
            ("neodbSettings".into(), "/api/v1/settings/neodb".into()),
            ("neodbOAuth".into(), "/api/v1/oauth/neodb".into()),
            ("mappingProfiles".into(), "/api/v1/mapping-profiles".into()),
        ]),
    })
}

#[utoipa::path(get, path = "/api/v1/schema", responses((status = 200, body = Object), (status = 403, body = ApiError)))]
async fn schema() -> Json<utoipa::openapi::OpenApi> {
    Json(openapi())
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        capabilities,
        schema,
        access::bootstrap,
        access::session,
        access::logout,
        access::create_pairing,
        access::exchange_pairing,
        access::clients,
        access::revoke,
        access::client_identity,
        oauth::status,
        oauth::start,
        oauth::cancel,
        oauth::callback,
        mapping_http::validate,
        mapping_http::load,
        mapping_http::save
    ),
    components(schemas(
        ApiError,
        HealthDocument,
        CapabilityDocument,
        BootstrapExchange,
        SessionView,
        PairingRequest,
        PairingLink,
        PairingExchange,
        PairingToken,
        ClientList,
        PairedClient,
        ActionResult,
        NeoDbStartRequest,
        NeoDbAuthorization,
        NeoDbStatus,
        MappingDocument,
        MappingProfile,
        SaveMappingProfile
    ))
)]
struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.info.title = "NeoDB Collector Core".into();
    doc.info.version = env!("CARGO_PKG_VERSION").into();
    doc.info.license = None;
    doc
}

pub fn contract_documents() -> serde_json::Value {
    serde_json::json!({
        "apiVersion": collector_domain::API_VERSION,
        "schemaVersion": SCHEMA_VERSION,
        "openapi": openapi(),
        "schemas": {
            "HealthDocument": schemars::schema_for!(HealthDocument),
            "CapabilityDocument": schemars::schema_for!(CapabilityDocument),
            "ApiError": schemars::schema_for!(ApiError),
            "BootstrapExchange": schemars::schema_for!(BootstrapExchange),
            "SessionView": schemars::schema_for!(SessionView),
            "PairingRequest": schemars::schema_for!(PairingRequest),
            "PairingLink": schemars::schema_for!(PairingLink),
            "PairingExchange": schemars::schema_for!(PairingExchange),
            "PairingToken": schemars::schema_for!(PairingToken),
            "ClientList": schemars::schema_for!(ClientList),
            "PairedClient": schemars::schema_for!(PairedClient),
            "ActionResult": schemars::schema_for!(ActionResult),
            "NeoDbStartRequest": schemars::schema_for!(NeoDbStartRequest),
            "NeoDbAuthorization": schemars::schema_for!(NeoDbAuthorization),
            "NeoDbStatus": schemars::schema_for!(NeoDbStatus),
            "MappingDocument": schemars::schema_for!(MappingDocument),
            "MappingProfile": schemars::schema_for!(MappingProfile),
            "SaveMappingProfile": schemars::schema_for!(SaveMappingProfile)
        }
    })
}

async fn index(State(state): State<AppState>) -> Response {
    match tokio::fs::read(state.assets.join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "webui_assets_missing",
            "Build the WebUI with pnpm build, or configure webui.assets_dir. Core API remains available.",
        ),
    }
}

async fn not_found() -> Response {
    error(
        StatusCode::NOT_FOUND,
        "route_not_found",
        "This route is not available in this Core version.",
    )
}
async fn method_not_allowed() -> Response {
    error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "This method is not available for this route.",
    )
}

async fn local_boundary(
    State(address): State<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok());
    let allowed = [address.to_string(), format!("localhost:{}", address.port())];
    if !host.is_some_and(|h| allowed.iter().any(|a| a == h)) {
        return error(
            StatusCode::FORBIDDEN,
            "host_not_allowed",
            "Use the configured local service address.",
        );
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let expected = format!("http://{}", host.unwrap_or_default());
        if origin.to_str().ok() != Some(expected.as_str()) {
            return error(
                StatusCode::FORBIDDEN,
                "origin_not_allowed",
                "Only same-origin browser requests are accepted.",
            );
        }
    }
    next.run(request).await
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    response
}

pub fn router(storage: Arc<dyn StorageHealth>, assets: PathBuf, address: SocketAddr) -> Router {
    protected_router(storage, assets, address, None)
}

pub fn protected_router(
    storage: Arc<dyn StorageHealth>,
    assets: PathBuf,
    address: SocketAddr,
    access: Option<access::AccessState>,
) -> Router {
    Router::new()
        .route(HEALTH_ROUTE, get(health))
        .route(CAPABILITIES_ROUTE, get(capabilities))
        .route(SCHEMA_ROUTE, get(schema))
        .route("/", get(index))
        .nest_service("/assets", ServeDir::new(assets.join("assets")))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(AppState { storage, assets })
        .merge(access.map(access::routes).unwrap_or_default())
        .layer(middleware::from_fn_with_state(address, local_boundary))
        .layer(middleware::from_fn(security_headers))
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    storage: Arc<dyn StorageHealth>,
    assets: PathBuf,
    access: access::AccessState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app = protected_router(storage, assets, listener.local_addr()?, Some(access));
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use tower::ServiceExt;

    #[tokio::test]
    async fn real_routes_assets_boundaries_and_degraded_storage() {
        let dir = tempfile::tempdir().unwrap();
        let storage = collector_storage::Storage::open(dir.path()).await.unwrap();
        let address = "127.0.0.1:19423".parse().unwrap();
        let app = router(
            Arc::new(storage.clone()),
            dir.path().join("missing"),
            address,
        );
        for (path, method, host, origin, expected) in [
            (HEALTH_ROUTE, "GET", "127.0.0.1:19423", None, 200),
            (CAPABILITIES_ROUTE, "GET", "localhost:19423", None, 200),
            (SCHEMA_ROUTE, "GET", "127.0.0.1:19423", None, 200),
            ("/", "GET", "127.0.0.1:19423", None, 503),
            ("/api/v1/imports", "GET", "127.0.0.1:19423", None, 404),
            (HEALTH_ROUTE, "POST", "127.0.0.1:19423", None, 405),
            (HEALTH_ROUTE, "GET", "attacker.test:19423", None, 403),
            (
                HEALTH_ROUTE,
                "GET",
                "127.0.0.1:19423",
                Some("http://127.0.0.1:19423"),
                200,
            ),
            (HEALTH_ROUTE, "GET", "127.0.0.1:19423", Some("null"), 403),
            (
                HEALTH_ROUTE,
                "GET",
                "127.0.0.1:19423",
                Some("https://attacker.test"),
                403,
            ),
        ] {
            let mut req = Request::builder()
                .uri(path)
                .method(method)
                .header(header::HOST, host);
            if let Some(origin) = origin {
                req = req.header(header::ORIGIN, origin);
            }
            let response = app
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected, "{method} {path}");
            assert!(
                response
                    .headers()
                    .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                    .is_none()
            );
            let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if path == CAPABILITIES_ROUTE {
                assert_eq!(value["connectorIds"], serde_json::json!([]));
            }
            if path == "/" {
                assert_eq!(value["code"], "webui_assets_missing");
            }
        }
        storage.close().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri(HEALTH_ROUTE)
                    .header(header::HOST, "127.0.0.1:19423")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn shutdown_releases_listener() {
        let dir = tempfile::tempdir().unwrap();
        let storage = collector_storage::Storage::open(dir.path()).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let access = access::AccessState::new(Arc::new(collector_core::access::AccessService {
            repository: Arc::new(storage.clone()),
            tokens: Arc::new(collector_storage::credentials::SecureTokens),
            clock: Arc::new(collector_storage::credentials::SystemClock),
            audience: format!("http://{address}"),
        }));
        serve(
            listener,
            Arc::new(storage.clone()),
            dir.path().into(),
            access,
            async {},
        )
        .await
        .unwrap();
        assert!(tokio::net::TcpListener::bind(address).await.is_ok());
        storage.close().await;
    }
}
