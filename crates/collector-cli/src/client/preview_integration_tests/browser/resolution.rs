//! Ignored native-browser host for explicit Search and fresh Manual resolution.
use super::*;
use crate::client::preview_integration_tests::resolution::{
    RESOLUTION_RAW_MARKER, RESOLUTION_UPSTREAM_MARKER, ResolutionState, SEARCH_ID, SEARCH_UUID,
    resolution_upstream,
};
use collector_core::{catalog::CatalogReference, preview_jobs::PreviewResolutionInput};

const CASES: [(&str, bool); 2] = [("manual_missing_id", false), ("manual_conflict_id", true)];
const CONFLICT_ID: &str = "tt9999999";

struct ResolutionServer {
    state: Arc<ResolutionState>,
    shutdown: Option<oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl ResolutionServer {
    async fn start(conflict: bool) -> Result<(Self, String), String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "resolution upstream listener bind failed")?;
        let base = format!(
            "http://{}",
            listener
                .local_addr()
                .map_err(|_| "resolution upstream listener address failed")?
        );
        let state = Arc::new(ResolutionState::new(base.clone(), conflict));
        let app = Router::new()
            .fallback(resolution_upstream)
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
            base,
        ))
    }

    async fn shutdown(mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.server
            .await
            .map_err(|_| "resolution upstream server join failed".to_owned())?
            .map_err(|_| "resolution upstream server failed".to_owned())
    }
}

#[derive(Clone, Copy)]
enum ExpectedOutcome {
    Unsupported,
    Search,
    ManualMissing,
    ManualConflict,
}

fn expected_input(outcome: ExpectedOutcome) -> PreviewResolutionInput {
    match outcome {
        ExpectedOutcome::Unsupported => PreviewResolutionInput::Native,
        ExpectedOutcome::Search => PreviewResolutionInput::Search("Exact candidate".into()),
        ExpectedOutcome::ManualMissing | ExpectedOutcome::ManualConflict => {
            PreviewResolutionInput::Manual(
                CatalogReference::from_stored(SEARCH_UUID.into(), "movie".into(), "Movie".into())
                    .unwrap(),
            )
        }
    }
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

async fn inspect_job(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
    expected: ExpectedOutcome,
) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "resolution status failed")?;
    if !matches!(job.status, PreviewJobStatusView::Completed)
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
        return Err("resolution job changed immutable metadata".into());
    }
    let first = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "resolution offset-zero page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "resolution offset-one page failed")?;
    if first.job_id != job_id
        || first.attempt != 1
        || first.offset != 0
        || first.limit != 1
        || first.items.len() != 1
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
        return Err("resolution ordinal pages changed".into());
    }
    let observations = fixture
        .storage
        .observations(job_id.into(), 0, 100)
        .await
        .map_err(|error| error.code)?;
    if observations.len() != 1 {
        return Err("resolution normalized observation was not retained".into());
    }
    let observation = &observations[0];
    if observation.source_connector_id != "imdb:ratings_csv"
        || observation.source_record_id != SEARCH_ID
        || observation.observed_at != NOW.to_string()
        || observation.item.media_type != "movie"
        || observation.item.canonical_url != format!("https://www.imdb.com/title/{SEARCH_ID}/")
        || observation
            .item
            .external_ids
            .get("imdb:title")
            .map(String::as_str)
            != Some(SEARCH_ID)
        || observation.item.title_hint.as_deref() != Some("中文候选")
        || !matches!(
            observation.state.get("rating"),
            Some(ScalarValue::Number(8.0))
        )
        || !observation.idempotency_key.starts_with("imdb:ratings_csv:")
    {
        return Err("resolution normalized observation changed".into());
    }
    if fixture
        .storage
        .resolution_input(job_id.into(), 0)
        .await
        .map_err(|error| error.code)?
        != expected_input(expected)
    {
        return Err("resolution input was not retained exactly".into());
    }
    let row = &first.items[0];
    if row.ordinal != 0
        || row.attempt != 1
        || row.committed_at != NOW
        || row.source_connector_id != "imdb:ratings_csv"
        || row.source_record_id != SEARCH_ID
        || row.source_fingerprint != observation.idempotency_key
        || row.profile_id != "imdb_ratings"
        || row.profile_version != 1
        || row.base_url != upstream_base
        || row.account != ACCOUNT
        || !row.historical
        || row.executable
    {
        return Err("resolution result binding or fingerprint changed".into());
    }
    let stored = fixture
        .storage
        .results(job_id.into(), 1, 0, 100)
        .await
        .map_err(|error| error.code)?;
    if stored.len() != 1
        || stored[0].ordinal() != 0
        || stored[0].source_fingerprint() != observation.idempotency_key
    {
        return Err("resolution stored result changed".into());
    }
    match expected {
        ExpectedOutcome::Unsupported => {
            if !matches!(row.outcome, PreviewOutcomeView::Unsupported)
                || !matches!(stored[0].outcome(), StoredPreviewOutcome::Unsupported)
            {
                return Err("native resolution was not unsupported".into());
            }
        }
        ExpectedOutcome::Search => {
            let PreviewOutcomeView::SearchCandidates {
                candidates,
                more_available,
            } = &row.outcome
            else {
                return Err("search resolution was not review-only candidates".into());
            };
            let StoredPreviewOutcome::SearchCandidates(stored_page) = stored[0].outcome() else {
                return Err("stored search resolution was not candidates".into());
            };
            if *more_available
                || stored_page.more_available
                || candidates.len() != 1
                || stored_page.candidates.len() != 1
                || candidates[0].title != "Exact review candidate"
                || stored_page.candidates[0].title != "Exact review candidate"
                || candidates[0].item.uuid != SEARCH_UUID
                || stored_page.candidates[0].item.uuid != SEARCH_UUID
                || candidates[0].item.canonical_url
                    != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || stored_page.candidates[0].item.canonical_url
                    != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || candidates[0].item.detail_url
                    != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || stored_page.candidates[0].item.detail_url
                    != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || candidates[0].item.media_type != "movie"
                || stored_page.candidates[0].item.media_type != "movie"
                || candidates[0].item.model != "Movie"
                || stored_page.candidates[0].item.model != "Movie"
                || candidates[0].item.parent_uuid.is_some()
                || stored_page.candidates[0].item.parent_uuid.is_some()
                || candidates[0].item.external_ids.len() != 1
                || candidates[0].item.external_ids[0].scheme != "imdb:title"
                || candidates[0].item.external_ids[0].value != SEARCH_ID
                || stored_page.candidates[0].item.external_ids
                    != vec![("imdb:title".into(), SEARCH_ID.into())]
            {
                return Err("search candidate evidence changed".into());
            }
        }
        ExpectedOutcome::ManualMissing => {
            let PreviewOutcomeView::Compared {
                item,
                observed_at,
                comparison,
                provenance,
            } = &row.outcome
            else {
                return Err("missing-identity manual resolution was not compared".into());
            };
            let StoredPreviewOutcome::Compared {
                item: stored_item,
                observed_at: stored_observed_at,
                comparison: stored_comparison,
                provenance: stored_provenance,
            } = stored[0].outcome()
            else {
                return Err("stored missing-identity manual result was not compared".into());
            };
            let rating = comparison
                .fields
                .iter()
                .find(|field| field.rule_id == "rating")
                .ok_or("manual rating comparison was missing")?;
            let stored_rating = stored_comparison
                .fields
                .iter()
                .find(|field| field.rule_id == "rating")
                .ok_or("stored manual rating comparison was missing")?;
            let shelf = comparison
                .fields
                .iter()
                .find(|field| field.rule_id == "rating_implies_complete")
                .ok_or("manual disabled shelf comparison was missing")?;
            let stored_shelf = stored_comparison
                .fields
                .iter()
                .find(|field| field.rule_id == "rating_implies_complete")
                .ok_or("stored manual disabled shelf comparison was missing")?;
            if item.uuid != SEARCH_UUID
                || item.canonical_url != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || item.detail_url != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || item.media_type != "movie"
                || item.model != "Movie"
                || item.parent_uuid.is_some()
                || !item.external_ids.is_empty()
                || stored_item.uuid != SEARCH_UUID
                || stored_item.canonical_url != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || stored_item.detail_url != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || stored_item.media_type != "movie"
                || stored_item.model != "Movie"
                || stored_item.parent_uuid.is_some()
                || !stored_item.external_ids.is_empty()
                || *observed_at != NOW
                || *stored_observed_at != NOW
                || comparison.profile_id != "imdb_ratings"
                || comparison.profile_version != 1
                || stored_comparison.profile_id != "imdb_ratings"
                || stored_comparison.profile_version != 1
                || comparison.fields.len() != 2
                || stored_comparison.fields.len() != 2
                || !matches!(provenance, PreviewProvenanceView::Manual)
                || !matches!(stored_provenance, PreviewRecordProvenance::Manual)
                || rating.field != "rating_grade"
                || stored_rating.field != "rating_grade"
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
                || shelf.field != "shelf_type"
                || shelf.suggestion.is_some()
                || !matches!(
                    shelf.sink,
                    PreviewSinkView::Known {
                        value: ScalarValue::Text(ref value)
                    } if value == "complete"
                )
                || !matches!(shelf.status, PreviewComparisonStatusView::Skipped)
                || shelf.reason != "rule_disabled"
                || stored_shelf.field != "shelf_type"
                || stored_shelf.suggestion.is_some()
                || !matches!(
                    stored_shelf.sink,
                    SinkValue::Known(ScalarValue::Text(ref value)) if value == "complete"
                )
                || stored_shelf.status != ComparisonStatus::Skipped
                || stored_shelf.reason != "rule_disabled"
            {
                return Err("manual comparison identity, provenance, or decision changed".into());
            }
        }
        ExpectedOutcome::ManualConflict => {
            let PreviewOutcomeView::NeedsReview { item, reason } = &row.outcome else {
                return Err("conflicting manual resolution was not review-only".into());
            };
            let StoredPreviewOutcome::NeedsReview {
                item: stored_item,
                reason: stored_reason,
            } = stored[0].outcome()
            else {
                return Err("stored conflicting manual result was not review-only".into());
            };
            if reason != "item_identity_conflict"
                || stored_reason != "item_identity_conflict"
                || item.uuid != SEARCH_UUID
                || item.canonical_url != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || item.detail_url != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || item.media_type != "movie"
                || item.model != "Movie"
                || item.parent_uuid.is_some()
                || item.external_ids.len() != 1
                || item.external_ids[0].scheme != "imdb:title"
                || item.external_ids[0].value != CONFLICT_ID
                || stored_item.uuid != SEARCH_UUID
                || stored_item.canonical_url != format!("{upstream_base}/movie/{SEARCH_UUID}")
                || stored_item.detail_url != format!("{upstream_base}/api/movie/{SEARCH_UUID}")
                || stored_item.media_type != "movie"
                || stored_item.model != "Movie"
                || stored_item.parent_uuid.is_some()
                || stored_item.external_ids != vec![("imdb:title".into(), CONFLICT_ID.into())]
            {
                return Err("manual identity-conflict evidence changed".into());
            }
        }
    }
    let saved_v1 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(1))
        .await
        .map_err(|error| error.code)?
        .ok_or("resolution mapping v1 was missing")?;
    let saved_v2 = MappingRepository::load(&fixture.storage, "imdb_ratings".into(), Some(2))
        .await
        .map_err(|error| error.code)?
        .ok_or("resolution mapping v2 was missing")?;
    let Operation::Clamp { max: v1_max, .. } = &saved_v1.rules[0].operation else {
        return Err("resolution mapping v1 rule changed".into());
    };
    let Operation::Clamp { max: v2_max, .. } = &saved_v2.rules[0].operation else {
        return Err("resolution mapping v2 rule changed".into());
    };
    if (*v1_max, *v2_max) != (10.0, 5.0) || saved_v1.enabled || saved_v2.enabled {
        return Err("resolution immutable disabled profile selection changed".into());
    }
    let public = vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap(),
        serde_json::to_value(observations).unwrap(),
    ];
    if !public_is_redacted(&public, &fixture.cli_token) {
        return Err("resolution public evidence retained a forbidden value".into());
    }
    Ok(public)
}

fn inspect_trace(
    state: &ResolutionState,
    cli_token: &str,
    conflict: bool,
    expected_count: usize,
) -> Result<(), String> {
    let requests = state.requests();
    let all_expected = [
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{SEARCH_ID}%2F"),
        format!("/api/catalog/fetch?url=https%3A%2F%2Fwww.imdb.com%2Ftitle%2F{SEARCH_ID}%2F"),
        "/api/catalog/search?query=Exact+candidate&category=movie&page=1".into(),
        format!("/api/movie/{SEARCH_UUID}"),
        format!("/api/me/shelf/item/{SEARCH_UUID}"),
    ];
    if !matches!(expected_count, 1 | 3 | 4 | 5) {
        return Err("resolution trace gate was invalid".into());
    }
    let expected = &all_expected[..expected_count];
    if requests.len() != expected_count || requests.len() != expected.len() {
        return Err("resolution request count changed".into());
    }
    for (index, (request, uri)) in requests.iter().zip(expected).enumerate() {
        let personal = !conflict && expected_count == 5 && index == 4;
        if request.method != "GET"
            || request.uri != *uri
            || request.cookie
            || request.authorization.as_deref()
                != personal.then_some("Bearer neodb-fixture-token-e5a")
            || request.authorization.as_deref() == Some(format!("Bearer {cli_token}").as_str())
        {
            return Err("resolution method, path, auth, or cookie trace changed".into());
        }
    }
    Ok(())
}

async fn run_case(
    reader: &ProtocolReader,
    assets: &std::path::Path,
    case: &'static str,
    conflict: bool,
) -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|_| "resolution temporary data directory failed")?;
    let (server, upstream_base) = ResolutionServer::start(conflict).await?;
    let state = server.state.clone();
    let mut fixture = None;
    let mut all_ids = Vec::<String>::new();
    let result = async {
        let (initial, bootstrap_url) =
            start_fixture(&temp, &upstream_base, assets.to_path_buf()).await?;
        fixture = Some(initial);
        let baseline = retained_state(fixture.as_ref().unwrap()).await?;
        send(json!({"phase":"ready", "case":case, "bootstrapUrl":bootstrap_url}))?;

        let native = reader.receive("native")?;
        let native_id = job_id(&native)?;
        all_ids.push(native_id.clone());
        let current = fixture
            .as_ref()
            .ok_or("resolution core fixture unavailable")?;
        let native_history = inspect_job(
            current,
            &native_id,
            &upstream_base,
            ExpectedOutcome::Unsupported,
        )
        .await?;
        inspect_trace(&state, &current.cli_token, conflict, 1)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 0
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
        {
            return Err("native phase performed a credential read or write".into());
        }
        send(json!({"phase":"native", "case":case}))?;

        let staged = reader.receive("search_staged")?;
        if job_id(&staged)? != native_id
            || inspect_job(
                current,
                &native_id,
                &upstream_base,
                ExpectedOutcome::Unsupported,
            )
            .await?
                != native_history
        {
            return Err("search staging changed native history".into());
        }
        inspect_trace(&state, &current.cli_token, conflict, 1)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 0 {
            return Err("search staging read a credential".into());
        }
        send(json!({"phase":"staged", "case":case, "kind":"search"}))?;

        let searched = reader.receive("searched")?;
        let search_id = job_id(&searched)?;
        if search_id == native_id {
            return Err("search submission reused the native job id".into());
        }
        all_ids.push(search_id.clone());
        if inspect_job(
            current,
            &native_id,
            &upstream_base,
            ExpectedOutcome::Unsupported,
        )
        .await?
            != native_history
        {
            return Err("search submission changed native history".into());
        }
        let search_history =
            inspect_job(current, &search_id, &upstream_base, ExpectedOutcome::Search).await?;
        inspect_trace(&state, &current.cli_token, conflict, 3)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 0 {
            return Err("search candidate resolution read a credential".into());
        }
        send(json!({"phase":"searched", "case":case}))?;

        let staged = reader.receive("manual_staged")?;
        if job_id(&staged)? != search_id
            || inspect_job(
                current,
                &native_id,
                &upstream_base,
                ExpectedOutcome::Unsupported,
            )
            .await?
                != native_history
            || inspect_job(current, &search_id, &upstream_base, ExpectedOutcome::Search).await?
                != search_history
        {
            return Err("manual staging changed prior history".into());
        }
        inspect_trace(&state, &current.cli_token, conflict, 3)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 0 {
            return Err("manual staging read a credential".into());
        }
        send(json!({"phase":"staged", "case":case, "kind":"manual"}))?;

        let retained = reader.receive("retained")?;
        let manual_id = job_id(&retained)?;
        if manual_id == native_id || manual_id == search_id {
            return Err("manual submission reused a prior job id".into());
        }
        all_ids.push(manual_id.clone());
        let expected_manual = if conflict {
            ExpectedOutcome::ManualConflict
        } else {
            ExpectedOutcome::ManualMissing
        };
        if inspect_job(
            current,
            &native_id,
            &upstream_base,
            ExpectedOutcome::Unsupported,
        )
        .await?
            != native_history
            || inspect_job(current, &search_id, &upstream_base, ExpectedOutcome::Search).await?
                != search_history
        {
            return Err("manual submission changed prior histories".into());
        }
        let manual_history =
            inspect_job(current, &manual_id, &upstream_base, expected_manual).await?;
        let request_count = if conflict { 4 } else { 5 };
        let secret_reads = usize::from(!conflict);
        inspect_trace(&state, &current.cli_token, conflict, request_count)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
        {
            return Err("manual credential trace changed".into());
        }
        let preserved = retained_state(current).await?;
        if !same_retained_state(&baseline, &preserved) {
            return Err("resolution completion changed unrelated state".into());
        }
        let address = current
            .report
            .core_url
            .trim_start_matches("http://")
            .parse()
            .map_err(|_| "resolution core origin was invalid")?;
        let secrets = current.secrets.clone();
        let cli_token = current.cli_token.clone();
        let report = fixture
            .take()
            .ok_or("resolution core fixture unavailable")?
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
            .ok_or("resolution reopened fixture unavailable")?;
        if inspect_job(
            current,
            &native_id,
            &upstream_base,
            ExpectedOutcome::Unsupported,
        )
        .await?
            != native_history
            || inspect_job(current, &search_id, &upstream_base, ExpectedOutcome::Search).await?
                != search_history
            || inspect_job(current, &manual_id, &upstream_base, expected_manual).await?
                != manual_history
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err("resolution history or unrelated state changed after reopen".into());
        }
        inspect_trace(&state, &current.cli_token, conflict, request_count)?;
        send(json!({"phase":"reopened", "case":case}))?;

        let finish = reader.receive("finish")?;
        if job_id(&finish)? != manual_id {
            return Err("resolution finish phase changed manual job id".into());
        }
        for id in &all_ids {
            if PreviewJobRepository::load(&current.storage, id.clone())
                .await
                .map_err(|error| error.code)?
                .is_some()
                || current.client.status(id).await.is_ok()
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
            {
                return Err("browser deletion left resolution job history".into());
            }
        }
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != secret_reads
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || !same_retained_state(&preserved, &retained_state(current).await?)
        {
            return Err("resolution deletion changed unrelated state or performed I/O".into());
        }
        inspect_trace(&state, &current.cli_token, conflict, request_count)?;
        let auth = status(&current.report.core_url, current.secrets.as_ref())
            .await
            .map_err(|error| error.code)?;
        if !auth.authenticated || auth.client.id != current.report.client_id {
            return Err("resolution original paired client did not remain authenticated".into());
        }
        Ok((cli_token, preserved, secret_reads, request_count))
    }
    .await;

    let final_secrets = fixture.as_ref().map(|current| current.secrets.clone());
    if let Some(current) = fixture.take() {
        current.shutdown().await;
    }
    let post = match (&result, final_secrets) {
        (Ok((cli_token, preserved, secret_reads, request_count)), Some(secrets)) => {
            scan_closed_database_with_markers(
                &temp,
                cli_token,
                &[RESOLUTION_RAW_MARKER, RESOLUTION_UPSTREAM_MARKER],
            )
            .and_then(|_| inspect_trace(&state, cli_token, conflict, *request_count))
            .and_then(|_| {
                if secrets.neodb_reads.load(Ordering::SeqCst) == *secret_reads
                    && secrets.neodb_writes.load(Ordering::SeqCst) == 0
                    && secret_values(&secrets) == preserved.secrets
                {
                    Ok(())
                } else {
                    Err("resolution shutdown changed credentials or secret values".into())
                }
            })
        }
        _ => Ok(()),
    };
    let server_result = server.shutdown().await;
    result.map(|_| ()).and(post).and(server_result)?;
    send(json!({"phase":"done", "case":case}))
}

async fn run_resolution_host() -> Result<(), String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/webui/dist")
        .canonicalize()
        .map_err(|_| "built WebUI assets were not found")?;
    let reader = ProtocolReader::start();
    for (case, conflict) in CASES {
        run_case(&reader, &assets, case, conflict).await?;
    }
    send(json!({"phase":"all_done"}))
}

#[test]
#[ignore = "driven explicitly by tests/preview-native-browser-smoke.mjs after WebUI build"]
fn browser_native_resolution_host() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_resolution_host()) {
        panic!("E5i browser resolution host failed: {error}");
    }
}
