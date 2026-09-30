use crate::{Storage, database_error};
use collector_core::{
    access::PortFuture,
    catalog::{CatalogItem, CatalogSearchCandidate, CatalogSearchPage},
    mapping,
    preview::{ComparedField, Comparison, ComparisonStatus, SinkValue},
    preview_jobs::{
        MAX_PREVIEW_OBSERVATION_BYTES, PreviewJobStatus, validate_imdb_observation,
        validate_transition,
    },
    preview_records::{PreviewRecordOutcome, PreviewRecordProvenance},
    preview_results::{
        MAX_PREVIEW_RESULT_BYTES, MAX_PREVIEW_RESULTS_TOTAL_BYTES, PreviewResultRepository,
        PreviewResultWrite, StoredPreviewMetadata, StoredPreviewOutcome, StoredPreviewResult,
        validate_historical_against_inputs, validate_result_page,
    },
};
use collector_domain::{
    ApiError,
    mapping::MappingProfile,
    observation::{Observation, ScalarValue},
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResultDto {
    schema_version: u32,
    source_connector_id: String,
    source_record_id: String,
    source_fingerprint: String,
    profile_id: String,
    profile_version: u32,
    base_url: String,
    account: String,
    outcome: OutcomeDto,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum OutcomeDto {
    Compared {
        item: ItemDto,
        #[serde(rename = "observedAt")]
        observed_at: i64,
        comparison: ComparisonDto,
        provenance: ProvenanceDto,
    },
    NeedsReview {
        item: ItemDto,
        reason: String,
    },
    SearchCandidates {
        candidates: Vec<CandidateDto>,
        #[serde(rename = "moreAvailable")]
        more_available: bool,
    },
    Unsupported,
    RateLimited,
    Unavailable,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProvenanceDto {
    Automatic,
    Manual,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ItemDto {
    uuid: String,
    canonical_url: String,
    detail_url: String,
    media_type: String,
    model: String,
    parent_uuid: Option<String>,
    external_ids: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CandidateDto {
    title: String,
    item: ItemDto,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonDto {
    profile_id: String,
    profile_version: u32,
    fields: Vec<FieldDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FieldDto {
    rule_id: String,
    field: String,
    suggestion: Option<ScalarValue>,
    sink: SinkDto,
    status: StatusDto,
    reason: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SinkDto {
    Known { value: ScalarValue },
    Empty,
    Unknown,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StatusDto {
    Accepted,
    Skipped,
    Rejected,
    NeedsReview,
}

impl From<&CatalogItem> for ItemDto {
    fn from(value: &CatalogItem) -> Self {
        Self {
            uuid: value.uuid.clone(),
            canonical_url: value.canonical_url.clone(),
            detail_url: value.detail_url.clone(),
            media_type: value.media_type.clone(),
            model: value.model.clone(),
            parent_uuid: value.parent_uuid.clone(),
            external_ids: value.external_ids.clone(),
        }
    }
}
impl From<ItemDto> for CatalogItem {
    fn from(value: ItemDto) -> Self {
        Self {
            uuid: value.uuid,
            canonical_url: value.canonical_url,
            detail_url: value.detail_url,
            media_type: value.media_type,
            model: value.model,
            parent_uuid: value.parent_uuid,
            external_ids: value.external_ids,
        }
    }
}

fn comparison_to_dto(value: &Comparison) -> ComparisonDto {
    ComparisonDto {
        profile_id: value.profile_id.clone(),
        profile_version: value.profile_version,
        fields: value
            .fields
            .iter()
            .map(|field| FieldDto {
                rule_id: field.rule_id.clone(),
                field: field.field.clone(),
                suggestion: field.suggestion.clone(),
                sink: match &field.sink {
                    SinkValue::Known(value) => SinkDto::Known {
                        value: value.clone(),
                    },
                    SinkValue::Empty => SinkDto::Empty,
                    SinkValue::Unknown => SinkDto::Unknown,
                },
                status: match field.status {
                    ComparisonStatus::Accepted => StatusDto::Accepted,
                    ComparisonStatus::Skipped => StatusDto::Skipped,
                    ComparisonStatus::Rejected => StatusDto::Rejected,
                    ComparisonStatus::NeedsReview => StatusDto::NeedsReview,
                },
                reason: field.reason.clone(),
            })
            .collect(),
    }
}

fn comparison_from_dto(value: ComparisonDto) -> Comparison {
    Comparison {
        profile_id: value.profile_id,
        profile_version: value.profile_version,
        fields: value
            .fields
            .into_iter()
            .map(|field| ComparedField {
                rule_id: field.rule_id,
                field: field.field,
                suggestion: field.suggestion,
                sink: match field.sink {
                    SinkDto::Known { value } => SinkValue::Known(value),
                    SinkDto::Empty => SinkValue::Empty,
                    SinkDto::Unknown => SinkValue::Unknown,
                },
                status: match field.status {
                    StatusDto::Accepted => ComparisonStatus::Accepted,
                    StatusDto::Skipped => ComparisonStatus::Skipped,
                    StatusDto::Rejected => ComparisonStatus::Rejected,
                    StatusDto::NeedsReview => ComparisonStatus::NeedsReview,
                },
                reason: field.reason,
            })
            .collect(),
    }
}

fn encode(write: &PreviewResultWrite) -> Result<String, ApiError> {
    let record = write.record();
    let outcome = match record.outcome() {
        PreviewRecordOutcome::Compared(value) => OutcomeDto::Compared {
            item: value.snapshot.item().into(),
            observed_at: value.snapshot.observed_at(),
            comparison: comparison_to_dto(&value.comparison),
            provenance: match value.provenance {
                PreviewRecordProvenance::Automatic => ProvenanceDto::Automatic,
                PreviewRecordProvenance::Manual => ProvenanceDto::Manual,
            },
        },
        PreviewRecordOutcome::NeedsReview { item, reason } => OutcomeDto::NeedsReview {
            item: item.into(),
            reason: reason.to_string(),
        },
        PreviewRecordOutcome::SearchCandidates(page) => OutcomeDto::SearchCandidates {
            candidates: page
                .candidates
                .iter()
                .map(|candidate| CandidateDto {
                    title: candidate.title.clone(),
                    item: (&candidate.item).into(),
                })
                .collect(),
            more_available: page.more_available,
        },
        PreviewRecordOutcome::Unsupported => OutcomeDto::Unsupported,
        PreviewRecordOutcome::RateLimited => OutcomeDto::RateLimited,
        PreviewRecordOutcome::Unavailable => OutcomeDto::Unavailable,
    };
    serde_json::to_string(&ResultDto {
        schema_version: 1,
        source_connector_id: record.source_connector_id().into(),
        source_record_id: record.source_record_id().into(),
        source_fingerprint: record.source_fingerprint().into(),
        profile_id: record.profile_id().into(),
        profile_version: record.profile_version(),
        base_url: record.base_url().into(),
        account: record.account().into(),
        outcome,
    })
    .map_err(|_| database_error())
}

fn decode(
    json: &str,
    ordinal: u32,
    attempt: u32,
    committed_at: i64,
) -> Result<StoredPreviewResult, ApiError> {
    if json.len() > MAX_PREVIEW_RESULT_BYTES {
        return Err(database_error());
    }
    let value: ResultDto = serde_json::from_str(json).map_err(|_| database_error())?;
    if value.schema_version != 1 {
        return Err(database_error());
    }
    let outcome = match value.outcome {
        OutcomeDto::Compared {
            item,
            observed_at,
            comparison,
            provenance,
        } => StoredPreviewOutcome::Compared {
            item: item.into(),
            observed_at,
            comparison: comparison_from_dto(comparison),
            provenance: match provenance {
                ProvenanceDto::Automatic => PreviewRecordProvenance::Automatic,
                ProvenanceDto::Manual => PreviewRecordProvenance::Manual,
            },
        },
        OutcomeDto::NeedsReview { item, reason } => StoredPreviewOutcome::NeedsReview {
            item: item.into(),
            reason,
        },
        OutcomeDto::SearchCandidates {
            candidates,
            more_available,
        } => StoredPreviewOutcome::SearchCandidates(CatalogSearchPage {
            candidates: candidates
                .into_iter()
                .map(|candidate| CatalogSearchCandidate {
                    title: candidate.title,
                    item: candidate.item.into(),
                })
                .collect(),
            more_available,
        }),
        OutcomeDto::Unsupported => StoredPreviewOutcome::Unsupported,
        OutcomeDto::RateLimited => StoredPreviewOutcome::RateLimited,
        OutcomeDto::Unavailable => StoredPreviewOutcome::Unavailable,
    };
    StoredPreviewResult::from_stored(
        StoredPreviewMetadata {
            ordinal,
            attempt,
            committed_at,
            source_connector_id: value.source_connector_id,
            source_record_id: value.source_record_id,
            source_fingerprint: value.source_fingerprint,
            profile_id: value.profile_id,
            profile_version: value.profile_version,
            base_url: value.base_url,
            account: value.account,
        },
        outcome,
    )
    .map_err(|_| database_error())
}

async fn stored_observation(
    conn: &mut SqliteConnection,
    job: &str,
    ordinal: u32,
) -> Result<Option<Observation>, ApiError> {
    let json: Option<String> = sqlx::query_scalar(
        "SELECT observation_json FROM preview_observations WHERE job_id=? AND ordinal=?",
    )
    .bind(job)
    .bind(ordinal)
    .fetch_optional(conn)
    .await
    .map_err(|_| database_error())?;
    json.map(|json| {
        if json.len() > MAX_PREVIEW_OBSERVATION_BYTES {
            return Err(database_error());
        }
        let value: Observation = serde_json::from_str(&json).map_err(|_| database_error())?;
        validate_imdb_observation(&value).map_err(|_| database_error())?;
        Ok(value)
    })
    .transpose()
}

fn write_guard(job: &collector_core::preview_jobs::PreviewJob, write: &PreviewResultWrite) -> bool {
    job.status() == PreviewJobStatus::Running
        && job.attempt() == write.attempt()
        && write.committed_at() >= job.updated_at()
        && write.committed_at() < job.expires_at()
        && job.is_bound_to(write.binding_proof())
        && job.profile_id() == write.profile_proof().id
        && job.profile_version() == write.profile_proof().version
        && write.record().source_connector_id() == write.observation_proof().source_connector_id
        && write.record().source_record_id() == write.observation_proof().source_record_id
        && write.record().source_fingerprint() == write.observation_proof().idempotency_key
}

fn coherent(
    job: &collector_core::preview_jobs::PreviewJob,
    profile: &MappingProfile,
    observation: &Observation,
    value: &StoredPreviewResult,
) -> Result<(), ApiError> {
    if value.attempt() != job.attempt()
        || value.committed_at() < job.created_at()
        || value.committed_at() > job.updated_at()
        || value.committed_at() >= job.expires_at()
        || value.profile_id() != profile.id
        || value.profile_version() != profile.version
        || value.base_url() != job.base_url()
        || value.account() != job.account()
        || value.source_connector_id() != observation.source_connector_id
        || value.source_record_id() != observation.source_record_id
        || value.source_fingerprint() != observation.idempotency_key
    {
        return Err(database_error());
    }
    if let StoredPreviewOutcome::Compared { observed_at, .. } = value.outcome()
        && (*observed_at < job.created_at() || *observed_at > value.committed_at())
    {
        return Err(database_error());
    }
    validate_historical_against_inputs(profile, observation, value).map_err(|_| database_error())
}

async fn validate_extent(
    conn: &mut SqliteConnection,
    job: &collector_core::preview_jobs::PreviewJob,
) -> Result<(), ApiError> {
    let row = sqlx::query("SELECT count(*) AS count,COALESCE(sum(length(CAST(r.result_json AS BLOB))),0) AS bytes,COALESCE(max(length(CAST(r.result_json AS BLOB))),0) AS max_bytes,min(r.ordinal) AS min_ordinal,max(r.ordinal) AS max_ordinal,min(r.attempt) AS min_attempt,max(r.attempt) AS max_attempt,min(r.committed_at) AS min_committed,max(r.committed_at) AS max_committed,COALESCE(sum(CASE WHEN o.ordinal IS NULL THEN 1 ELSE 0 END),0) AS orphan_count FROM preview_results r LEFT JOIN preview_observations o ON o.job_id=r.job_id AND o.ordinal=r.ordinal WHERE r.job_id=?")
        .bind(job.id()).fetch_one(conn).await.map_err(|_| database_error())?;
    let count: i64 = row.try_get("count").map_err(|_| database_error())?;
    let bytes: i64 = row.try_get("bytes").map_err(|_| database_error())?;
    let max_bytes: i64 = row.try_get("max_bytes").map_err(|_| database_error())?;
    if count < 0
        || count > i64::from(job.observation_count())
        || bytes < 0
        || max_bytes < 0
        || usize::try_from(max_bytes).map_err(|_| database_error())? > MAX_PREVIEW_RESULT_BYTES
        || usize::try_from(bytes).map_err(|_| database_error())? > MAX_PREVIEW_RESULTS_TOTAL_BYTES
    {
        return Err(database_error());
    }
    if count > 0 {
        let min_ordinal: i64 = row.try_get("min_ordinal").map_err(|_| database_error())?;
        let max_ordinal: i64 = row.try_get("max_ordinal").map_err(|_| database_error())?;
        let orphan_count: i64 = row.try_get("orphan_count").map_err(|_| database_error())?;
        let min_attempt: i64 = row.try_get("min_attempt").map_err(|_| database_error())?;
        let max_attempt: i64 = row.try_get("max_attempt").map_err(|_| database_error())?;
        let min_committed: i64 = row.try_get("min_committed").map_err(|_| database_error())?;
        let max_committed: i64 = row.try_get("max_committed").map_err(|_| database_error())?;
        if min_ordinal < 0
            || max_ordinal >= i64::from(job.observation_count())
            || orphan_count != 0
            || min_attempt != i64::from(job.attempt())
            || max_attempt != i64::from(job.attempt())
            || min_committed < job.created_at()
            || max_committed > job.updated_at()
            || max_committed >= job.expires_at()
        {
            return Err(database_error());
        }
    }
    if job.status() == PreviewJobStatus::Completed && count != i64::from(job.observation_count()) {
        return Err(database_error());
    }
    Ok(())
}

impl PreviewResultRepository for Storage {
    fn store(&self, write: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async move {
            let json = encode(&write)?;
            if json.len() > MAX_PREVIEW_RESULT_BYTES {
                return Err(ApiError::new(
                    "preview_result_too_large",
                    "A normalized preview result exceeds 64 KiB.",
                ));
            }
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            let Some(job) = super::preview_jobs::load_job(&mut tx, write.job_id()).await? else {
                return Ok(false);
            };
            if !write_guard(&job, &write) {
                return Ok(false);
            }
            if !super::preview_jobs::current_job_binding(&mut tx, write.job_id()).await? {
                return Ok(false);
            }
            let profile =
                super::preview_jobs::load_profile(&mut tx, job.profile_id(), job.profile_version())
                    .await?
                    .ok_or_else(database_error)?;
            if profile != *write.profile_proof() {
                return Ok(false);
            }
            let Some(observation) =
                stored_observation(&mut tx, write.job_id(), write.ordinal()).await?
            else {
                return Ok(false);
            };
            if observation != *write.observation_proof() {
                return Ok(false);
            }
            validate_extent(&mut tx, &job).await?;
            mapping::evaluate(&profile, &observation).map_err(|_| database_error())?;
            let historical = decode(
                &json,
                write.ordinal(),
                write.attempt(),
                write.committed_at(),
            )?;
            validate_historical_against_inputs(&profile, &observation, &historical)
                .map_err(|_| database_error())?;
            if let StoredPreviewOutcome::Compared { observed_at, .. } = historical.outcome()
                && *observed_at < job.created_at()
            {
                return Err(database_error());
            }
            let duplicate: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM preview_results WHERE job_id=? AND ordinal=?)",
            )
            .bind(write.job_id())
            .bind(write.ordinal())
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| database_error())?;
            if duplicate == 1 {
                return Ok(false);
            }
            let total: i64 = sqlx::query_scalar("SELECT COALESCE(sum(length(CAST(result_json AS BLOB))),0) FROM preview_results WHERE job_id=?")
                .bind(write.job_id()).fetch_one(&mut *tx).await.map_err(|_| database_error())?;
            let next = usize::try_from(total)
                .map_err(|_| database_error())?
                .checked_add(json.len())
                .ok_or_else(database_error)?;
            if next > MAX_PREVIEW_RESULTS_TOTAL_BYTES {
                return Err(ApiError::new(
                    "preview_results_too_large",
                    "Preview results exceed 32 MiB.",
                ));
            }
            let inserted = sqlx::query("INSERT INTO preview_results(job_id,ordinal,attempt,committed_at,result_json) VALUES(?,?,?,?,?) ON CONFLICT(job_id,ordinal) DO NOTHING")
                .bind(write.job_id()).bind(write.ordinal()).bind(write.attempt()).bind(write.committed_at()).bind(json)
                .execute(&mut *tx).await.map_err(|_| database_error())?;
            if inserted.rows_affected() == 1 {
                let updated = sqlx::query("UPDATE preview_jobs SET updated_at=? WHERE id=? AND status='running' AND attempt=? AND updated_at<=? AND expires_at>?")
                    .bind(write.committed_at()).bind(write.job_id()).bind(write.attempt())
                    .bind(write.committed_at()).bind(write.committed_at())
                    .execute(&mut *tx).await.map_err(|_| database_error())?;
                if updated.rows_affected() != 1 {
                    return Err(database_error());
                }
            }
            tx.commit().await.map_err(|_| database_error())?;
            Ok(inserted.rows_affected() == 1)
        })
    }

    fn results(
        &self,
        job_id: String,
        attempt: u32,
        offset: u32,
        limit: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>> {
        Box::pin(async move {
            validate_transition(attempt, 0)?;
            validate_result_page(limit)?;
            let mut tx = self.pool.begin().await.map_err(|_| database_error())?;
            let Some(job) = super::preview_jobs::load_job(&mut tx, &job_id).await? else {
                return Ok(vec![]);
            };
            if job.attempt() != attempt {
                return Ok(vec![]);
            }
            validate_extent(&mut tx, &job).await?;
            let profile =
                super::preview_jobs::load_profile(&mut tx, job.profile_id(), job.profile_version())
                    .await?
                    .ok_or_else(database_error)?;
            let rows = sqlx::query("SELECT ordinal,attempt,committed_at,result_json FROM preview_results WHERE job_id=? AND attempt=? AND ordinal>=? ORDER BY ordinal LIMIT ?")
                .bind(&job_id).bind(attempt).bind(offset).bind(limit).fetch_all(&mut *tx).await.map_err(|_| database_error())?;
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                let ordinal_i64: i64 = row.try_get("ordinal").map_err(|_| database_error())?;
                let ordinal = u32::try_from(ordinal_i64).map_err(|_| database_error())?;
                let saved_attempt = u32::try_from(
                    row.try_get::<i64, _>("attempt")
                        .map_err(|_| database_error())?,
                )
                .map_err(|_| database_error())?;
                let committed_at: i64 =
                    row.try_get("committed_at").map_err(|_| database_error())?;
                let json: String = row.try_get("result_json").map_err(|_| database_error())?;
                let value = decode(&json, ordinal, saved_attempt, committed_at)?;
                let observation = stored_observation(&mut tx, &job_id, ordinal)
                    .await?
                    .ok_or_else(database_error)?;
                coherent(&job, &profile, &observation, &value)?;
                values.push(value);
            }
            tx.commit().await.map_err(|_| database_error())?;
            Ok(values)
        })
    }

    fn complete(&self, job_id: String, attempt: u32, now: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            validate_transition(attempt, now)?;
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            let Some(job) = super::preview_jobs::load_job(&mut tx, &job_id).await? else {
                return Ok(false);
            };
            if job.status() != PreviewJobStatus::Running
                || job.attempt() != attempt
                || now < job.updated_at()
                || now >= job.expires_at()
                || !super::preview_jobs::current_job_binding(&mut tx, &job_id).await?
            {
                return Ok(false);
            }
            validate_extent(&mut tx, &job).await?;
            let profile =
                super::preview_jobs::load_profile(&mut tx, job.profile_id(), job.profile_version())
                    .await?
                    .ok_or_else(database_error)?;
            let rows = sqlx::query("SELECT ordinal,attempt,committed_at,result_json FROM preview_results WHERE job_id=? ORDER BY ordinal")
                .bind(&job_id).fetch_all(&mut *tx).await.map_err(|_| database_error())?;
            if rows.len() != job.observation_count() as usize {
                return Ok(false);
            }
            for (index, row) in rows.into_iter().enumerate() {
                let ordinal = u32::try_from(
                    row.try_get::<i64, _>("ordinal")
                        .map_err(|_| database_error())?,
                )
                .map_err(|_| database_error())?;
                if ordinal as usize != index {
                    return Ok(false);
                }
                let saved_attempt = u32::try_from(
                    row.try_get::<i64, _>("attempt")
                        .map_err(|_| database_error())?,
                )
                .map_err(|_| database_error())?;
                let committed_at: i64 =
                    row.try_get("committed_at").map_err(|_| database_error())?;
                let json: String = row.try_get("result_json").map_err(|_| database_error())?;
                let value = decode(&json, ordinal, saved_attempt, committed_at)?;
                let observation = stored_observation(&mut tx, &job_id, ordinal)
                    .await?
                    .ok_or_else(database_error)?;
                coherent(&job, &profile, &observation, &value)?;
            }
            let updated = sqlx::query("UPDATE preview_jobs SET status='completed',updated_at=? WHERE id=? AND attempt=? AND status='running' AND updated_at<=? AND expires_at>?")
                .bind(now).bind(&job_id).bind(attempt).bind(now).bind(now).execute(&mut *tx).await.map_err(|_| database_error())?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(updated.rows_affected() == 1)
        })
    }
}

#[cfg(test)]
mod tests;
