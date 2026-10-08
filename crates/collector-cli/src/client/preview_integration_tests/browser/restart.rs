//! Ignored native-browser host for Running interruption and explicit fresh resume.
use super::*;
use crate::client::preview::PreviewFailure;
use crate::client::preview_integration_tests::restart::{
    LATER, RESTART_RAW_MARKER, RESTART_UPSTREAM_MARKER, RestartState, restart_upstream,
};
use collector_core::preview_jobs::PreviewResolutionInput;
use collector_domain::preview::{PreviewResultPage, PreviewResultView};

const CASE: &str = "current_schema";

struct RestartServer {
    state: Arc<RestartState>,
    shutdown: Option<oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl RestartServer {
    async fn start() -> Result<(Self, String), String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "restart upstream listener bind failed")?;
        let upstream_base = format!(
            "http://{}",
            listener
                .local_addr()
                .map_err(|_| "restart upstream listener address failed")?
        );
        let state = Arc::new(RestartState::new(upstream_base.clone()));
        let app = Router::new()
            .fallback(restart_upstream)
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
            .map_err(|_| "restart upstream server join failed".to_owned())?
            .map_err(|_| "restart upstream server failed".to_owned())
    }
}

fn redacted(values: &[Value], cli_token: &str) -> bool {
    values.iter().all(|value| {
        let encoded = serde_json::to_string(value).unwrap();
        [
            CREDENTIAL_REF,
            NEODB_TOKEN,
            cli_token,
            RAW_COLUMN_MARKER,
            UPSTREAM_MARKER,
            RESTART_RAW_MARKER,
            RESTART_UPSTREAM_MARKER,
        ]
        .into_iter()
        .all(|forbidden| !encoded.contains(forbidden))
    })
}

fn inspect_trace(
    state: &RestartState,
    cli_token: &str,
    expected_count: usize,
) -> Result<(), String> {
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
    if requests.len() != expected_count {
        return Err("restart native request count changed".into());
    }
    for (index, request) in requests.iter().enumerate() {
        let personal = matches!(index, 2 | 6 | 9);
        if request.method != "GET"
            || request.uri != expected[index]
            || request.cookie
            || request.authorization.as_deref()
                != personal.then_some("Bearer neodb-fixture-token-e5a")
            || request.authorization.as_deref() == Some(format!("Bearer {cli_token}").as_str())
        {
            return Err("restart method, path, auth, or cookie trace changed".into());
        }
    }
    Ok(())
}

async fn original_inputs(
    fixture: &CoreFixture,
    job_id: &str,
) -> Result<Vec<collector_domain::observation::Observation>, String> {
    let observations = fixture
        .storage
        .observations(job_id.into(), 0, 100)
        .await
        .map_err(|error| error.code)?;
    if observations.len() != 2 {
        return Err("restart normalized observation count changed".into());
    }
    for (ordinal, observation) in observations.iter().enumerate() {
        if observation.schema_version != 1
            || observation.source_connector_id != "imdb:ratings_csv"
            || observation.source_record_id != IDS[ordinal]
            || observation.observed_at != NOW.to_string()
            || observation.item.media_type != "movie"
            || observation.item.canonical_url
                != format!("https://www.imdb.com/title/{}/", IDS[ordinal])
            || observation
                .item
                .external_ids
                .get("imdb:title")
                .map(String::as_str)
                != Some(IDS[ordinal])
            || observation.item.title_hint.as_deref()
                != Some(if ordinal == 0 {
                    "First"
                } else {
                    "中文重启"
                })
            || !matches!(
                observation.state.get("rating"),
                Some(ScalarValue::Number(value))
                    if *value == if ordinal == 0 { 8.0 } else { 7.0 }
            )
            || fixture
                .storage
                .resolution_input(job_id.into(), ordinal as u32)
                .await
                .map_err(|error| error.code)?
                != PreviewResolutionInput::Native
        {
            return Err("restart normalized observation or Native input changed".into());
        }
    }
    if serde_json::to_string(&observations)
        .unwrap()
        .contains(RESTART_RAW_MARKER)
    {
        return Err("restart normalized observations retained the discarded raw column".into());
    }
    Ok(observations)
}

fn stored_sink_matches(stored: &SinkValue, public: &PreviewSinkView) -> bool {
    match (stored, public) {
        (SinkValue::Known(stored), PreviewSinkView::Known { value }) => stored == value,
        (SinkValue::Empty, PreviewSinkView::Empty)
        | (SinkValue::Unknown, PreviewSinkView::Unknown) => true,
        _ => false,
    }
}

fn stored_status_matches(stored: ComparisonStatus, public: PreviewComparisonStatusView) -> bool {
    matches!(
        (stored, public),
        (
            ComparisonStatus::Accepted,
            PreviewComparisonStatusView::Accepted
        ) | (
            ComparisonStatus::Skipped,
            PreviewComparisonStatusView::Skipped
        ) | (
            ComparisonStatus::Rejected,
            PreviewComparisonStatusView::Rejected
        ) | (
            ComparisonStatus::NeedsReview,
            PreviewComparisonStatusView::NeedsReview
        )
    )
}

async fn inspect_stored_partial(
    storage: &Storage,
    job_id: &str,
    original: &PreviewResultView,
) -> Result<(), String> {
    let stored = storage
        .results(job_id.into(), 1, 0, 100)
        .await
        .map_err(|error| error.code)?;
    if stored.len() != 1 {
        return Err("restart stored partial result count changed".into());
    }
    let stored = &stored[0];
    let StoredPreviewOutcome::Compared {
        item,
        observed_at,
        comparison,
        provenance,
    } = stored.outcome()
    else {
        return Err("restart stored partial outcome was not compared".into());
    };
    let PreviewOutcomeView::Compared {
        item: original_item,
        observed_at: original_observed_at,
        comparison: original_comparison,
        provenance: original_provenance,
    } = &original.outcome
    else {
        return Err("restart original partial outcome was not compared".into());
    };
    let fields_match = comparison.fields.len() == original_comparison.fields.len()
        && comparison
            .fields
            .iter()
            .zip(&original_comparison.fields)
            .all(|(stored, public)| {
                stored.rule_id == public.rule_id
                    && stored.field == public.field
                    && stored.suggestion == public.suggestion
                    && stored_sink_matches(&stored.sink, &public.sink)
                    && stored_status_matches(stored.status, public.status)
                    && stored.reason == public.reason
            });
    let claims_match = item.external_ids.len() == original_item.external_ids.len()
        && item
            .external_ids
            .iter()
            .zip(&original_item.external_ids)
            .all(|((scheme, value), public)| scheme == &public.scheme && value == &public.value);
    if stored.ordinal() != original.ordinal
        || stored.attempt() != original.attempt
        || stored.committed_at() != original.committed_at
        || stored.source_connector_id() != original.source_connector_id
        || stored.source_record_id() != original.source_record_id
        || stored.source_fingerprint() != original.source_fingerprint
        || stored.profile_id() != original.profile_id
        || stored.profile_version() != original.profile_version
        || stored.base_url() != original.base_url
        || stored.account() != original.account
        || item.uuid != original_item.uuid
        || item.canonical_url != original_item.canonical_url
        || item.detail_url != original_item.detail_url
        || item.media_type != original_item.media_type
        || item.model != original_item.model
        || item.parent_uuid != original_item.parent_uuid
        || !claims_match
        || observed_at != original_observed_at
        || comparison.profile_id != original_comparison.profile_id
        || comparison.profile_version != original_comparison.profile_version
        || !fields_match
        || !matches!(
            (provenance, original_provenance),
            (
                PreviewRecordProvenance::Automatic,
                PreviewProvenanceView::Automatic
            ) | (
                PreviewRecordProvenance::Manual,
                PreviewProvenanceView::Manual
            )
        )
    {
        return Err("restart stored partial fingerprint, item, snapshot, or fields changed".into());
    }
    Ok(())
}

async fn inspect_partial(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
) -> Result<(Vec<Value>, Vec<collector_domain::observation::Observation>), String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "restart Running status failed")?;
    let first = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "restart partial offset-zero page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "restart partial offset-one page failed")?;
    if !matches!(job.status, PreviewJobStatusView::Running)
        || job.id != job_id
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
        || first.job_id != job_id
        || first.attempt != 1
        || first.offset != 0
        || first.limit != 1
        || first.items.len() != 1
        || second.job_id != job_id
        || second.attempt != 1
        || second.offset != 1
        || second.limit != 1
        || !second.items.is_empty()
        || !first.historical
        || first.executable
        || !second.historical
        || second.executable
    {
        return Err("restart Running metadata or exact partial pages changed".into());
    }
    let observations = original_inputs(fixture, job_id).await?;
    let row = &first.items[0];
    let PreviewOutcomeView::Compared {
        item,
        observed_at,
        comparison,
        provenance,
    } = &row.outcome
    else {
        return Err("restart initial row was not compared".into());
    };
    let rating = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating")
        .ok_or("restart initial rating comparison was missing")?;
    let shelf = comparison
        .fields
        .iter()
        .find(|field| field.rule_id == "rating_implies_complete")
        .ok_or("restart initial disabled shelf comparison was missing")?;
    if row.ordinal != 0
        || row.attempt != 1
        || row.committed_at != NOW
        || row.source_connector_id != "imdb:ratings_csv"
        || row.source_record_id != IDS[0]
        || row.source_fingerprint != observations[0].idempotency_key
        || row.profile_id != "imdb_ratings"
        || row.profile_version != 1
        || row.base_url != upstream_base
        || row.account != ACCOUNT
        || !row.historical
        || row.executable
        || item.uuid != UUIDS[0]
        || item.canonical_url != format!("{upstream_base}/movie/{}", UUIDS[0])
        || item.detail_url != format!("{upstream_base}/api/movie/{}", UUIDS[0])
        || item.media_type != "movie"
        || item.model != "Movie"
        || item.parent_uuid.is_some()
        || item.external_ids.len() != 1
        || item.external_ids[0].scheme != "imdb:title"
        || item.external_ids[0].value != IDS[0]
        || *observed_at != NOW
        || comparison.profile_id != "imdb_ratings"
        || comparison.profile_version != 1
        || comparison.fields.len() != 2
        || !matches!(provenance, PreviewProvenanceView::Automatic)
        || rating.field != "rating_grade"
        || !matches!(rating.suggestion, Some(ScalarValue::Number(8.0)))
        || !matches!(
            rating.sink,
            PreviewSinkView::Known {
                value: ScalarValue::Number(8.0)
            }
        )
        || !matches!(rating.status, PreviewComparisonStatusView::Skipped)
        || rating.reason != "already_equal"
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
    {
        return Err("restart initial identity, fingerprint, or comparison changed".into());
    }
    inspect_stored_partial(&fixture.storage, job_id, row).await?;
    let public = vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap(),
        serde_json::to_value(&observations).unwrap(),
    ];
    if !redacted(&public, &fixture.cli_token) {
        return Err("restart partial public evidence retained a forbidden value".into());
    }
    Ok((public, observations))
}

async fn inspect_interrupted(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
    partial_history: &[Value],
    observations: &[collector_domain::observation::Observation],
) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "restart Interrupted status failed")?;
    let first = fixture
        .client
        .results(job_id, 1, 0, 1)
        .await
        .map_err(|_| "restart Interrupted offset-zero page failed")?;
    let second = fixture
        .client
        .results(job_id, 1, 1, 1)
        .await
        .map_err(|_| "restart Interrupted offset-one page failed")?;
    if !matches!(job.status, PreviewJobStatusView::Interrupted)
        || job.id != job_id
        || job.attempt != 1
        || job.created_at != NOW
        || job.updated_at != LATER
        || job.expires_at != NOW + 30 * 86_400
        || job.observation_count != 2
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.base_url != upstream_base
        || job.account != ACCOUNT
        || !matches!(job.binding_state, PreviewBindingState::Current)
        || !job.historical
        || job.executable
        || first != serde_json::from_value(partial_history[1].clone()).unwrap()
        || second != serde_json::from_value(partial_history[2].clone()).unwrap()
        || original_inputs(fixture, job_id).await? != observations
    {
        return Err(
            "restart recovery changed immutable metadata, inputs, or partial history".into(),
        );
    }
    inspect_stored_partial(&fixture.storage, job_id, &first.items[0]).await?;
    let public = vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap(),
        serde_json::to_value(observations).unwrap(),
    ];
    if !redacted(&public, &fixture.cli_token) {
        return Err("restart Interrupted public evidence retained a forbidden value".into());
    }
    Ok(public)
}

async fn inspect_fresh_held(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
) -> Result<(), String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "restart resumed Running status failed")?;
    let first = fixture
        .client
        .results(job_id, 2, 0, 1)
        .await
        .map_err(|_| "restart resumed offset-zero page failed")?;
    let second = fixture
        .client
        .results(job_id, 2, 1, 1)
        .await
        .map_err(|_| "restart resumed offset-one page failed")?;
    if !matches!(job.status, PreviewJobStatusView::Running)
        || job.id != job_id
        || job.attempt != 2
        || job.created_at != NOW
        || job.updated_at != LATER
        || job.expires_at != NOW + 30 * 86_400
        || job.observation_count != 2
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.base_url != upstream_base
        || job.account != ACCOUNT
        || !matches!(job.binding_state, PreviewBindingState::Current)
        || !job.historical
        || job.executable
        || first.job_id != job_id
        || first.attempt != 2
        || first.offset != 0
        || first.limit != 1
        || !first.historical
        || first.executable
        || !first.items.is_empty()
        || second.job_id != job_id
        || second.attempt != 2
        || second.offset != 1
        || second.limit != 1
        || !second.historical
        || second.executable
        || !second.items.is_empty()
        || !fixture
            .storage
            .results(job_id.into(), 1, 0, 100)
            .await
            .map_err(|error| error.code)?
            .is_empty()
        || !fixture
            .storage
            .results(job_id.into(), 2, 0, 100)
            .await
            .map_err(|error| error.code)?
            .is_empty()
    {
        return Err("restart fresh attempt did not atomically clear old results".into());
    }
    match fixture.client.results(job_id, 1, 0, 1).await {
        Err(PreviewFailure::Error(error)) if error.code == "preview_attempt_changed" => {}
        _ => return Err("restart obsolete attempt-1 GET was not rejected".into()),
    }
    Ok(())
}

async fn inspect_completed(
    fixture: &CoreFixture,
    job_id: &str,
    upstream_base: &str,
    observations: &[collector_domain::observation::Observation],
) -> Result<Vec<Value>, String> {
    let job = fixture
        .client
        .status(job_id)
        .await
        .map_err(|_| "restart Completed status failed")?;
    let pages = [
        fixture
            .client
            .results(job_id, 2, 0, 1)
            .await
            .map_err(|_| "restart completed offset-zero page failed")?,
        fixture
            .client
            .results(job_id, 2, 1, 1)
            .await
            .map_err(|_| "restart completed offset-one page failed")?,
        fixture
            .client
            .results(job_id, 2, 2, 1)
            .await
            .map_err(|_| "restart completed offset-two page failed")?,
    ];
    if !matches!(job.status, PreviewJobStatusView::Completed)
        || job.id != job_id
        || job.attempt != 2
        || job.created_at != NOW
        || job.updated_at != LATER
        || job.expires_at != NOW + 30 * 86_400
        || job.observation_count != 2
        || job.profile_id != "imdb_ratings"
        || job.profile_version != 1
        || job.base_url != upstream_base
        || job.account != ACCOUNT
        || !matches!(job.binding_state, PreviewBindingState::Current)
        || !job.historical
        || job.executable
        || pages[0].items.len() != 1
        || pages[1].items.len() != 1
        || !pages[2].items.is_empty()
        || pages.iter().enumerate().any(|(offset, page)| {
            page.job_id != job_id
                || page.attempt != 2
                || page.offset != offset as u32
                || page.limit != 1
                || !page.historical
                || page.executable
        })
        || original_inputs(fixture, job_id).await? != observations
    {
        return Err("restart completed metadata, pages, or original inputs changed".into());
    }
    let stored = fixture
        .storage
        .results(job_id.into(), 2, 0, 100)
        .await
        .map_err(|error| error.code)?;
    if stored.len() != 2 {
        return Err("restart stored fresh results were incomplete".into());
    }
    for ordinal in 0..2 {
        let row = &pages[ordinal].items[0];
        let PreviewOutcomeView::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } = &row.outcome
        else {
            return Err("restart fresh public outcome was not compared".into());
        };
        let StoredPreviewOutcome::Compared {
            item: stored_item,
            observed_at: stored_observed_at,
            comparison: stored_comparison,
            provenance: stored_provenance,
        } = stored[ordinal].outcome()
        else {
            return Err("restart fresh stored outcome was not compared".into());
        };
        let rating = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .ok_or("restart fresh rating comparison was missing")?;
        let stored_rating = stored_comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating")
            .ok_or("restart fresh stored rating comparison was missing")?;
        let shelf = comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating_implies_complete")
            .ok_or("restart fresh disabled shelf comparison was missing")?;
        let stored_shelf = stored_comparison
            .fields
            .iter()
            .find(|field| field.rule_id == "rating_implies_complete")
            .ok_or("restart fresh stored disabled shelf comparison was missing")?;
        let suggested = if ordinal == 0 { 8.0 } else { 7.0 };
        let known = if ordinal == 0 { 5.0 } else { 4.0 };
        if row.ordinal != ordinal as u32
            || row.attempt != 2
            || row.committed_at != LATER
            || row.source_connector_id != "imdb:ratings_csv"
            || row.source_record_id != IDS[ordinal]
            || row.source_fingerprint != observations[ordinal].idempotency_key
            || row.profile_id != "imdb_ratings"
            || row.profile_version != 1
            || row.base_url != upstream_base
            || row.account != ACCOUNT
            || !row.historical
            || row.executable
            || stored[ordinal].ordinal() != ordinal as u32
            || stored[ordinal].source_fingerprint() != observations[ordinal].idempotency_key
            || item.uuid != UUIDS[ordinal]
            || item.canonical_url != format!("{upstream_base}/movie/{}", UUIDS[ordinal])
            || item.detail_url != format!("{upstream_base}/api/movie/{}", UUIDS[ordinal])
            || item.media_type != "movie"
            || item.model != "Movie"
            || item.parent_uuid.is_some()
            || item.external_ids.len() != 1
            || item.external_ids[0].scheme != "imdb:title"
            || item.external_ids[0].value != IDS[ordinal]
            || stored_item.uuid != UUIDS[ordinal]
            || stored_item.canonical_url != format!("{upstream_base}/movie/{}", UUIDS[ordinal])
            || stored_item.detail_url != format!("{upstream_base}/api/movie/{}", UUIDS[ordinal])
            || stored_item.media_type != "movie"
            || stored_item.model != "Movie"
            || stored_item.parent_uuid.is_some()
            || stored_item.external_ids != vec![("imdb:title".into(), IDS[ordinal].into())]
            || *observed_at != LATER
            || *stored_observed_at != LATER
            || comparison.profile_id != "imdb_ratings"
            || comparison.profile_version != 1
            || stored_comparison.profile_id != "imdb_ratings"
            || stored_comparison.profile_version != 1
            || comparison.fields.len() != 2
            || stored_comparison.fields.len() != 2
            || !matches!(provenance, PreviewProvenanceView::Automatic)
            || !matches!(stored_provenance, PreviewRecordProvenance::Automatic)
            || rating.field != "rating_grade"
            || stored_rating.field != "rating_grade"
            || !matches!(rating.suggestion, Some(ScalarValue::Number(value)) if value == suggested)
            || !matches!(stored_rating.suggestion, Some(ScalarValue::Number(value)) if value == suggested)
            || !matches!(rating.sink, PreviewSinkView::Known { value: ScalarValue::Number(value) } if value == known)
            || !matches!(stored_rating.sink, SinkValue::Known(ScalarValue::Number(value)) if value == known)
            || !matches!(rating.status, PreviewComparisonStatusView::NeedsReview)
            || stored_rating.status != ComparisonStatus::NeedsReview
            || rating.reason != "sink_conflict_needs_review"
            || stored_rating.reason != "sink_conflict_needs_review"
            || shelf.field != "shelf_type"
            || shelf.suggestion.is_some()
            || !matches!(shelf.sink, PreviewSinkView::Known { value: ScalarValue::Text(ref value) } if value == "complete")
            || !matches!(shelf.status, PreviewComparisonStatusView::Skipped)
            || shelf.reason != "rule_disabled"
            || stored_shelf.field != "shelf_type"
            || stored_shelf.suggestion.is_some()
            || !matches!(stored_shelf.sink, SinkValue::Known(ScalarValue::Text(ref value)) if value == "complete")
            || stored_shelf.status != ComparisonStatus::Skipped
            || stored_shelf.reason != "rule_disabled"
        {
            return Err(
                "restart fresh public/stored identity or ordered comparison changed".into(),
            );
        }
    }
    let public = vec![
        serde_json::to_value(job).unwrap(),
        serde_json::to_value(&pages[0]).unwrap(),
        serde_json::to_value(&pages[1]).unwrap(),
        serde_json::to_value(&pages[2]).unwrap(),
        serde_json::to_value(observations).unwrap(),
    ];
    if !redacted(&public, &fixture.cli_token) {
        return Err("restart completed public evidence retained a forbidden value".into());
    }
    Ok(public)
}

async fn run_case(reader: &ProtocolReader, assets: &std::path::Path) -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|_| "restart temporary data directory failed")?;
    let (server, upstream_base) = RestartServer::start().await?;
    let state = server.state.clone();
    let mut fixture = None;
    let result = async {
        let (initial, bootstrap_url) =
            start_fixture(&temp, &upstream_base, assets.to_path_buf()).await?;
        fixture = Some(initial);
        let baseline = retained_state(fixture.as_ref().unwrap()).await?;
        let initial_secrets = secret_values(&fixture.as_ref().unwrap().secrets);
        send(json!({"phase":"ready", "case":CASE, "bootstrapUrl":bootstrap_url}))?;

        let native = reader.receive("native")?;
        let id = job_id(&native)?;
        state.try_wait_for_initial().await?;
        let current = fixture.as_ref().ok_or("restart Core fixture unavailable")?;
        let (partial_history, observations) = inspect_partial(current, &id, &upstream_base).await?;
        inspect_trace(&state, &current.cli_token, 4)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 1
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || secret_values(&current.secrets) != initial_secrets
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err(
                "restart initial partial state changed credentials or configuration".into(),
            );
        }
        send(json!({"phase":"pending", "case":CASE}))?;

        let restart = reader.receive("restart")?;
        if job_id(&restart)? != id {
            return Err("restart phase changed job id".into());
        }
        let current = fixture.as_ref().ok_or("restart Core fixture unavailable")?;
        let address = current
            .report
            .core_url
            .trim_start_matches("http://")
            .parse()
            .map_err(|_| "restart Core origin was invalid")?;
        let secrets = current.secrets.clone();
        let cli_token = current.cli_token.clone();
        let report = fixture
            .take()
            .ok_or("restart Core fixture unavailable")?
            .shutdown()
            .await;
        state.release_initial();
        let _responded_or_dropped = state.try_wait_for_initial_completion().await?;
        inspect_trace(&state, &cli_token, 4)?;
        scan_closed_database_with_markers(
            &temp,
            &cli_token,
            &[RESTART_RAW_MARKER, RESTART_UPSTREAM_MARKER],
        )?;

        let closed = Storage::open(temp.path())
            .await
            .map_err(|error| error.code)?;
        let original_first: PreviewResultPage =
            serde_json::from_value(partial_history[1].clone()).unwrap();
        let pre_recovery_result = async {
            let pre_recovery = PreviewJobRepository::load(&closed, id.clone())
                .await
                .map_err(|error| error.code)?
                .ok_or("restart pre-recovery job was missing")?;
            if pre_recovery.status() != PreviewJobStatus::Running
                || pre_recovery.attempt() != 1
                || pre_recovery.created_at() != NOW
                || pre_recovery.expires_at() != NOW + 30 * 86_400
            {
                return Err("restart pre-recovery database did not retain Running metadata".into());
            }
            inspect_stored_partial(&closed, &id, &original_first.items[0]).await
        }
        .await;
        closed.close().await;
        pre_recovery_result?;
        scan_closed_database_with_markers(
            &temp,
            &cli_token,
            &[RESTART_RAW_MARKER, RESTART_UPSTREAM_MARKER],
        )?;

        fixture = Some(
            reopen_fixture_at(
                &temp,
                address,
                assets.to_path_buf(),
                secrets,
                report,
                cli_token.clone(),
                LATER,
            )
            .await?,
        );
        let current = fixture
            .as_ref()
            .ok_or("restart recovered Core fixture unavailable")?;
        let interrupted = inspect_interrupted(
            current,
            &id,
            &upstream_base,
            &partial_history,
            &observations,
        )
        .await?;
        inspect_trace(&state, &current.cli_token, 4)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 1
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || secret_values(&current.secrets) != initial_secrets
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err("restart recovery performed I/O or changed retained state".into());
        }
        state.set_changed();
        send(json!({"phase":"restarted", "case":CASE}))?;

        let inspected = reader.receive("inspected")?;
        if job_id(&inspected)? != id {
            return Err("inspected phase changed job id".into());
        }
        let current = fixture.as_ref().ok_or("restart Core fixture unavailable")?;
        if inspect_interrupted(
            current,
            &id,
            &upstream_base,
            &partial_history,
            &observations,
        )
        .await?
            != interrupted
            || state.requests().len() != 4
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != 1
        {
            return Err("restart browser inspection triggered remote or credential I/O".into());
        }
        send(json!({"phase":"inspected", "case":CASE}))?;

        let resumed = reader.receive("resumed")?;
        if job_id(&resumed)? != id {
            return Err("resumed phase changed job id".into());
        }
        state.try_wait_for_fresh().await?;
        let current = fixture.as_ref().ok_or("restart Core fixture unavailable")?;
        inspect_fresh_held(current, &id, &upstream_base).await?;
        inspect_trace(&state, &current.cli_token, 5)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 1
            || original_inputs(current, &id).await? != observations
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err("restart held fresh attempt changed inputs or retained state".into());
        }
        state.release_fresh();
        send(json!({"phase":"continuing", "case":CASE}))?;

        let retained = reader.receive("retained")?;
        if job_id(&retained)? != id {
            return Err("retained phase changed job id".into());
        }
        let current = fixture.as_ref().ok_or("restart Core fixture unavailable")?;
        let completed = inspect_completed(current, &id, &upstream_base, &observations).await?;
        inspect_trace(&state, &current.cli_token, 10)?;
        if current.secrets.neodb_reads.load(Ordering::SeqCst) != 3
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || secret_values(&current.secrets) != initial_secrets
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err("restart completed state changed credentials or configuration".into());
        }
        let address = current
            .report
            .core_url
            .trim_start_matches("http://")
            .parse()
            .map_err(|_| "restart terminal Core origin was invalid")?;
        let secrets = current.secrets.clone();
        let cli_token = current.cli_token.clone();
        let report = fixture
            .take()
            .ok_or("restart Core fixture unavailable")?
            .shutdown()
            .await;
        scan_closed_database_with_markers(
            &temp,
            &cli_token,
            &[RESTART_RAW_MARKER, RESTART_UPSTREAM_MARKER],
        )?;
        fixture = Some(
            reopen_fixture_at(
                &temp,
                address,
                assets.to_path_buf(),
                secrets,
                report,
                cli_token.clone(),
                LATER,
            )
            .await?,
        );
        let current = fixture
            .as_ref()
            .ok_or("restart terminal reopened fixture unavailable")?;
        if inspect_completed(current, &id, &upstream_base, &observations).await? != completed
            || state.requests().len() != 10
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != 3
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err("restart terminal reopen changed history or performed I/O".into());
        }
        send(json!({"phase":"reopened", "case":CASE}))?;

        let finish = reader.receive("finish")?;
        if job_id(&finish)? != id {
            return Err("restart finish phase changed job id".into());
        }
        let current = fixture
            .as_ref()
            .ok_or("restart terminal reopened fixture unavailable")?;
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
                .results(id.clone(), 2, 0, 100)
                .await
                .map_err(|error| error.code)?
                .is_empty()
            || state.requests().len() != 10
            || current.secrets.neodb_reads.load(Ordering::SeqCst) != 3
            || current.secrets.neodb_writes.load(Ordering::SeqCst) != 0
            || secret_values(&current.secrets) != initial_secrets
            || !same_retained_state(&baseline, &retained_state(current).await?)
        {
            return Err(
                "restart job-only deletion changed unrelated state or performed I/O".into(),
            );
        }
        match current.client.status(&id).await {
            Err(PreviewFailure::Error(error)) if error.code == "preview_job_not_found" => {}
            _ => return Err("restart deleted job did not return not-found".into()),
        }
        let auth = status(&current.report.core_url, current.secrets.as_ref())
            .await
            .map_err(|error| error.code)?;
        if !auth.authenticated || auth.client.id != current.report.client_id {
            return Err("restart original paired client did not remain authenticated".into());
        }
        Ok((cli_token, initial_secrets, current.secrets.clone()))
    }
    .await;

    // Always drain Core/runtime before releasing held upstream work. Afterwards both
    // gates are opened so every handler can finish before the upstream owner joins.
    if let Some(current) = fixture.take() {
        current.shutdown().await;
    }
    state.release_initial();
    state.release_fresh();
    let post = match &result {
        Ok((cli_token, initial_secrets, secrets)) => scan_closed_database_with_markers(
            &temp,
            cli_token,
            &[RESTART_RAW_MARKER, RESTART_UPSTREAM_MARKER],
        )
        .and_then(|_| {
            if state.requests().len() == 10 {
                Ok(())
            } else {
                Err("restart shutdown changed the upstream request trace".into())
            }
        })
        .and_then(|_| {
            if secret_values(secrets) == *initial_secrets
                && secrets.neodb_reads.load(Ordering::SeqCst) == 3
                && secrets.neodb_writes.load(Ordering::SeqCst) == 0
                && secrets.client_sets.load(Ordering::SeqCst) == 1
                && secrets.probe_sets.load(Ordering::SeqCst) == 1
                && secrets.probe_deletes.load(Ordering::SeqCst) == 1
            {
                Ok(())
            } else {
                Err("restart shutdown changed credentials or the secret baseline".into())
            }
        }),
        Err(_) => Ok(()),
    };
    let server_result = server.shutdown().await;
    result.map(|_| ()).and(post).and(server_result)?;
    send(json!({"phase":"done", "case":CASE}))
}

async fn run_restart_host() -> Result<(), String> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/webui/dist")
        .canonicalize()
        .map_err(|_| "built WebUI assets were not found")?;
    let reader = ProtocolReader::start();
    run_case(&reader, &assets).await?;
    send(json!({"phase":"all_done"}))
}

#[test]
#[ignore = "driven explicitly by tests/preview-native-browser-smoke.mjs after WebUI build"]
fn browser_native_restart_host() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    if let Err(error) = runtime.block_on(run_restart_host()) {
        panic!("E5j browser restart host failed: {error}");
    }
}
