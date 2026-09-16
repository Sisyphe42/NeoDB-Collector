use super::*;
use axum::body::{Body, to_bytes};
use collector_core::access::AccessService;
use collector_storage::{
    Storage,
    credentials::{SecureTokens, SystemClock},
};
use serde_json::{Value, json};
use tower::ServiceExt;

const HOST: &str = "127.0.0.1:19423";
const ORIGIN: &str = "http://127.0.0.1:19423";
async fn setup() -> (tempfile::TempDir, Storage, Arc<AccessService>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(SystemClock),
        audience: ORIGIN.into(),
    });
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        HOST.parse().unwrap(),
        Some(access::AccessState::new(service.clone())),
    );
    (dir, storage, service, app)
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    assert!(response.headers().contains_key("content-security-policy"));
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), 100_000).await.unwrap();
    (status, headers, serde_json::from_slice(&body).unwrap())
}
async fn login(app: &Router, service: &AccessService) -> (String, String) {
    let key = service.bootstrap().await.unwrap();
    let (status, headers, view) = request(
        app,
        "POST",
        "/api/v1/session/bootstrap",
        json!({"key": key}),
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(status, 200);
    let set_cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
    for attr in ["HttpOnly", "SameSite=Strict", "Path=/", "Max-Age=28800"] {
        assert!(set_cookie.contains(attr));
    }
    let replay = request(
        app,
        "POST",
        "/api/v1/session/bootstrap",
        json!({"key": key}),
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(replay.0, 401);
    (
        set_cookie.split(';').next().unwrap().into(),
        view["csrfToken"].as_str().unwrap().into(),
    )
}

#[tokio::test]
async fn private_routes_require_session_csrf_and_origin() {
    let (_dir, storage, service, app) = setup().await;
    assert_eq!(
        request(&app, "GET", "/api/v1/health", Value::Null, &[])
            .await
            .0,
        200
    );
    assert_eq!(
        request(&app, "GET", "/api/v1/clients", Value::Null, &[])
            .await
            .0,
        401
    );
    let (cookie, csrf) = login(&app, &service).await;
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/v1/session",
            Value::Null,
            &[("cookie", &cookie)]
        )
        .await
        .0,
        200
    );
    for headers in [
        vec![("cookie", cookie.as_str())],
        vec![("cookie", cookie.as_str()), ("origin", ORIGIN)],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", "http://attacker.test"),
            ("x-csrf-token", csrf.as_str()),
        ],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", ORIGIN),
            ("x-csrf-token", "wrong"),
        ],
    ] {
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/pairings",
                json!({"clientName":"CLI", "clientKind":"cli"}),
                &headers
            )
            .await
            .0,
            403
        );
    }
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", ORIGIN),
        ("x-csrf-token", csrf.as_str()),
    ];
    let (status, _, paired) = request(
        &app,
        "POST",
        "/api/v1/pairings",
        json!({"clientName":"CLI", "clientKind":"cli"}),
        &headers,
    )
    .await;
    assert_eq!(status, 200);
    let key = paired["uri"]
        .as_str()
        .unwrap()
        .split("#key=")
        .nth(1)
        .unwrap();
    let (status, _, token) = request(
        &app,
        "POST",
        "/api/v1/pairings/exchange",
        json!({"one_time_key":key, "client_name":"CLI", "client_kind":"cli"}),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    let bearer = format!("Bearer {}", token["token"].as_str().unwrap());
    for auth in [&bearer, "Bearer invalid"] {
        assert_eq!(
            request(
                &app,
                "GET",
                "/api/v1/clients",
                Value::Null,
                &[("authorization", auth), ("cookie", &cookie)]
            )
            .await
            .0,
            403
        );
    }
    let (_, _, clients) = request(
        &app,
        "GET",
        "/api/v1/clients",
        Value::Null,
        &[("cookie", &cookie)],
    )
    .await;
    assert!(
        !clients
            .to_string()
            .contains(token["token"].as_str().unwrap())
    );
    let id = token["client_id"].as_str().unwrap();
    assert_eq!(
        request(
            &app,
            "DELETE",
            &format!("/api/v1/clients/{id}"),
            Value::Null,
            &headers
        )
        .await
        .0,
        200
    );
    assert!(
        service
            .bearer(token["token"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        request(&app, "DELETE", "/api/v1/session", Value::Null, &headers)
            .await
            .0,
        200
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/v1/session",
            Value::Null,
            &[("cookie", &cookie)]
        )
        .await
        .0,
        401
    );
    storage.close().await;
}

#[tokio::test]
async fn access_rejects_untrusted_origins_oversize_unknown_fields_and_rate_excess() {
    let (_dir, storage, service, app) = setup().await;
    let key = service.bootstrap().await.unwrap();
    for headers in [
        vec![],
        vec![("origin", "null")],
        vec![("origin", "http://attacker.test")],
    ] {
        assert_eq!(
            request(
                &app,
                "POST",
                "/api/v1/session/bootstrap",
                json!({"key":key}),
                &headers
            )
            .await
            .0,
            403
        );
    }
    let large_secret = "sensitive".repeat(1000);
    let oversized = request(
        &app,
        "POST",
        "/api/v1/session/bootstrap",
        json!({"key":large_secret}),
        &[("origin", ORIGIN)],
    )
    .await;
    assert_eq!(oversized.0, 413);
    assert!(!oversized.2.to_string().contains("sensitive"));
    let unknown = request(
        &app,
        "POST",
        "/api/v1/session/bootstrap",
        json!({"key":key, "admin":true}),
        &[("origin", ORIGIN)],
    )
    .await;
    assert!(unknown.0.is_client_error());
    assert!(!unknown.2.to_string().contains(&key));
    let mut limited = false;
    for _ in 0..61 {
        let result = request(
            &app,
            "POST",
            "/api/v1/pairings/exchange",
            json!({"one_time_key":"x", "client_name":"CLI", "client_kind":"cli"}),
            &[],
        )
        .await;
        if result.0 == 429 {
            limited = true;
            break;
        }
    }
    assert!(limited);
    storage.close().await;
}
