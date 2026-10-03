use crate::{Storage, database_error};
use collector_core::{
    access::{Endpoint, PortFuture},
    catalog::{CatalogReference, MAX_CATALOG_REFERENCE_BYTES},
    catalog_resolver::validate_catalog_search_query,
    mapping,
    preview_jobs::{
        MAX_PREVIEW_OBSERVATION_BYTES, MAX_PREVIEW_TOTAL_BYTES, PreviewJob, PreviewJobIdentity,
        PreviewJobLifecycle, PreviewJobRepository, PreviewJobStatus, PreviewResolutionInput,
        PreviewSubmission, validate_imdb_observation, validate_page, validate_transition,
    },
};
use collector_domain::{ApiError, mapping::MappingProfile, observation::Observation};
use sqlx::{Row, SqliteConnection};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManualSelectionDto {
    schema_version: u32,
    uuid: String,
    media_type: String,
    model: String,
}

fn binding_changed() -> ApiError {
    ApiError::new(
        "preview_binding_changed",
        "The NeoDB account connection changed; create a fresh preview.",
    )
}

fn profile_changed() -> ApiError {
    ApiError::new(
        "preview_profile_changed",
        "The selected mapping profile revision is unavailable or changed.",
    )
}

async fn current_binding(
    conn: &mut SqliteConnection,
    binding: &Endpoint,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM neodb_endpoint WHERE singleton=1 AND base_url=? AND account=? AND credential_ref=?)")
        .bind(&binding.base_url)
        .bind(&binding.account)
        .bind(&binding.credential_ref)
        .fetch_one(conn)
        .await
        .map(|value| value == 1)
        .map_err(|_| database_error())
}

pub(crate) async fn current_job_binding(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM preview_jobs j JOIN neodb_endpoint e ON e.singleton=1 AND e.base_url=j.base_url AND e.account=j.account AND e.credential_ref=j.credential_ref WHERE j.id=?)")
        .bind(id)
        .fetch_one(conn)
        .await
        .map(|value| value == 1)
        .map_err(|_| database_error())
}

pub(crate) async fn load_profile(
    conn: &mut SqliteConnection,
    id: &str,
    version: u32,
) -> Result<Option<MappingProfile>, ApiError> {
    let row =
        sqlx::query("SELECT profile_json FROM mapping_profiles WHERE id=? AND version=? LIMIT 1")
            .bind(id)
            .bind(version)
            .fetch_optional(conn)
            .await
            .map_err(|_| database_error())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let json: String = row.try_get("profile_json").map_err(|_| database_error())?;
    let profile: MappingProfile = serde_json::from_str(&json).map_err(|_| database_error())?;
    mapping::validate(&profile).map_err(|_| database_error())?;
    if profile.id != id || profile.version != version {
        return Err(database_error());
    }
    Ok(Some(profile))
}

pub(crate) async fn load_resolution_input(
    conn: &mut SqliteConnection,
    id: &str,
    ordinal: u32,
) -> Result<Option<PreviewResolutionInput>, ApiError> {
    let row = sqlx::query("SELECT search_query,manual_selection FROM preview_observations WHERE job_id=? AND ordinal=?")
            .bind(id)
            .bind(ordinal)
            .fetch_optional(conn)
            .await
            .map_err(|_| database_error())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let query: Option<String> = row.try_get("search_query").map_err(|_| database_error())?;
    let selection: Option<String> = row
        .try_get("manual_selection")
        .map_err(|_| database_error())?;
    decode_resolution_input(query, selection).map(Some)
}

pub(crate) fn decode_search_query(query: Option<String>) -> Result<Option<String>, ApiError> {
    query
        .map(|query| {
            let normalized = validate_catalog_search_query(&query).map_err(|_| database_error())?;
            if normalized != query {
                return Err(database_error());
            }
            Ok(query)
        })
        .transpose()
}

pub(crate) fn decode_resolution_input(
    query: Option<String>,
    selection: Option<String>,
) -> Result<PreviewResolutionInput, ApiError> {
    match (
        decode_search_query(query)?,
        decode_manual_selection(selection)?,
    ) {
        (None, None) => Ok(PreviewResolutionInput::Native),
        (Some(query), None) => Ok(PreviewResolutionInput::Search(query)),
        (None, Some(reference)) => Ok(PreviewResolutionInput::Manual(reference)),
        (Some(_), Some(_)) => Err(database_error()),
    }
}

fn encode_manual_selection(reference: &CatalogReference) -> Result<String, ApiError> {
    let json = serde_json::to_string(&ManualSelectionDto {
        schema_version: 1,
        uuid: reference.uuid().into(),
        media_type: reference.media_type().into(),
        model: reference.model().into(),
    })
    .map_err(|_| database_error())?;
    if json.len() > MAX_CATALOG_REFERENCE_BYTES {
        return Err(database_error());
    }
    Ok(json)
}

fn decode_manual_selection(
    selection: Option<String>,
) -> Result<Option<CatalogReference>, ApiError> {
    selection
        .map(|json| {
            if json.len() > MAX_CATALOG_REFERENCE_BYTES {
                return Err(database_error());
            }
            let value: ManualSelectionDto =
                serde_json::from_str(&json).map_err(|_| database_error())?;
            if value.schema_version != 1 {
                return Err(database_error());
            }
            let reference =
                CatalogReference::from_stored(value.uuid, value.media_type, value.model)
                    .map_err(|_| database_error())?;
            if encode_manual_selection(&reference)? != json {
                return Err(database_error());
            }
            Ok(reference)
        })
        .transpose()
}

pub(crate) async fn load_job(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Option<PreviewJob>, ApiError> {
    let row = sqlx::query("SELECT id,profile_id,profile_version,base_url,account,credential_ref,status,attempt,created_at,updated_at,expires_at,observation_count FROM preview_jobs WHERE id=?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|_| database_error())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let profile_id: String = row.try_get("profile_id").map_err(|_| database_error())?;
    let profile_version_i64: i64 = row
        .try_get("profile_version")
        .map_err(|_| database_error())?;
    let profile_version = u32::try_from(profile_version_i64).map_err(|_| database_error())?;
    if load_profile(conn, &profile_id, profile_version)
        .await?
        .is_none()
    {
        return Err(database_error());
    }
    let extent = sqlx::query("SELECT count(*) AS row_count,min(ordinal) AS min_ordinal,max(ordinal) AS max_ordinal,COALESCE(sum(length(CAST(observation_json AS BLOB))),0) AS total_bytes FROM preview_observations WHERE job_id=?")
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| database_error())?;
    let count: i64 = extent.try_get("row_count").map_err(|_| database_error())?;
    let declared_i64: i64 = row
        .try_get("observation_count")
        .map_err(|_| database_error())?;
    let valid_ordinals = if count == 0 {
        extent
            .try_get::<Option<i64>, _>("min_ordinal")
            .map_err(|_| database_error())?
            .is_none()
            && extent
                .try_get::<Option<i64>, _>("max_ordinal")
                .map_err(|_| database_error())?
                .is_none()
    } else {
        extent
            .try_get::<i64, _>("min_ordinal")
            .map_err(|_| database_error())?
            == 0
            && extent
                .try_get::<i64, _>("max_ordinal")
                .map_err(|_| database_error())?
                == count - 1
    };
    let total_bytes: i64 = extent
        .try_get("total_bytes")
        .map_err(|_| database_error())?;
    if count != declared_i64
        || !valid_ordinals
        || total_bytes < 0
        || usize::try_from(total_bytes).map_err(|_| database_error())? > MAX_PREVIEW_TOTAL_BYTES
    {
        return Err(database_error());
    }
    PreviewJob::from_stored(
        PreviewJobIdentity {
            id: row.try_get("id").map_err(|_| database_error())?,
            profile_id,
            profile_version,
            observation_count: u32::try_from(declared_i64).map_err(|_| database_error())?,
        },
        PreviewJobLifecycle {
            status: PreviewJobStatus::parse(
                &row.try_get::<String, _>("status")
                    .map_err(|_| database_error())?,
            )?,
            attempt: u32::try_from(
                row.try_get::<i64, _>("attempt")
                    .map_err(|_| database_error())?,
            )
            .map_err(|_| database_error())?,
            created_at: row.try_get("created_at").map_err(|_| database_error())?,
            updated_at: row.try_get("updated_at").map_err(|_| database_error())?,
            expires_at: row.try_get("expires_at").map_err(|_| database_error())?,
        },
        Endpoint {
            base_url: row.try_get("base_url").map_err(|_| database_error())?,
            account: row.try_get("account").map_err(|_| database_error())?,
            credential_ref: row
                .try_get("credential_ref")
                .map_err(|_| database_error())?,
        },
    )
    .map(Some)
}

impl PreviewJobRepository for Storage {
    fn submit(&self, submission: PreviewSubmission) -> PortFuture<'_, PreviewJob> {
        Box::pin(async move {
            let mut encoded = Vec::with_capacity(submission.observations().len());
            let mut total = 0usize;
            for observation in submission.observations() {
                validate_imdb_observation(observation)?;
                mapping::evaluate(submission.profile(), observation)?;
                let json = serde_json::to_string(observation).map_err(|_| database_error())?;
                if json.len() > MAX_PREVIEW_OBSERVATION_BYTES {
                    return Err(ApiError::new(
                        "preview_observation_too_large",
                        "A normalized observation exceeds 16 KiB.",
                    ));
                }
                total = total.checked_add(json.len()).ok_or_else(|| {
                    ApiError::new(
                        "preview_submission_too_large",
                        "Preview submission exceeds 32 MiB.",
                    )
                })?;
                if total > MAX_PREVIEW_TOTAL_BYTES {
                    return Err(ApiError::new(
                        "preview_submission_too_large",
                        "Preview submission exceeds 32 MiB.",
                    ));
                }
                encoded.push(json);
            }

            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            if !current_binding(&mut tx, submission.binding()).await? {
                return Err(binding_changed());
            }
            let stored = load_profile(
                &mut tx,
                &submission.profile().id,
                submission.profile().version,
            )
            .await?
            .ok_or_else(profile_changed)?;
            if stored != *submission.profile() {
                return Err(profile_changed());
            }
            sqlx::query("INSERT INTO preview_jobs(id,profile_id,profile_version,base_url,account,credential_ref,status,attempt,created_at,updated_at,expires_at,observation_count) VALUES(?,?,?,?,?,?,'queued',1,?,?,?,?)")
                .bind(submission.id())
                .bind(&submission.profile().id)
                .bind(submission.profile().version)
                .bind(&submission.binding().base_url)
                .bind(&submission.binding().account)
                .bind(&submission.binding().credential_ref)
                .bind(submission.created_at())
                .bind(submission.created_at())
                .bind(submission.expires_at())
                .bind(i64::try_from(encoded.len()).map_err(|_| database_error())?)
                .execute(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            for (ordinal, json) in encoded.into_iter().enumerate() {
                let ordinal = u32::try_from(ordinal).map_err(|_| database_error())?;
                let search_query = submission.search_query(ordinal);
                let manual_selection = submission
                    .manual_selection(ordinal)
                    .map(encode_manual_selection)
                    .transpose()?;
                sqlx::query("INSERT INTO preview_observations(job_id,ordinal,observation_json,search_query,manual_selection) VALUES(?,?,?,?,?)")
                    .bind(submission.id())
                    .bind(ordinal)
                    .bind(json)
                    .bind(search_query)
                    .bind(manual_selection)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| database_error())?;
            }
            let job = load_job(&mut tx, submission.id())
                .await?
                .ok_or_else(database_error)?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(job)
        })
    }

    fn load(&self, id: String) -> PortFuture<'_, Option<PreviewJob>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|_| database_error())?;
            let job = load_job(&mut tx, &id).await?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(job)
        })
    }

    fn observations(
        &self,
        id: String,
        offset: u32,
        limit: u32,
    ) -> PortFuture<'_, Vec<Observation>> {
        Box::pin(async move {
            validate_page(limit)?;
            let mut tx = self.pool.begin().await.map_err(|_| database_error())?;
            let Some(job) = load_job(&mut tx, &id).await? else {
                tx.commit().await.map_err(|_| database_error())?;
                return Ok(Vec::new());
            };
            let profile = load_profile(&mut tx, job.profile_id(), job.profile_version())
                .await?
                .ok_or_else(database_error)?;
            let rows = sqlx::query("SELECT ordinal,observation_json,search_query,manual_selection FROM preview_observations WHERE job_id=? AND ordinal>=? ORDER BY ordinal LIMIT ?")
                .bind(&id)
                .bind(offset)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            let observations = rows
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    let ordinal: i64 = row.try_get("ordinal").map_err(|_| database_error())?;
                    if ordinal
                        != i64::from(offset) + i64::try_from(index).map_err(|_| database_error())?
                    {
                        return Err(database_error());
                    }
                    let json: String = row
                        .try_get("observation_json")
                        .map_err(|_| database_error())?;
                    if json.len() > MAX_PREVIEW_OBSERVATION_BYTES {
                        return Err(database_error());
                    }
                    let observation: Observation =
                        serde_json::from_str(&json).map_err(|_| database_error())?;
                    validate_imdb_observation(&observation).map_err(|_| database_error())?;
                    mapping::evaluate(&profile, &observation).map_err(|_| database_error())?;
                    let search_query: Option<String> =
                        row.try_get("search_query").map_err(|_| database_error())?;
                    let manual_selection: Option<String> = row
                        .try_get("manual_selection")
                        .map_err(|_| database_error())?;
                    decode_resolution_input(search_query, manual_selection)?;
                    Ok(observation)
                })
                .collect::<Result<Vec<_>, _>>()?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(observations)
        })
    }

    fn resolution_input(&self, id: String, ordinal: u32) -> PortFuture<'_, PreviewResolutionInput> {
        Box::pin(async move {
            if ordinal as usize >= collector_core::preview_jobs::MAX_PREVIEW_OBSERVATIONS {
                return Err(ApiError::new(
                    "preview_search_plan_invalid",
                    "Preview search inputs must identify distinct submitted records.",
                ));
            }
            let mut tx = self.pool.begin().await.map_err(|_| database_error())?;
            let input = load_resolution_input(&mut tx, &id, ordinal)
                .await?
                .ok_or_else(database_error)?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(input)
        })
    }

    fn claim(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            validate_transition(attempt, now)?;
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            let Some(_job) = load_job(&mut tx, &id).await? else {
                return Ok(false);
            };
            let result = sqlx::query("UPDATE preview_jobs SET status='running',updated_at=? WHERE id=? AND attempt=? AND status='queued' AND created_at<=? AND updated_at<=? AND expires_at>? AND EXISTS(SELECT 1 FROM neodb_endpoint e WHERE e.singleton=1 AND e.base_url=preview_jobs.base_url AND e.account=preview_jobs.account AND e.credential_ref=preview_jobs.credential_ref)")
                .bind(now).bind(&id).bind(attempt).bind(now).bind(now).bind(now)
                .execute(&mut *tx).await.map_err(|_| database_error())?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(result.rows_affected() == 1)
        })
    }

    fn cancel(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            validate_transition(attempt, now)?;
            let result = sqlx::query("UPDATE preview_jobs SET status='cancelled',updated_at=? WHERE id=? AND attempt=? AND status IN ('queued','running') AND created_at<=? AND updated_at<=?")
                .bind(now).bind(&id).bind(attempt).bind(now).bind(now)
                .execute(&self.pool).await.map_err(|_| database_error())?;
            if result.rows_affected() == 1 {
                return Ok(true);
            }
            let already: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM preview_jobs WHERE id=? AND attempt=? AND status='cancelled')")
                .bind(id).bind(attempt).fetch_one(&self.pool).await.map_err(|_| database_error())?;
            Ok(already == 1)
        })
    }

    fn interrupt_running(&self, now: i64) -> PortFuture<'_, u64> {
        Box::pin(async move {
            if now < 0 {
                return Err(ApiError::new(
                    "preview_transition_invalid",
                    "Preview transition metadata is invalid.",
                ));
            }
            sqlx::query("UPDATE preview_jobs SET status='interrupted',updated_at=MAX(updated_at,?) WHERE status IN ('queued','running')")
                .bind(now).execute(&self.pool).await
                .map(|result| result.rows_affected()).map_err(|_| database_error())
        })
    }

    fn fail(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            validate_transition(attempt, now)?;
            sqlx::query("UPDATE preview_jobs SET status='failed',updated_at=? WHERE id=? AND attempt=? AND status='running' AND created_at<=? AND updated_at<=?")
                .bind(now).bind(id).bind(attempt).bind(now).bind(now)
                .execute(&self.pool).await.map(|result| result.rows_affected() == 1).map_err(|_| database_error())
        })
    }

    fn resume(&self, id: String, attempt: u32, now: i64) -> PortFuture<'_, Option<u32>> {
        Box::pin(async move {
            validate_transition(attempt, now)?;
            let Some(next) = attempt.checked_add(1) else {
                return Err(ApiError::new(
                    "preview_attempt_exhausted",
                    "Preview attempt counter is exhausted.",
                ));
            };
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            let Some(_job) = load_job(&mut tx, &id).await? else {
                return Ok(None);
            };
            if !current_job_binding(&mut tx, &id).await? {
                return Err(binding_changed());
            }
            let result = sqlx::query("UPDATE preview_jobs SET status='queued',attempt=?,updated_at=? WHERE id=? AND attempt=? AND status IN ('interrupted','failed','cancelled') AND created_at<=? AND updated_at<=? AND expires_at>? AND EXISTS(SELECT 1 FROM neodb_endpoint e WHERE e.singleton=1 AND e.base_url=preview_jobs.base_url AND e.account=preview_jobs.account AND e.credential_ref=preview_jobs.credential_ref) AND EXISTS(SELECT 1 FROM mapping_profiles p WHERE p.id=preview_jobs.profile_id AND p.version=preview_jobs.profile_version)")
                .bind(next).bind(now).bind(&id).bind(attempt).bind(now).bind(now).bind(now)
                .execute(&mut *tx).await.map_err(|_| database_error())?;
            if result.rows_affected() == 1 {
                sqlx::query("DELETE FROM preview_results WHERE job_id=?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| database_error())?;
            }
            tx.commit().await.map_err(|_| database_error())?;
            Ok((result.rows_affected() == 1).then_some(next))
        })
    }

    fn delete(&self, id: String) -> PortFuture<'_, bool> {
        Box::pin(async move {
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            sqlx::query("DELETE FROM preview_results WHERE job_id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            sqlx::query("DELETE FROM preview_observations WHERE job_id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            let deleted = sqlx::query("DELETE FROM preview_jobs WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(deleted.rows_affected() == 1)
        })
    }

    fn cleanup_expired(&self, now: i64) -> PortFuture<'_, u64> {
        Box::pin(async move {
            if now < 0 {
                return Err(ApiError::new(
                    "preview_transition_invalid",
                    "Preview transition metadata is invalid.",
                ));
            }
            let mut tx = self
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|_| database_error())?;
            sqlx::query("DELETE FROM preview_results WHERE job_id IN (SELECT id FROM preview_jobs WHERE expires_at<=?)")
                .bind(now).execute(&mut *tx).await.map_err(|_| database_error())?;
            sqlx::query("DELETE FROM preview_observations WHERE job_id IN (SELECT id FROM preview_jobs WHERE expires_at<=?)")
                .bind(now).execute(&mut *tx).await.map_err(|_| database_error())?;
            let deleted = sqlx::query("DELETE FROM preview_jobs WHERE expires_at<=?")
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|_| database_error())?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(deleted.rows_affected())
        })
    }
}

#[cfg(test)]
mod tests;
