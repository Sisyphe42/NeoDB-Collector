//! Ignored native-browser host for retained preview failure evidence.
use super::*;

const CASES: [(Fault, &str); 6] = [
    (Fault::Fetch422, "fetch_422"),
    (Fault::Fetch429, "fetch_429"),
    (Fault::FetchForeignRedirect, "fetch_foreign_redirect"),
    (Fault::Mark401, "mark_401"),
    (Fault::Mark404Item, "mark_404_item"),
    (Fault::MarkHang, "mark_hang"),
];

struct CaseServers {
    state: Arc<FailureUpstreamState>,
    trap_requests: Arc<AtomicUsize>,
    upstream_shutdown: Option<oneshot::Sender<()>>,
    upstream_server: tokio::task::JoinHandle<std::io::Result<()>>,
    trap_shutdown: Option<oneshot::Sender<()>>,
    trap_server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl CaseServers {
    async fn start(fault: Fault) -> Result<(Self, String), String> {
        let trap_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "trap listener bind failed")?;
        let trap_requests = Arc::new(AtomicUsize::new(0));
        let trap_count = trap_requests.clone();
        let trap_app = Router::new().fallback(move || {
            let trap_count = trap_count.clone();
            async move {
                trap_count.fetch_add(1, Ordering::SeqCst);
                StatusCode::IM_A_TEAPOT
            }
        });
        let foreign_redirect = format!(
            "http://{}/api/movie/{}",
            trap_listener
                .local_addr()
                .map_err(|_| "trap listener address failed")?,
            UUIDS[1]
        );
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "upstream listener bind failed")?;
        let upstream_base = format!(
            "http://{}",
            upstream_listener
                .local_addr()
                .map_err(|_| "upstream listener address failed")?
        );
        let state = Arc::new(FailureUpstreamState {
            base: upstream_base.clone(),
            foreign_redirect,
            fault,
            requests: Mutex::new(Vec::new()),
            hang_entered: AtomicBool::new(false),
            hang_released: AtomicBool::new(false),
            handler_completed: AtomicBool::new(false),
            handler_responded: AtomicBool::new(false),
            entered: Notify::new(),
            released: Notify::new(),
            completed: Notify::new(),
        });
        let upstream_app = Router::new()
            .fallback(failure_upstream)
            .with_state(state.clone());
        let (trap_shutdown, trap_shutdown_rx) = oneshot::channel();
        let trap_server = tokio::spawn(async move {
            axum::serve(trap_listener, trap_app)
                .with_graceful_shutdown(async move {
                    let _ = trap_shutdown_rx.await;
                })
                .await
        });
        let (upstream_shutdown, upstream_shutdown_rx) = oneshot::channel();
        let upstream_server = tokio::spawn(async move {
            axum::serve(upstream_listener, upstream_app)
                .with_graceful_shutdown(async move {
                    let _ = upstream_shutdown_rx.await;
                })
                .await
        });
        Ok((
            Self {
                state,
                trap_requests,
                upstream_shutdown: Some(upstream_shutdown),
                upstream_server,
                trap_shutdown: Some(trap_shutdown),
                trap_server,
            },
            upstream_base,
        ))
    }

    async fn shutdown(mut self) -> Result<(), String> {
        self.state.release_hang();
        if let Some(shutdown) = self.upstream_shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(shutdown) = self.trap_shutdown.take() {
            let _ = shutdown.send(());
        }
        let upstream = self
            .upstream_server
            .await
            .map_err(|_| "upstream server join failed".to_owned())
            .and_then(|result| result.map_err(|_| "upstream server failed".to_owned()));
        let trap = self
            .trap_server
            .await
            .map_err(|_| "trap server join failed".to_owned())
            .and_then(|result| result.map_err(|_| "trap server failed".to_owned()));
        let unused = if self.trap_requests.load(Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err("foreign redirect reached the trap server".into())
        };
        upstream.and(trap).and(unused)
    }
}

fn expected_secret_reads(fault: Fault) -> usize {
    if matches!(fault, Fault::Mark401 | Fault::Mark404Item | Fault::MarkHang) {
        2
    } else {
        1
    }
}

fn inspect_failure_trace(
    state: &FailureUpstreamState,
    fault: Fault,
    cli_token: &str,
) -> Result<(), String> {
    let requests = state.requests.lock().unwrap();
    let mut expected = vec![
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
    ];
    if matches!(fault, Fault::Mark401 | Fault::Mark404Item | Fault::MarkHang) {
        expected.push(format!("/api/movie/{}", UUIDS[1]));
        expected.push(format!("/api/me/shelf/item/{}", UUIDS[1]));
    }
    if requests.len() != expected.len() || requests.len() != fault.expected_requests() {
        return Err("native request count changed".into());
    }
    for (request, expected_uri) in requests.iter().zip(expected) {
        let personal = expected_uri.starts_with("/api/me/shelf/item/");
        if request.method != "GET"
            || request.uri != expected_uri
            || request.cookie
            || request.authorization.as_deref()
                != personal.then_some("Bearer neodb-fixture-token-e5a")
            || request.authorization.as_deref() == Some(format!("Bearer {cli_token}").as_str())
        {
            return Err("native method/path/auth/cookie trace changed".into());
        }
    }
    Ok(())
}

async fn inspect_terminal(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
    fault: Fault,
) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "preview status failed")?;
    let completed = fault.completes();
    if job.id != job_id
        || job.attempt != 1
        || job.created_at != NOW
        || job.updated_at != NOW
        || job.expires_at != NOW + 30 * 86_400
        || job.observation_count != 2
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.base_url != upstream_base
        || job.account != ACCOUNT
        || !matches!(job.binding_state, PreviewBindingState::Current)
        || !job.historical
        || job.executable
        || (completed && !matches!(job.status, PreviewJobStatusView::Completed))
        || (!completed && !matches!(job.status, PreviewJobStatusView::Failed))
    {
        return Err("terminal public job changed its immutable binding or outcome".into());
    }
    let first_page = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "preview results failed")?;
    let second_page = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "preview ordinal-one results failed")?;
    if first_page.job_id != job_id
        || first_page.attempt != 1
        || first_page.offset != 0
        || first_page.limit != 1
        || first_page.items.len() != 1
        || !first_page.historical
        || first_page.executable
        || second_page.job_id != job_id
        || second_page.attempt != 1
        || second_page.offset != 1
        || second_page.limit != 1
        || second_page.items.len() != usize::from(completed)
        || !second_page.historical
        || second_page.executable
    {
        return Err("terminal public page did not preserve the exact result set".into());
    }
    let first = &first_page.items[0];
    let PreviewOutcomeView::Compared {
        item,
        observed_at,
        comparison,
        provenance,
    } = &first.outcome
    else {
        return Err("the committed first row was not a comparison".into());
    };
    let rating = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating")
        .ok_or("the committed rating comparison was missing")?;
    if first.ordinal != 0
        || first.attempt != 1
        || first.committed_at != NOW
        || first.source_connector_id != "imdb:ratings_csv"
        || first.source_record_id != IDS[0]
        || first.profile_id != "imdb_ratings"
        || first.profile_version != 1
        || first.base_url != upstream_base
        || first.account != ACCOUNT
        || !first.historical
        || first.executable
        || item.uuid != UUIDS[0]
        || item.external_ids.len() != 1
        || item.external_ids[0].scheme != "imdb:title"
        || item.external_ids[0].value != IDS[0]
        || *observed_at != NOW
        || comparison.profile_id != "imdb_ratings"
        || comparison.profile_version != 1
        || !matches!(provenance, PreviewProvenanceView::Automatic)
        || !matches!(rating.suggestion, Some(ScalarValue::Number(8.0)))
        || !matches!(
            rating.sink,
            PreviewSinkView::Known {
                value: ScalarValue::Number(8.0)
            }
        )
        || !matches!(rating.status, PreviewComparisonStatusView::Skipped)
        || rating.reason != "already_equal"
    {
        return Err("the committed first comparison changed".into());
    }
    if completed {
        let second = &second_page.items[0];
        let expected = match fault {
            Fault::Fetch422 => matches!(second.outcome, PreviewOutcomeView::Unsupported),
            Fault::Fetch429 => matches!(second.outcome, PreviewOutcomeView::RateLimited),
            _ => false,
        };
        if !expected
            || second.ordinal != 1
            || second.attempt != 1
            || second.committed_at != NOW
            || second.source_connector_id != "imdb:ratings_csv"
            || second.source_record_id != IDS[1]
            || second.profile_id != "imdb_ratings"
            || second.profile_version != 1
            || second.base_url != upstream_base
            || second.account != ACCOUNT
            || !second.historical
            || second.executable
        {
            return Err("completed transport outcome changed".into());
        }
    }
    let observations = fixture
        .storage
        .observations(job_id.into(), 0, 100)
        .await
        .map_err(|e| e.code)?;
    let stored = fixture
        .storage
        .results(job_id.into(), 1, 0, 100)
        .await
        .map_err(|e| e.code)?;
    if observations.len() != 2 || stored.len() != 1 + usize::from(completed) {
        return Err("normalized retained history was incomplete".into());
    }
    for (ordinal, observation) in observations.iter().enumerate() {
        if observation.source_record_id != IDS[ordinal]
            || observation.item.title_hint.as_deref() != Some(["Equal", "Fault"][ordinal])
            || observation.observed_at != NOW.to_string()
        {
            return Err("normalized observation changed".into());
        }
    }
    let public_items = first_page
        .items
        .iter()
        .chain(second_page.items.iter())
        .collect::<Vec<_>>();
    for (ordinal, (stored, public)) in stored.iter().zip(public_items).enumerate() {
        if stored.ordinal() != ordinal as u32
            || public.ordinal != ordinal as u32
            || stored.source_fingerprint() != observations[ordinal].idempotency_key
            || public.source_fingerprint != observations[ordinal].idempotency_key
        {
            return Err("stored/public fingerprints or ordinals changed".into());
        }
    }
    let saved_v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .map_err(|e| e.code)?
        .ok_or("mapping v1 missing")?;
    let saved_v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .map_err(|e| e.code)?
        .ok_or("mapping v2 missing")?;
    let Operation::Clamp { max: v1_max, .. } = &saved_v1.rules[0].operation else {
        return Err("saved v1 rating rule changed".into());
    };
    let Operation::Clamp { max: v2_max, .. } = &saved_v2.rules[0].operation else {
        return Err("saved v2 rating rule changed".into());
    };
    if (*v1_max, *v2_max) != (10.0, 5.0) {
        return Err("the original immutable v1 selection changed".into());
    }
    let public = [
        serde_json::to_value(&job).unwrap(),
        serde_json::to_value(&first_page).unwrap(),
        serde_json::to_value(&second_page).unwrap(),
    ];
    for value in &public {
        let encoded = serde_json::to_string(value).unwrap();
        for forbidden in [
            CREDENTIAL_REF,
            NEODB_TOKEN,
            fixture.cli_token.as_str(),
            FAILURE_RAW_MARKER,
            FAILURE_UPSTREAM_MARKER,
        ] {
            if encoded.contains(forbidden) {
                return Err("public failure evidence retained a forbidden value".into());
            }
        }
    }
    Ok(vec![
        public[0].clone(),
        public[1].clone(),
        public[2].clone(),
        serde_json::to_value(observations).unwrap(),
    ])
}

async fn run_case(
    reader: &ProtocolReader,
    assets: &std::path::Path,
    fault: Fault,
    case: &'static str,
) -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|_| "temporary data directory failed")?;
    let (servers, upstream_base) = CaseServers::start(fault).await?;
    let state = servers.state.clone();
    let mut fixture = None;
    let result = async {
        let (initial, bootstrap_url) =
            start_fixture(&temp, &upstream_base, assets.to_path_buf()).await?;
        fixture = Some(initial);
        let baseline = retained_state(fixture.as_ref().unwrap()).await?;
        send(json!({"phase":"ready", "case":case, "bootstrapUrl":bootstrap_url}))?;

        let id = if matches!(fault, Fault::MarkHang) {
            let partial = reader.receive("partial")?;
            let id = job_id(&partial)?;
            state.wait_for_hang().await;
            let current = fixture.as_ref().ok_or("core fixture unavailable")?;
            let running = current
                .client
                .status(&id)
                .await
                .map_err(|_| "partial status failed")?;
            let page = current
                .client
                .results(&id, 1, 0, 100)
                .await
                .map_err(|_| "partial page failed")?;
            if !matches!(running.status, PreviewJobStatusView::Running)
                || page.items.len() != 1
                || !state.hang_entered.load(Ordering::SeqCst)
                || state.hang_released.load(Ordering::SeqCst)
            {
                return Err("held mark request did not preserve one committed running row".into());
            }
            send(json!({"phase":"held", "case":case}))?;
            id
        } else {
            String::new()
        };

        let retained = reader.receive("retained")?;
        let retained_id = job_id(&retained)?;
        if !id.is_empty() && retained_id != id {
            return Err("retained phase changed job id".into());
        }
        let id = retained_id;
        let current = fixture.as_ref().ok_or("core fixture unavailable")?;
        let terminal = current
            .client
            .status(&id)
            .await
            .map_err(|_| "terminal status failed")?;
        if !matches!(
            terminal.status,
            PreviewJobStatusView::Completed | PreviewJobStatusView::Failed
        ) {
            return Err("browser retained a nonterminal job".into());
        }
        let history = inspect_terminal(current, &id, &upstream_base, fault).await?;
        if matches!(fault, Fault::MarkHang) {
            state.release_hang();
            let _ = state.wait_for_handler_completion().await;
            if inspect_terminal(current, &id, &upstream_base, fault).await? != history {
                return Err("late valid hanging reply revived terminal history".into());
            }
        }
        let request_count = state.requests.lock().unwrap().len();
        let secret_reads = expected_secret_reads(fault);
        inspect_failure_trace(&state, fault, &current.cli_token)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
        {
            return Err("native credential trace changed".into());
        }
        let preserved = retained_state(current).await?;
        if !same_retained_state(&baseline, &preserved) {
            return Err("terminal failure changed unrelated state".into());
        }
        let address = current
            .report
            .core_url
            .trim_start_matches("http://")
            .parse()
            .map_err(|_| "core origin invalid")?;
        let secrets = current.secrets.clone();
        let cli_token = current.cli_token.clone();
        let report = fixture
            .take()
            .ok_or("core fixture unavailable")?
            .shutdown()
            .await;
        scan_closed_database_with_markers(
            &temp,
            &cli_token,
            &[FAILURE_RAW_MARKER, FAILURE_UPSTREAM_MARKER],
        )?;
        fixture = Some(
            reopen_fixture(
                &temp,
                address,
                assets.to_path_buf(),
                secrets,
                report,
                cli_token.clone(),
            )
            .await?,
        );
        let current = fixture
            .as_ref()
            .ok_or("reopened core fixture unavailable")?;
        if inspect_terminal(current, &id, &upstream_base, fault).await? != history
            || state.requests.lock().unwrap().len() != request_count
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err("failure history or unrelated state changed after reopen".into());
        }
        send(json!({"phase":"reopened", "case":case}))?;

        let finish = reader.receive("finish")?;
        if job_id(&finish)? != id {
            return Err("finish phase changed job id".into());
        }
        let current = fixture
            .as_ref()
            .ok_or("reopened core fixture unavailable")?;
        if PreviewJobRepository::load(&current.storage, id.clone())
            .await
            .map_err(|e| e.code)?
            .is_some()
            || !current
                .storage
                .observations(id.clone(), 0, 100)
                .await
                .map_err(|e| e.code)?
                .is_empty()
            || !current
                .storage
                .results(id.clone(), 1, 0, 100)
                .await
                .map_err(|e| e.code)?
                .is_empty()
            || state.requests.lock().unwrap().len() != request_count
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err("job-only deletion changed unrelated state or performed I/O".into());
        }
        let auth = status(&current.report.core_url, current.secrets.as_ref())
            .await
            .map_err(|e| e.code)?;
        if !auth.authenticated || auth.client.id != current.report.client_id {
            return Err("original paired client did not remain authenticated".into());
        }
        inspect_failure_trace(&state, fault, &current.cli_token)?;
        Ok((cli_token, preserved, secret_reads))
    }
    .await;

    state.release_hang();
    if matches!(fault, Fault::MarkHang) && state.hang_entered.load(Ordering::SeqCst) {
        let _ = state.wait_for_handler_completion().await;
    }
    let final_secrets = fixture.as_ref().map(|current| current.secrets.clone());
    if let Some(current) = fixture.take() {
        current.shutdown().await;
    }
    let post = match (&result, final_secrets) {
        (Ok((cli_token, preserved, secret_reads)), Some(secrets)) => {
            scan_closed_database_with_markers(
                &temp,
                cli_token,
                &[FAILURE_RAW_MARKER, FAILURE_UPSTREAM_MARKER],
            )
            .and_then(|_| inspect_failure_trace(&state, fault, cli_token))
            .and_then(|_| {
                if secrets.neodb_reads.load(Ordering::SeqCst) == *secret_reads
                    && secrets.neodb_writes.load(Ordering::SeqCst) == 0
                    && secret_values(&secrets) == preserved.secrets
                {
                    Ok(())
                } else {
                    Err("shutdown changed credentials or secret values".into())
                }
            })
        }
        _ => Ok(()),
    };
    let server_result = servers.shutdown().await;
    result.map(|_| ()).and(post).and(server_result)?;
    send(json!({"phase":"done", "case":case}))
}

async fn run_failure_host() -> Result<(), String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/webui/dist")
        .canonicalize()
        .map_err(|_| "built WebUI assets were not found")?;
    let reader = ProtocolReader::start();
    for (fault, case) in CASES {
        run_case(&reader, &assets, fault, case).await?;
    }
    send(json!({"phase":"all_done"}))
}

#[test]
#[ignore = "driven explicitly by tests/preview-native-browser-smoke.mjs after WebUI build"]
fn browser_native_failure_host() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_failure_host()) {
        panic!("E5g browser failure host failed: {error}");
    }
}
