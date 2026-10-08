//! Ignored host for the browser-to-real-adapter acceptance driver.
//! It is test-only and communicates through a four-phase bounded line protocol.
use super::*;
use collector_domain::preview::PreviewJobStatusView;
use serde_json::json;
use std::{
    io::{Read, Write},
    sync::mpsc,
};

mod failure;
mod polling;
mod resolution;
mod restart;

const HOST_PREFIX: &str = "E5F:";

fn send(value: Value) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "{HOST_PREFIX}{}",
        serde_json::to_string(&value).unwrap()
    )
    .and_then(|_| stdout.flush())
    .map_err(|_| "protocol stdout closed".into())
}

struct ProtocolReader {
    lines: mpsc::Receiver<Result<String, String>>,
}

impl ProtocolReader {
    fn start() -> Self {
        let (sender, lines) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin().lock();
            loop {
                let mut bytes = Vec::with_capacity(256);
                let result = loop {
                    let mut byte = [0_u8; 1];
                    match stdin.read(&mut byte) {
                        Ok(0) if bytes.is_empty() => break Err("protocol stdin closed".into()),
                        Ok(0) => {
                            break String::from_utf8(bytes)
                                .map_err(|_| "protocol line was not UTF-8".into());
                        }
                        Ok(_) if byte[0] == b'\n' => {
                            break String::from_utf8(bytes)
                                .map_err(|_| "protocol line was not UTF-8".into());
                        }
                        Ok(_) if bytes.len() == 4096 => {
                            break Err("protocol line exceeded 4096 bytes".into());
                        }
                        Ok(_) => bytes.push(byte[0]),
                        Err(_) => break Err("protocol stdin read failed".into()),
                    }
                };
                let terminal = result.is_err();
                if sender.send(result).is_err() || terminal {
                    break;
                }
            }
        });
        Self { lines }
    }

    fn receive(&self, expected: &'static str) -> Result<Value, String> {
        let line =
            self.lines
                .recv_timeout(Duration::from_secs(30))
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => format!("timed out waiting for {expected}"),
                    mpsc::RecvTimeoutError::Disconnected => {
                        format!("stdin closed while waiting for {expected}")
                    }
                })??;
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|_| format!("invalid protocol message while waiting for {expected}"))?;
        if value.get("phase").and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "unexpected protocol phase while waiting for {expected}"
            ));
        }
        Ok(value)
    }
}

fn job_id(value: &Value) -> Result<String, String> {
    value
        .get("jobId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "protocol jobId was missing".into())
}

fn scan_closed_database(temp: &tempfile::TempDir, cli_token: &str) -> Result<(), String> {
    scan_closed_database_with_markers(temp, cli_token, &[])
}

fn scan_closed_database_with_markers(
    temp: &tempfile::TempDir,
    cli_token: &str,
    extra_markers: &[&str],
) -> Result<(), String> {
    for name in ["collector.db", "collector.db-wal"] {
        let path = temp.path().join(name);
        if name.ends_with("-wal") && !path.exists() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|_| format!("could not scan {name}"))?;
        for forbidden in [RAW_COLUMN_MARKER, UPSTREAM_MARKER, NEODB_TOKEN, cli_token]
            .into_iter()
            .chain(extra_markers.iter().copied())
        {
            if bytes
                .windows(forbidden.len())
                .any(|window| window == forbidden.as_bytes())
            {
                return Err(format!("closed {name} retained a forbidden value"));
            }
        }
    }
    Ok(())
}

async fn start_fixture(
    temp: &tempfile::TempDir,
    upstream_base: &str,
    assets: PathBuf,
) -> Result<(CoreFixture, String), String> {
    let (fixture, access) =
        CoreFixture::start_with_assets(temp, upstream_base, "E5f browser host", Some(assets)).await;
    let bootstrap = access.bootstrap().await.map_err(|e| e.code)?;
    let bootstrap_url = format!("{}/#key={bootstrap}", fixture.report.core_url);
    Ok((fixture, bootstrap_url))
}

async fn reopen_fixture(
    temp: &tempfile::TempDir,
    address: std::net::SocketAddr,
    assets: PathBuf,
    secrets: Arc<FixtureSecrets>,
    report: super::super::PairReport,
    cli_token: String,
) -> Result<CoreFixture, String> {
    reopen_fixture_at(temp, address, assets, secrets, report, cli_token, NOW).await
}

async fn reopen_fixture_at(
    temp: &tempfile::TempDir,
    address: std::net::SocketAddr,
    assets: PathBuf,
    secrets: Arc<FixtureSecrets>,
    report: super::super::PairReport,
    cli_token: String,
    now: i64,
) -> Result<CoreFixture, String> {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|_| "same-origin core listener rebind failed")?;
    let clock: Arc<dyn Clock> = Arc::new(FixedClock(now));
    Ok(CoreFixture::reopen_with_assets(
        temp,
        secrets,
        clock,
        listener,
        report,
        cli_token,
        Some(assets),
    )
    .await)
}

async fn inspect_completed(fixture: &CoreFixture, job_id: &str) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "preview status failed")?;
    if !matches!(job.status, PreviewJobStatusView::Completed)
        || job.attempt != 1
        || job.observation_count != 3
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.account != ACCOUNT
        || !job.historical
        || job.executable
    {
        return Err("completed public job did not preserve its binding".into());
    }
    let first = fixture
        .client
        .results(job_id, 1, 0, 2)
        .await
        .map_err(|_| "preview page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 2, 2)
        .await
        .map_err(|_| "preview page failed")?;
    if first.items.len() != 2 || second.items.len() != 1 || first.offset != 0 || second.offset != 2
    {
        return Err("ordinal pagination did not preserve 2 plus 1 results".into());
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
    if observations.len() != 3 || stored.len() != 3 {
        return Err("normalized retained history was incomplete".into());
    }
    for (ordinal, observation) in observations.iter().enumerate() {
        if observation.source_record_id != IDS[ordinal]
            || observation.item.title_hint.as_deref()
                != Some(["Equal", "中文标题", "Missing"][ordinal])
            || observation.observed_at != NOW.to_string()
            || observation.idempotency_key != stored[ordinal].source_fingerprint()
        {
            return Err("normalized observation/result evidence changed".into());
        }
        let StoredPreviewOutcome::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } = stored[ordinal].outcome()
        else {
            return Err("native result was not a comparison".into());
        };
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .ok_or("rating comparison was missing")?;
        if *observed_at != NOW
            || comparison.profile_id != "imdb_ratings"
            || comparison.profile_version != 1
            || !matches!(provenance, PreviewRecordProvenance::Automatic)
            || item.uuid != UUIDS[ordinal]
            || item.external_ids != vec![("imdb:title".into(), IDS[ordinal].into())]
            || !matches!(&rating.suggestion, Some(ScalarValue::Number(value)) if *value == [8.0, 7.0, 6.0][ordinal])
        {
            return Err("stored identity, provenance, time, or suggestion changed".into());
        }
        let decision_matches = match ordinal {
            0 => {
                matches!(&rating.sink, SinkValue::Known(ScalarValue::Number(8.0)))
                    && rating.status == ComparisonStatus::Skipped
                    && rating.reason == "already_equal"
            }
            1 => {
                matches!(&rating.sink, SinkValue::Known(ScalarValue::Number(5.0)))
                    && rating.status == ComparisonStatus::NeedsReview
                    && rating.reason == "sink_conflict_needs_review"
            }
            _ => {
                matches!(rating.sink, SinkValue::Unknown)
                    && rating.status == ComparisonStatus::NeedsReview
                    && rating.reason == "rating_requires_shelf"
            }
        };
        if !decision_matches {
            return Err("stored comparison decision changed".into());
        }
    }
    Ok(vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap(),
        serde_json::to_value(observations).unwrap(),
    ])
}

fn inspect_trace(state: &UpstreamState, cli_token: &str) -> Result<(), String> {
    let requests = state.requests.lock().unwrap();
    let expected = [
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
    if requests.len() != expected.len() {
        return Err("native request count changed".into());
    }
    for (index, request) in requests.iter().enumerate() {
        let personal = matches!(index, 2 | 5 | 8 | 9);
        if request.method != "GET"
            || request.uri != expected[index]
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

struct RetainedState {
    endpoint: Endpoint,
    profile_v1: Value,
    profile_v2: Value,
    client: collector_core::access::AccessRecord,
    secrets: Vec<(String, String)>,
}

async fn retained_state(fixture: &CoreFixture) -> Result<RetainedState, String> {
    let endpoint = fixture
        .storage
        .endpoint()
        .await
        .map_err(|e| e.code)?
        .ok_or("endpoint missing")?;
    let v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .map_err(|e| e.code)?
        .ok_or("mapping v1 missing")?;
    let v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .map_err(|e| e.code)?
        .ok_or("mapping v2 missing")?;
    let hash = SecureTokens.digest(&fixture.cli_token);
    let client = fixture
        .storage
        .find(hash, "client".into(), fixture.report.core_url.clone(), NOW)
        .await
        .map_err(|e| e.code)?
        .ok_or("original paired client missing")?;
    let mut secrets = fixture
        .secrets
        .values
        .lock()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.expose_secret().to_owned()))
        .collect::<Vec<_>>();
    secrets.sort();
    Ok(RetainedState {
        endpoint,
        profile_v1: serde_json::to_value(v1).unwrap(),
        profile_v2: serde_json::to_value(v2).unwrap(),
        client,
        secrets,
    })
}

fn same_retained_state(before: &RetainedState, after: &RetainedState) -> bool {
    before.endpoint.base_url == after.endpoint.base_url
        && before.endpoint.account == after.endpoint.account
        && before.endpoint.credential_ref == after.endpoint.credential_ref
        && before.profile_v1 == after.profile_v1
        && before.profile_v2 == after.profile_v2
        && before.client.id == after.client.id
        && before.client.hash == after.client.hash
        && before.client.kind == after.client.kind
        && before.client.audience == after.client.audience
        && before.client.name == after.client.name
        && before.client.client_kind == after.client.client_kind
        && before.client.expires_at == after.client.expires_at
        && before.secrets == after.secrets
}

fn secret_values(secrets: &FixtureSecrets) -> Vec<(String, String)> {
    let mut values = secrets
        .values
        .lock()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.expose_secret().to_owned()))
        .collect::<Vec<_>>();
    values.sort();
    values
}

async fn run_protocol(
    reader: &ProtocolReader,
    fixture: &mut Option<CoreFixture>,
    temp: &tempfile::TempDir,
    assets: PathBuf,
    upstream_base: &str,
    upstream_state: &Arc<UpstreamState>,
    baseline: &RetainedState,
) -> Result<String, String> {
    let partial = reader.receive("partial")?;
    let id = job_id(&partial)?;
    upstream_state.wait_for_final_request().await;
    let current = fixture.as_ref().ok_or("core fixture unavailable")?;
    let running = current
        .client
        .status(&id)
        .await
        .map_err(|_| "partial status failed")?;
    let page = current
        .client
        .results(&id, 1, 0, 2)
        .await
        .map_err(|_| "partial page failed")?;
    if !matches!(running.status, PreviewJobStatusView::Running) || page.items.len() != 2 {
        return Err("partial phase did not retain two committed running results".into());
    }
    upstream_state.release_final_request();
    send(json!({"phase":"released"}))?;

    let retained = reader.receive("retained")?;
    if job_id(&retained)? != id {
        return Err("retained phase changed job id".into());
    }
    let current = fixture.as_ref().ok_or("core fixture unavailable")?;
    let completed_history = inspect_completed(current, &id).await?;
    let requests_before = upstream_state.requests.lock().unwrap().len();
    inspect_trace(upstream_state, &current.cli_token)?;
    if current.secrets.neodb_reads.load(Ordering::SeqCst) != 3 {
        return Err("native request or secret-read trace was not exact".into());
    }
    let preserved = retained_state(current).await?;
    if !same_retained_state(baseline, &preserved) {
        return Err("preview completion changed endpoint/profile/client/secret state".into());
    }
    let endpoint = &preserved.endpoint;
    if endpoint.base_url != upstream_base
        || endpoint.account != ACCOUNT
        || endpoint.credential_ref != CREDENTIAL_REF
    {
        return Err("endpoint binding changed".into());
    }
    let address: std::net::SocketAddr = current
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
    scan_closed_database(temp, &cli_token)?;
    *fixture =
        Some(reopen_fixture(temp, address, assets, secrets, report, cli_token.clone()).await?);
    let current = fixture
        .as_ref()
        .ok_or("reopened core fixture unavailable")?;
    if inspect_completed(current, &id).await? != completed_history {
        return Err("completed public or normalized history changed after reopen".into());
    }
    if upstream_state.requests.lock().unwrap().len() != requests_before
        || current.secrets.neodb_reads.load(Ordering::SeqCst) != 3
    {
        return Err("completed-history reopen performed remote or credential reads".into());
    }
    if !same_retained_state(&preserved, &retained_state(current).await?) {
        return Err("completed-history reopen changed endpoint/profile/client/secret state".into());
    }
    send(json!({"phase":"reopened"}))?;

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
    {
        return Err("browser deletion left the preview job".into());
    }
    if current.client.status(&id).await.is_ok()
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
        || upstream_state.requests.lock().unwrap().len() != requests_before
        || current.secrets.neodb_reads.load(Ordering::SeqCst) != 3
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
    Ok(cli_token)
}

async fn run_host() -> Result<(), String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/webui/dist")
        .canonicalize()
        .map_err(|_| "built WebUI assets were not found")?;
    let temp = tempfile::tempdir().map_err(|_| "temporary data directory failed")?;
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "upstream listener bind failed")?;
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
        .fallback(axum::routing::any(upstream))
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

    let reader = ProtocolReader::start();
    let initial = match start_fixture(&temp, &upstream_base, assets.clone()).await {
        Ok(value) => value,
        Err(error) => {
            let _ = upstream_shutdown_tx.send(());
            let _ = upstream_server.await;
            return Err(error);
        }
    };
    let (initial, bootstrap_url) = initial;
    let mut fixture = Some(initial);
    let baseline = retained_state(fixture.as_ref().unwrap()).await;
    let ready = match baseline.as_ref() {
        Ok(_) => send(json!({"phase":"ready", "bootstrapUrl": bootstrap_url})),
        Err(error) => Err(error.clone()),
    };
    let result = match ready {
        Ok(()) => {
            run_protocol(
                &reader,
                &mut fixture,
                &temp,
                assets,
                &upstream_base,
                &upstream_state,
                baseline.as_ref().unwrap(),
            )
            .await
        }
        Err(error) => Err(error),
    };
    upstream_state.release_final_request();
    let final_secrets = fixture.as_ref().map(|current| current.secrets.clone());
    if let Some(current) = fixture.take() {
        current.shutdown().await;
    }
    let post_result = match (&result, final_secrets) {
        (Ok(cli_token), Some(secrets)) => scan_closed_database(&temp, cli_token)
            .and_then(|_| inspect_trace(&upstream_state, cli_token))
            .and_then(|_| {
                if secrets.neodb_reads.load(Ordering::SeqCst) == 3
                    && secrets.neodb_writes.load(Ordering::SeqCst) == 0
                    && secret_values(&secrets) == baseline.as_ref().unwrap().secrets
                {
                    Ok(())
                } else {
                    Err("shutdown changed the exact credential trace or secret values".into())
                }
            }),
        _ => Ok(()),
    };
    let _ = upstream_shutdown_tx.send(());
    let join_result = upstream_server
        .await
        .map_err(|_| "upstream server join failed".to_owned());
    if result.is_ok() && post_result.is_ok() && join_result.is_ok() {
        send(json!({"phase":"done"}))?;
    }
    result.map(|_| ()).and(post_result).and(join_result)
}

#[test]
#[ignore = "driven explicitly by tests/preview-native-browser-smoke.mjs after WebUI build"]
fn browser_native_preview_host() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_host()) {
        panic!("E5f browser host failed: {error}");
    }
}
