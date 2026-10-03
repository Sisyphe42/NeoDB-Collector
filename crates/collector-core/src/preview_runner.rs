//! Callable single-attempt preview orchestration. Runtime ownership lives outside Core.
use crate::{
    access::{AccessRepository, Clock, Endpoint},
    catalog::{NeoDbCatalogSearchTransport, NeoDbCatalogTransport},
    catalog_resolver::{CatalogIdentity, RESOLUTION_BUDGET, ResolutionControl},
    mapping::{self, MappingRepository},
    preview_jobs::{
        MAX_PREVIEW_PAGE, PreviewJob, PreviewJobRepository, PreviewJobStatus,
        PreviewResolutionInput, valid_id, validate_imdb_observation, validate_transition,
    },
    preview_records::{PreviewRecordRequest, PreviewRecordService, PreviewResolutionMode},
    preview_results::{PreviewResultRepository, PreviewResultWrite},
};
use collector_domain::{
    ApiError,
    mapping::MappingProfile,
    observation::{Observation, ObservedItem},
};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewAttemptOutcome {
    Completed,
    NotClaimed,
    Stopped,
}

pub struct PreviewAttemptRunner {
    jobs: Arc<dyn PreviewJobRepository>,
    results: Arc<dyn PreviewResultRepository>,
    mappings: Arc<dyn MappingRepository>,
    access: Arc<dyn AccessRepository>,
    clock: Arc<dyn Clock>,
    producer: Arc<PreviewRecordService>,
}

impl PreviewAttemptRunner {
    pub fn new(
        jobs: Arc<dyn PreviewJobRepository>,
        results: Arc<dyn PreviewResultRepository>,
        mappings: Arc<dyn MappingRepository>,
        access: Arc<dyn AccessRepository>,
        clock: Arc<dyn Clock>,
        producer: Arc<PreviewRecordService>,
    ) -> Self {
        Self {
            jobs,
            results,
            mappings,
            access,
            clock,
            producer,
        }
    }

    pub async fn run<T, C, F>(
        &self,
        job_id: &str,
        attempt: u32,
        transport: &T,
        control: &C,
        identity: &F,
    ) -> Result<PreviewAttemptOutcome, ApiError>
    where
        T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport,
        C: ResolutionControl,
        F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync,
    {
        let now = self.clock.now();
        validate_transition(attempt, now)?;
        if !valid_id(job_id) {
            return Err(ApiError::new(
                "preview_job_invalid",
                "Preview job metadata is invalid.",
            ));
        }

        let setup_deadline = checked_deadline(control, RESOLUTION_BUDGET)?;
        let Some(job) = control
            .run(setup_deadline, self.jobs.load(job_id.to_owned()))
            .await?
        else {
            return Ok(PreviewAttemptOutcome::NotClaimed);
        };
        if !eligible_to_claim(&job, attempt, self.clock.now()) {
            return Ok(PreviewAttemptOutcome::NotClaimed);
        }

        let Some(retention_deadline) = self.deadline(control, &job)? else {
            return Ok(PreviewAttemptOutcome::NotClaimed);
        };
        let claim_deadline = setup_deadline.min(retention_deadline);
        let claim = control
            .run(
                claim_deadline,
                Box::pin(async {
                    let Some(endpoint) = self.current_endpoint(&job).await? else {
                        return Ok(None);
                    };
                    let Some(profile) = self
                        .mappings
                        .load(job.profile_id().to_owned(), Some(job.profile_version()))
                        .await?
                    else {
                        return Ok(None);
                    };
                    mapping::validate(&profile)?;
                    if profile.id != job.profile_id()
                        || profile.version != job.profile_version()
                        || !job.is_bound_to(&endpoint)
                        || !eligible_to_claim(&job, attempt, self.clock.now())
                    {
                        return Ok(None);
                    }
                    if !self
                        .jobs
                        .claim(job.id().to_owned(), attempt, self.clock.now())
                        .await?
                    {
                        return Ok(None);
                    }
                    Ok(Some((endpoint, profile)))
                }),
            )
            .await?;
        let Some((expected, profile)) = claim else {
            return Ok(PreviewAttemptOutcome::NotClaimed);
        };

        let result = self
            .run_claimed(
                ClaimedAttempt {
                    job: &job,
                    expected: &expected,
                    attempt,
                    profile: &profile,
                },
                transport,
                control,
                identity,
            )
            .await;
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) if error.code == "catalog_resolution_cancelled" => Err(error),
            Err(_) if !valid_live_time(&job, self.clock.now()) => {
                Ok(PreviewAttemptOutcome::Stopped)
            }
            Err(error) => {
                self.record_failure(&job, attempt, control).await?;
                Err(error)
            }
        }
    }

    async fn run_claimed<T, C, F>(
        &self,
        claimed: ClaimedAttempt<'_>,
        transport: &T,
        control: &C,
        identity: &F,
    ) -> Result<PreviewAttemptOutcome, ApiError>
    where
        T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport,
        C: ResolutionControl,
        F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync,
    {
        let ClaimedAttempt {
            job,
            expected,
            attempt,
            profile,
        } = claimed;
        let mut offset = 0_u32;
        while offset < job.observation_count() {
            let page_len = (job.observation_count() - offset).min(MAX_PREVIEW_PAGE);
            let Some(page_deadline) = self.deadline(control, job)? else {
                return Ok(PreviewAttemptOutcome::Stopped);
            };
            let page = control
                .run(
                    page_deadline,
                    Box::pin(async {
                        if !self.still_running(job, expected, attempt).await? {
                            return Ok(PageStep::Stopped);
                        }
                        let observations = self
                            .jobs
                            .observations(job.id().to_owned(), offset, page_len)
                            .await?;
                        if observations.len() != page_len as usize {
                            return Err(invalid_page());
                        }
                        for observation in &observations {
                            validate_imdb_observation(observation)?;
                            mapping::evaluate(profile, observation)?;
                        }
                        Ok(PageStep::Ready(observations))
                    }),
                )
                .await?;
            let PageStep::Ready(observations) = page else {
                return Ok(PreviewAttemptOutcome::Stopped);
            };

            for (index, observation) in observations.into_iter().enumerate() {
                let ordinal = offset + index as u32;
                let Some(record_deadline) = self.deadline(control, job)? else {
                    return Ok(PreviewAttemptOutcome::Stopped);
                };
                let step = control
                    .run(
                        record_deadline,
                        Box::pin(async {
                            if !self.still_running(job, expected, attempt).await? {
                                return Ok(RecordStep::Stopped);
                            }
                            let catalog_identity = identity(&observation.item)?;
                            let resolution_input = self
                                .jobs
                                .resolution_input(job.id().to_owned(), ordinal)
                                .await?;
                            if !self.still_running(job, expected, attempt).await? {
                                return Ok(RecordStep::Stopped);
                            }
                            let manual_path;
                            let mode = match &resolution_input {
                                PreviewResolutionInput::Native => PreviewResolutionMode::Native,
                                PreviewResolutionInput::Search(query) => {
                                    PreviewResolutionMode::Search { query }
                                }
                                PreviewResolutionInput::Manual(reference) => {
                                    manual_path = reference.detail_path();
                                    PreviewResolutionMode::Manual {
                                        reference: &manual_path,
                                    }
                                }
                            };
                            let record = self
                                .producer
                                .preview(
                                    transport,
                                    control,
                                    PreviewRecordRequest {
                                        expected,
                                        profile,
                                        observation: &observation,
                                        identity: &catalog_identity,
                                        mode,
                                    },
                                )
                                .await?;
                            if !self.still_running(job, expected, attempt).await? {
                                return Ok(RecordStep::Stopped);
                            }
                            let write = PreviewResultWrite::new(
                                job.id().to_owned(),
                                ordinal,
                                attempt,
                                self.clock.now(),
                                record,
                            )?;
                            if !self.results.store(write).await? {
                                return Ok(RecordStep::Stopped);
                            }
                            Ok(RecordStep::Stored)
                        }),
                    )
                    .await?;
                if step == RecordStep::Stopped {
                    return Ok(PreviewAttemptOutcome::Stopped);
                }
            }
            offset += page_len;
        }

        let Some(completion_deadline) = self.deadline(control, job)? else {
            return Ok(PreviewAttemptOutcome::Stopped);
        };
        control
            .run(
                completion_deadline,
                Box::pin(async {
                    if !self.still_running(job, expected, attempt).await? {
                        return Ok(PreviewAttemptOutcome::Stopped);
                    }
                    if self
                        .results
                        .complete(job.id().to_owned(), attempt, self.clock.now())
                        .await?
                    {
                        Ok(PreviewAttemptOutcome::Completed)
                    } else {
                        Ok(PreviewAttemptOutcome::Stopped)
                    }
                }),
            )
            .await
    }

    async fn current_endpoint(&self, job: &PreviewJob) -> Result<Option<Endpoint>, ApiError> {
        let endpoint = self.access.endpoint().await?;
        Ok(endpoint.filter(|endpoint| job.is_bound_to(endpoint)))
    }

    async fn still_running(
        &self,
        original: &PreviewJob,
        expected: &Endpoint,
        attempt: u32,
    ) -> Result<bool, ApiError> {
        let Some(job) = self.jobs.load(original.id().to_owned()).await? else {
            return Ok(false);
        };
        if job.status() != PreviewJobStatus::Running
            || job.attempt() != attempt
            || job.profile_id() != original.profile_id()
            || job.profile_version() != original.profile_version()
            || job.observation_count() != original.observation_count()
            || job.created_at() != original.created_at()
            || job.expires_at() != original.expires_at()
            || !valid_live_time(&job, self.clock.now())
            || !original.is_bound_to(expected)
            || !job.is_bound_to(expected)
        {
            return Ok(false);
        }
        let live = self.access.endpoint().await?;
        Ok(live.is_some_and(|live| {
            same_endpoint(&live, expected)
                && original.is_bound_to(&live)
                && job.is_bound_to(&live)
                && valid_live_time(&job, self.clock.now())
        }))
    }

    fn deadline<C: ResolutionControl>(
        &self,
        control: &C,
        job: &PreviewJob,
    ) -> Result<Option<Duration>, ApiError> {
        let now = self.clock.now();
        if !valid_live_time(job, now) {
            return Ok(None);
        }
        let remaining = u64::try_from(job.expires_at() - now).map_err(|_| deadline_error())?;
        checked_deadline(
            control,
            RESOLUTION_BUDGET.min(Duration::from_secs(remaining)),
        )
        .map(Some)
    }

    async fn record_failure<C: ResolutionControl>(
        &self,
        job: &PreviewJob,
        attempt: u32,
        control: &C,
    ) -> Result<(), ApiError> {
        let now = self.clock.now();
        if !valid_live_time(job, now) {
            return Err(cleanup_error());
        }
        let remaining = u64::try_from(job.expires_at() - now).map_err(|_| cleanup_error())?;
        let deadline = checked_deadline(
            control,
            Duration::from_secs(10).min(Duration::from_secs(remaining)),
        )
        .map_err(|_| cleanup_error())?;
        match control
            .run(
                deadline,
                Box::pin(async {
                    if self
                        .jobs
                        .fail(job.id().to_owned(), attempt, self.clock.now())
                        .await?
                    {
                        return Ok(true);
                    }
                    let current = self.jobs.load(job.id().to_owned()).await?;
                    Ok(current.is_none_or(|current| {
                        current.status() != PreviewJobStatus::Running
                            || current.attempt() != attempt
                    }))
                }),
            )
            .await
        {
            Ok(true) => Ok(()),
            Ok(false) => Err(cleanup_error()),
            Err(error) if error.code == "catalog_resolution_cancelled" => Err(error),
            Err(_) => Err(cleanup_error()),
        }
    }
}

enum PageStep {
    Ready(Vec<Observation>),
    Stopped,
}

struct ClaimedAttempt<'a> {
    job: &'a PreviewJob,
    expected: &'a Endpoint,
    attempt: u32,
    profile: &'a MappingProfile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordStep {
    Stored,
    Stopped,
}

fn eligible_to_claim(job: &PreviewJob, attempt: u32, now: i64) -> bool {
    job.status() == PreviewJobStatus::Queued
        && job.attempt() == attempt
        && valid_live_time(job, now)
}

fn valid_live_time(job: &PreviewJob, now: i64) -> bool {
    now >= 0 && now >= job.created_at() && now >= job.updated_at() && now < job.expires_at()
}

fn same_endpoint(left: &Endpoint, right: &Endpoint) -> bool {
    left.base_url == right.base_url
        && left.account == right.account
        && left.credential_ref == right.credential_ref
}

fn checked_deadline<C: ResolutionControl>(
    control: &C,
    budget: Duration,
) -> Result<Duration, ApiError> {
    control.now().checked_add(budget).ok_or_else(deadline_error)
}

fn deadline_error() -> ApiError {
    ApiError::new(
        "catalog_resolution_timeout",
        "Catalog resolution exceeded its time budget.",
    )
}

fn invalid_page() -> ApiError {
    ApiError::new(
        "preview_observation_page_invalid",
        "Stored preview observations are incomplete or out of order.",
    )
}

fn cleanup_error() -> ApiError {
    ApiError::new(
        "preview_failure_cleanup_failed",
        "Preview attempt failure could not be recorded; query job status.",
    )
}

#[cfg(test)]
mod tests;
