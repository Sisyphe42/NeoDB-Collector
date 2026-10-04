use super::*;
use collector_core::preview_jobs::PreviewResolutionInput;

const POLL_ID: &str = "tt4444444";
const POLL_UUID: &str = "Movie4444444";
const SEARCH_ID: &str = "tt5555555";
const SEARCH_UUID: &str = "Movie5555555";
const NATIVE_ID: &str = "tt6666666";
const NATIVE_UUID: &str = "Movie6666666";
const LIMITED_ID: &str = "tt7777777";
const MANUAL_CONFLICT_ID: &str = "tt8888888";
const MANUAL_CONFLICT_UUID: &str = "Movie8888888";
const RESOLUTION_RAW_MARKER: &str = "DISCARDED_RAW_RESOLUTION_E5C";
const RESOLUTION_UPSTREAM_MARKER: &str = "DISCARDED_UPSTREAM_RESOLUTION_E5C";

fn assert_public_redacted<T: serde::Serialize>(value: &T, extra_markers: &[&str]) {
    let public = serde_json::to_string(value).unwrap();
    for forbidden in [CREDENTIAL_REF, NEODB_TOKEN, RESOLUTION_RAW_MARKER]
        .into_iter()
        .chain(extra_markers.iter().copied())
    {
        assert!(!public.contains(forbidden));
    }
}

async fn assert_fixture_preserved(fixture: &CoreFixture, base: &str, expected_neodb_reads: usize) {
    let endpoint = fixture.storage.endpoint().await.unwrap().unwrap();
    assert_eq!(endpoint.base_url, base);
    assert_eq!(endpoint.account, ACCOUNT);
    assert_eq!(endpoint.credential_ref, CREDENTIAL_REF);
    let v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .unwrap()
        .unwrap();
    assert!(v1 == imdb_default());
    let v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v2.version, 2);
    assert!(!v2.enabled);
    assert!(matches!(
        &v2.rules[0].operation,
        Operation::Clamp { max, .. } if *max == 5.0
    ));
    assert_eq!(
        fixture.secrets.neodb_reads.load(Ordering::SeqCst),
        expected_neodb_reads
    );
    assert_eq!(fixture.secrets.neodb_writes.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.secrets.client_sets.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.secrets.probe_sets.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.secrets.probe_deletes.load(Ordering::SeqCst), 1);
    let saved_secrets = fixture.secrets.values.lock().unwrap();
    assert_eq!(saved_secrets.len(), 2);
    assert_eq!(
        saved_secrets
            .get(CREDENTIAL_REF)
            .map(|value| value.expose_secret()),
        Some(NEODB_TOKEN)
    );
    assert_eq!(
        saved_secrets
            .iter()
            .find(|(reference, _)| reference.starts_with("collector-cli:"))
            .map(|(_, value)| value.expose_secret()),
        Some(fixture.cli_token.as_str())
    );
}

fn assert_closed_database(
    temp: &tempfile::TempDir,
    cli_token: &str,
    raw_marker: &str,
    upstream_marker: &str,
) {
    let database = temp.path().join("collector.db");
    assert!(
        database.is_file(),
        "the closed primary database is mandatory"
    );
    for path in [database, temp.path().join("collector.db-wal")] {
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        for forbidden in [raw_marker, upstream_marker, NEODB_TOKEN, cli_token] {
            assert!(
                !bytes
                    .windows(forbidden.len())
                    .any(|window| window == forbidden.as_bytes())
            );
        }
    }
}

fn assert_equal_comparison(
    row: &collector_domain::preview::PreviewResultView,
    source_id: &str,
    uuid: &str,
    base: &str,
    manual: bool,
) {
    assert_eq!(row.attempt, 1);
    assert_eq!(row.committed_at, NOW);
    assert_eq!(row.source_connector_id, "imdb:ratings_csv");
    assert_eq!(row.source_record_id, source_id);
    assert_eq!(row.profile_id, "imdb_ratings");
    assert_eq!(row.profile_version, 1);
    assert_eq!(row.base_url, base);
    assert_eq!(row.account, ACCOUNT);
    assert!(row.historical && !row.executable);
    let PreviewOutcomeView::Compared {
        item,
        observed_at,
        comparison,
        provenance,
    } = &row.outcome
    else {
        panic!("expected a compared preview row")
    };
    assert_eq!(item.uuid, uuid);
    assert_eq!(*observed_at, NOW);
    assert_eq!(comparison.profile_id, "imdb_ratings");
    assert_eq!(comparison.profile_version, 1);
    assert_eq!(matches!(provenance, PreviewProvenanceView::Manual), manual);
    let rating = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating")
        .unwrap();
    assert!(matches!(rating.suggestion, Some(ScalarValue::Number(8.0))));
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

#[derive(Clone)]
struct TimedRequest {
    seen: SeenRequest,
    received_at: Instant,
    response_prepared_at: Instant,
}

struct PollState {
    base: String,
    requests: Mutex<Vec<TimedRequest>>,
    first_fetch_seen: AtomicBool,
    first_fetch: Notify,
    fetches: AtomicUsize,
}

impl PollState {
    async fn wait_for_first_fetch(&self) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let notified = self.first_fetch.notified();
                if self.first_fetch_seen.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
        })
        .await
        .expect("the first native fetch must arrive");
    }

    fn record(&self, request: &Request<Body>, received_at: Instant, response_prepared_at: Instant) {
        self.requests.lock().unwrap().push(TimedRequest {
            seen: SeenRequest {
                method: request.method().to_string(),
                uri: request.uri().to_string(),
                authorization: request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned),
                cookie: request.headers().contains_key(header::COOKIE),
            },
            received_at,
            response_prepared_at,
        });
    }
}

fn poll_item(base: &str) -> Value {
    json!({
        "id": format!("{base}/movie/{POLL_UUID}"),
        "uuid": POLL_UUID,
        "api_url": format!("/api/movie/{POLL_UUID}"),
        "category": "movie",
        "type": "Movie",
        "parent_uuid": null,
        "imdb": POLL_ID,
        "external_resources": [{"url": format!("https://www.imdb.com/title/{POLL_ID}/")}],
        "unsupported_payload": RESOLUTION_UPSTREAM_MARKER
    })
}

async fn polling_upstream(State(state): State<Arc<PollState>>, request: Request<Body>) -> Response {
    let received_at = Instant::now();
    let path = request.uri().path().to_owned();
    let response = if path == "/api/catalog/fetch" {
        let fetch = state.fetches.fetch_add(1, Ordering::SeqCst);
        if fetch == 0 {
            state.first_fetch_seen.store(true, Ordering::SeqCst);
            state.first_fetch.notify_waiters();
            StatusCode::ACCEPTED.into_response()
        } else {
            (
                StatusCode::FOUND,
                [(header::LOCATION, format!("/api/movie/{POLL_UUID}"))],
            )
                .into_response()
        }
    } else if path == format!("/api/movie/{POLL_UUID}") {
        Json(poll_item(&state.base)).into_response()
    } else if path == format!("/api/me/shelf/item/{POLL_UUID}") {
        Json(json!({
            "item": poll_item(&state.base),
            "owner": {"username": ACCOUNT},
            "shelf_type": "complete",
            "rating_grade": 8,
            "unsupported_payload": RESOLUTION_UPSTREAM_MARKER
        }))
        .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    };
    state.record(&request, received_at, Instant::now());
    response
}

async fn wait_for_status(
    client: &PreviewClient,
    job_id: &str,
    expected: PreviewJobStatusView,
    bound: Duration,
) -> collector_domain::preview::PreviewJobView {
    tokio::time::timeout(bound, async {
        loop {
            let status = client.status(job_id).await.unwrap();
            if status.status == expected {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("preview must reach the expected lifecycle")
}

async fn run_polling_success() {
    let temp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(PollState {
        base: base.clone(),
        requests: Mutex::new(Vec::new()),
        first_fetch_seen: AtomicBool::new(false),
        first_fetch: Notify::new(),
        fetches: AtomicUsize::new(0),
    });
    let app = Router::new()
        .fallback(polling_upstream)
        .with_state(state.clone());
    let (upstream_shutdown_tx, upstream_shutdown_rx) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = upstream_shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let fixture = CoreFixture::start(&temp, &base, "E5c polling CLI").await;
    let csv = format!(
        "Const,Your Rating,Title Type,Title,Raw Column\r\n{POLL_ID},8,movie,Polling,{RESOLUTION_RAW_MARKER}\r\n"
    );
    let accepted = fixture
        .client
        .import_imdb_ratings(csv.into_bytes(), "imdb_ratings", 1)
        .await
        .unwrap();
    state.wait_for_first_fetch().await;
    let running = fixture
        .client
        .status(&accepted.preview.job_id)
        .await
        .unwrap();
    assert!(matches!(running.status, PreviewJobStatusView::Running));
    assert_public_redacted(&running, &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token]);
    assert!(
        fixture
            .client
            .results(&accepted.preview.job_id, 1, 0, 100)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(fixture.secrets.neodb_reads.load(Ordering::SeqCst), 0);

    let terminal = wait_for_status(
        &fixture.client,
        &accepted.preview.job_id,
        PreviewJobStatusView::Completed,
        Duration::from_secs(22),
    )
    .await;
    assert!(matches!(terminal.status, PreviewJobStatusView::Completed));
    assert_eq!(terminal.created_at, NOW);
    assert_eq!(terminal.updated_at, NOW);
    assert_eq!(terminal.expires_at, NOW + 30 * 86_400);
    assert_eq!(terminal.profile_version, 1);
    let page = fixture
        .client
        .results(&accepted.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_public_redacted(&terminal, &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token]);
    assert_public_redacted(&page, &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token]);
    assert_equal_comparison(&page.items[0], POLL_ID, POLL_UUID, &base, false);
    let PreviewOutcomeView::Compared {
        item,
        observed_at,
        comparison,
        provenance,
    } = &page.items[0].outcome
    else {
        panic!("polling must finish with a real comparison")
    };
    assert_eq!(item.uuid, POLL_UUID);
    assert_eq!(*observed_at, NOW);
    assert_eq!(comparison.profile_version, 1);
    assert!(matches!(provenance, PreviewProvenanceView::Automatic));
    let rating = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating")
        .unwrap();
    assert!(matches!(
        rating.status,
        PreviewComparisonStatusView::Skipped
    ));
    assert_eq!(rating.reason, "already_equal");

    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 4);
    assert!(
        requests[1]
            .received_at
            .duration_since(requests[0].response_prepared_at)
            >= Duration::from_secs(15)
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.seen.uri.as_str())
            .collect::<Vec<_>>(),
        vec![
            format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F"),
            format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F"),
            format!("/api/movie/{POLL_UUID}"),
            format!("/api/me/shelf/item/{POLL_UUID}"),
        ]
    );
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request.seen.method, "GET");
        assert!(!request.seen.cookie);
        if index == 3 {
            assert_eq!(
                request.seen.authorization.as_deref(),
                Some("Bearer neodb-fixture-token-e5a")
            );
        } else {
            assert!(request.seen.authorization.is_none());
        }
        assert!(request.response_prepared_at >= request.received_at);
    }
    assert_eq!(fixture.secrets.neodb_reads.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.secrets.neodb_writes.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture
            .storage
            .resolution_input(accepted.preview.job_id.clone(), 0)
            .await
            .unwrap(),
        PreviewResolutionInput::Native
    );
    let observations = fixture
        .storage
        .observations(accepted.preview.job_id.clone(), 0, 100)
        .await
        .unwrap();
    assert!(
        !serde_json::to_string(&observations)
            .unwrap()
            .contains(RESOLUTION_RAW_MARKER)
    );
    assert_fixture_preserved(&fixture, &base, 1).await;
    let cli_token = fixture.cli_token.clone();
    fixture.shutdown().await;
    let drained_requests = state.requests.lock().unwrap().clone();
    assert_eq!(drained_requests.len(), 4);
    assert_eq!(
        drained_requests
            .iter()
            .map(|request| &request.seen.uri)
            .collect::<Vec<_>>(),
        requests
            .iter()
            .map(|request| &request.seen.uri)
            .collect::<Vec<_>>()
    );
    assert_closed_database(
        &temp,
        &cli_token,
        RESOLUTION_RAW_MARKER,
        RESOLUTION_UPSTREAM_MARKER,
    );
    let _ = upstream_shutdown_tx.send(());
    upstream_server.await.unwrap();
}

async fn run_polling_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(PollState {
        base: base.clone(),
        requests: Mutex::new(Vec::new()),
        first_fetch_seen: AtomicBool::new(false),
        first_fetch: Notify::new(),
        fetches: AtomicUsize::new(0),
    });
    let app = Router::new()
        .fallback(polling_upstream)
        .with_state(state.clone());
    let (upstream_shutdown_tx, upstream_shutdown_rx) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = upstream_shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let fixture = CoreFixture::start(&temp, &base, "E5c cancellation CLI").await;
    let csv = format!(
        "Const,Your Rating,Title Type,Title,Raw Column\r\n{POLL_ID},8,movie,Cancel,{RESOLUTION_RAW_MARKER}\r\n"
    );
    let accepted = fixture
        .client
        .import_imdb_ratings(csv.into_bytes(), "imdb_ratings", 1)
        .await
        .unwrap();
    state.wait_for_first_fetch().await;
    assert_eq!(fixture.secrets.neodb_reads.load(Ordering::SeqCst), 0);
    assert!(
        fixture
            .client
            .cancel(&accepted.preview.job_id, 1)
            .await
            .unwrap()
            .applied
    );
    let cancelled = wait_for_status(
        &fixture.client,
        &accepted.preview.job_id,
        PreviewJobStatusView::Cancelled,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(cancelled.attempt, 1);
    let page = fixture
        .client
        .results(&accepted.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert!(page.items.is_empty());
    assert_public_redacted(
        &cancelled,
        &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token],
    );
    assert_public_redacted(&page, &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token]);
    let stored = PreviewJobRepository::load(&fixture.storage, accepted.preview.job_id.clone())
        .await
        .unwrap();
    assert!(
        stored.is_some(),
        "cancellation retains local historical job metadata"
    );
    assert_eq!(
        fixture
            .storage
            .resolution_input(accepted.preview.job_id.clone(), 0)
            .await
            .unwrap(),
        PreviewResolutionInput::Native
    );
    let observations = fixture
        .storage
        .observations(accepted.preview.job_id.clone(), 0, 100)
        .await
        .unwrap();
    assert!(
        !serde_json::to_string(&observations)
            .unwrap()
            .contains(RESOLUTION_RAW_MARKER)
    );
    assert_fixture_preserved(&fixture, &base, 0).await;
    let cli_token = fixture.cli_token.clone();
    fixture.shutdown().await;
    let requests = state.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].seen.uri,
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F")
    );
    assert_eq!(requests[0].seen.method, "GET");
    assert!(requests[0].seen.authorization.is_none());
    assert!(!requests[0].seen.cookie);
    assert_closed_database(
        &temp,
        &cli_token,
        RESOLUTION_RAW_MARKER,
        RESOLUTION_UPSTREAM_MARKER,
    );
    let _ = upstream_shutdown_tx.send(());
    upstream_server.await.unwrap();
}

struct ResolutionState {
    base: String,
    requests: Mutex<Vec<SeenRequest>>,
}

fn resolution_item(base: &str, uuid: &str, imdb: Option<&str>) -> Value {
    json!({
        "id": format!("{base}/movie/{uuid}"),
        "uuid": uuid,
        "api_url": format!("/api/movie/{uuid}"),
        "category": "movie",
        "type": "Movie",
        "parent_uuid": null,
        "imdb": imdb,
        "external_resources": imdb.map(|id| vec![json!({"url": format!("https://www.imdb.com/title/{id}/")})]).unwrap_or_default(),
        "title": "Exact review candidate",
        "unsupported_payload": RESOLUTION_UPSTREAM_MARKER
    })
}

async fn resolution_upstream(
    State(state): State<Arc<ResolutionState>>,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path().to_owned();
    let uri = request.uri().to_string();
    state.requests.lock().unwrap().push(SeenRequest {
        method: request.method().to_string(),
        uri,
        authorization: request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        cookie: request.headers().contains_key(header::COOKIE),
    });
    if path == "/api/catalog/fetch" {
        let query = request.uri().query().unwrap_or_default();
        if query.contains(SEARCH_ID) || query.contains(MANUAL_CONFLICT_ID) {
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
        if query.contains(LIMITED_ID) {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        if query.contains(NATIVE_ID) {
            return (
                StatusCode::FOUND,
                [(header::LOCATION, format!("/api/movie/{NATIVE_UUID}"))],
            )
                .into_response();
        }
    }
    if path == "/api/catalog/search" {
        return Json(json!({
            "data": [resolution_item(&state.base, SEARCH_UUID, Some(SEARCH_ID))],
            "pages": 1,
            "count": 1
        }))
        .into_response();
    }
    if path == format!("/api/movie/{SEARCH_UUID}") {
        return Json(resolution_item(&state.base, SEARCH_UUID, None)).into_response();
    }
    if path == format!("/api/movie/{MANUAL_CONFLICT_UUID}") {
        return Json(resolution_item(
            &state.base,
            MANUAL_CONFLICT_UUID,
            Some("tt9999999"),
        ))
        .into_response();
    }
    if path == format!("/api/movie/{NATIVE_UUID}") {
        return Json(resolution_item(&state.base, NATIVE_UUID, Some(NATIVE_ID))).into_response();
    }
    if path.starts_with("/api/me/shelf/item/") {
        let uuid = path.rsplit('/').next().unwrap();
        let imdb = if uuid == SEARCH_UUID {
            None
        } else {
            Some(NATIVE_ID)
        };
        return Json(json!({
            "item": resolution_item(&state.base, uuid, imdb),
            "owner": {"username": ACCOUNT},
            "shelf_type": "complete",
            "rating_grade": 8,
            "unsupported_payload": RESOLUTION_UPSTREAM_MARKER
        }))
        .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn structured_submit(
    fixture: &CoreFixture,
    temp: &tempfile::TempDir,
    stem: &str,
    csv: &str,
    choices: Value,
) -> collector_domain::preview::PreviewImportAccepted {
    let csv_path = temp.path().join(format!("{stem}.csv"));
    let choices_path = temp.path().join(format!("{stem}.json"));
    fs::write(&csv_path, csv.as_bytes()).unwrap();
    fs::write(&choices_path, serde_json::to_vec(&choices).unwrap()).unwrap();
    let (client, encoded) = PreviewClient::connect_with_resolution_request(
        &fixture.report.core_url,
        fixture.secrets.as_ref(),
        csv_path.to_str().unwrap(),
        choices_path.to_str().unwrap(),
    )
    .await
    .unwrap();
    client
        .import_imdb_ratings_with_resolution(encoded, "imdb_ratings", 1)
        .await
        .unwrap()
}

async fn run_resolution_choices() {
    let temp = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let state = Arc::new(ResolutionState {
        base: base.clone(),
        requests: Mutex::new(Vec::new()),
    });
    let app = Router::new()
        .fallback(resolution_upstream)
        .with_state(state.clone());
    let (upstream_shutdown_tx, upstream_shutdown_rx) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = upstream_shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    let fixture = CoreFixture::start(&temp, &base, "E5c resolution CLI").await;
    let search_csv = format!(
        "\u{feff}Const,Your Rating,Title Type,Title,Raw Column\r\n{SEARCH_ID},8,movie,Search,{RESOLUTION_RAW_MARKER}\r\n{NATIVE_ID},8,movie,Native,{RESOLUTION_RAW_MARKER}\r\n{LIMITED_ID},8,movie,Limited,{RESOLUTION_RAW_MARKER}\r\n"
    );
    let search = structured_submit(
        &fixture,
        &temp,
        "search",
        &search_csv,
        json!([
            {"kind":"search","ordinal":0,"sourceRecordId":SEARCH_ID,"query":"  Exact candidate  "},
            {"kind":"search","ordinal":1,"sourceRecordId":NATIVE_ID,"query":"Must not run"},
            {"kind":"search","ordinal":2,"sourceRecordId":LIMITED_ID,"query":"Must not run either"}
        ]),
    )
    .await;
    let original_status = wait_for_status(
        &fixture.client,
        &search.preview.job_id,
        PreviewJobStatusView::Completed,
        Duration::from_secs(5),
    )
    .await;
    let search_page = fixture
        .client
        .results(&search.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(search_page.items.len(), 3);
    let PreviewOutcomeView::SearchCandidates {
        candidates,
        more_available,
    } = &search_page.items[0].outcome
    else {
        panic!("unsupported native row must remain review-only search candidates")
    };
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].title, "Exact review candidate");
    assert_eq!(candidates[0].item.uuid, SEARCH_UUID);
    assert!(
        candidates[0]
            .item
            .external_ids
            .iter()
            .any(|claim| claim.scheme == "imdb:title" && claim.value == SEARCH_ID)
    );
    assert!(!more_available);
    assert!(matches!(
        search_page.items[1].outcome,
        PreviewOutcomeView::Compared {
            provenance: PreviewProvenanceView::Automatic,
            ..
        }
    ));
    assert!(matches!(
        search_page.items[2].outcome,
        PreviewOutcomeView::RateLimited
    ));
    assert_equal_comparison(&search_page.items[1], NATIVE_ID, NATIVE_UUID, &base, false);
    assert_public_redacted(
        &original_status,
        &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token],
    );
    assert_public_redacted(
        &search_page,
        &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token],
    );
    assert_eq!(
        fixture
            .storage
            .resolution_input(search.preview.job_id.clone(), 0)
            .await
            .unwrap(),
        PreviewResolutionInput::Search("Exact candidate".into())
    );
    assert_eq!(
        fixture
            .storage
            .resolution_input(search.preview.job_id.clone(), 1)
            .await
            .unwrap(),
        PreviewResolutionInput::Search("Must not run".into())
    );
    assert_eq!(
        fixture
            .storage
            .resolution_input(search.preview.job_id.clone(), 2)
            .await
            .unwrap(),
        PreviewResolutionInput::Search("Must not run either".into())
    );

    let reads_after_search = fixture.secrets.neodb_reads.load(Ordering::SeqCst);
    assert_eq!(
        reads_after_search, 1,
        "only the independently native match may read a credential"
    );
    let manual_csv =
        format!("Const,Your Rating,Title Type,Title\r\n{SEARCH_ID},8,movie,Manual\r\n");
    let manual = structured_submit(&fixture, &temp, "manual", &manual_csv, json!([
        {"kind":"manual","ordinal":0,"sourceRecordId":SEARCH_ID,"item":{"uuid":SEARCH_UUID,"mediaType":"movie","model":"Movie"}}
    ])).await;
    assert_ne!(manual.preview.job_id, search.preview.job_id);
    wait_for_status(
        &fixture.client,
        &manual.preview.job_id,
        PreviewJobStatusView::Completed,
        Duration::from_secs(5),
    )
    .await;
    let manual_page = fixture
        .client
        .results(&manual.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert!(matches!(
        manual_page.items[0].outcome,
        PreviewOutcomeView::Compared {
            provenance: PreviewProvenanceView::Manual,
            ..
        }
    ));
    assert_equal_comparison(&manual_page.items[0], SEARCH_ID, SEARCH_UUID, &base, true);
    let PreviewOutcomeView::Compared { item, .. } = &manual_page.items[0].outcome else {
        unreachable!()
    };
    assert!(item.external_ids.is_empty());
    assert_eq!(fixture.secrets.neodb_reads.load(Ordering::SeqCst), 2);
    assert_public_redacted(
        &manual_page,
        &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token],
    );
    assert_eq!(
        fixture
            .storage
            .resolution_input(manual.preview.job_id.clone(), 0)
            .await
            .unwrap(),
        PreviewResolutionInput::Manual(
            collector_core::catalog::CatalogReference::from_stored(
                SEARCH_UUID.into(),
                "movie".into(),
                "Movie".into()
            )
            .unwrap()
        )
    );

    let conflict_csv =
        format!("Const,Your Rating,Title Type,Title\r\n{MANUAL_CONFLICT_ID},8,movie,Conflict\r\n");
    let before_conflict_reads = fixture.secrets.neodb_reads.load(Ordering::SeqCst);
    let conflict = structured_submit(&fixture, &temp, "conflict", &conflict_csv, json!([
        {"kind":"manual","ordinal":0,"sourceRecordId":MANUAL_CONFLICT_ID,"item":{"uuid":MANUAL_CONFLICT_UUID,"mediaType":"movie","model":"Movie"}}
    ])).await;
    wait_for_status(
        &fixture.client,
        &conflict.preview.job_id,
        PreviewJobStatusView::Completed,
        Duration::from_secs(5),
    )
    .await;
    let conflict_page = fixture
        .client
        .results(&conflict.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert!(
        matches!(&conflict_page.items[0].outcome, PreviewOutcomeView::NeedsReview { reason, .. } if reason == "item_identity_conflict")
    );
    let PreviewOutcomeView::NeedsReview { item, .. } = &conflict_page.items[0].outcome else {
        unreachable!()
    };
    assert!(
        item.external_ids
            .iter()
            .any(|claim| claim.scheme == "imdb:title" && claim.value == "tt9999999")
    );
    assert_public_redacted(
        &conflict_page,
        &[RESOLUTION_UPSTREAM_MARKER, &fixture.cli_token],
    );
    assert_eq!(
        fixture.secrets.neodb_reads.load(Ordering::SeqCst),
        before_conflict_reads
    );
    assert!(
        fixture
            .client
            .results(&search.preview.job_id, 1, 0, 100)
            .await
            .unwrap()
            == search_page
    );
    assert!(fixture.client.status(&search.preview.job_id).await.unwrap() == original_status);
    assert_eq!(
        fixture
            .storage
            .resolution_input(conflict.preview.job_id.clone(), 0)
            .await
            .unwrap(),
        PreviewResolutionInput::Manual(
            collector_core::catalog::CatalogReference::from_stored(
                MANUAL_CONFLICT_UUID.into(),
                "movie".into(),
                "Movie".into()
            )
            .unwrap()
        )
    );

    for id in [
        &search.preview.job_id,
        &manual.preview.job_id,
        &conflict.preview.job_id,
    ] {
        let job = PreviewJobRepository::load(&fixture.storage, id.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.profile_version(), 1);
        let endpoint = fixture.storage.endpoint().await.unwrap().unwrap();
        assert_eq!(endpoint.base_url, base);
        assert_eq!(endpoint.account, ACCOUNT);
        assert_eq!(endpoint.credential_ref, CREDENTIAL_REF);
        assert!(job.is_bound_to(&endpoint));
        let observations = fixture
            .storage
            .observations(id.clone(), 0, 100)
            .await
            .unwrap();
        assert!(
            !serde_json::to_string(&observations)
                .unwrap()
                .contains(RESOLUTION_RAW_MARKER)
        );
    }
    let requests = state.requests.lock().unwrap().clone();
    let expected_uris = vec![
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{SEARCH_ID}%2F"),
        "/api/catalog/search?query=Exact+candidate&category=movie&page=1".into(),
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{NATIVE_ID}%2F"),
        format!("/api/movie/{NATIVE_UUID}"),
        format!("/api/me/shelf/item/{NATIVE_UUID}"),
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{LIMITED_ID}%2F"),
        format!("/api/movie/{SEARCH_UUID}"),
        format!("/api/me/shelf/item/{SEARCH_UUID}"),
        format!("/api/movie/{MANUAL_CONFLICT_UUID}"),
    ];
    assert_eq!(
        requests
            .iter()
            .map(|request| &request.uri)
            .collect::<Vec<_>>(),
        expected_uris.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.uri.starts_with("/api/catalog/search?"))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.uri == format!("/api/movie/{SEARCH_UUID}"))
            .count(),
        1,
        "candidate metadata is not reused; manual rereads detail exactly once"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.uri == format!("/api/movie/{MANUAL_CONFLICT_UUID}"))
            .count(),
        1
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert!(!request.cookie);
        if request.uri.starts_with("/api/me/shelf/item/") {
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer neodb-fixture-token-e5a")
            );
        } else {
            assert!(request.authorization.is_none());
        }
    }
    assert_fixture_preserved(&fixture, &base, 2).await;
    let cli_token = fixture.cli_token.clone();
    fixture.shutdown().await;
    let drained_requests = state.requests.lock().unwrap().clone();
    assert_eq!(
        drained_requests
            .iter()
            .map(|request| &request.uri)
            .collect::<Vec<_>>(),
        expected_uris.iter().collect::<Vec<_>>()
    );
    let reopened = Storage::open(temp.path()).await.unwrap();
    assert_eq!(
        reopened
            .results(search.preview.job_id.clone(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        3
    );
    reopened.close().await;
    assert_closed_database(
        &temp,
        &cli_token,
        RESOLUTION_RAW_MARKER,
        RESOLUTION_UPSTREAM_MARKER,
    );
    let _ = upstream_shutdown_tx.send(());
    upstream_server.await.unwrap();
}

#[tokio::test]
async fn cli_native_pending_polling_uses_real_interval_before_comparison() {
    tokio::time::timeout(Duration::from_secs(25), run_polling_success())
        .await
        .unwrap();
}

#[tokio::test]
async fn cli_cancel_during_pending_drains_without_follow_up_or_result() {
    tokio::time::timeout(Duration::from_secs(10), run_polling_cancel())
        .await
        .unwrap();
}

#[tokio::test]
async fn cli_structured_search_and_fresh_manual_jobs_preserve_resolution_boundaries() {
    tokio::time::timeout(Duration::from_secs(15), run_resolution_choices())
        .await
        .unwrap();
}
