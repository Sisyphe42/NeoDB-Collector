use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{Method, StatusCode},
    response::Response,
    routing::any,
};
use collector_core::access::PortFuture;
use collector_domain::preview::{PreviewAdmissionFailure, PreviewMutationResult};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
struct Seen {
    method: Method,
    target: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

#[derive(Clone, Copy)]
enum Mode {
    Normal,
    Admission,
    ResumeAdmission,
    ImportAttemptTwo,
    UnsafeStatus,
    UnsafePage,
    Delay,
    Unauthorized,
    ExactStatus,
    Oversized,
    ExactResults,
    OversizedResults,
    Redirect,
    WrongResume,
    AppliedTrue,
    CsvError,
    WrongImportStatus,
    WrongReadStatus,
    WrongMutationStatus,
    Incompatible,
}

struct ServerState {
    mode: Mode,
    seen: Mutex<Vec<Seen>>,
}

#[derive(Default)]
struct Secrets {
    gets: AtomicUsize,
}

impl SecretStore for Secrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn get(&self, _: String) -> PortFuture<'_, SecretString> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(SecretString::from("a".repeat(43))) })
    }
    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn json(status: StatusCode, value: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

async fn handler(State(state): State<Arc<ServerState>>, request: Request) -> Response {
    let method = request.method().clone();
    let target = request.uri().to_string();
    if request.uri().path() == "/api/v1/health" {
        return json(
            StatusCode::OK,
            serde_json::json!({
                "status":"ok", "coreVersion":"0.1.0", "apiVersion":if matches!(state.mode, Mode::Incompatible) { "v999" } else { API_VERSION },
                "schemaVersion":SCHEMA_VERSION, "storage":"ready"
            }),
        );
    }
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let body = to_bytes(request.into_body(), 11 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    state.seen.lock().unwrap().push(Seen {
        method: method.clone(),
        target: target.clone(),
        authorization,
        content_type,
        body,
    });
    match state.mode {
        Mode::Delay => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            json(StatusCode::OK, serde_json::json!({"applied":true}))
        }
        Mode::Unauthorized => json(
            StatusCode::UNAUTHORIZED,
            serde_json::json!({"code":"authentication_required","message":"secret upstream body","retryable":false}),
        ),
        Mode::ExactStatus => padded(StatusCode::OK, valid_status(), 64 * 1024),
        Mode::Oversized => padded(StatusCode::OK, valid_status(), 64 * 1024 + 1),
        Mode::ExactResults => padded(StatusCode::OK, valid_page(), 8 * 1024 * 1024),
        Mode::OversizedResults => padded(StatusCode::OK, valid_page(), 8 * 1024 * 1024 + 1),
        Mode::Redirect => Response::builder()
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header("location", "http://127.0.0.1:1/private")
            .body(Body::empty())
            .unwrap(),
        Mode::WrongResume => json(
            StatusCode::ACCEPTED,
            serde_json::json!({"jobId":"job:other","attempt":4,"executable":false}),
        ),
        Mode::AppliedTrue => json(StatusCode::OK, serde_json::json!({"applied":true})),
        Mode::CsvError => json(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"code":"csv_column_missing","message":"do not echo","field":"Your Rating","retryable":true}),
        ),
        Mode::WrongImportStatus => json(
            StatusCode::OK,
            serde_json::json!({
                "preview":{"jobId":"job:new","attempt":1,"executable":false},
                "observationCount":0,"skippedRowCount":0,"skippedColumnCount":0,"duplicateRowCount":0
            }),
        ),
        Mode::WrongReadStatus => json(StatusCode::ACCEPTED, valid_status()),
        Mode::WrongMutationStatus => {
            json(StatusCode::ACCEPTED, serde_json::json!({"applied":true}))
        }
        Mode::Admission => json(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "jobId":"job:new", "attempt":1,
                "error":{"code":"preview_runtime_busy","message":"token=SECRET\u{001b}[31m","field":"credentialRef","retryable":true}
            }),
        ),
        Mode::ResumeAdmission => json(
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "jobId":"job:1", "attempt":3,
                "error":{"code":"preview_resume_failed","message":"private","retryable":false}
            }),
        ),
        Mode::ImportAttemptTwo => json(
            StatusCode::ACCEPTED,
            serde_json::json!({
                "preview":{"jobId":"job:new","attempt":2,"executable":false},
                "observationCount":0,"skippedRowCount":0,"skippedColumnCount":0,"duplicateRowCount":0
            }),
        ),
        Mode::UnsafeStatus => json(
            StatusCode::OK,
            serde_json::json!({
                "id":"job:other", "status":"running", "attempt":0, "createdAt":-1, "updatedAt":0,
                "expiresAt":10, "observationCount":10001, "profileId":"profile:1", "profileVersion":1,
                "baseUrl":"https://neodb.example", "account":"alice", "bindingState":"current",
                "historical":false, "executable":true
            }),
        ),
        Mode::UnsafePage => json(StatusCode::OK, unsafe_page()),
        Mode::Normal | Mode::Incompatible => normal_response(&method, &target),
    }
}

fn padded(status: StatusCode, value: serde_json::Value, size: usize) -> Response {
    let mut text = value.to_string();
    assert!(text.len() <= size);
    text.push_str(&" ".repeat(size - text.len()));
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(text))
        .unwrap()
}

fn valid_status() -> serde_json::Value {
    serde_json::json!({"id":"job:1","status":"running","attempt":2,"createdAt":1,"updatedAt":2,"expiresAt":3,"observationCount":1,"profileId":"profile:1","profileVersion":4,"baseUrl":"https://neodb.example","account":"alice","bindingState":"current","historical":true,"executable":false})
}

fn valid_page() -> serde_json::Value {
    serde_json::json!({"jobId":"job:1","attempt":2,"offset":7,"limit":9,"historical":true,"executable":false,"items":[]})
}

fn normal_response(method: &Method, target: &str) -> Response {
    if target.starts_with("/api/v1/imports/imdb/ratings?") {
        return json(
            StatusCode::ACCEPTED,
            serde_json::json!({
                "preview":{"jobId":"job:new","attempt":1,"executable":false},
                "observationCount":1,"skippedRowCount":0,"skippedColumnCount":0,"duplicateRowCount":0
            }),
        );
    }
    if target == "/api/v1/previews/job:1" && method == Method::GET {
        return json(StatusCode::OK, valid_status());
    }
    if target.starts_with("/api/v1/previews/job:1/results?") {
        return json(StatusCode::OK, valid_page());
    }
    if target == "/api/v1/previews/job:1/resume" {
        return json(
            StatusCode::ACCEPTED,
            serde_json::json!({"jobId":"job:1","attempt":3,"executable":false}),
        );
    }
    json(StatusCode::OK, serde_json::json!({"applied":false}))
}

fn unsafe_page() -> serde_json::Value {
    serde_json::json!({
        "jobId":"job:1","attempt":2,"offset":7,"limit":9,"historical":true,"executable":false,
        "items":[{
            "ordinal":10000,"attempt":2,"committedAt":-1,"sourceConnectorId":"imdb-ratings-csv",
            "sourceRecordId":"imdb:title:tt1","sourceFingerprint":"fp","profileId":"profile:1",
            "profileVersion":1,"baseUrl":"https://neodb.example","account":"alice",
            "historical":true,"executable":false,
            "outcome":{"kind":"compared","item":{"uuid":"u","canonicalUrl":"https://neodb.example/m/u","detailUrl":"https://neodb.example/api/m/u","mediaType":"movie","model":"Movie","parentUuid":null,"externalIds":[]},"observedAt":-1,"comparison":{"profileId":"profile:1","profileVersion":0,"fields":[]},"provenance":"automatic"}
        }]
    })
}

async fn server(mode: Mode) -> (String, Arc<ServerState>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(ServerState {
        mode,
        seen: Mutex::new(Vec::new()),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .fallback(any(handler))
        .with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, state, task)
}

#[test]
fn import_reader_preserves_exact_bytes_and_enforces_sentinel_limit() {
    let bytes = b"\xef\xbb\xbfTitle,Const\r\nA,tt1\r\n";
    assert_eq!(preview::read_import(bytes.as_slice()).unwrap(), bytes);
    assert_eq!(
        preview::read_import(vec![b'x'; preview::MAX_IMPORT_BYTES].as_slice())
            .unwrap()
            .len(),
        preview::MAX_IMPORT_BYTES
    );
    assert_eq!(
        preview::read_import(vec![b'x'; preview::MAX_IMPORT_BYTES + 1].as_slice())
            .unwrap_err()
            .code,
        "preview_input_too_large"
    );
    assert!(preview::parse_id("job:one-1").is_ok());
    assert!(preview::parse_id("bad/id").is_err());
    let missing = preview::read_import_file("definitely-missing-sensitive-name.csv").unwrap_err();
    assert_eq!(missing.code, "preview_input_invalid");
    assert!(!missing.message.contains("sensitive-name"));
}

#[tokio::test]
async fn all_six_operations_use_one_token_exact_bytes_and_one_page_queries() {
    let (base, state, task) = server(Mode::Normal).await;
    let secrets = Secrets::default();
    let client = preview::PreviewClient::connect(&base, &secrets)
        .await
        .unwrap();
    let raw = b"\xef\xbb\xbfTitle,Const\r\nA,tt1\r\n".to_vec();
    let accepted = client
        .import_imdb_ratings(raw.clone(), "profile:1", 4)
        .await
        .unwrap();
    assert_eq!(accepted.preview.attempt, 1);
    assert_eq!(client.status("job:1").await.unwrap().attempt, 2);
    assert!(
        client
            .results("job:1", 2, 7, 9)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(!client.cancel("job:1", 2).await.unwrap().applied);
    assert_eq!(client.resume("job:1", 2).await.unwrap().attempt, 3);
    assert!(!client.delete("job:1").await.unwrap().applied);
    assert_eq!(secrets.gets.load(Ordering::SeqCst), 1);
    let seen = state.seen.lock().unwrap();
    assert_eq!(seen.len(), 6);
    assert_eq!(seen[0].method, Method::POST);
    assert_eq!(
        seen[0].target,
        "/api/v1/imports/imdb/ratings?profileId=profile%3A1&profileVersion=4"
    );
    assert_eq!(seen[0].body, raw);
    assert_eq!(seen[0].content_type.as_deref(), Some("text/csv"));
    assert_eq!(
        (
            &seen[1].method,
            seen[1].target.as_str(),
            seen[1].body.as_slice()
        ),
        (&Method::GET, "/api/v1/previews/job:1", &[][..])
    );
    assert_eq!(
        seen[2].target,
        "/api/v1/previews/job:1/results?attempt=2&offset=7&limit=9"
    );
    assert_eq!(seen[2].method, Method::GET);
    assert!(seen[2].body.is_empty());
    assert_eq!(
        (seen[3].method.clone(), seen[3].target.as_str()),
        (Method::POST, "/api/v1/previews/job:1/cancel")
    );
    assert_eq!(seen[3].body, br#"{"expectedAttempt":2}"#);
    assert_eq!(
        (seen[4].method.clone(), seen[4].target.as_str()),
        (Method::POST, "/api/v1/previews/job:1/resume")
    );
    assert_eq!(seen[4].body, br#"{"expectedAttempt":2}"#);
    assert_eq!(
        (seen[5].method.clone(), seen[5].target.as_str()),
        (Method::DELETE, "/api/v1/previews/job:1")
    );
    assert!(seen[5].body.is_empty());
    assert!(
        seen.iter().all(|request| request.authorization.as_deref()
            == Some(&format!("Bearer {}", "a".repeat(43))))
    );
    drop(seen);
    task.abort();
}

#[tokio::test]
async fn rejects_unsafe_echoes_flags_counts_times_and_deep_result_metadata() {
    for (mode, status_call) in [(Mode::UnsafeStatus, true), (Mode::UnsafePage, false)] {
        let (base, _, task) = server(mode).await;
        let client = preview::PreviewClient::connect(&base, &Secrets::default())
            .await
            .unwrap();
        let failure = if status_call {
            client.status("job:1").await.err().unwrap()
        } else {
            client.results("job:1", 2, 7, 9).await.err().unwrap()
        };
        assert!(
            matches!(failure, preview::PreviewFailure::Error(ApiError { code, .. }) if code == "core_unavailable")
        );
        task.abort();
    }
}

#[tokio::test]
async fn admission_metadata_is_retained_but_diagnostics_and_fields_are_sanitized() {
    let (base, state, task) = server(Mode::Admission).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    let failure = client
        .import_imdb_ratings(Vec::new(), "profile:1", 1)
        .await
        .err()
        .unwrap();
    let preview::PreviewFailure::Admission(PreviewAdmissionFailure {
        job_id,
        attempt,
        error,
    }) = failure
    else {
        panic!("expected admission envelope")
    };
    assert_eq!((job_id.as_str(), attempt), ("job:new", 1));
    assert_eq!(error.code, "preview_runtime_busy");
    assert!(error.message.contains("uncertain"));
    assert!(!error.message.contains("SECRET"));
    assert!(error.field.is_none());
    assert!(!error.retryable);
    assert_eq!(state.seen.lock().unwrap().len(), 1, "no retry");
    task.abort();

    let (base, _, task) = server(Mode::ResumeAdmission).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    let failure = client.resume("job:1", 2).await.err().unwrap();
    assert!(
        matches!(failure, preview::PreviewFailure::Admission(PreviewAdmissionFailure { job_id, attempt: 3, .. }) if job_id == "job:1")
    );
    task.abort();

    let (base, _, task) = server(Mode::Admission).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        matches!(client.resume("job:1", 2).await, Err(preview::PreviewFailure::Error(ApiError { code, .. })) if code == "core_unavailable")
    );
    task.abort();

    let (base, _, task) = server(Mode::ImportAttemptTwo).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        matches!(client.import_imdb_ratings(Vec::new(), "profile:1", 1).await, Err(preview::PreviewFailure::Error(ApiError { code, .. })) if code == "core_unavailable")
    );
    task.abort();
}

#[tokio::test]
async fn timeout_and_authentication_failures_are_static_and_not_retried() {
    let (base, state, task) = server(Mode::Delay).await;
    let client = preview::PreviewClient::connect_with_timeout(
        &base,
        &Secrets::default(),
        Duration::from_millis(25),
    )
    .await
    .unwrap();
    let failure = client.delete("job:1").await.err().unwrap();
    assert!(
        matches!(failure, preview::PreviewFailure::Error(ApiError { code, .. }) if code == "core_unavailable")
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(state.seen.lock().unwrap().len(), 1);
    task.abort();

    let (base, state, task) = server(Mode::Unauthorized).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    let failure = client.status("job:1").await.err().unwrap();
    assert!(
        matches!(failure, preview::PreviewFailure::Error(ApiError { code, message, .. }) if code == "authentication_required" && !message.contains("secret"))
    );
    assert_eq!(state.seen.lock().unwrap().len(), 1);
    task.abort();
}

#[tokio::test]
async fn rejects_response_bounds_redirects_and_wrong_resume_echo() {
    for mode in [Mode::Oversized, Mode::Redirect] {
        let (base, _, task) = server(mode).await;
        let client = preview::PreviewClient::connect(&base, &Secrets::default())
            .await
            .unwrap();
        assert!(
            matches!(client.status("job:1").await, Err(preview::PreviewFailure::Error(ApiError { code, .. })) if code == "core_unavailable")
        );
        task.abort();
    }
    let (base, _, task) = server(Mode::OversizedResults).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        matches!(client.results("job:1", 2, 7, 9).await, Err(preview::PreviewFailure::Error(ApiError { code, .. })) if code == "core_unavailable")
    );
    task.abort();

    let (base, _, task) = server(Mode::ExactStatus).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert_eq!(client.status("job:1").await.unwrap().attempt, 2);
    task.abort();

    let (base, _, task) = server(Mode::ExactResults).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        client
            .results("job:1", 2, 7, 9)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    task.abort();

    let (base, _, task) = server(Mode::WrongResume).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        matches!(client.resume("job:1", 2).await, Err(preview::PreviewFailure::Error(ApiError { code, .. })) if code == "core_unavailable")
    );
    task.abort();
}

#[tokio::test]
async fn preserves_true_and_false_mutations_and_static_csv_field_error() {
    let (base, _, task) = server(Mode::AppliedTrue).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(client.cancel("job:1", 2).await.unwrap().applied);
    assert!(client.delete("job:1").await.unwrap().applied);
    task.abort();

    let (base, _, task) = server(Mode::CsvError).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    let failure = client
        .import_imdb_ratings(Vec::new(), "profile:1", 1)
        .await
        .err()
        .unwrap();
    let preview::PreviewFailure::Error(error) = failure else {
        panic!("expected ordinary error")
    };
    assert_eq!(error.code, "csv_column_missing");
    assert_eq!(error.field.as_deref(), Some("Your Rating"));
    assert!(!error.message.contains("do not echo"));
    assert!(!error.retryable);
    task.abort();
}

#[tokio::test]
async fn resume_attempt_overflow_is_local_and_sends_no_request() {
    let (base, state, task) = server(Mode::Normal).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    let failure = client.resume("job:1", u32::MAX).await.err().unwrap();
    assert!(
        matches!(failure, preview::PreviewFailure::Error(ApiError { code, .. }) if code == "preview_attempt_exhausted")
    );
    assert!(state.seen.lock().unwrap().is_empty());
    assert!(
        client
            .import_imdb_ratings(vec![0; preview::MAX_IMPORT_BYTES + 1], "profile:1", 1)
            .await
            .is_err()
    );
    assert!(
        client
            .import_imdb_ratings(Vec::new(), "bad/id", 1)
            .await
            .is_err()
    );
    assert!(
        client
            .import_imdb_ratings(Vec::new(), "profile:1", 0)
            .await
            .is_err()
    );
    assert!(state.seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn dot_segment_ids_never_send_credentials_to_normalized_routes() {
    let (base, state, task) = server(Mode::Normal).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    for id in [".", ".."] {
        assert!(client.status(id).await.is_err());
        assert!(client.results(id, 1, 0, 50).await.is_err());
        assert!(client.cancel(id, 1).await.is_err());
        assert!(client.resume(id, 1).await.is_err());
        assert!(client.delete(id).await.is_err());
    }
    assert!(state.seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn otherwise_valid_payloads_require_exact_operation_status_codes() {
    let (base, state, task) = server(Mode::WrongImportStatus).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(
        client
            .import_imdb_ratings(Vec::new(), "profile:1", 1)
            .await
            .is_err()
    );
    assert_eq!(state.seen.lock().unwrap().len(), 1);
    task.abort();

    let (base, _, task) = server(Mode::WrongReadStatus).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(client.status("job:1").await.is_err());
    task.abort();

    let (base, state, task) = server(Mode::WrongMutationStatus).await;
    let client = preview::PreviewClient::connect(&base, &Secrets::default())
        .await
        .unwrap();
    assert!(client.cancel("job:1", 1).await.is_err());
    assert!(client.delete("job:1").await.is_err());
    assert_eq!(state.seen.lock().unwrap().len(), 2);
    task.abort();
}

#[tokio::test]
async fn preview_preflight_refuses_unsafe_or_incompatible_core_before_secrets() {
    let secrets = Secrets::default();
    for input in [
        "https://example.com",
        "http://localhost:19423",
        "http://127.0.0.1:19423/?token=private",
    ] {
        let Err(error) = preview::PreviewClient::connect(input, &secrets).await else {
            panic!("unsafe origin accepted");
        };
        assert_eq!(error.code, "core_url_invalid");
    }
    let (base, state, task) = server(Mode::Incompatible).await;
    let Err(error) = preview::PreviewClient::connect(&base, &secrets).await else {
        panic!("incompatible Core accepted");
    };
    assert_eq!(error.code, "core_incompatible");
    assert_eq!(secrets.gets.load(Ordering::SeqCst), 0);
    assert!(state.seen.lock().unwrap().is_empty());
    task.abort();
}

#[test]
fn human_json_escapes_terminal_controls() {
    let value = PreviewMutationResult { applied: true };
    assert_eq!(preview::print_json(&value), "{\n  \"applied\": true\n}");
    let error = ApiError::new(
        "unsafe",
        "中文line\n\u{1b}[31m\u{009b}31m\u{202e}data\u{2066}",
    );
    let rendered = preview::print_json(&error);
    assert!(rendered.contains("\\n\\u001b"));
    assert!(!rendered.contains('\u{1b}'));
    assert!(rendered.contains("中文"));
    for character in ['\u{009b}', '\u{202e}', '\u{2066}'] {
        assert!(!rendered.contains(character));
    }
    assert_eq!(serde_json::from_str::<ApiError>(&rendered).unwrap(), error);
}
