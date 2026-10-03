//! Routed backend acceptance with fake upstream ports, not live NeoDB HTTP.
use super::*;
use axum::{
    body::{Body, to_bytes},
    extract::Request,
};
use collector_core::{
    access::{AccessRepository, AccessService, Clock, Endpoint, PortFuture, SecretStore},
    catalog::{
        CatalogFetch, CatalogItem, CatalogSearchPage, NeoDbCatalogSearchTransport,
        NeoDbCatalogTransport,
    },
    mapping::{MappingRepository, imdb_default},
    marks::{MarkRead, MarkSnapshot, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    preview::PreviewSnapshotService,
    preview_jobs::{PreviewJobRepository, PreviewJobStatus, PreviewRetention},
    preview_records::PreviewRecordService,
    preview_results::{PreviewResultRepository, StoredPreviewOutcome},
    preview_runner::PreviewAttemptRunner,
    secrets::{ExposeSecret, SecretString},
};
use collector_domain::{
    access::{PairingExchange, PairingRequest},
    mapping::Operation,
    observation::ObservedItem,
};
use collector_storage::{Storage, credentials::SecureTokens};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use tower::ServiceExt;

const HOST: &str = "127.0.0.1:19423";
const ORIGIN: &str = "http://127.0.0.1:19423";
const BASE: &str = "https://neodb.example";
const ACCOUNT: &str = "alice";
const CREDENTIAL: &str = "credential-a";
const TEST_TOKEN: &str = "test-token";

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct CountingSecrets {
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl SecretStore for CountingSecrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn get(&self, reference: String) -> PortFuture<'_, SecretString> {
        assert_eq!(reference, CREDENTIAL);
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(TEST_TOKEN.into()) })
    }

    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

struct CountingIdentity {
    calls: AtomicUsize,
}

impl NeoDbOAuthTransport for CountingIdentity {
    fn canonical_base(&self, _: String) -> PortFuture<'_, String> {
        Box::pin(async { unreachable!() })
    }

    fn register(&self, _: String, _: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async { unreachable!() })
    }

    fn authorization_url(
        &self,
        _: String,
        _: String,
        _: String,
        _: String,
    ) -> PortFuture<'_, String> {
        Box::pin(async { unreachable!() })
    }

    fn exchange(
        &self,
        _: String,
        _: OAuthApp,
        _: SecretString,
        _: String,
    ) -> PortFuture<'_, OAuthToken> {
        Box::pin(async { unreachable!() })
    }

    fn identity<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
    ) -> PortFuture<'a, OAuthIdentity> {
        assert_eq!(base, BASE);
        assert_eq!(token.expose_secret(), TEST_TOKEN);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(OAuthIdentity {
                username: ACCOUNT.into(),
            })
        })
    }
}

struct ChangingMarks {
    calls: AtomicUsize,
    rating: AtomicU8,
}

impl NeoDbMarkTransport for ChangingMarks {
    fn read_mark<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
        expected_account: String,
        expected_item: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        assert_eq!(base, BASE);
        assert_eq!(token.expose_secret(), TEST_TOKEN);
        assert_eq!(expected_account, ACCOUNT);
        self.calls.fetch_add(1, Ordering::SeqCst);
        let item = clone_item(expected_item);
        let rating_grade = self.rating.load(Ordering::SeqCst);
        Box::pin(async move {
            Ok(MarkRead::Present(Box::new(MarkSnapshot {
                item,
                owner: ACCOUNT.into(),
                shelf_type: "complete".into(),
                rating_grade: Some(rating_grade),
            })))
        })
    }
}

struct GatedCatalog {
    calls: AtomicUsize,
    detail_calls: AtomicUsize,
    hold_call: AtomicUsize,
    entered: Notify,
    gate: Semaphore,
    source_urls: Mutex<Vec<String>>,
}

impl GatedCatalog {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            detail_calls: AtomicUsize::new(0),
            hold_call: AtomicUsize::new(2),
            entered: Notify::new(),
            gate: Semaphore::new(0),
            source_urls: Mutex::new(Vec::new()),
        }
    }

    async fn wait_for_calls(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let entered = self.entered.notified();
                if self.calls.load(Ordering::SeqCst) >= expected {
                    return;
                }
                entered.await;
            }
        })
        .await
        .expect("catalog call gate must be reached within its bound");
    }

    fn release(&self) {
        self.gate.add_permits(1);
    }
}

impl NeoDbCatalogTransport for GatedCatalog {
    fn fetch(&self, base: String, source_url: String) -> PortFuture<'_, CatalogFetch> {
        assert_eq!(base, BASE);
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.source_urls.lock().unwrap().push(source_url.clone());
            self.entered.notify_waiters();
            if self.hold_call.load(Ordering::SeqCst) == call {
                self.gate.acquire().await.unwrap().forget();
            }
            let id = source_url.trim_end_matches('/').rsplit('/').next().unwrap();
            Ok(CatalogFetch::Detail(format!("/api/movie/{id}")))
        })
    }

    fn detail(&self, base: String, reference: String) -> PortFuture<'_, CatalogItem> {
        assert_eq!(base, BASE);
        self.detail_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let id = reference.rsplit('/').next().unwrap().to_owned();
            Ok(CatalogItem {
                uuid: id.clone(),
                canonical_url: format!("{base}/movie/{id}"),
                detail_url: format!("{base}{reference}"),
                media_type: "movie".into(),
                model: "Movie".into(),
                parent_uuid: None,
                external_ids: vec![("imdb:title".into(), id)],
            })
        })
    }
}

impl NeoDbCatalogSearchTransport for GatedCatalog {
    fn search(&self, _: String, _: String, _: String) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async { unreachable!() })
    }
}

fn clone_item(item: &CatalogItem) -> CatalogItem {
    CatalogItem {
        uuid: item.uuid.clone(),
        canonical_url: item.canonical_url.clone(),
        detail_url: item.detail_url.clone(),
        media_type: item.media_type.clone(),
        model: item.model.clone(),
        parent_uuid: item.parent_uuid.clone(),
        external_ids: item.external_ids.clone(),
    }
}

fn endpoint() -> Endpoint {
    Endpoint {
        base_url: BASE.into(),
        account: ACCOUNT.into(),
        credential_ref: CREDENTIAL.into(),
    }
}

fn runner(
    storage: &Storage,
    clock: Arc<TestClock>,
    secrets: Arc<CountingSecrets>,
    identity: Arc<CountingIdentity>,
    marks: Arc<ChangingMarks>,
) -> Arc<PreviewAttemptRunner> {
    let snapshots = Arc::new(PreviewSnapshotService::new(
        Arc::new(storage.clone()),
        secrets,
        clock.clone(),
        identity,
        marks,
    ));
    Arc::new(PreviewAttemptRunner::new(
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        clock,
        Arc::new(PreviewRecordService::new(snapshots)),
    ))
}

fn app(
    storage: &Storage,
    service: Arc<AccessService>,
    management: Arc<dyn preview_runtime::PreviewManagement>,
    submission: Arc<dyn preview_runtime::PreviewSubmissionRuntime>,
) -> Router {
    let mut state = access::AccessState::new(service);
    state.mappings = Some(Arc::new(storage.clone()));
    state.preview_jobs = Some(Arc::new(storage.clone()));
    state.preview_results = Some(Arc::new(storage.clone()));
    state.preview_management = Some(management);
    state.preview_submission = Some(submission);
    protected_router(
        Arc::new(storage.clone()),
        "missing-assets".into(),
        HOST.parse().unwrap(),
        Some(state),
    )
}

fn access_service(storage: &Storage, clock: Arc<TestClock>) -> Arc<AccessService> {
    Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock,
        audience: ORIGIN.into(),
    })
}

async fn paired(service: &AccessService) -> String {
    let link = service
        .create_pairing(PairingRequest {
            client_name: "restart client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let token = service
        .pair(PairingExchange {
            one_time_key: link.uri.split("#key=").nth(1).unwrap().into(),
            client_name: "restart client".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    format!("Bearer {}", token.token)
}

async fn request(
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

async fn scenario() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(110)));
    let secrets = Arc::new(CountingSecrets {
        reads: AtomicUsize::new(0),
        writes: AtomicUsize::new(0),
    });
    let oauth_identity = Arc::new(CountingIdentity {
        calls: AtomicUsize::new(0),
    });
    let marks = Arc::new(ChangingMarks {
        calls: AtomicUsize::new(0),
        rating: AtomicU8::new(8),
    });
    let transport = Arc::new(GatedCatalog::new());
    let resolver_calls = Arc::new(AtomicUsize::new(0));

    let storage = Storage::open(dir.path()).await.unwrap();
    storage.set_endpoint(Some(endpoint())).await.unwrap();
    storage.save(imdb_default(), 0).await.unwrap();
    let first_runner = runner(
        &storage,
        clock.clone(),
        secrets.clone(),
        oauth_identity.clone(),
        marks.clone(),
    );
    let first_resolver_calls = resolver_calls.clone();
    let (first_runtime, first_maintenance) = preview_runtime::PreviewRuntime::start(
        first_runner,
        Arc::new(storage.clone()),
        clock.clone(),
        transport.clone(),
        move |item: &ObservedItem| {
            first_resolver_calls.fetch_add(1, Ordering::SeqCst);
            collector_connectors::imdb::catalog_identity(item)
        },
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    let first_runtime = Arc::new(first_runtime);
    let service = access_service(&storage, clock.clone());
    let first_app = app(
        &storage,
        service.clone(),
        first_runtime.clone(),
        first_runtime.clone(),
    );
    let (session_cookie, _) = access_tests::login(&first_app, &service).await;
    let authorization = paired(&service).await;
    let csv = concat!(
        "Const,Your Rating,Title Type,Title\n",
        "tt0000001,8,movie,First\n",
        "tt0000002,7,movie,Second\n"
    );
    let (status, accepted) = request(
        &first_app,
        "POST",
        "/api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1",
        "text/csv",
        csv,
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    assert_eq!(accepted["observationCount"], 2);
    let job_id = accepted["preview"]["jobId"].as_str().unwrap().to_owned();

    transport.wait_for_calls(2).await;
    let first_results = storage.results(job_id.clone(), 1, 0, 100).await.unwrap();
    assert_eq!(first_results.len(), 1);
    assert_eq!(first_results[0].ordinal(), 0);
    assert!(matches!(
        first_results[0].outcome(),
        StoredPreviewOutcome::Compared {
            observed_at: 110,
            ..
        }
    ));
    let (status, initial_page) = request(
        &first_app,
        "GET",
        &format!("/api/v1/previews/{job_id}/results?attempt=1&limit=100"),
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initial_page}");
    let initial_item = initial_page["items"][0].clone();
    let initial_field = &initial_item["outcome"]["comparison"]["fields"][0];
    assert_eq!(initial_item["outcome"]["kind"], "compared");
    assert_eq!(initial_item["outcome"]["observedAt"], 110);
    assert_eq!(initial_item["outcome"]["comparison"]["profileVersion"], 1);
    assert_eq!(initial_field["suggestion"], 8.0);
    assert_eq!(
        initial_field["sink"],
        json!({"kind": "known", "value": 8.0})
    );
    assert_eq!(initial_field["status"], "skipped");
    assert_eq!(initial_field["reason"], "already_equal");
    assert_eq!(initial_item["historical"], true);
    assert_eq!(initial_item["executable"], false);
    assert_eq!(secrets.reads.load(Ordering::SeqCst), 1);
    assert_eq!(marks.calls.load(Ordering::SeqCst), 1);
    assert_eq!(transport.detail_calls.load(Ordering::SeqCst), 1);

    first_runtime.shutdown().await;
    first_maintenance.wait().await.unwrap();
    assert_eq!(
        PreviewJobRepository::load(&storage, job_id.clone())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Running,
        "host shutdown must not fabricate user cancellation or failure"
    );
    drop(first_app);
    drop(service);
    drop(first_runtime);
    storage.close().await;
    drop(storage);

    clock.0.store(120, Ordering::SeqCst);
    let storage = Storage::open(dir.path()).await.unwrap();
    let before_start = (
        transport.calls.load(Ordering::SeqCst),
        transport.detail_calls.load(Ordering::SeqCst),
        secrets.reads.load(Ordering::SeqCst),
        secrets.writes.load(Ordering::SeqCst),
        oauth_identity.calls.load(Ordering::SeqCst),
        marks.calls.load(Ordering::SeqCst),
        resolver_calls.load(Ordering::SeqCst),
    );
    let second_runner = runner(
        &storage,
        clock.clone(),
        secrets.clone(),
        oauth_identity.clone(),
        marks.clone(),
    );
    let second_resolver_calls = resolver_calls.clone();
    let (second_runtime, second_maintenance) = preview_runtime::PreviewRuntime::start(
        second_runner,
        Arc::new(storage.clone()),
        clock.clone(),
        transport.clone(),
        move |item: &ObservedItem| {
            second_resolver_calls.fetch_add(1, Ordering::SeqCst);
            collector_connectors::imdb::catalog_identity(item)
        },
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        before_start,
        (
            transport.calls.load(Ordering::SeqCst),
            transport.detail_calls.load(Ordering::SeqCst),
            secrets.reads.load(Ordering::SeqCst),
            secrets.writes.load(Ordering::SeqCst),
            oauth_identity.calls.load(Ordering::SeqCst),
            marks.calls.load(Ordering::SeqCst),
            resolver_calls.load(Ordering::SeqCst),
        ),
        "startup recovery must not resume or touch upstream/credentials"
    );
    let interrupted = PreviewJobRepository::load(&storage, job_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(interrupted.status(), PreviewJobStatus::Interrupted);
    assert_eq!(interrupted.attempt(), 1);
    assert_eq!(interrupted.created_at(), 110);
    assert_eq!(interrupted.expires_at(), 110 + 30 * 86_400);
    assert_eq!(
        storage
            .results(job_id.clone(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );

    let second_runtime = Arc::new(second_runtime);
    let service = access_service(&storage, clock.clone());
    let second_app = app(
        &storage,
        service.clone(),
        second_runtime.clone(),
        second_runtime.clone(),
    );
    for headers in [
        vec![("cookie", session_cookie.as_str())],
        vec![("authorization", authorization.as_str())],
    ] {
        let (status, body) = request(
            &second_app,
            "GET",
            &format!("/api/v1/previews/{job_id}"),
            "application/json",
            Body::empty(),
            &headers,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "interrupted");
        assert_eq!(body["bindingState"], "current");
        assert_eq!(body["baseUrl"], BASE);
        assert_eq!(body["account"], ACCOUNT);
        assert!(!body.to_string().contains(CREDENTIAL));
        assert!(!body.to_string().contains(TEST_TOKEN));
    }
    let (status, reopened_page) = request(
        &second_app,
        "GET",
        &format!("/api/v1/previews/{job_id}/results?attempt=1&limit=100"),
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopened_page}");
    assert_eq!(reopened_page["items"][0], initial_item);
    let retained_endpoint = storage.endpoint().await.unwrap().unwrap();
    assert_eq!(retained_endpoint.base_url, BASE);
    assert_eq!(retained_endpoint.account, ACCOUNT);
    assert_eq!(retained_endpoint.credential_ref, CREDENTIAL);

    let mut v2 = imdb_default();
    v2.version = 2;
    let Operation::Clamp { max, .. } = &mut v2.rules[0].operation else {
        panic!("the IMDb v1 rating rule must remain a clamp")
    };
    *max = 5.0;
    storage.save(v2, 1).await.unwrap();
    clock.0.store(130, Ordering::SeqCst);
    marks.rating.store(5, Ordering::SeqCst);
    transport.hold_call.store(3, Ordering::SeqCst);
    let (status, resumed) = request(
        &second_app,
        "POST",
        &format!("/api/v1/previews/{job_id}/resume"),
        "application/json",
        json!({"expectedAttempt": 1}).to_string(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{resumed}");
    assert_eq!(resumed["attempt"], 2);
    transport.wait_for_calls(3).await;
    assert!(
        storage
            .results(job_id.clone(), 1, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .results(job_id.clone(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let resumed_job = PreviewJobRepository::load(&storage, job_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed_job.profile_version(), 1);
    assert_eq!(resumed_job.expires_at(), interrupted.expires_at());
    let (status, obsolete_page) = request(
        &second_app,
        "GET",
        &format!("/api/v1/previews/{job_id}/results?attempt=1"),
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{obsolete_page}");
    assert_eq!(obsolete_page["code"], "preview_attempt_changed");

    transport.release();
    transport.wait_for_calls(4).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let current = PreviewJobRepository::load(&storage, job_id.clone())
                .await
                .unwrap()
                .unwrap();
            if current.status() == PreviewJobStatus::Completed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("resumed attempt must complete within its bound");
    let urls = transport.source_urls.lock().unwrap().clone();
    assert_eq!(
        urls,
        vec![
            "https://www.imdb.com/title/tt0000001/",
            "https://www.imdb.com/title/tt0000002/",
            "https://www.imdb.com/title/tt0000001/",
            "https://www.imdb.com/title/tt0000002/",
        ]
    );
    let (status, page) = request(
        &second_app,
        "GET",
        &format!("/api/v1/previews/{job_id}/results?attempt=2&limit=100"),
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    for (ordinal, suggestion) in [(0, 8.0), (1, 7.0)] {
        let item = &page["items"][ordinal];
        let field = &item["outcome"]["comparison"]["fields"][0];
        assert_eq!(item["ordinal"], ordinal);
        assert_eq!(item["historical"], true);
        assert_eq!(item["executable"], false);
        assert_eq!(item["outcome"]["kind"], "compared");
        assert_eq!(item["outcome"]["observedAt"], 130);
        assert_eq!(item["outcome"]["comparison"]["profileVersion"], 1);
        assert_eq!(item["attempt"], 2);
        assert_eq!(item["outcome"]["provenance"], "automatic");
        assert_eq!(
            item["outcome"]["item"]["uuid"],
            format!("tt000000{}", ordinal + 1)
        );
        assert_eq!(
            item["outcome"]["item"]["externalIds"],
            json!([{"scheme": "imdb:title", "value": format!("tt000000{}", ordinal + 1)}])
        );
        assert_eq!(field["suggestion"], suggestion);
        assert_eq!(field["sink"], json!({"kind": "known", "value": 5.0}));
        assert_eq!(field["status"], "needs_review");
        assert_eq!(field["reason"], "sink_conflict_needs_review");
    }
    assert_eq!(secrets.reads.load(Ordering::SeqCst), 3);
    assert_eq!(marks.calls.load(Ordering::SeqCst), 3);
    assert_eq!(transport.detail_calls.load(Ordering::SeqCst), 3);
    assert_eq!(resolver_calls.load(Ordering::SeqCst), 4);
    assert_eq!(oauth_identity.calls.load(Ordering::SeqCst), 0);
    assert!(!page.to_string().contains(CREDENTIAL));
    assert!(!page.to_string().contains(TEST_TOKEN));

    let (status, deleted) = request(
        &second_app,
        "DELETE",
        &format!("/api/v1/previews/{job_id}"),
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["applied"], true);
    assert!(
        PreviewJobRepository::load(&storage, job_id.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .observations(job_id.clone(), 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .results(job_id.clone(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        MappingRepository::load(&storage, "imdb_ratings".into(), Some(1))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        MappingRepository::load(&storage, "imdb_ratings".into(), Some(2))
            .await
            .unwrap()
            .is_some()
    );
    let retained_endpoint = storage.endpoint().await.unwrap().unwrap();
    assert_eq!(retained_endpoint.base_url, BASE);
    assert_eq!(retained_endpoint.account, ACCOUNT);
    assert_eq!(retained_endpoint.credential_ref, CREDENTIAL);
    assert_eq!(storage.clients().await.unwrap().len(), 1);
    assert_eq!(secrets.writes.load(Ordering::SeqCst), 0);

    let (status, _) = request(
        &second_app,
        "GET",
        "/api/v1/client",
        "application/json",
        Body::empty(),
        &[("authorization", &authorization)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request(
        &second_app,
        "GET",
        "/api/v1/session",
        "application/json",
        Body::empty(),
        &[("cookie", &session_cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for path in [
        format!("/api/v1/previews/{job_id}"),
        format!("/api/v1/previews/{job_id}/results?attempt=2"),
    ] {
        let (status, body) = request(
            &second_app,
            "GET",
            &path,
            "application/json",
            Body::empty(),
            &[("authorization", &authorization)],
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["code"], "preview_job_not_found");
    }
    assert_eq!(secrets.reads.load(Ordering::SeqCst), 3);
    assert_eq!(marks.calls.load(Ordering::SeqCst), 3);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 4);
    assert_eq!(transport.detail_calls.load(Ordering::SeqCst), 3);
    assert_eq!(secrets.writes.load(Ordering::SeqCst), 0);

    second_runtime.shutdown().await;
    second_maintenance.wait().await.unwrap();
    drop(second_app);
    drop(service);
    drop(second_runtime);
    storage.close().await;
}

#[tokio::test]
async fn authenticated_import_restart_resume_and_delete_preserve_unrelated_state() {
    tokio::time::timeout(Duration::from_secs(15), scenario())
        .await
        .expect("restart acceptance must complete within its outer bound");
}
