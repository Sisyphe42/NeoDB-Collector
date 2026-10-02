use super::*;
use axum::{
    body::{Body, to_bytes},
    extract::Request,
};
use collector_core::{
    access::{AccessRecord, AccessRepository, AccessService, Clock, TokenSource},
    mapping::{MappingRepository, imdb_default},
    preview_jobs::{
        PreviewJob, PreviewJobRepository, PreviewJobStatus, PreviewRetention, PreviewSubmission,
    },
};
use collector_domain::{
    ApiError,
    access::{PairingExchange, PairingRequest},
};
use collector_storage::credentials::SecureTokens;
use serde_json::Value;
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tower::ServiceExt;

use crate::preview_runtime::{PreviewAdmissionObserver, PreviewSubmissionRuntime};

const HOST: &str = "127.0.0.1:19423";
const ORIGIN: &str = "http://127.0.0.1:19423";

#[derive(Clone, Copy)]
struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

struct FixedTokens;

impl TokenSource for FixedTokens {
    fn random(&self) -> Result<String, ApiError> {
        Ok("job:http-import".into())
    }

    fn digest(&self, value: &str) -> String {
        format!("digest:{value}")
    }
}

#[derive(Clone, Copy)]
enum SubmissionBehavior {
    DirectFailure,
    DelayedFailure(std::time::Duration),
}

struct SubmissionRuntime {
    behavior: SubmissionBehavior,
    calls: Mutex<usize>,
    entered: tokio::sync::Notify,
}

impl PreviewSubmissionRuntime for SubmissionRuntime {
    fn retention(&self) -> PreviewRetention {
        PreviewRetention::default()
    }

    fn submit(&self, _: PreviewSubmission) -> Result<PreviewAdmissionObserver, ApiError> {
        *self.calls.lock().unwrap() += 1;
        let result = match self.behavior {
            SubmissionBehavior::DirectFailure => Err(ApiError::new(
                "private_runtime_failure",
                "secret runtime diagnostic",
            )),
            SubmissionBehavior::DelayedFailure(delay) => {
                Ok(PreviewAdmissionObserver::delayed_test_error(
                    delay,
                    ApiError::new("preview_submit_failed", "secret database diagnostic"),
                    true,
                ))
            }
        };
        self.entered.notify_waiters();
        result
    }

    fn resume(&self, _: &PreviewJob) -> Result<PreviewAdmissionObserver, ApiError> {
        Err(ApiError::new(
            "private_runtime_failure",
            "secret runtime diagnostic",
        ))
    }
}

struct ChunkBody {
    chunks: std::collections::VecDeque<Result<Vec<u8>, std::io::Error>>,
    polled: Option<Arc<AtomicBool>>,
}

impl ChunkBody {
    fn new(chunks: impl IntoIterator<Item = Result<Vec<u8>, std::io::Error>>) -> Self {
        Self {
            chunks: chunks.into_iter().collect(),
            polled: None,
        }
    }

    fn observed(polled: Arc<AtomicBool>) -> Self {
        Self {
            chunks: [Ok(b"must not be read".to_vec())].into_iter().collect(),
            polled: Some(polled),
        }
    }
}

impl http_body::Body for ChunkBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if let Some(polled) = &self.polled {
            polled.store(true, Ordering::SeqCst);
        }
        Poll::Ready(self.chunks.pop_front().map(|result| {
            result.map(|bytes| http_body::Frame::data(axum::body::Bytes::from(bytes)))
        }))
    }
}

async fn bearer(service: &AccessService) -> String {
    paired(service).await.0
}

async fn paired(service: &AccessService) -> (String, String) {
    let link = service
        .create_pairing(PairingRequest {
            client_name: "test client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let key = link.uri.split("#key=").nth(1).unwrap();
    let token = service
        .pair(PairingExchange {
            one_time_key: key.into(),
            client_name: "test client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    (format!("Bearer {}", token.token), token.client_id)
}

fn app_for_fixture(
    fixture: &preview_runtime::tests::HttpRuntimeFixture,
) -> (Arc<AccessService>, Router) {
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.mappings = Some(Arc::new(fixture.storage.clone()));
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_results = Some(Arc::new(fixture.storage.clone()));
    state.preview_management = Some(fixture.management());
    state.preview_submission = Some(fixture.submission());
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    (service, app)
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    content_type: &str,
    body: impl Into<Body>,
    authorization: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", content_type);
    if let Some(value) = authorization {
        builder = builder.header("authorization", value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(body.into()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn request_with_headers(
    app: &Router,
    method: &str,
    path: &str,
    content_type: &str,
    body: impl Into<Body>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", content_type);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(body.into()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn raw_csv_persists_only_normalized_observations_and_returns_bounded_counts() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let mut v2 = imdb_default();
    v2.version = 2;
    fixture.storage.save(v2, 1).await.unwrap();
    let (service, app) = app_for_fixture(&fixture);
    let authorization = bearer(&service).await;
    let csv = concat!(
        "\u{feff}Const,Your Rating,Title Type,Title,Unsupported\n",
        "tt1234567,8,movie,One,discard me\n",
        "tt1234567,8,movie,Duplicate,also discarded\n",
        "tt7654321,7,podcast,Skipped,private unsupported value\n"
    );
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv; charset=UTF-8",
        csv,
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["preview"]["attempt"], 1);
    assert_eq!(body["preview"]["executable"], false);
    assert_eq!(body["observationCount"], 1);
    assert_eq!(body["skippedRowCount"], 1);
    assert_eq!(body["skippedColumnCount"], 1);
    assert_eq!(body["duplicateRowCount"], 1);
    let id = body["preview"]["jobId"].as_str().unwrap();
    let job = PreviewJobRepository::load(&fixture.storage, id.into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.profile_id(), "imdb_ratings");
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.observation_count(), 1);
    let endpoint = fixture.storage.endpoint().await.unwrap().unwrap();
    assert!(job.is_bound_to(&endpoint));
    let observations = PreviewJobRepository::observations(&fixture.storage, id.into(), 0, 100)
        .await
        .unwrap();
    let encoded = serde_json::to_string(&observations[0]).unwrap();
    assert!(!encoded.contains("discard me"));
    assert!(!encoded.contains("private unsupported value"));
    fixture.shutdown().await;
}

#[tokio::test]
async fn strict_query_content_type_and_stream_limit_fail_before_submission() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let (service, app) = app_for_fixture(&fixture);
    let authorization = bearer(&service).await;
    for (path, content_type, expected) in [
        (
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=0",
            "text/csv",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1&extra=x",
            "text/csv",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileId=other&profileVersion=1",
            "text/csv",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=4294967296",
            "text/csv",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
            "application/json",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
    ] {
        let (status, _) = request(
            &app,
            "POST",
            path,
            content_type,
            "Const,Your Rating,Title Type\n",
            Some(&authorization),
        )
        .await;
        assert_eq!(status, expected, "{path}");
    }
    let exact = ChunkBody::new([
        Ok(vec![b' '; collector_connectors::imdb::MAX_BYTES / 2]),
        Ok(vec![b' '; collector_connectors::imdb::MAX_BYTES / 2]),
    ]);
    let (status, _) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        Body::new(exact),
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let over = ChunkBody::new([
        Ok(vec![b' '; collector_connectors::imdb::MAX_BYTES]),
        Ok(vec![b'!']),
    ]);
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        Body::new(over),
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");

    let failed = ChunkBody::new([Err(std::io::Error::other("private stream failure"))]);
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        Body::new(failed),
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "request_invalid");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1")
                .header("host", HOST)
                .header("authorization", &authorization)
                .header("content-type", "text/csv")
                .header("content-type", "text/csv; charset=utf-8")
                .body(Body::from("Const,Your Rating,Title Type\n"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    for (payload, code) in [
        (vec![0xff], "csv_encoding_invalid"),
        (
            b"Const,Your Rating,Title Type\n\"tt1234567,8,movie\n".to_vec(),
            "csv_structure_invalid",
        ),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
            "text/csv",
            payload,
            Some(&authorization),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["code"], code);
        assert!(!body.to_string().contains("tt1234567"));
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn authentication_precedes_upload_validation_and_body_collection() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let (_service, app) = app_for_fixture(&fixture);
    let polled = Arc::new(AtomicBool::new(false));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/imports/imdb/ratings?bad=query")
                .header("host", HOST)
                .header("content-type", "application/json")
                .body(Body::new(ChunkBody::observed(polled.clone())))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(response.into_body(), 10_000).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "authentication_required");
    assert!(!polled.load(Ordering::SeqCst));
    fixture.shutdown().await;
}

#[tokio::test]
async fn session_csrf_and_bearer_precedence_are_enforced_for_upload() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let (service, app) = app_for_fixture(&fixture);
    let (cookie, csrf) = access_tests::login(&app, &service).await;
    let path = "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1";
    let csv = "Const,Your Rating,Title Type\n";
    let (status, body) = request_with_headers(
        &app,
        "POST",
        path,
        "text/csv",
        csv,
        &[("cookie", &cookie), ("x-csrf-token", &csrf)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "origin_not_allowed");

    let (status, body) = request_with_headers(
        &app,
        "POST",
        path,
        "text/csv",
        csv,
        &[
            ("cookie", &cookie),
            ("origin", ORIGIN),
            ("x-csrf-token", &csrf),
            ("authorization", "Bearer invalid"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "authentication_required");

    let (status, body) = request_with_headers(
        &app,
        "POST",
        path,
        "text/csv",
        csv,
        &[
            ("cookie", &cookie),
            ("origin", ORIGIN),
            ("x-csrf-token", &csrf),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");

    let (revoked_authorization, client_id) = paired(&service).await;
    fixture
        .storage
        .revoke(client_id, "client".into())
        .await
        .unwrap();
    let (status, body) = request(
        &app,
        "POST",
        path,
        "text/csv",
        csv,
        Some(&revoked_authorization),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "authentication_required");
    fixture.shutdown().await;
}

#[tokio::test]
async fn resume_envelope_cap_attempt_and_eligibility_are_strict() {
    let fixture = preview_runtime::tests::http_fixture(1, 0).await;
    let id = fixture.job_id(0).to_owned();
    let (service, app) = app_for_fixture(&fixture);
    let authorization = bearer(&service).await;
    for (body, expected) in [
        (
            r#"{"expectedAttempt":1,"extra":true}"#.to_owned(),
            StatusCode::BAD_REQUEST,
        ),
        (
            r#"{"expectedAttempt":1,"expectedAttempt":1}"#.to_owned(),
            StatusCode::BAD_REQUEST,
        ),
        (
            r#"{"expectedAttempt":0}"#.to_owned(),
            StatusCode::BAD_REQUEST,
        ),
        (r#"{"expectedAttempt":2}"#.to_owned(), StatusCode::CONFLICT),
        (r#"{"expectedAttempt":1}"#.to_owned(), StatusCode::CONFLICT),
        (
            format!(
                r#"{{"expectedAttempt":1,"padding":"{}"}}"#,
                "x".repeat(4096)
            ),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let (status, response) = request(
            &app,
            "POST",
            &format!("/api/v1/previews/{id}/resume"),
            "application/json",
            body,
            Some(&authorization),
        )
        .await;
        assert_eq!(status, expected, "{response}");
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn resume_requires_current_eligible_attempt_and_runs_a_fresh_generation() {
    let fixture = preview_runtime::tests::http_fixture(1, 1).await;
    let id = fixture.job_id(0).to_owned();
    let before = PreviewJobRepository::load(&fixture.storage, id.clone())
        .await
        .unwrap()
        .unwrap();
    let fixed_expiry = before.expires_at();
    assert_eq!(fixture.storage.interrupt_running(110).await.unwrap(), 1);
    let mut v2 = imdb_default();
    v2.version = 2;
    fixture.storage.save(v2, 1).await.unwrap();
    let (service, app) = app_for_fixture(&fixture);
    let authorization = bearer(&service).await;

    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/previews/{id}/resume"),
        "application/json",
        r#"{"expectedAttempt":1}"#,
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["attempt"], 2);
    assert_eq!(body["executable"], false);
    let job = PreviewJobRepository::load(&fixture.storage, id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.attempt(), 2);
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.expires_at(), fixed_expiry);
    assert!(matches!(
        job.status(),
        PreviewJobStatus::Queued | PreviewJobStatus::Running
    ));
    fixture.wait_for_calls(1).await;
    fixture.release();

    let (status, body) = request(
        &app,
        "POST",
        &format!("/api/v1/previews/{id}/resume"),
        "application/json",
        r#"{"expectedAttempt":1}"#,
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "preview_attempt_changed");
    fixture.shutdown().await;
}

#[tokio::test]
async fn disconnected_and_missing_selection_or_job_are_static() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let (service, app) = app_for_fixture(&fixture);
    let authorization = bearer(&service).await;
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=2",
        "text/csv",
        "Const,Your Rating,Title Type\n",
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "mapping_profile_not_found");

    fixture.storage.set_endpoint(None).await.unwrap();
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        "Const,Your Rating,Title Type\n",
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "neodb_not_connected");

    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/previews/missing/resume",
        "application/json",
        r#"{"expectedAttempt":1}"#,
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "preview_job_not_found");
    fixture.shutdown().await;
}

#[tokio::test]
async fn full_runtime_rejects_before_creating_the_known_submission_id() {
    let fixture = preview_runtime::tests::http_fixture(9, 1).await;
    let mut observers = Vec::new();
    for index in 0..9 {
        observers.push(fixture.enqueue(index));
    }
    let tokens = Arc::new(FixedTokens);
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: tokens.clone(),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    fixture
        .storage
        .insert(AccessRecord {
            id: "client:http".into(),
            hash: tokens.digest("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            kind: "client".into(),
            audience: ORIGIN.into(),
            name: "test".into(),
            client_kind: "cli".into(),
            expires_at: 1_000,
        })
        .await
        .unwrap();
    let mut state = access::AccessState::new(service);
    state.mappings = Some(Arc::new(fixture.storage.clone()));
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_submission = Some(fixture.submission());
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        "Const,Your Rating,Title Type\n",
        Some("Bearer aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["code"], "preview_runtime_busy");
    assert!(
        PreviewJobRepository::load(&fixture.storage, "job:http-import".into())
            .await
            .unwrap()
            .is_none()
    );
    drop(observers);
    fixture.shutdown().await;
}

#[tokio::test]
async fn exact_deadline_after_reservation_returns_reconcilable_sanitized_envelope() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    let fake = Arc::new(SubmissionRuntime {
        behavior: SubmissionBehavior::DelayedFailure(std::time::Duration::from_secs(10)),
        calls: Mutex::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.mappings = Some(Arc::new(fixture.storage.clone()));
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_submission = Some(fake.clone());
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    let authorization = bearer(&service).await;
    let entered = fake.entered.notified();
    tokio::pin!(entered);
    entered.as_mut().enable();
    let task = tokio::spawn(async move {
        request(
            &app,
            "POST",
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
            "text/csv",
            "Const,Your Rating,Title Type\n",
            Some(&authorization),
        )
        .await
    });
    entered.await;
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    let (status, body) = task.await.unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body["jobId"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(body["attempt"], 1);
    assert_eq!(body["error"]["code"], "preview_submit_failed");
    assert!(!body.to_string().contains("secret"));
    fixture.shutdown().await;
}

#[tokio::test]
async fn unknown_pre_admission_error_is_static_and_has_no_reconciliation_claim() {
    let fixture = preview_runtime::tests::http_fixture(0, 0).await;
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    let fake = Arc::new(SubmissionRuntime {
        behavior: SubmissionBehavior::DirectFailure,
        calls: Mutex::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.mappings = Some(Arc::new(fixture.storage.clone()));
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_submission = Some(fake);
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    let authorization = bearer(&service).await;
    let (status, body) = request(
        &app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        "Const,Your Rating,Title Type\n",
        Some(&authorization),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "preview_unavailable");
    assert!(body.get("jobId").is_none());
    assert!(!body.to_string().contains("secret"));
    fixture.shutdown().await;
}

#[tokio::test]
async fn post_commit_submission_timeout_returns_id_and_persisted_status_reconciles() {
    let fixture = preview_runtime::tests::controlled_http_fixture(0, 0, 5).await;
    let tokens = Arc::new(FixedTokens);
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: tokens.clone(),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    fixture
        .storage
        .insert(AccessRecord {
            id: "client:http".into(),
            hash: tokens.digest("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            kind: "client".into(),
            audience: ORIGIN.into(),
            name: "test".into(),
            client_kind: "cli".into(),
            expires_at: 1_000,
        })
        .await
        .unwrap();
    let mut state = access::AccessState::new(service);
    state.mappings = Some(Arc::new(fixture.storage.clone()));
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_submission = Some(fixture.submission());
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    let task = tokio::spawn(async move {
        request(
            &app,
            "POST",
            "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
            "text/csv",
            "Const,Your Rating,Title Type\n",
            Some("Bearer aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        )
        .await
    });
    fixture.wait_for_commit_hold().await;
    assert!(
        PreviewJobRepository::load(&fixture.storage, "job:http-import".into())
            .await
            .unwrap()
            .is_some()
    );
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    let (status, body) = task.await.unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["jobId"], "job:http-import");
    assert_eq!(body["attempt"], 1);
    assert_eq!(body["error"]["code"], "preview_submit_failed");
    let persisted = PreviewJobRepository::load(&fixture.storage, "job:http-import".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.attempt(), 1);
    fixture.release_mutation();
    fixture.shutdown().await;
}

#[tokio::test]
async fn post_commit_resume_timeout_reports_next_attempt_and_keeps_fixed_binding() {
    let fixture = preview_runtime::tests::controlled_http_fixture(1, 1, 5).await;
    let id = fixture.job_id(0).to_owned();
    let before = PreviewJobRepository::load(&fixture.storage, id.clone())
        .await
        .unwrap()
        .unwrap();
    let fixed_expiry = before.expires_at();
    assert_eq!(fixture.storage.interrupt_running(110).await.unwrap(), 1);
    let service = Arc::new(AccessService {
        repository: Arc::new(fixture.storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: ORIGIN.into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_jobs = Some(Arc::new(fixture.storage.clone()));
    state.preview_submission = Some(fixture.submission());
    let app = protected_router(
        Arc::new(fixture.storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    );
    let authorization = bearer(&service).await;
    let request_id = id.clone();
    let task = tokio::spawn(async move {
        request(
            &app,
            "POST",
            &format!("/api/v1/previews/{request_id}/resume"),
            "application/json",
            r#"{"expectedAttempt":1}"#,
            Some(&authorization),
        )
        .await
    });
    fixture.wait_for_commit_hold().await;
    assert_eq!(
        PreviewJobRepository::load(&fixture.storage, id.clone())
            .await
            .unwrap()
            .unwrap()
            .attempt(),
        2
    );
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    let (status, body) = task.await.unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["jobId"], id);
    assert_eq!(body["attempt"], 2);
    assert_eq!(body["error"]["code"], "preview_resume_failed");
    let persisted =
        PreviewJobRepository::load(&fixture.storage, body["jobId"].as_str().unwrap().into())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(persisted.attempt(), 2);
    assert_eq!(persisted.profile_version(), 1);
    assert_eq!(persisted.expires_at(), fixed_expiry);
    fixture.release_mutation();
    fixture.shutdown().await;
}

#[test]
fn generated_submission_schemas_are_closed_bounded_and_non_executable() {
    let documents = contract_documents();
    for name in [
        "PreviewImportSelection",
        "PreviewResumeRequest",
        "PreviewAccepted",
        "PreviewImportAccepted",
        "PreviewAdmissionFailure",
    ] {
        assert_eq!(
            documents["schemas"][name]["additionalProperties"], false,
            "{name}"
        );
    }
    assert_eq!(
        documents["schemas"]["PreviewAccepted"]["properties"]["executable"]["const"],
        false
    );
    let import = &documents["openapi"]["paths"]["/api/v1/imports/imdb/ratings"]["post"];
    assert!(import["requestBody"]["content"]["text/csv"].is_object());
    for name in ["profileId", "profileVersion"] {
        let parameter = import["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|parameter| parameter["name"] == name)
            .unwrap();
        assert_eq!(parameter["required"], true);
    }
    assert_eq!(
        import["responses"]["202"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PreviewImportAccepted"
    );
    for path in [
        "/api/v1/imports/imdb/ratings",
        "/api/v1/previews/{id}/resume",
    ] {
        for status in ["400", "409", "413", "503"] {
            assert_eq!(
                documents["openapi"]["paths"][path]["post"]["responses"][status]["content"]
                    ["application/json"]["schema"]["oneOf"]
                    .as_array()
                    .unwrap()
                    .len(),
                2,
                "{path} {status}"
            );
        }
    }
}
