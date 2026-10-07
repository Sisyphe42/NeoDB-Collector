//! Ignored native-browser host for pending catalog polling and cancellation.
use super::*;
use crate::client::preview_integration_tests::resolution::{
    POLL_ID, POLL_UUID, PollState, RESOLUTION_RAW_MARKER, RESOLUTION_UPSTREAM_MARKER,
    polling_upstream,
};
use collector_core::preview_jobs::PreviewResolutionInput;

const CASES: [(&str, bool); 2] = [("poll_complete", false), ("poll_cancel", true)];

struct PollServer {
    state: Arc<PollState>,
    shutdown: Option<oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl PollServer {
    async fn start() -> Result<(Self, String), String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "polling upstream listener bind failed")?;
        let upstream_base = format!(
            "http://{}",
            listener
                .local_addr()
                .map_err(|_| "polling upstream listener address failed")?
        );
        let state = Arc::new(PollState::new(upstream_base.clone()));
        let app = Router::new()
            .fallback(polling_upstream)
            .with_state(state.clone());
        let (shutdown, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });
        Ok((
            Self {
                state,
                shutdown: Some(shutdown),
                server,
            },
            upstream_base,
        ))
    }

    async fn shutdown(mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.server
            .await
            .map_err(|_| "polling upstream server join failed".to_owned())?
            .map_err(|_| "polling upstream server failed".to_owned())
    }
}

fn inspect_trace(state: &PollState, cli_token: &str, cancelled: bool) -> Result<usize, String> {
    let requests = state.requests();
    let expected = if cancelled {
        vec![format!(
            "/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F"
        )]
    } else {
        vec![
            format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F"),
            format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{POLL_ID}%2F"),
            format!("/api/movie/{POLL_UUID}"),
            format!("/api/me/shelf/item/{POLL_UUID}"),
        ]
    };
    if requests.len() != expected.len() {
        return Err("polling request count changed".into());
    }
    for (index, (request, expected_uri)) in requests.iter().zip(expected).enumerate() {
        let personal = !cancelled && index == 3;
        if request.seen.method != "GET"
            || request.seen.uri != expected_uri
            || request.seen.cookie
            || request.seen.authorization.as_deref()
                != personal.then_some("Bearer neodb-fixture-token-e5a")
            || request.seen.authorization.as_deref() == Some(format!("Bearer {cli_token}").as_str())
            || request.response_prepared_at < request.received_at
        {
            return Err("polling method, path, auth, cookie, or timing trace changed".into());
        }
    }
    if !cancelled
        && requests[1]
            .received_at
            .duration_since(requests[0].response_prepared_at)
            < Duration::from_secs(15)
    {
        return Err("polling interval was shorter than the production bound".into());
    }
    Ok(requests.len())
}

fn public_is_redacted(values: &[Value], cli_token: &str) -> bool {
    values.iter().all(|value| {
        let encoded = serde_json::to_string(value).unwrap();
        [
            CREDENTIAL_REF,
            NEODB_TOKEN,
            cli_token,
            RAW_COLUMN_MARKER,
            UPSTREAM_MARKER,
            RESOLUTION_RAW_MARKER,
            RESOLUTION_UPSTREAM_MARKER,
        ]
        .into_iter()
        .all(|forbidden| !encoded.contains(forbidden))
    })
}

async fn inspect_terminal(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
    cancelled: bool,
) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "polling terminal status failed")?;
    let status_matches = if cancelled {
        matches!(job.status, PreviewJobStatusView::Cancelled)
    } else {
        matches!(job.status, PreviewJobStatusView::Completed)
    };
    if !status_matches
        || job.id != job_id
        || job.attempt != 1
        || job.created_at != NOW
        || job.updated_at != NOW
        || job.expires_at != NOW + 30 * 86_400
        || job.observation_count != 1
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.base_url != upstream_base
        || job.account != ACCOUNT
        || !matches!(job.binding_state, PreviewBindingState::Current)
        || !job.historical
        || job.executable
    {
        return Err("polling terminal job changed its immutable metadata".into());
    }
    let first = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "polling offset-zero page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "polling offset-one page failed")?;
    let expected_results = usize::from(!cancelled);
    if first.job_id != job_id
        || first.attempt != 1
        || first.offset != 0
        || first.limit != 1
        || first.items.len() != expected_results
        || !first.historical
        || first.executable
        || second.job_id != job_id
        || second.attempt != 1
        || second.offset != 1
        || second.limit != 1
        || !second.items.is_empty()
        || !second.historical
        || second.executable
    {
        return Err("polling terminal pages changed their exact ordinal contents".into());
    }
    let observations = fixture
        .storage
        .observations(job_id.into(), 0, 100)
        .await
        .map_err(|error| error.code)?;
    if observations.len() != 1 {
        return Err("polling normalized observation was not retained".into());
    }
    let observation = &observations[0];
    if observation.source_connector_id != "imdb:ratings_csv"
        || observation.source_record_id != POLL_ID
        || observation.observed_at != NOW.to_string()
        || observation.item.media_type != "movie"
        || observation.item.canonical_url != format!("https://www.imdb.com/title/{POLL_ID}/")
        || observation
            .item
            .external_ids
            .get("imdb:title")
            .map(String::as_str)
            != Some(POLL_ID)
        || observation.item.title_hint.as_deref() != Some("Polling")
        || !matches!(
            observation.state.get("rating"),
            Some(ScalarValue::Number(8.0))
        )
        || !observation.idempotency_key.starts_with("imdb:ratings_csv:")
    {
        return Err("polling normalized observation changed".into());
    }
    if fixture
        .storage
        .resolution_input(job_id.into(), 0)
        .await
        .map_err(|error| error.code)?
        != PreviewResolutionInput::Native
    {
        return Err("polling observation did not retain native resolution input".into());
    }
    let stored = fixture
        .storage
        .results(job_id.into(), 1, 0, 100)
        .await
        .map_err(|error| error.code)?;
    if stored.len() != expected_results {
        return Err("polling stored result set changed".into());
    }
    if !cancelled {
        let public = &first.items[0];
        let StoredPreviewOutcome::Compared {
            item: stored_item,
            observed_at: stored_observed_at,
            comparison: stored_comparison,
            provenance: stored_provenance,
        } = stored[0].outcome()
        else {
            return Err("polling stored outcome was not a comparison".into());
        };
        let PreviewOutcomeView::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } = &public.outcome
        else {
            return Err("polling public outcome was not a comparison".into());
        };
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .ok_or("polling rating comparison was missing")?;
        let stored_rating = stored_comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .ok_or("polling stored rating comparison was missing")?;
        let expected_canonical = format!("{upstream_base}/movie/{POLL_UUID}");
        let expected_detail = format!("{upstream_base}/api/movie/{POLL_UUID}");
        if public.ordinal != 0
            || public.attempt != 1
            || public.committed_at != NOW
            || public.source_connector_id != "imdb:ratings_csv"
            || public.source_record_id != POLL_ID
            || public.source_fingerprint != observation.idempotency_key
            || public.profile_id != "imdb_ratings"
            || public.profile_version != 1
            || public.base_url != upstream_base
            || public.account != ACCOUNT
            || !public.historical
            || public.executable
            || stored[0].ordinal() != 0
            || stored[0].source_fingerprint() != observation.idempotency_key
            || item.uuid != POLL_UUID
            || item.canonical_url != expected_canonical
            || item.detail_url != expected_detail
            || item.media_type != "movie"
            || item.model != "Movie"
            || item.parent_uuid.is_some()
            || item.external_ids.len() != 1
            || item.external_ids[0].scheme != "imdb:title"
            || item.external_ids[0].value != POLL_ID
            || stored_item.uuid != POLL_UUID
            || stored_item.canonical_url != expected_canonical
            || stored_item.detail_url != expected_detail
            || stored_item.external_ids != vec![("imdb:title".into(), POLL_ID.into())]
            || *observed_at != NOW
            || *stored_observed_at != NOW
            || comparison.profile_id != "imdb_ratings"
            || comparison.profile_version != 1
            || stored_comparison.profile_id != "imdb_ratings"
            || stored_comparison.profile_version != 1
            || !matches!(provenance, PreviewProvenanceView::Automatic)
            || !matches!(stored_provenance, PreviewRecordProvenance::Automatic)
            || !matches!(rating.suggestion, Some(ScalarValue::Number(8.0)))
            || !matches!(stored_rating.suggestion, Some(ScalarValue::Number(8.0)))
            || !matches!(
                rating.sink,
                PreviewSinkView::Known {
                    value: ScalarValue::Number(8.0)
                }
            )
            || !matches!(
                stored_rating.sink,
                SinkValue::Known(ScalarValue::Number(8.0))
            )
            || !matches!(rating.status, PreviewComparisonStatusView::Skipped)
            || stored_rating.status != ComparisonStatus::Skipped
            || rating.reason != "already_equal"
            || stored_rating.reason != "already_equal"
        {
            return Err("polling comparison identity, fingerprint, or decision changed".into());
        }
    }
    let saved_v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .map_err(|error| error.code)?
        .ok_or("polling mapping v1 was missing")?;
    let saved_v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .map_err(|error| error.code)?
        .ok_or("polling mapping v2 was missing")?;
    let Operation::Clamp { max: v1_max, .. } = &saved_v1.rules[0].operation else {
        return Err("polling mapping v1 rule changed".into());
    };
    let Operation::Clamp { max: v2_max, .. } = &saved_v2.rules[0].operation else {
        return Err("polling mapping v2 rule changed".into());
    };
    if (*v1_max, *v2_max) != (10.0, 5.0) || saved_v1.enabled || saved_v2.enabled {
        return Err("polling immutable disabled profile selection changed".into());
    }
    let public = vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap(),
        serde_json::to_value(observations).unwrap(),
    ];
    if !public_is_redacted(&public, &fixture.cli_token) {
        return Err("polling public evidence retained a forbidden value".into());
    }
    Ok(public)
}

async fn inspect_pending(
    fixture: &CoreFixture,
    state: &PollState,
    job_id: &str,
) -> Result<(), String> {
    state.wait_for_first_fetch().await;
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "polling pending status failed")?;
    let first = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "polling pending page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "polling pending offset-one page failed")?;
    if !matches!(job.status, PreviewJobStatusView::Running)
        || job.attempt != 1
        || !first.items.is_empty()
        || !second.items.is_empty()
        || fixture.secrets.neodb_reads.load(Ordering::SeqCst) != 0
        || state.requests().len() != 1
    {
        return Err(
            "first prepared polling response did not preserve empty running history".into(),
        );
    }
    if !public_is_redacted(
        &[
            serde_json::to_value(job).unwrap(),
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap(),
        ],
        &fixture.cli_token,
    ) {
        return Err("polling pending evidence retained a forbidden value".into());
    }
    Ok(())
}

async fn run_case(
    reader: &ProtocolReader,
    assets: &std::path::Path,
    case: &'static str,
    cancelled: bool,
) -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|_| "polling temporary data directory failed")?;
    let (server, upstream_base) = PollServer::start().await?;
    let state = server.state.clone();
    let mut fixture = None;
    let result = async {
        let (initial, bootstrap_url) =
            start_fixture(&temp, &upstream_base, assets.to_path_buf()).await?;
        fixture = Some(initial);
        let baseline = retained_state(fixture.as_ref().unwrap()).await?;
        send(json!({"phase":"ready", "case":case, "bootstrapUrl":bootstrap_url}))?;

        let pending = reader.receive("pending")?;
        let id = job_id(&pending)?;
        let current = fixture.as_ref().ok_or("polling core fixture unavailable")?;
        inspect_pending(current, &state, &id).await?;
        send(json!({"phase":"pending", "case":case}))?;

        let retained = reader.receive("retained")?;
        if job_id(&retained)? != id {
            return Err("polling retained phase changed job id".into());
        }
        let current = fixture.as_ref().ok_or("polling core fixture unavailable")?;
        let history = inspect_terminal(current, &id, &upstream_base, cancelled).await?;
        if cancelled {
            let first_response_prepared_at = state
                .requests()
                .first()
                .ok_or("polling cancellation had no prepared response")?
                .response_prepared_at;
            if state
                .request_count_exceeds_until(
                    1,
                    first_response_prepared_at + Duration::from_secs(16),
                )
                .await
            {
                return Err("polling cancellation allowed a follow-up request".into());
            }
        }
        let expected_reads = usize::from(!cancelled);
        let request_count = inspect_trace(&state, &current.cli_token, cancelled)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != expected_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
        {
            return Err("polling credential trace changed".into());
        }
        let preserved = retained_state(current).await?;
        if !same_retained_state(&baseline, &preserved) {
            return Err("polling terminal state changed unrelated configuration".into());
        }
        let address = current
            .report
            .core_url
            .trim_start_matches("http://")
            .parse()
            .map_err(|_| "polling core origin was invalid")?;
        let secrets = current.secrets.clone();
        let cli_token = current.cli_token.clone();
        let report = fixture
            .take()
            .ok_or("polling core fixture unavailable")?
            .shutdown()
            .await;
        scan_closed_database_with_markers(
            &temp,
            &cli_token,
            &[RESOLUTION_RAW_MARKER, RESOLUTION_UPSTREAM_MARKER],
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
            .ok_or("polling reopened fixture unavailable")?;
        if inspect_terminal(current, &id, &upstream_base, cancelled).await? != history
            || inspect_trace(&state, &current.cli_token, cancelled)? != request_count
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != expected_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err("polling history or unrelated state changed after reopen".into());
        }
        send(json!({"phase":"reopened", "case":case}))?;

        let finish = reader.receive("finish")?;
        if job_id(&finish)? != id {
            return Err("polling finish phase changed job id".into());
        }
        let current = fixture
            .as_ref()
            .ok_or("polling reopened fixture unavailable")?;
        if PreviewJobRepository::load(&current.storage, id.clone())
            .await
            .map_err(|error| error.code)?
            .is_some()
            || !current
                .storage
                .observations(id.clone(), 0, 100)
                .await
                .map_err(|error| error.code)?
                .is_empty()
            || !current
                .storage
                .results(id.clone(), 1, 0, 100)
                .await
                .map_err(|error| error.code)?
                .is_empty()
            || inspect_trace(&state, &current.cli_token, cancelled)? != request_count
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != expected_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err(
                "polling job-only deletion changed unrelated state or performed I/O".into(),
            );
        }
        let auth = status(&current.report.core_url, current.secrets.as_ref())
            .await
            .map_err(|error| error.code)?;
        if !auth.authenticated || auth.client.id != current.report.client_id {
            return Err("polling original paired client did not remain authenticated".into());
        }
        Ok((cli_token, preserved, expected_reads, request_count))
    }
    .await;

    let final_secrets = fixture.as_ref().map(|current| current.secrets.clone());
    if let Some(current) = fixture.take() {
        current.shutdown().await;
    }
    let post = match (&result, final_secrets) {
        (Ok((cli_token, preserved, expected_reads, request_count)), Some(secrets)) => {
            scan_closed_database_with_markers(
                &temp,
                cli_token,
                &[RESOLUTION_RAW_MARKER, RESOLUTION_UPSTREAM_MARKER],
            )
            .and_then(|_| {
                if inspect_trace(&state, cli_token, cancelled)? == *request_count
                    && secrets.neodb_reads.load(Ordering::SeqCst) == *expected_reads
                    && secrets.neodb_writes.load(Ordering::SeqCst) == 0
                    && secret_values(&secrets) == preserved.secrets
                {
                    Ok(())
                } else {
                    Err("polling shutdown changed requests, credentials, or secrets".into())
                }
            })
        }
        _ => Ok(()),
    };
    let server_result = server.shutdown().await;
    result.map(|_| ()).and(post).and(server_result)?;
    send(json!({"phase":"done", "case":case}))
}

async fn run_polling_host() -> Result<(), String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/webui/dist")
        .canonicalize()
        .map_err(|_| "built WebUI assets were not found")?;
    let reader = ProtocolReader::start();
    for (case, cancelled) in CASES {
        run_case(&reader, &assets, case, cancelled).await?;
    }
    send(json!({"phase":"all_done"}))
}

#[test]
#[ignore = "driven explicitly by tests/preview-native-browser-smoke.mjs after WebUI build"]
fn browser_native_polling_host() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_polling_host()) {
        panic!("E5h browser polling host failed: {error}");
    }
}
