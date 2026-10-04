//! Rust CLI client acceptance over real loopback Core and NeoDB adapters.
//! This does not launch the shipped CLI process or access native credentials.
use super::{pair, preview::PreviewClient};
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use collector_connectors::neodb::NeoDbClient;
use collector_core::{
    access::{AccessRepository, AccessService, Clock, Endpoint, PortFuture, SecretStore},
    mapping::{MappingRepository, imdb_default},
    preview::{ComparisonStatus, SinkValue},
    preview_jobs::{PreviewJobRepository, PreviewJobStatus, PreviewRetention},
    preview_records::{PreviewRecordProvenance, PreviewRecordService},
    preview_results::{PreviewResultRepository, StoredPreviewOutcome},
    preview_runner::PreviewAttemptRunner,
    secrets::{ExposeSecret, SecretString},
};
use collector_domain::{
    access::PairingRequest,
    mapping::Operation,
    observation::ScalarValue,
    preview::{
        PreviewBindingState, PreviewComparisonStatusView, PreviewJobStatusView, PreviewOutcomeView,
        PreviewProvenanceView, PreviewSinkView,
    },
};
use collector_server::{access::AccessState, preview_runtime::PreviewRuntime};
use collector_storage::{Storage, credentials::SecureTokens};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, oneshot};

const NOW: i64 = 1_800_000_000;
const ACCOUNT: &str = "alice";
const CREDENTIAL_REF: &str = "fixture-neodb-credential";
const NEODB_TOKEN: &str = "neodb-fixture-token-e5a";
const RAW_COLUMN_MARKER: &str = "DISCARDED_RAW_COLUMN_E5A";
const UPSTREAM_MARKER: &str = "DISCARDED_UPSTREAM_PAYLOAD_E5A";
const IDS: [&str; 3] = ["tt1111111", "tt2222222", "tt3333333"];
const UUIDS: [&str; 3] = ["Movie1111111", "Movie2222222", "Movie3333333"];

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        NOW
    }
}

#[derive(Default)]
struct FixtureSecrets {
    values: Mutex<HashMap<String, SecretString>>,
    client_sets: AtomicUsize,
    probe_sets: AtomicUsize,
    probe_deletes: AtomicUsize,
    neodb_reads: AtomicUsize,
    neodb_writes: AtomicUsize,
}

impl FixtureSecrets {
    fn new() -> Self {
        let fixture = Self::default();
        fixture
            .values
            .lock()
            .unwrap()
            .insert(CREDENTIAL_REF.into(), NEODB_TOKEN.into());
        fixture
    }

    fn cli_token(&self) -> String {
        self.values
            .lock()
            .unwrap()
            .iter()
            .find(|(reference, _)| reference.starts_with("collector-cli:"))
            .map(|(_, value)| value.expose_secret().to_owned())
            .expect("pairing stored the CLI credential")
    }
}

impl SecretStore for FixtureSecrets {
    fn set(&self, reference: String, secret: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if reference == CREDENTIAL_REF {
                self.neodb_writes.fetch_add(1, Ordering::SeqCst);
            } else if reference.starts_with("collector-cli-probe:") {
                self.probe_sets.fetch_add(1, Ordering::SeqCst);
            } else if reference.starts_with("collector-cli:") {
                self.client_sets.fetch_add(1, Ordering::SeqCst);
            }
            self.values.lock().unwrap().insert(reference, secret);
            Ok(())
        })
    }

    fn get(&self, reference: String) -> PortFuture<'_, SecretString> {
        Box::pin(async move {
            if reference == CREDENTIAL_REF {
                self.neodb_reads.fetch_add(1, Ordering::SeqCst);
            }
            self.values
                .lock()
                .unwrap()
                .get(&reference)
                .cloned()
                .ok_or_else(|| {
                    collector_domain::ApiError::new("credential_missing", "Credential missing.")
                })
        })
    }

    fn delete(&self, reference: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if reference == CREDENTIAL_REF {
                self.neodb_writes.fetch_add(1, Ordering::SeqCst);
            } else if reference.starts_with("collector-cli-probe:") {
                self.probe_deletes.fetch_add(1, Ordering::SeqCst);
            }
            self.values.lock().unwrap().remove(&reference);
            Ok(())
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SeenRequest {
    method: String,
    uri: String,
    authorization: Option<String>,
    cookie: bool,
}

struct UpstreamState {
    base: String,
    requests: Mutex<Vec<SeenRequest>>,
    final_entered: AtomicBool,
    final_released: AtomicBool,
    entered: Notify,
    released: Notify,
}

impl UpstreamState {
    async fn wait_for_final_request(&self) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let notified = self.entered.notified();
                if self.final_entered.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
        })
        .await
        .expect("the final identity request must arrive within its bound");
    }

    fn release_final_request(&self) {
        self.final_released.store(true, Ordering::SeqCst);
        self.released.notify_waiters();
    }
}

fn item(base: &str, index: usize) -> Value {
    json!({
        "id": format!("{base}/movie/{}", UUIDS[index]),
        "uuid": UUIDS[index],
        "api_url": format!("/api/movie/{}", UUIDS[index]),
        "category": "movie",
        "type": "Movie",
        "parent_uuid": null,
        "imdb": IDS[index],
        "external_resources": [{"url": format!("https://www.imdb.com/title/{}/", IDS[index])}],
        "unsupported_payload": UPSTREAM_MARKER
    })
}

fn index_for_path(path: &str) -> Option<usize> {
    UUIDS.iter().position(|uuid| path.ends_with(uuid))
}

async fn upstream(State(state): State<Arc<UpstreamState>>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    let uri = request.uri().to_string();
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let cookie = request.headers().contains_key(header::COOKIE);
    state.requests.lock().unwrap().push(SeenRequest {
        method: request.method().to_string(),
        uri,
        authorization,
        cookie,
    });

    if path == "/api/catalog/fetch" {
        let query = request.uri().query().unwrap_or_default();
        let Some(index) = IDS.iter().position(|id| query.contains(id)) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        return (
            StatusCode::FOUND,
            [(header::LOCATION, format!("/api/movie/{}", UUIDS[index]))],
        )
            .into_response();
    }
    if path.starts_with("/api/movie/") {
        let Some(index) = index_for_path(&path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        return Json(item(&state.base, index)).into_response();
    }
    if path.starts_with("/api/me/shelf/item/") {
        let Some(index) = index_for_path(&path) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if index == 2 {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"message": "Mark not found"})),
            )
                .into_response();
        }
        return Json(json!({
            "item": item(&state.base, index),
            "owner": {"username": ACCOUNT},
            "shelf_type": "complete",
            "rating_grade": if index == 0 { 8 } else { 5 },
            "unsupported_payload": UPSTREAM_MARKER
        }))
        .into_response();
    }
    if path == "/api/me" {
        state.final_entered.store(true, Ordering::SeqCst);
        state.entered.notify_waiters();
        loop {
            let notified = state.released.notified();
            if state.final_released.load(Ordering::SeqCst) {
                break;
            }
            notified.await;
        }
        return Json(json!({"username": ACCOUNT, "unsupported_payload": UPSTREAM_MARKER}))
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn run_scenario() {
    let temp = tempfile::tempdir().unwrap();
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_base = format!("http://{}", upstream_listener.local_addr().unwrap());
    let upstream_state = Arc::new(UpstreamState {
        base: upstream_base.clone(),
        requests: Mutex::new(Vec::new()),
        final_entered: AtomicBool::new(false),
        final_released: AtomicBool::new(false),
        entered: Notify::new(),
        released: Notify::new(),
    });
    let upstream_app = Router::new()
        .fallback(upstream)
        .with_state(upstream_state.clone());
    let (upstream_shutdown_tx, upstream_shutdown_rx) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_app)
            .with_graceful_shutdown(async move {
                let _ = upstream_shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let storage = Storage::open(temp.path()).await.unwrap();
    storage
        .set_endpoint(Some(Endpoint {
            base_url: upstream_base.clone(),
            account: ACCOUNT.into(),
            credential_ref: CREDENTIAL_REF.into(),
        }))
        .await
        .unwrap();
    storage.save(imdb_default(), 0).await.unwrap();
    let mut v2 = imdb_default();
    v2.version = 2;
    let Operation::Clamp { max, .. } = &mut v2.rules[0].operation else {
        panic!("the default rating rule remains a clamp")
    };
    *max = 5.0;
    storage.save(v2, 1).await.unwrap();

    let clock = Arc::new(FixedClock);
    let secrets = Arc::new(FixtureSecrets::new());
    let neodb = Arc::new(NeoDbClient::new().unwrap());
    let snapshots = Arc::new(collector_core::preview::PreviewSnapshotService::new(
        Arc::new(storage.clone()),
        secrets.clone(),
        clock.clone(),
        neodb.clone(),
        neodb.clone(),
    ));
    let runner = Arc::new(PreviewAttemptRunner::new(
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        clock.clone(),
        Arc::new(PreviewRecordService::new(snapshots)),
    ));
    let (runtime, maintenance) = PreviewRuntime::start(
        runner,
        Arc::new(storage.clone()),
        clock.clone(),
        neodb,
        collector_connectors::imdb::catalog_identity,
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    let runtime = Arc::new(runtime);

    let core_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core_address = core_listener.local_addr().unwrap();
    let core_base = format!("http://{core_address}");
    let access = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: clock.clone(),
        audience: core_base.clone(),
    });
    let mut access_state = AccessState::new(access.clone());
    access_state.mappings = Some(Arc::new(storage.clone()));
    access_state.preview_jobs = Some(Arc::new(storage.clone()));
    access_state.preview_results = Some(Arc::new(storage.clone()));
    access_state.preview_management = Some(runtime.clone());
    access_state.preview_submission = Some(runtime.clone());
    let missing_assets = temp.path().join("missing-assets");
    let (core_shutdown_tx, core_shutdown_rx) = oneshot::channel();
    let core_storage = storage.clone();
    let core_server = tokio::spawn(async move {
        collector_server::serve(
            core_listener,
            Arc::new(core_storage),
            missing_assets,
            access_state,
            async move {
                let _ = core_shutdown_rx.await;
            },
        )
        .await
        .unwrap();
    });

    let pairing = access
        .create_pairing(PairingRequest {
            client_name: "E5a CLI".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let report = pair(&pairing.uri, "E5a CLI", secrets.as_ref())
        .await
        .unwrap();
    assert_eq!(secrets.client_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_deletes.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.neodb_writes.load(Ordering::SeqCst), 0);
    let cli_token = secrets.cli_token();
    assert_ne!(cli_token, NEODB_TOKEN);
    assert!(!serde_json::to_string(&report).unwrap().contains(&cli_token));

    let client = PreviewClient::connect(&core_base, secrets.as_ref())
        .await
        .unwrap();
    let csv = format!(
        "\u{feff}Const,Your Rating,Title Type,Title,Raw Column\r\n\
         tt1111111,8,movie,Equal,{RAW_COLUMN_MARKER}\r\n\
         tt2222222,7,movie,中文标题,{RAW_COLUMN_MARKER}\r\n\
         tt3333333,6,movie,Missing,{RAW_COLUMN_MARKER}\r\n\
         tt3333333,6,movie,Duplicate,{RAW_COLUMN_MARKER}\r\n\
         tt4444444,9,tvEpisode,Skipped,{RAW_COLUMN_MARKER}\r\n"
    );
    let accepted = client
        .import_imdb_ratings(csv.into_bytes(), "imdb_ratings", 1)
        .await
        .unwrap();
    assert_eq!(accepted.observation_count, 3);
    assert_eq!(accepted.skipped_row_count, 1);
    assert_eq!(accepted.skipped_column_count, 1);
    assert_eq!(accepted.duplicate_row_count, 1);
    assert_eq!(accepted.preview.attempt, 1);
    assert!(!accepted.preview.executable);
    let job_id = accepted.preview.job_id;

    upstream_state.wait_for_final_request().await;
    let in_flight = client.status(&job_id).await.unwrap();
    assert!(matches!(in_flight.status, PreviewJobStatusView::Running));
    upstream_state.release_final_request();

    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let status = client.status(&job_id).await.unwrap();
            if matches!(status.status, PreviewJobStatusView::Completed) {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the accepted attempt must persist completion within its bound");
    assert_eq!(completed.attempt, 1);
    assert_eq!(completed.observation_count, 3);
    assert_eq!(completed.profile_id, "imdb_ratings");
    assert_eq!(completed.profile_version, 1);
    assert_eq!(completed.base_url, upstream_base);
    assert_eq!(completed.account, ACCOUNT);
    assert!(matches!(
        completed.binding_state,
        PreviewBindingState::Current
    ));
    assert_eq!(completed.created_at, NOW);
    assert_eq!(completed.updated_at, NOW);
    assert_eq!(completed.expires_at, NOW + 30 * 86_400);
    assert!(completed.historical);
    assert!(!completed.executable);

    let first_page = client.results(&job_id, 1, 0, 2).await.unwrap();
    let second_page = client.results(&job_id, 1, 2, 2).await.unwrap();
    for public in [
        serde_json::to_string(&completed).unwrap(),
        serde_json::to_string(&first_page).unwrap(),
        serde_json::to_string(&second_page).unwrap(),
    ] {
        for forbidden in [
            CREDENTIAL_REF,
            NEODB_TOKEN,
            cli_token.as_str(),
            RAW_COLUMN_MARKER,
            UPSTREAM_MARKER,
        ] {
            assert!(!public.contains(forbidden));
        }
    }
    assert_eq!(first_page.items.len(), 2);
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(first_page.offset, 0);
    assert_eq!(first_page.limit, 2);
    assert_eq!(second_page.offset, 2);
    assert_eq!(second_page.limit, 2);
    assert!(first_page.historical && second_page.historical);
    assert!(!first_page.executable && !second_page.executable);
    let items = first_page
        .items
        .iter()
        .chain(second_page.items.iter())
        .collect::<Vec<_>>();
    for (ordinal, result) in items.iter().enumerate() {
        assert_eq!(result.ordinal, ordinal as u32);
        assert_eq!(result.attempt, 1);
        assert_eq!(result.committed_at, NOW);
        assert_eq!(result.source_connector_id, "imdb:ratings_csv");
        assert_eq!(result.source_record_id, IDS[ordinal]);
        assert!(result.source_fingerprint.starts_with("imdb:ratings_csv:"));
        assert_eq!(
            result.source_fingerprint.len(),
            "imdb:ratings_csv:".len() + 64
        );
        assert_eq!(result.profile_id, "imdb_ratings");
        assert_eq!(result.profile_version, 1);
        assert_eq!(result.base_url, upstream_base);
        assert_eq!(result.account, ACCOUNT);
        assert!(result.historical);
        assert!(!result.executable);
        let PreviewOutcomeView::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } = &result.outcome
        else {
            panic!("every native record must have a comparison")
        };
        assert_eq!(*observed_at, NOW);
        assert_eq!(comparison.profile_id, "imdb_ratings");
        assert_eq!(comparison.profile_version, 1);
        assert!(matches!(provenance, PreviewProvenanceView::Automatic));
        assert_eq!(item.uuid, UUIDS[ordinal]);
        assert_eq!(
            item.canonical_url,
            format!("{upstream_base}/movie/{}", UUIDS[ordinal])
        );
        assert_eq!(
            item.detail_url,
            format!("{upstream_base}/api/movie/{}", UUIDS[ordinal])
        );
        assert_eq!(item.external_ids.len(), 1);
        assert_eq!(item.external_ids[0].scheme, "imdb:title");
        assert_eq!(item.external_ids[0].value, IDS[ordinal]);
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .unwrap();
        assert_eq!(rating.field, "rating_grade");
        assert!(matches!(
            &rating.suggestion,
            Some(ScalarValue::Number(value)) if *value == [8.0, 7.0, 6.0][ordinal]
        ));
        match ordinal {
            0 => {
                assert!(matches!(
                    rating.sink,
                    PreviewSinkView::Known {
                        value: ScalarValue::Number(8.0)
                    }
                ));
                assert!(matches!(
                    rating.status,
                    PreviewComparisonStatusView::Skipped
                ));
                assert_eq!(rating.reason, "already_equal");
            }
            1 => {
                assert!(matches!(
                    rating.sink,
                    PreviewSinkView::Known {
                        value: ScalarValue::Number(5.0)
                    }
                ));
                assert!(matches!(
                    rating.status,
                    PreviewComparisonStatusView::NeedsReview
                ));
                assert_eq!(rating.reason, "sink_conflict_needs_review");
            }
            _ => {
                assert!(matches!(rating.sink, PreviewSinkView::Unknown));
                assert!(matches!(
                    rating.status,
                    PreviewComparisonStatusView::NeedsReview
                ));
                assert_eq!(rating.reason, "rating_requires_shelf");
            }
        }
    }

    let observations = storage.observations(job_id.clone(), 0, 100).await.unwrap();
    assert_eq!(observations.len(), 3);
    for (ordinal, observation) in observations.iter().enumerate() {
        assert_eq!(observation.source_record_id, IDS[ordinal]);
        assert_eq!(observation.observed_at, NOW.to_string());
        assert_eq!(observation.state.len(), 1);
        assert!(matches!(
            &observation.state["rating"],
            ScalarValue::Number(value) if *value == [8.0, 7.0, 6.0][ordinal]
        ));
        assert_eq!(observation.item.external_ids.len(), 1);
        assert_eq!(observation.item.external_ids["imdb:title"], IDS[ordinal]);
        assert_eq!(
            observation.item.title_hint.as_deref(),
            Some(["Equal", "中文标题", "Missing"][ordinal])
        );
        assert!(
            !serde_json::to_string(observation)
                .unwrap()
                .contains(RAW_COLUMN_MARKER)
        );
    }
    let stored = storage.results(job_id.clone(), 1, 0, 100).await.unwrap();
    assert_eq!(stored.len(), 3);
    for (ordinal, result) in stored.iter().enumerate() {
        assert_eq!(result.ordinal(), ordinal as u32);
        assert_eq!(result.attempt(), 1);
        assert_eq!(result.committed_at(), NOW);
        assert_eq!(result.source_connector_id(), "imdb:ratings_csv");
        assert_eq!(result.source_record_id(), IDS[ordinal]);
        assert_eq!(
            result.source_fingerprint(),
            observations[ordinal].idempotency_key
        );
        assert_eq!(result.profile_id(), "imdb_ratings");
        assert_eq!(result.profile_version(), 1);
        assert_eq!(result.base_url(), upstream_base);
        assert_eq!(result.account(), ACCOUNT);
        let StoredPreviewOutcome::Compared {
            observed_at,
            comparison,
            provenance,
            ..
        } = result.outcome()
        else {
            panic!("stored native result must remain compared")
        };
        assert_eq!(*observed_at, NOW);
        assert_eq!(comparison.profile_id, "imdb_ratings");
        assert_eq!(comparison.profile_version, 1);
        assert!(matches!(provenance, PreviewRecordProvenance::Automatic));
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .unwrap();
        assert!(matches!(
            &rating.suggestion,
            Some(ScalarValue::Number(value)) if *value == [8.0, 7.0, 6.0][ordinal]
        ));
        match ordinal {
            0 => {
                assert!(matches!(
                    rating.sink,
                    SinkValue::Known(ScalarValue::Number(8.0))
                ));
                assert_eq!(rating.status, ComparisonStatus::Skipped);
            }
            1 => {
                assert!(matches!(
                    rating.sink,
                    SinkValue::Known(ScalarValue::Number(5.0))
                ));
                assert_eq!(rating.status, ComparisonStatus::NeedsReview);
            }
            _ => {
                assert!(matches!(rating.sink, SinkValue::Unknown));
                assert_eq!(rating.status, ComparisonStatus::NeedsReview);
            }
        }
    }
    let stored_job = PreviewJobRepository::load(&storage, job_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_job.status(), PreviewJobStatus::Completed);
    assert_eq!(stored_job.attempt(), 1);
    assert_eq!(stored_job.profile_version(), 1);
    assert_eq!(stored_job.created_at(), NOW);
    assert_eq!(stored_job.updated_at(), NOW);
    assert_eq!(stored_job.expires_at(), NOW + 30 * 86_400);
    assert_eq!(stored_job.observation_count(), 3);
    assert!(stored_job.is_bound_to(&storage.endpoint().await.unwrap().unwrap()));
    assert_eq!(secrets.neodb_reads.load(Ordering::SeqCst), 3);
    assert_eq!(secrets.neodb_writes.load(Ordering::SeqCst), 0);
    assert_eq!(secrets.client_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_deletes.load(Ordering::SeqCst), 1);
    {
        let secret_values = secrets.values.lock().unwrap();
        assert_eq!(secret_values.len(), 2);
        assert_eq!(
            secret_values.get(CREDENTIAL_REF).unwrap().expose_secret(),
            NEODB_TOKEN
        );
    }

    let requests = upstream_state.requests.lock().unwrap().clone();
    let expected_paths = vec![
        format!(
            "/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{}%2F",
            IDS[0]
        ),
        format!("/api/movie/{}", UUIDS[0]),
        format!("/api/me/shelf/item/{}", UUIDS[0]),
        format!(
            "/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{}%2F",
            IDS[1]
        ),
        format!("/api/movie/{}", UUIDS[1]),
        format!("/api/me/shelf/item/{}", UUIDS[1]),
        format!(
            "/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{}%2F",
            IDS[2]
        ),
        format!("/api/movie/{}", UUIDS[2]),
        format!("/api/me/shelf/item/{}", UUIDS[2]),
        "/api/me".into(),
    ];
    assert_eq!(
        requests
            .iter()
            .map(|request| &request.uri)
            .collect::<Vec<_>>(),
        expected_paths.iter().collect::<Vec<_>>()
    );
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request.method, "GET");
        assert!(!request.cookie);
        if matches!(index, 2 | 5 | 8 | 9) {
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer neodb-fixture-token-e5a")
            );
        } else {
            assert!(
                request.authorization.is_none(),
                "public catalog request {index} carried authorization"
            );
        }
        assert_ne!(
            request.authorization.as_deref(),
            Some(format!("Bearer {cli_token}").as_str())
        );
    }

    runtime.shutdown().await;
    maintenance.wait().await.unwrap();
    let _ = core_shutdown_tx.send(());
    core_server.await.unwrap();
    storage.close().await;

    let reopened = Storage::open(temp.path()).await.unwrap();
    let reopened_job = PreviewJobRepository::load(&reopened, job_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reopened_job.status(), PreviewJobStatus::Completed);
    assert_eq!(reopened_job.profile_version(), 1);
    assert_eq!(reopened.results(job_id, 1, 0, 100).await.unwrap().len(), 3);
    reopened.close().await;

    for name in ["collector.db", "collector.db-wal"] {
        let path = temp.path().join(name);
        if name == "collector.db-wal" && !path.exists() {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        for forbidden in [
            RAW_COLUMN_MARKER.as_bytes(),
            UPSTREAM_MARKER.as_bytes(),
            NEODB_TOKEN.as_bytes(),
            cli_token.as_bytes(),
        ] {
            assert!(
                !bytes
                    .windows(forbidden.len())
                    .any(|window| window == forbidden)
            );
        }
    }

    let _ = upstream_shutdown_tx.send(());
    upstream_server.await.unwrap();
}

#[tokio::test]
async fn cli_import_uses_real_native_neodb_adapters_and_persists_bounded_history() {
    tokio::time::timeout(Duration::from_secs(15), run_scenario())
        .await
        .expect("E5a CLI native acceptance must finish within its outer bound");
}
