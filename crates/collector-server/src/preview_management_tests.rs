use super::*;
use axum::{
    body::{Body, to_bytes},
    extract::Request,
};
use collector_core::access::{AccessRepository, AccessService, Clock, PortFuture};
use collector_domain::{
    ApiError,
    access::{PairingExchange, PairingRequest},
};
use collector_storage::{Storage, credentials::SecureTokens};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;
use tower::ServiceExt;

const ORIGIN: &str = "http://127.0.0.1:19423";

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy)]
enum Behavior {
    Ready(bool),
    Failed,
    Pending,
    Delayed(Duration, bool),
}

struct Management {
    cancel: Behavior,
    delete: Behavior,
    calls: Mutex<Vec<(String, String, Option<u32>)>>,
    entered: Notify,
}

impl Management {
    fn new(cancel: Behavior, delete: Behavior) -> Self {
        Self {
            cancel,
            delete,
            calls: Mutex::new(Vec::new()),
            entered: Notify::new(),
        }
    }

    async fn run(&self, behavior: Behavior) -> Result<bool, ApiError> {
        self.entered.notify_waiters();
        match behavior {
            Behavior::Ready(applied) => Ok(applied),
            Behavior::Failed => Err(ApiError::new(
                "database_diagnostic_must_not_escape",
                "private database path",
            )),
            Behavior::Pending => std::future::pending().await,
            Behavior::Delayed(delay, applied) => {
                tokio::time::sleep(delay).await;
                Ok(applied)
            }
        }
    }
}

impl preview_runtime::PreviewManagement for Management {
    fn cancel<'a>(&'a self, id: &'a str, expected_attempt: u32) -> PortFuture<'a, bool> {
        self.calls
            .lock()
            .unwrap()
            .push(("cancel".into(), id.into(), Some(expected_attempt)));
        Box::pin(self.run(self.cancel))
    }

    fn delete<'a>(&'a self, id: &'a str) -> PortFuture<'a, bool> {
        self.calls
            .lock()
            .unwrap()
            .push(("delete".into(), id.into(), None));
        Box::pin(self.run(self.delete))
    }
}

async fn raw_request(
    app: &Router,
    method: &str,
    path: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:19423")
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 100_000).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn setup(
    management: Option<Arc<dyn preview_runtime::PreviewManagement>>,
) -> (tempfile::TempDir, Storage, Arc<AccessService>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(100)),
        audience: ORIGIN.into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_management = management;
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    (dir, storage, service, app)
}

fn app_for_storage(
    storage: &Storage,
    management: Arc<dyn preview_runtime::PreviewManagement>,
) -> (Arc<AccessService>, Router) {
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_management = Some(management);
    let app = protected_router(
        Arc::new(storage.clone()),
        std::path::PathBuf::from("missing-assets"),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    (service, app)
}

async fn session_mutation(
    app: &Router,
    method: &str,
    path: &str,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> (StatusCode, serde_json::Value) {
    let response = access_tests::request(
        app,
        method,
        path,
        body,
        &[
            ("cookie", cookie),
            ("origin", ORIGIN),
            ("x-csrf-token", csrf),
        ],
    )
    .await;
    (response.0, response.2)
}

#[tokio::test]
async fn router_drives_single_real_runtime_and_preserves_guarded_state() {
    use collector_core::{
        mapping::MappingRepository, preview_jobs::PreviewJobRepository,
        preview_results::PreviewResultRepository,
    };

    let fixture = preview_runtime::tests::http_fixture(3, 2).await;
    let (service, app) = app_for_storage(&fixture.storage, fixture.management());
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let link = service
        .create_pairing(PairingRequest {
            client_name: "retained client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    service
        .pair(PairingExchange {
            one_time_key: link.uri.split("#key=").nth(1).unwrap().into(),
            client_name: "retained client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();

    let first_id = fixture.job_id(0).to_owned();
    let first = fixture.enqueue(0);
    fixture.wait_for_calls(1).await;
    let wrong = session_mutation(
        &app,
        "POST",
        &format!("/api/v1/previews/{first_id}/cancel"),
        json!({"expectedAttempt": 2}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(wrong, (StatusCode::OK, json!({"applied": false})));
    assert_eq!(fixture.active_calls(), 1);

    fixture.release();
    fixture.wait_for_calls(2).await;
    let cancelled = session_mutation(
        &app,
        "POST",
        &format!("/api/v1/previews/{first_id}/cancel"),
        json!({"expectedAttempt": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(cancelled, (StatusCode::OK, json!({"applied": true})));
    assert_eq!(
        first.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    let partial = PreviewResultRepository::results(&fixture.storage, first_id, 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(partial.len(), 1, "cancel retains already committed history");

    let active_id = fixture.job_id(1).to_owned();
    let queued_id = fixture.job_id(2).to_owned();
    let active = fixture.enqueue(1);
    fixture.wait_for_calls(3).await;
    let queued = fixture.enqueue(2);
    let queued_cancel = session_mutation(
        &app,
        "POST",
        &format!("/api/v1/previews/{queued_id}/cancel"),
        json!({"expectedAttempt": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(queued_cancel, (StatusCode::OK, json!({"applied": true})));
    assert_eq!(
        queued.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );

    let deleted = session_mutation(
        &app,
        "DELETE",
        &format!("/api/v1/previews/{active_id}"),
        json!({}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(deleted, (StatusCode::OK, json!({"applied": true})));
    assert_eq!(
        active.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert!(
        PreviewJobRepository::load(&fixture.storage, active_id)
            .await
            .unwrap()
            .is_none(),
        "late worker completion cannot recreate a deleted job"
    );
    assert!(
        AccessRepository::endpoint(&fixture.storage)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        AccessRepository::clients(&fixture.storage)
            .await
            .unwrap()
            .len(),
        1
    );
    fixture.shutdown().await;
    fixture.storage.close().await;
}

#[tokio::test]
async fn session_mutations_require_origin_and_csrf_and_return_truthful_results() {
    let management = Arc::new(Management::new(
        Behavior::Ready(true),
        Behavior::Ready(false),
    ));
    let (_dir, storage, service, app) = setup(Some(management.clone())).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;

    for headers in [
        vec![("cookie", cookie.as_str())],
        vec![("cookie", cookie.as_str()), ("origin", ORIGIN)],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", "http://attacker.test"),
            ("x-csrf-token", csrf.as_str()),
        ],
    ] {
        let response = access_tests::request(
            &app,
            "POST",
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 1}),
            &headers,
        )
        .await;
        assert_eq!(response.0, StatusCode::FORBIDDEN);
    }

    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", ORIGIN),
        ("x-csrf-token", csrf.as_str()),
    ];
    let cancelled = access_tests::request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 7}),
        &headers,
    )
    .await;
    assert_eq!(cancelled.0, StatusCode::OK);
    assert_eq!(cancelled.2, json!({"applied": true}));

    let deleted = access_tests::request(
        &app,
        "DELETE",
        "/api/v1/previews/job:test",
        json!({}),
        &headers,
    )
    .await;
    assert_eq!(deleted.0, StatusCode::OK);
    assert_eq!(deleted.2, json!({"applied": false}));
    assert_eq!(
        *management.calls.lock().unwrap(),
        vec![
            ("cancel".into(), "job:test".into(), Some(7)),
            ("delete".into(), "job:test".into(), None),
        ]
    );
    storage.close().await;
}

#[tokio::test]
async fn bearer_is_exclusive_and_revocation_and_duplicates_fail_closed() {
    let management = Arc::new(Management::new(
        Behavior::Ready(true),
        Behavior::Ready(true),
    ));
    let (_dir, storage, service, app) = setup(Some(management)).await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    let link = service
        .create_pairing(PairingRequest {
            client_name: "management test".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let paired = service
        .pair(PairingExchange {
            one_time_key: link.uri.split("#key=").nth(1).unwrap().into(),
            client_name: "management test".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let authorization = format!("Bearer {}", paired.token);
    let ok = access_tests::request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 1}),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(ok.0, StatusCode::OK);

    let duplicate = access_tests::request(
        &app,
        "DELETE",
        "/api/v1/previews/job:test",
        json!({}),
        &[
            ("authorization", &authorization),
            ("authorization", &authorization),
            ("cookie", &cookie),
        ],
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::UNAUTHORIZED);

    AccessRepository::revoke(&storage, paired.client_id, "client".into())
        .await
        .unwrap();
    let revoked = access_tests::request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 1}),
        &[("authorization", &authorization), ("cookie", &cookie)],
    )
    .await;
    assert_eq!(revoked.0, StatusCode::UNAUTHORIZED);
    storage.close().await;
}

#[tokio::test]
async fn strict_inputs_body_cap_and_optional_composition_fail_closed() {
    let management = Arc::new(Management::new(
        Behavior::Ready(true),
        Behavior::Ready(true),
    ));
    let (_dir, storage, service, app) = setup(Some(management.clone())).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", ORIGIN),
        ("x-csrf-token", csrf.as_str()),
    ];
    for (path, body) in [
        ("/api/v1/previews/job:test/cancel", json!({})),
        (
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 0}),
        ),
        (
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 1.5}),
        ),
        (
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": -1}),
        ),
        (
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 4294967296_u64}),
        ),
        (
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 1, "extra": true}),
        ),
        (
            "/api/v1/previews/bad%2Fid/cancel",
            json!({"expectedAttempt": 1}),
        ),
    ] {
        let response = access_tests::request(&app, "POST", path, body, &headers).await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST, "{path}");
        assert!(
            response.2["code"] == "preview_request_invalid"
                || response.2["code"] == "request_invalid"
        );
    }
    assert!(management.calls.lock().unwrap().is_empty());

    let duplicate = raw_request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        r#"{"expectedAttempt":1,"expectedAttempt":2}"#.into(),
        &headers,
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::BAD_REQUEST);
    assert_eq!(duplicate.1["code"], "request_invalid");

    let base = r#"{"expectedAttempt":1}"#;
    let exact_body = format!("{base}{}", " ".repeat(4096 - base.len()));
    assert_eq!(exact_body.len(), 4096);
    let exact = raw_request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        exact_body.clone(),
        &headers,
    )
    .await;
    assert_eq!(exact.0, StatusCode::OK);
    assert_eq!(exact.1, json!({"applied": true}));

    let plus_one = raw_request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        format!("{exact_body} "),
        &headers,
    )
    .await;
    assert_eq!(plus_one.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(plus_one.1["code"], "request_invalid");

    let oversized = access_tests::request(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 1, "padding": "x".repeat(5000)}),
        &headers,
    )
    .await;
    assert_eq!(oversized.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(oversized.2["code"], "request_invalid");

    let (_dir2, storage2, service2, app2) = setup(None).await;
    let (cookie2, csrf2) = access_tests::login(&app2, &service2).await;
    let absent = access_tests::request(
        &app2,
        "DELETE",
        "/api/v1/previews/job:test",
        json!({}),
        &[
            ("cookie", &cookie2),
            ("origin", ORIGIN),
            ("x-csrf-token", &csrf2),
        ],
    )
    .await;
    assert_eq!(absent.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(absent.2["code"], "preview_unavailable");
    storage.close().await;
    storage2.close().await;
}

#[tokio::test]
async fn errors_hung_and_exact_deadline_replies_are_static() {
    let management = Arc::new(Management::new(Behavior::Pending, Behavior::Failed));
    let (_dir, storage, service, app) = setup(Some(management.clone())).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", ORIGIN),
        ("x-csrf-token", csrf.as_str()),
    ];
    let failed = access_tests::request(
        &app,
        "DELETE",
        "/api/v1/previews/job:test",
        json!({}),
        &headers,
    )
    .await;
    assert_eq!(failed.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failed.2["code"], "preview_delete_failed");
    assert!(!failed.2.to_string().contains("database"));

    tokio::time::pause();
    let cancel_app = app.clone();
    let cancel_cookie = cookie.clone();
    let cancel_csrf = csrf.clone();
    let entered = management.entered.notified();
    let pending = tokio::spawn(async move {
        access_tests::request(
            &cancel_app,
            "POST",
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 1}),
            &[
                ("cookie", &cancel_cookie),
                ("origin", ORIGIN),
                ("x-csrf-token", &cancel_csrf),
            ],
        )
        .await
    });
    entered.await;
    let started = tokio::time::Instant::now();
    tokio::time::advance(Duration::from_secs(10)).await;
    let response = pending.await.unwrap();
    let elapsed = tokio::time::Instant::now() - started;
    assert!(elapsed >= Duration::from_secs(10));
    assert!(elapsed <= Duration::from_millis(10_001));
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_cancel_failed");
    tokio::time::resume();
    storage.close().await;

    let exact = Arc::new(Management::new(
        Behavior::Delayed(Duration::from_secs(10), true),
        Behavior::Ready(false),
    ));
    let (_dir, storage, service, app) = setup(Some(exact.clone())).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    tokio::time::pause();
    let entered = exact.entered.notified();
    let task = tokio::spawn(async move {
        access_tests::request(
            &app,
            "POST",
            "/api/v1/previews/job:test/cancel",
            json!({"expectedAttempt": 1}),
            &[
                ("cookie", &cookie),
                ("origin", ORIGIN),
                ("x-csrf-token", &csrf),
            ],
        )
        .await
    });
    entered.await;
    let started = tokio::time::Instant::now();
    tokio::time::advance(Duration::from_secs(10)).await;
    let response = task.await.unwrap();
    let elapsed = tokio::time::Instant::now() - started;
    assert!(elapsed >= Duration::from_secs(10));
    assert!(elapsed <= Duration::from_millis(10_001));
    tokio::time::resume();
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_cancel_failed");
    storage.close().await;
}

#[tokio::test]
async fn delete_session_guards_and_cancel_composition_fail_before_mutation() {
    let management = Arc::new(Management::new(
        Behavior::Ready(true),
        Behavior::Ready(true),
    ));
    let (_dir, storage, service, app) = setup(Some(management.clone())).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let (_other_cookie, other_csrf) = access_tests::login(&app, &service).await;
    for headers in [
        vec![("cookie", cookie.as_str()), ("x-csrf-token", csrf.as_str())],
        vec![("cookie", cookie.as_str()), ("origin", ORIGIN)],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", ORIGIN),
            ("x-csrf-token", "wrong-csrf"),
        ],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", ORIGIN),
            ("x-csrf-token", other_csrf.as_str()),
        ],
        vec![
            ("cookie", cookie.as_str()),
            ("origin", "http://attacker.test"),
            ("x-csrf-token", csrf.as_str()),
        ],
    ] {
        let response = access_tests::request(
            &app,
            "DELETE",
            "/api/v1/previews/job:test",
            json!({}),
            &headers,
        )
        .await;
        assert_eq!(response.0, StatusCode::FORBIDDEN);
    }
    assert!(management.calls.lock().unwrap().is_empty());
    storage.close().await;

    let (_dir, storage, service, app) = setup(None).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let absent = session_mutation(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(
        absent,
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "code": "preview_unavailable",
                "message": "Preview inspection is temporarily unavailable.",
                "retryable": false
            })
        )
    );
    storage.close().await;

    let failed_management = Arc::new(Management::new(Behavior::Failed, Behavior::Ready(false)));
    let (_dir, storage, service, app) = setup(Some(failed_management)).await;
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let failed = session_mutation(
        &app,
        "POST",
        "/api/v1/previews/job:test/cancel",
        json!({"expectedAttempt": 1}),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(failed.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failed.1["code"], "preview_cancel_failed");
    assert!(!failed.1.to_string().contains("database"));
    storage.close().await;
}

#[tokio::test]
async fn delete_hung_and_exact_deadline_replies_are_static() {
    for behavior in [
        Behavior::Pending,
        Behavior::Delayed(Duration::from_secs(10), true),
    ] {
        let management = Arc::new(Management::new(Behavior::Ready(false), behavior));
        let (_dir, storage, service, app) = setup(Some(management.clone())).await;
        let (cookie, csrf) = access_tests::login(&app, &service).await;
        tokio::time::pause();
        let entered = management.entered.notified();
        let task = tokio::spawn(async move {
            access_tests::request(
                &app,
                "DELETE",
                "/api/v1/previews/job:test",
                json!({}),
                &[
                    ("cookie", &cookie),
                    ("origin", ORIGIN),
                    ("x-csrf-token", &csrf),
                ],
            )
            .await
        });
        entered.await;
        let started = tokio::time::Instant::now();
        tokio::time::advance(Duration::from_secs(10)).await;
        let response = task.await.unwrap();
        let elapsed = tokio::time::Instant::now() - started;
        tokio::time::resume();
        assert!(elapsed >= Duration::from_secs(10));
        assert!(elapsed <= Duration::from_millis(10_001));
        assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.2["code"], "preview_delete_failed");
        assert!(response.2.get("applied").is_none());
        storage.close().await;
    }
}
