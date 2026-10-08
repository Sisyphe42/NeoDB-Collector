use super::*;
use crate::client::preview::PreviewFailure;

pub(super) const RESTART_RAW_MARKER: &str = "DISCARDED_RAW_RESTART_E5D";
pub(super) const RESTART_UPSTREAM_MARKER: &str = "DISCARDED_UPSTREAM_RESTART_E5D";
pub(super) const LATER: i64 = NOW + 60;

pub(super) struct RestartState {
    base: String,
    requests: Mutex<Vec<SeenRequest>>,
    fetches: AtomicUsize,
    changed: AtomicBool,
    initial_entered: AtomicBool,
    initial_released: AtomicBool,
    initial_completed: AtomicBool,
    initial_responded: AtomicBool,
    fresh_entered: AtomicBool,
    fresh_released: AtomicBool,
    initial_entered_notify: Notify,
    initial_released_notify: Notify,
    initial_completed_notify: Notify,
    fresh_entered_notify: Notify,
    fresh_released_notify: Notify,
}

struct InitialHandlerGuard {
    state: Arc<RestartState>,
    completed: bool,
}

impl InitialHandlerGuard {
    fn new(state: Arc<RestartState>) -> Self {
        Self {
            state,
            completed: false,
        }
    }

    fn responded(mut self) {
        self.state.initial_responded.store(true, Ordering::SeqCst);
        self.complete();
    }

    fn complete(&mut self) {
        self.completed = true;
        self.state.initial_completed.store(true, Ordering::SeqCst);
        self.state.initial_completed_notify.notify_waiters();
    }
}

impl Drop for InitialHandlerGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.complete();
        }
    }
}

impl RestartState {
    pub(super) fn new(base: String) -> Self {
        Self {
            base,
            requests: Mutex::new(Vec::new()),
            fetches: AtomicUsize::new(0),
            changed: AtomicBool::new(false),
            initial_entered: AtomicBool::new(false),
            initial_released: AtomicBool::new(false),
            initial_completed: AtomicBool::new(false),
            initial_responded: AtomicBool::new(false),
            fresh_entered: AtomicBool::new(false),
            fresh_released: AtomicBool::new(false),
            initial_entered_notify: Notify::new(),
            initial_released_notify: Notify::new(),
            initial_completed_notify: Notify::new(),
            fresh_entered_notify: Notify::new(),
            fresh_released_notify: Notify::new(),
        }
    }

    pub(super) fn requests(&self) -> Vec<SeenRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub(super) fn set_changed(&self) {
        self.changed.store(true, Ordering::SeqCst);
    }

    async fn try_wait_for(
        flag: &AtomicBool,
        notify: &Notify,
        message: &'static str,
    ) -> Result<(), &'static str> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let notified = notify.notified();
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| message)
    }

    async fn wait_for(flag: &AtomicBool, notify: &Notify, message: &'static str) {
        Self::try_wait_for(flag, notify, message)
            .await
            .expect(message);
    }

    pub(super) async fn wait_for_initial(&self) {
        Self::wait_for(
            &self.initial_entered,
            &self.initial_entered_notify,
            "the attempt-1 second fetch must arrive",
        )
        .await;
    }

    pub(super) async fn try_wait_for_initial(&self) -> Result<(), String> {
        Self::try_wait_for(
            &self.initial_entered,
            &self.initial_entered_notify,
            "the attempt-1 second fetch must arrive",
        )
        .await
        .map_err(str::to_owned)
    }

    pub(super) fn release_initial(&self) {
        self.initial_released.store(true, Ordering::SeqCst);
        self.initial_released_notify.notify_waiters();
    }

    pub(super) async fn wait_for_initial_completion(&self) -> bool {
        Self::wait_for(
            &self.initial_completed,
            &self.initial_completed_notify,
            "the held attempt-1 handler must respond or be dropped",
        )
        .await;
        self.initial_responded.load(Ordering::SeqCst)
    }

    pub(super) async fn try_wait_for_initial_completion(&self) -> Result<bool, String> {
        Self::try_wait_for(
            &self.initial_completed,
            &self.initial_completed_notify,
            "the held attempt-1 handler must respond or be dropped",
        )
        .await
        .map_err(str::to_owned)?;
        Ok(self.initial_responded.load(Ordering::SeqCst))
    }

    pub(super) async fn wait_for_fresh(&self) {
        Self::wait_for(
            &self.fresh_entered,
            &self.fresh_entered_notify,
            "the fresh attempt-2 fetch must arrive",
        )
        .await;
    }

    pub(super) async fn try_wait_for_fresh(&self) -> Result<(), String> {
        Self::try_wait_for(
            &self.fresh_entered,
            &self.fresh_entered_notify,
            "the fresh attempt-2 fetch must arrive",
        )
        .await
        .map_err(str::to_owned)
    }

    pub(super) fn release_fresh(&self) {
        self.fresh_released.store(true, Ordering::SeqCst);
        self.fresh_released_notify.notify_waiters();
    }

    async fn hold(flag: &AtomicBool, notify: &Notify) {
        loop {
            let notified = notify.notified();
            if flag.load(Ordering::SeqCst) {
                break;
            }
            notified.await;
        }
    }
}

pub(super) async fn restart_upstream(
    State(state): State<Arc<RestartState>>,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path().to_owned();
    state.requests.lock().unwrap().push(SeenRequest {
        method: request.method().to_string(),
        uri: request.uri().to_string(),
        authorization: request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        cookie: request.headers().contains_key(header::COOKIE),
    });

    if path == "/api/catalog/fetch" {
        let query = request.uri().query().unwrap_or_default();
        let index = IDS
            .iter()
            .position(|id| query.contains(id))
            .expect("fixture fetch URL contains an original IMDb ID");
        let fetch = state.fetches.fetch_add(1, Ordering::SeqCst) + 1;
        if fetch == 2 {
            let guard = InitialHandlerGuard::new(state.clone());
            state.initial_entered.store(true, Ordering::SeqCst);
            state.initial_entered_notify.notify_waiters();
            RestartState::hold(&state.initial_released, &state.initial_released_notify).await;
            let response = (
                StatusCode::FOUND,
                [(header::LOCATION, format!("/api/movie/{}", UUIDS[index]))],
            )
                .into_response();
            guard.responded();
            return response;
        }
        if fetch == 3 {
            state.fresh_entered.store(true, Ordering::SeqCst);
            state.fresh_entered_notify.notify_waiters();
            RestartState::hold(&state.fresh_released, &state.fresh_released_notify).await;
        }
        return (
            StatusCode::FOUND,
            [(header::LOCATION, format!("/api/movie/{}", UUIDS[index]))],
        )
            .into_response();
    }
    if path.starts_with("/api/movie/") {
        let index = index_for_path(&path).expect("fixture detail UUID must be known");
        return Json(json!({
            "id": format!("{}/movie/{}", state.base, UUIDS[index]),
            "uuid": UUIDS[index],
            "api_url": format!("/api/movie/{}", UUIDS[index]),
            "category": "movie",
            "type": "Movie",
            "parent_uuid": null,
            "imdb": IDS[index],
            "external_resources": [{"url": format!("https://www.imdb.com/title/{}/", IDS[index])}],
            "unsupported_payload": RESTART_UPSTREAM_MARKER
        }))
        .into_response();
    }
    if path.starts_with("/api/me/shelf/item/") {
        let index = index_for_path(&path).expect("fixture mark UUID must be known");
        let rating = if state.changed.load(Ordering::SeqCst) {
            if index == 0 { 5 } else { 4 }
        } else {
            if index == 0 { 8 } else { 7 }
        };
        return Json(json!({
            "item": {
                "id": format!("{}/movie/{}", state.base, UUIDS[index]),
                "uuid": UUIDS[index],
                "api_url": format!("/api/movie/{}", UUIDS[index]),
                "category": "movie",
                "type": "Movie",
                "parent_uuid": null,
                "imdb": IDS[index],
                "external_resources": [{"url": format!("https://www.imdb.com/title/{}/", IDS[index])}]
            },
            "owner": {"username": ACCOUNT},
            "shelf_type": "complete",
            "rating_grade": rating,
            "unsupported_payload": RESTART_UPSTREAM_MARKER
        }))
        .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn wait_for_status(
    client: &PreviewClient,
    id: &str,
    expected: PreviewJobStatusView,
) -> collector_domain::preview::PreviewJobView {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = client.status(id).await.unwrap();
            if current.status == expected {
                break current;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("preview status must settle within its bound")
}

fn assert_redacted<T: serde::Serialize>(value: &T, cli_token: &str) {
    let public = serde_json::to_string(value).unwrap();
    for forbidden in [
        CREDENTIAL_REF,
        NEODB_TOKEN,
        cli_token,
        RESTART_RAW_MARKER,
        RESTART_UPSTREAM_MARKER,
    ] {
        assert!(!public.contains(forbidden));
    }
}

fn assert_restart_database_closed(temp: &tempfile::TempDir, cli_token: &str) {
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
        for forbidden in [
            NEODB_TOKEN,
            cli_token,
            RESTART_RAW_MARKER,
            RESTART_UPSTREAM_MARKER,
        ] {
            assert!(
                !bytes
                    .windows(forbidden.len())
                    .any(|window| window == forbidden.as_bytes())
            );
        }
    }
}

fn assert_access_record_eq(
    left: &collector_core::access::AccessRecord,
    right: &collector_core::access::AccessRecord,
) {
    assert_eq!(left.id, right.id);
    assert_eq!(left.hash, right.hash);
    assert_eq!(left.kind, right.kind);
    assert_eq!(left.audience, right.audience);
    assert_eq!(left.name, right.name);
    assert_eq!(left.client_kind, right.client_kind);
    assert_eq!(left.expires_at, right.expires_at);
}

fn secret_snapshot(secrets: &FixtureSecrets) -> Vec<(String, String)> {
    let mut values = secrets
        .values
        .lock()
        .unwrap()
        .iter()
        .map(|(reference, value)| (reference.clone(), value.expose_secret().to_owned()))
        .collect::<Vec<_>>();
    values.sort();
    values
}

pub(super) async fn run_restart_resume_delete(prior_schema: Option<super::migration::PriorSchema>) {
    let temp = tempfile::tempdir().unwrap();
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_base = format!("http://{}", upstream_listener.local_addr().unwrap());
    let state = Arc::new(RestartState::new(upstream_base.clone()));
    let upstream_app = Router::new()
        .fallback(restart_upstream)
        .with_state(state.clone());
    let (upstream_shutdown_tx, upstream_shutdown_rx) = oneshot::channel();
    let upstream_server = tokio::spawn(async move {
        axum::serve(upstream_listener, upstream_app)
            .with_graceful_shutdown(async move {
                let _ = upstream_shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let fixture = CoreFixture::start(&temp, &upstream_base, "E5d restart CLI").await;
    let core_address = fixture
        .report
        .core_url
        .strip_prefix("http://")
        .unwrap()
        .to_owned();
    let secrets = fixture.secrets.clone();
    let cli_token = fixture.cli_token.clone();
    let first_identity = status(&fixture.report.core_url, secrets.as_ref())
        .await
        .unwrap();
    assert_eq!(first_identity.client.id, fixture.report.client_id);
    assert_eq!(first_identity.client.name, "E5d restart CLI");
    assert_redacted(&fixture.report, &cli_token);
    assert_redacted(&first_identity, &cli_token);
    let token_hash = SecureTokens.digest(&cli_token);
    let original_access = AccessRepository::find(
        &fixture.storage,
        token_hash.clone(),
        "client".into(),
        fixture.report.core_url.clone(),
        NOW,
    )
    .await
    .unwrap()
    .unwrap();

    let csv = format!(
        "Const,Your Rating,Title Type,Title,Raw Column\r\n\
         {},8,movie,First,{RESTART_RAW_MARKER}\r\n\
         {},7,movie,Second,{RESTART_RAW_MARKER}\r\n",
        IDS[0], IDS[1]
    );
    let accepted = fixture
        .client
        .import_imdb_ratings(csv.into_bytes(), "imdb_ratings", 1)
        .await
        .unwrap();
    assert_eq!(accepted.preview.attempt, 1);
    assert!(!accepted.preview.executable);
    assert_redacted(&accepted, &cli_token);
    state.wait_for_initial().await;
    let running = fixture
        .client
        .status(&accepted.preview.job_id)
        .await
        .unwrap();
    assert!(matches!(running.status, PreviewJobStatusView::Running));
    let initial_page = fixture
        .client
        .results(&accepted.preview.job_id, 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(initial_page.items.len(), 1);
    assert_eq!(initial_page.items[0].ordinal, 0);
    assert_eq!(initial_page.items[0].attempt, 1);
    assert_eq!(initial_page.items[0].committed_at, NOW);
    assert_eq!(
        initial_page.items[0].source_connector_id,
        "imdb:ratings_csv"
    );
    assert_eq!(initial_page.items[0].source_record_id, IDS[0]);
    assert_eq!(initial_page.items[0].profile_id, "imdb_ratings");
    assert_eq!(initial_page.items[0].profile_version, 1);
    assert_eq!(initial_page.items[0].base_url, upstream_base);
    assert_eq!(initial_page.items[0].account, ACCOUNT);
    assert!(initial_page.historical && !initial_page.executable);
    assert!(initial_page.items[0].historical && !initial_page.items[0].executable);
    assert!(matches!(
        &initial_page.items[0].outcome,
        PreviewOutcomeView::Compared {
            observed_at: NOW,
            provenance: PreviewProvenanceView::Automatic,
            ..
        }
    ));
    let PreviewOutcomeView::Compared {
        item, comparison, ..
    } = &initial_page.items[0].outcome
    else {
        unreachable!()
    };
    assert_eq!(item.uuid, UUIDS[0]);
    assert_eq!(
        item.canonical_url,
        format!("{upstream_base}/movie/{}", UUIDS[0])
    );
    assert_eq!(
        item.detail_url,
        format!("{upstream_base}/api/movie/{}", UUIDS[0])
    );
    assert_eq!(item.media_type, "movie");
    assert_eq!(item.model, "Movie");
    assert!(item.parent_uuid.is_none());
    assert!(
        item.external_ids
            .iter()
            .any(|claim| { claim.scheme == "imdb:title" && claim.value == IDS[0] })
    );
    assert_eq!(comparison.profile_id, "imdb_ratings");
    assert_eq!(comparison.profile_version, 1);
    let initial_rating = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating")
        .unwrap();
    assert!(matches!(
        initial_rating.suggestion,
        Some(ScalarValue::Number(8.0))
    ));
    assert!(matches!(
        initial_rating.sink,
        PreviewSinkView::Known {
            value: ScalarValue::Number(8.0)
        }
    ));
    assert!(matches!(
        initial_rating.status,
        PreviewComparisonStatusView::Skipped
    ));
    assert_eq!(initial_rating.reason, "already_equal");
    let original_job =
        PreviewJobRepository::load(&fixture.storage, accepted.preview.job_id.clone())
            .await
            .unwrap()
            .unwrap();
    let original_observations = fixture
        .storage
        .observations(accepted.preview.job_id.clone(), 0, 100)
        .await
        .unwrap();
    assert_eq!(
        initial_page.items[0].source_fingerprint,
        original_observations[0].idempotency_key
    );
    assert_eq!(original_observations[0].source_record_id, IDS[0]);
    assert_eq!(original_observations[1].source_record_id, IDS[1]);
    for (ordinal, observation) in original_observations.iter().enumerate() {
        assert_eq!(observation.schema_version, 1);
        assert_eq!(observation.source_connector_id, "imdb:ratings_csv");
        assert_eq!(observation.observed_at, NOW.to_string());
        assert_eq!(observation.item.media_type, "movie");
        assert_eq!(
            observation.item.canonical_url,
            format!("https://www.imdb.com/title/{}/", IDS[ordinal])
        );
        assert_eq!(observation.item.external_ids["imdb:title"], IDS[ordinal]);
        assert!(matches!(
            &observation.state["rating"],
            ScalarValue::Number(value)
                if *value == if ordinal == 0 { 8.0 } else { 7.0 }
        ));
    }
    assert!(
        !serde_json::to_string(&original_observations)
            .unwrap()
            .contains(RESTART_RAW_MARKER)
    );
    let original_v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .unwrap()
        .unwrap();
    let original_v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .unwrap()
        .unwrap();
    let original_secrets = secret_snapshot(&secrets);
    assert_eq!(original_secrets.len(), 2);
    assert!(original_v1 == imdb_default());
    assert_eq!(original_v2.version, 2);
    assert!(!original_v2.enabled);
    assert!(matches!(
        &original_v2.rules[0].operation,
        Operation::Clamp { max, .. } if *max == 5.0
    ));
    assert_eq!(
        original_secrets
            .iter()
            .find(|(reference, _)| reference == CREDENTIAL_REF)
            .map(|(_, value)| value.as_str()),
        Some(NEODB_TOKEN)
    );
    assert_eq!(original_observations.len(), 2);
    for ordinal in 0..2 {
        assert_eq!(
            fixture
                .storage
                .resolution_input(accepted.preview.job_id.clone(), ordinal)
                .await
                .unwrap(),
            collector_core::preview_jobs::PreviewResolutionInput::Native
        );
    }
    assert_redacted(&running, &cli_token);
    assert_redacted(&initial_page, &cli_token);

    let report = fixture.shutdown().await;
    state.release_initial();
    let _responded_or_dropped = state.wait_for_initial_completion().await;
    assert_eq!(state.requests().len(), 4);
    assert_restart_database_closed(&temp, &cli_token);
    let migration_proof = match prior_schema {
        Some(prior_schema) => {
            Some(super::migration::downgrade_closed_fixture(&temp, prior_schema).await)
        }
        None => None,
    };
    let closed_storage = Storage::open(temp.path()).await.unwrap();
    if let Some(proof) = &migration_proof {
        super::migration::assert_upgraded_fixture(&temp, proof).await;
    }
    let pre_recovery = PreviewJobRepository::load(&closed_storage, accepted.preview.job_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pre_recovery.status(), PreviewJobStatus::Running);
    assert_eq!(pre_recovery.attempt(), 1);
    assert_eq!(
        closed_storage
            .results(accepted.preview.job_id.clone(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    closed_storage.close().await;
    assert_restart_database_closed(&temp, &cli_token);

    let before_reopen = (
        state.requests().len(),
        secrets.neodb_reads.load(Ordering::SeqCst),
    );
    assert_eq!(before_reopen, (4, 1));
    let listener = tokio::net::TcpListener::bind(&core_address).await.unwrap();
    let fixture = CoreFixture::reopen(
        &temp,
        secrets.clone(),
        Arc::new(FixedClock(LATER)),
        listener,
        report,
        cli_token.clone(),
    )
    .await;
    assert_eq!(
        before_reopen,
        (
            state.requests().len(),
            secrets.neodb_reads.load(Ordering::SeqCst),
        ),
        "startup recovery must not touch upstream or the NeoDB credential"
    );
    let interrupted = fixture
        .client
        .status(&accepted.preview.job_id)
        .await
        .unwrap();
    assert!(matches!(
        interrupted.status,
        PreviewJobStatusView::Interrupted
    ));
    assert_eq!(interrupted.attempt, 1);
    assert_eq!(interrupted.created_at, original_job.created_at());
    assert_eq!(interrupted.expires_at, original_job.expires_at());
    assert_eq!(interrupted.profile_version, 1);
    assert_eq!(interrupted.profile_id, "imdb_ratings");
    assert_eq!(interrupted.observation_count, 2);
    assert_eq!(interrupted.base_url, upstream_base);
    assert_eq!(interrupted.account, ACCOUNT);
    assert!(matches!(
        interrupted.binding_state,
        PreviewBindingState::Current
    ));
    assert!(
        fixture
            .client
            .results(&accepted.preview.job_id, 1, 0, 100)
            .await
            .unwrap()
            == initial_page
    );
    assert!(
        fixture
            .storage
            .observations(accepted.preview.job_id.clone(), 0, 100)
            .await
            .unwrap()
            == original_observations
    );
    let reopened_endpoint = fixture.storage.endpoint().await.unwrap().unwrap();
    assert_eq!(reopened_endpoint.base_url, upstream_base);
    assert_eq!(reopened_endpoint.account, ACCOUNT);
    assert_eq!(reopened_endpoint.credential_ref, CREDENTIAL_REF);
    let reopened_job =
        PreviewJobRepository::load(&fixture.storage, accepted.preview.job_id.clone())
            .await
            .unwrap()
            .unwrap();
    assert!(reopened_job.is_bound_to(&reopened_endpoint));
    for ordinal in 0..2 {
        assert_eq!(
            fixture
                .storage
                .resolution_input(accepted.preview.job_id.clone(), ordinal)
                .await
                .unwrap(),
            collector_core::preview_jobs::PreviewResolutionInput::Native
        );
    }
    let reopened_identity = status(&fixture.report.core_url, secrets.as_ref())
        .await
        .unwrap();
    assert_eq!(reopened_identity.client.id, first_identity.client.id);
    assert_eq!(reopened_identity.client.name, first_identity.client.name);
    assert_redacted(&reopened_identity, &cli_token);
    let reopened_access = AccessRepository::find(
        &fixture.storage,
        token_hash.clone(),
        "client".into(),
        fixture.report.core_url.clone(),
        LATER,
    )
    .await
    .unwrap()
    .unwrap();
    assert_access_record_eq(&original_access, &reopened_access);
    assert!(
        MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
            .await
            .unwrap()
            .unwrap()
            == original_v1
    );
    assert!(
        MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
            .await
            .unwrap()
            .unwrap()
            == original_v2
    );
    assert_eq!(secret_snapshot(&secrets), original_secrets);
    assert_redacted(&interrupted, &cli_token);
    assert_eq!(
        (
            state.requests().len(),
            secrets.neodb_reads.load(Ordering::SeqCst),
        ),
        (4, 1),
        "authenticated recovery inspection must not trigger remote work"
    );

    state.set_changed();
    let resumed = fixture
        .client
        .resume(&accepted.preview.job_id, 1)
        .await
        .unwrap();
    assert_eq!(resumed.job_id, accepted.preview.job_id);
    assert_eq!(resumed.attempt, 2);
    assert!(!resumed.executable);
    assert_redacted(&resumed, &cli_token);
    state.wait_for_fresh().await;
    assert!(
        fixture
            .storage
            .results(accepted.preview.job_id.clone(), 1, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .storage
            .results(accepted.preview.job_id.clone(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    match fixture
        .client
        .results(&accepted.preview.job_id, 1, 0, 100)
        .await
    {
        Err(PreviewFailure::Error(error)) => assert_eq!(error.code, "preview_attempt_changed"),
        Err(PreviewFailure::Admission(_)) => {
            panic!("historical GET cannot produce admission failure")
        }
        Ok(_) => panic!("obsolete attempt-1 page must be rejected"),
    }
    state.release_fresh();
    let completed = wait_for_status(
        &fixture.client,
        &accepted.preview.job_id,
        PreviewJobStatusView::Completed,
    )
    .await;
    assert_eq!(completed.attempt, 2);
    assert_eq!(completed.created_at, original_job.created_at());
    assert_eq!(completed.expires_at, original_job.expires_at());
    let fresh_page = fixture
        .client
        .results(&accepted.preview.job_id, 2, 0, 100)
        .await
        .unwrap();
    assert_eq!(fresh_page.items.len(), 2);
    for (ordinal, row) in fresh_page.items.iter().enumerate() {
        assert_eq!(row.ordinal, ordinal as u32);
        assert_eq!(row.attempt, 2);
        assert_eq!(row.committed_at, LATER);
        assert_eq!(row.profile_version, 1);
        assert_eq!(row.profile_id, "imdb_ratings");
        assert_eq!(row.source_connector_id, "imdb:ratings_csv");
        assert_eq!(row.source_record_id, IDS[ordinal]);
        assert_eq!(
            row.source_fingerprint,
            original_observations[ordinal].idempotency_key
        );
        assert_eq!(row.base_url, upstream_base);
        assert_eq!(row.account, ACCOUNT);
        assert!(row.historical && !row.executable);
        let PreviewOutcomeView::Compared {
            item,
            observed_at,
            comparison,
            provenance,
            ..
        } = &row.outcome
        else {
            panic!("fresh original row must be compared")
        };
        assert_eq!(*observed_at, LATER);
        assert_eq!(item.uuid, UUIDS[ordinal]);
        assert_eq!(item.media_type, "movie");
        assert_eq!(item.model, "Movie");
        assert!(
            item.external_ids
                .iter()
                .any(|claim| { claim.scheme == "imdb:title" && claim.value == IDS[ordinal] })
        );
        assert_eq!(comparison.profile_version, 1);
        assert!(matches!(provenance, PreviewProvenanceView::Automatic));
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .unwrap();
        assert!(matches!(
            rating.suggestion,
            Some(ScalarValue::Number(value)) if value == if ordinal == 0 { 8.0 } else { 7.0 }
        ));
        assert!(matches!(
            rating.sink,
            PreviewSinkView::Known { value: ScalarValue::Number(value) }
                if value == if ordinal == 0 { 5.0 } else { 4.0 }
        ));
        assert!(matches!(
            rating.status,
            PreviewComparisonStatusView::NeedsReview
        ));
        assert_eq!(rating.reason, "sink_conflict_needs_review");
    }
    assert_redacted(&completed, &cli_token);
    assert_redacted(&fresh_page, &cli_token);
    assert!(
        fixture
            .storage
            .observations(accepted.preview.job_id.clone(), 0, 100)
            .await
            .unwrap()
            == original_observations
    );
    for ordinal in 0..2 {
        assert_eq!(
            fixture
                .storage
                .resolution_input(accepted.preview.job_id.clone(), ordinal)
                .await
                .unwrap(),
            collector_core::preview_jobs::PreviewResolutionInput::Native
        );
    }

    let expected_uris = vec![
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
    ];
    let requests = state.requests();
    assert_eq!(
        requests
            .iter()
            .map(|request| &request.uri)
            .collect::<Vec<_>>(),
        expected_uris.iter().collect::<Vec<_>>()
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
    let v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .unwrap()
        .unwrap();
    let v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .unwrap()
        .unwrap();
    assert!(v1 == original_v1 && v2 == original_v2);
    assert!(!v1.enabled && !v2.enabled);
    assert_eq!(secret_snapshot(&secrets), original_secrets);
    assert_eq!(secrets.neodb_reads.load(Ordering::SeqCst), 3);
    assert_eq!(secrets.neodb_writes.load(Ordering::SeqCst), 0);
    assert_eq!(secrets.client_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_deletes.load(Ordering::SeqCst), 1);

    let remote_before_delete = state.requests().len();
    let reads_before_delete = secrets.neodb_reads.load(Ordering::SeqCst);
    assert!(
        fixture
            .client
            .delete(&accepted.preview.job_id)
            .await
            .unwrap()
            .applied
    );
    assert!(
        !fixture
            .client
            .delete(&accepted.preview.job_id)
            .await
            .unwrap()
            .applied
    );
    match fixture.client.status(&accepted.preview.job_id).await {
        Err(PreviewFailure::Error(error)) => assert_eq!(error.code, "preview_job_not_found"),
        Err(PreviewFailure::Admission(_)) => panic!("status cannot produce admission failure"),
        Ok(_) => panic!("deleted job status must be not-found"),
    }
    assert!(
        PreviewJobRepository::load(&fixture.storage, accepted.preview.job_id.clone())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .storage
            .observations(accepted.preview.job_id.clone(), 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .storage
            .results(accepted.preview.job_id.clone(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.requests().len(), remote_before_delete);
    assert_eq!(
        secrets.neodb_reads.load(Ordering::SeqCst),
        reads_before_delete
    );
    let endpoint = fixture.storage.endpoint().await.unwrap().unwrap();
    assert_eq!(endpoint.base_url, upstream_base);
    assert_eq!(endpoint.account, ACCOUNT);
    assert_eq!(endpoint.credential_ref, CREDENTIAL_REF);
    assert!(
        MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
            .await
            .unwrap()
            .unwrap()
            == original_v1
    );
    assert!(
        MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
            .await
            .unwrap()
            .unwrap()
            == original_v2
    );
    assert_eq!(secret_snapshot(&secrets), original_secrets);
    let final_identity = status(&fixture.report.core_url, secrets.as_ref())
        .await
        .unwrap();
    assert_eq!(final_identity.client.id, first_identity.client.id);
    assert_redacted(&final_identity, &cli_token);
    let final_access = AccessRepository::find(
        &fixture.storage,
        token_hash,
        "client".into(),
        fixture.report.core_url.clone(),
        LATER,
    )
    .await
    .unwrap()
    .unwrap();
    assert_access_record_eq(&original_access, &final_access);

    fixture.shutdown().await;
    assert_eq!(state.requests().as_slice(), requests.as_slice());
    assert_eq!(secret_snapshot(&secrets), original_secrets);
    assert_eq!(secrets.neodb_reads.load(Ordering::SeqCst), 3);
    assert_eq!(secrets.neodb_writes.load(Ordering::SeqCst), 0);
    assert_eq!(secrets.client_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_sets.load(Ordering::SeqCst), 1);
    assert_eq!(secrets.probe_deletes.load(Ordering::SeqCst), 1);
    assert_restart_database_closed(&temp, &cli_token);
    let _ = upstream_shutdown_tx.send(());
    upstream_server.await.unwrap();
}

#[tokio::test]
async fn cli_restart_resume_and_delete_preserve_auth_and_unrelated_state() {
    tokio::time::timeout(Duration::from_secs(20), run_restart_resume_delete(None))
        .await
        .expect("E5d restart acceptance must complete within its outer bound");
}
