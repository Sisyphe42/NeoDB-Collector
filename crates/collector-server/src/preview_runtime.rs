//! Bounded ownership for explicitly selected preview attempts.

use crate::resolution_control::TokioResolutionControl;
use collector_core::{
    access::{Clock, PortFuture},
    catalog::{NeoDbCatalogSearchTransport, NeoDbCatalogTransport},
    catalog_resolver::{CatalogIdentity, ResolutionControl},
    preview_jobs::{
        PreviewJob, PreviewJobRepository, PreviewJobStatus, PreviewRetention, PreviewSubmission,
    },
    preview_runner::{PreviewAttemptOutcome, PreviewAttemptRunner},
};
use collector_domain::{ApiError, observation::ObservedItem};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::{
    sync::{Semaphore, oneshot},
    time::{Instant, MissedTickBehavior},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const MAX_ADMITTED_ATTEMPTS: usize = 9;
const MANAGEMENT_BUDGET: Duration = Duration::from_secs(10);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);

/// Object-safe HTTP composition view over the single owned preview runtime.
pub trait PreviewManagement: Send + Sync {
    fn cancel<'a>(&'a self, id: &'a str, expected_attempt: u32) -> PortFuture<'a, bool>;
    fn delete<'a>(&'a self, id: &'a str) -> PortFuture<'a, bool>;
}

/// Object-safe HTTP composition view over owned submission and resume admission.
pub trait PreviewSubmissionRuntime: Send + Sync {
    fn retention(&self) -> PreviewRetention;
    fn submit(&self, submission: PreviewSubmission) -> Result<PreviewAdmissionObserver, ApiError>;
    fn resume(&self, job: &PreviewJob) -> Result<PreviewAdmissionObserver, ApiError>;
}

pub struct PreviewRuntime<T, F> {
    runner: Arc<PreviewAttemptRunner>,
    jobs: Arc<dyn PreviewJobRepository>,
    clock: Arc<dyn Clock>,
    transport: Arc<T>,
    identity: Arc<F>,
    retention: PreviewRetention,
    active: Arc<Semaphore>,
    tracker: TaskTracker,
    shared: Arc<Shared>,
    shutdown_guard: Option<tokio_util::sync::DropGuard>,
}

struct Shared {
    state: Mutex<State>,
    shutdown: CancellationToken,
}

struct State {
    closed: bool,
    admitted: HashMap<String, Admission>,
}

struct Admission {
    attempt: u32,
    expires_at: i64,
    cancellation: Arc<CancellationToken>,
}

#[derive(Debug)]
pub struct PreviewAttemptObserver {
    receiver: oneshot::Receiver<Result<PreviewAttemptOutcome, ApiError>>,
}

#[derive(Debug)]
pub struct PreviewAcceptedAttempt {
    pub job_id: String,
    pub attempt: u32,
    pub completion: PreviewAttemptObserver,
}

#[derive(Debug)]
pub struct PreviewAdmissionObserver {
    receiver: oneshot::Receiver<Result<PreviewAcceptedAttempt, ApiError>>,
    closed_error: fn() -> ApiError,
}

impl PreviewAdmissionObserver {
    pub async fn wait(self) -> Result<PreviewAcceptedAttempt, ApiError> {
        self.receiver
            .await
            .unwrap_or_else(|_| Err((self.closed_error)()))
    }

    #[cfg(test)]
    pub(crate) fn delayed_test_error(delay: Duration, error: ApiError, submission: bool) -> Self {
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = sender.send(Err(error));
        });
        Self {
            receiver,
            closed_error: if submission {
                submit_failed
            } else {
                resume_failed
            },
        }
    }
}

impl PreviewAttemptObserver {
    pub async fn wait(self) -> Result<PreviewAttemptOutcome, ApiError> {
        self.receiver.await.unwrap_or_else(|_| Err(worker_failed()))
    }
}

#[derive(Debug)]
pub struct PreviewMaintenanceObserver {
    receiver: oneshot::Receiver<Result<(), ApiError>>,
}

impl PreviewMaintenanceObserver {
    pub async fn wait(self) -> Result<(), ApiError> {
        self.receiver
            .await
            .unwrap_or_else(|_| Err(maintenance_failed()))
    }
}

impl<T, F> PreviewRuntime<T, F>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport + Send + Sync + 'static,
    F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync + 'static,
{
    pub fn new(
        runner: Arc<PreviewAttemptRunner>,
        jobs: Arc<dyn PreviewJobRepository>,
        clock: Arc<dyn Clock>,
        transport: Arc<T>,
        identity: F,
    ) -> Self {
        let shutdown = CancellationToken::new();
        Self {
            runner,
            jobs,
            clock,
            transport,
            identity: Arc::new(identity),
            retention: PreviewRetention::default(),
            active: Arc::new(Semaphore::new(1)),
            tracker: TaskTracker::new(),
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    closed: false,
                    admitted: HashMap::new(),
                }),
                shutdown: shutdown.clone(),
            }),
            shutdown_guard: Some(shutdown.drop_guard()),
        }
    }

    pub async fn start(
        runner: Arc<PreviewAttemptRunner>,
        jobs: Arc<dyn PreviewJobRepository>,
        clock: Arc<dyn Clock>,
        transport: Arc<T>,
        identity: F,
        retention: PreviewRetention,
    ) -> Result<(Self, PreviewMaintenanceObserver), ApiError> {
        let mut runtime = Self::new(runner, jobs, clock, transport, identity);
        runtime.retention = retention;
        let handle = tokio::runtime::Handle::try_current().map_err(|_| runtime_unavailable())?;

        let startup_token = CancellationToken::new();
        let _startup_guard = startup_token.clone().drop_guard();
        let startup_control = TokioResolutionControl::new(startup_token);
        let deadline = startup_control
            .now()
            .checked_add(MANAGEMENT_BUDGET)
            .ok_or_else(startup_recovery_failed)?;
        let now = runtime.clock.now();
        startup_control
            .run(deadline, runtime.jobs.interrupt_running(now))
            .await
            .map_err(|_| startup_recovery_failed())?;
        startup_control
            .run(deadline, runtime.jobs.cleanup_expired(now))
            .await
            .map_err(|_| startup_cleanup_failed())?;

        let (sender, receiver) = oneshot::channel();
        let jobs = runtime.jobs.clone();
        let clock = runtime.clock.clone();
        let shared = runtime.shared.clone();
        runtime.tracker.spawn_on(
            async move {
                let task_shared = shared.clone();
                let task = tokio::spawn(async move {
                    let first = Instant::now() + MAINTENANCE_INTERVAL;
                    let mut interval = tokio::time::interval_at(first, MAINTENANCE_INTERVAL);
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {
                            biased;
                            _ = task_shared.shutdown.cancelled() => return Ok(()),
                            _ = interval.tick() => {}
                        }
                        let now = clock.now();
                        signal_expired(&task_shared, now);
                        let token = task_shared.shutdown.child_token();
                        let control = TokioResolutionControl::new(token);
                        let deadline = control
                            .now()
                            .checked_add(MANAGEMENT_BUDGET)
                            .ok_or_else(maintenance_failed)?;
                        match control.run(deadline, jobs.cleanup_expired(now)).await {
                            Ok(_) => {}
                            Err(error)
                                if error.code == "catalog_resolution_cancelled"
                                    && task_shared.shutdown.is_cancelled() =>
                            {
                                return Ok(());
                            }
                            Err(_) => return Err(maintenance_failed()),
                        }
                    }
                });
                let result = task.await.unwrap_or_else(|_| Err(maintenance_failed()));
                if result.is_err() {
                    close_and_cancel(&shared);
                }
                let _ = sender.send(result);
            },
            &handle,
        );
        Ok((runtime, PreviewMaintenanceObserver { receiver }))
    }

    pub fn retention(&self) -> PreviewRetention {
        self.retention
    }

    pub fn enqueue(&self, job: &PreviewJob) -> Result<PreviewAttemptObserver, ApiError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| runtime_unavailable())?;
        if job.status() != PreviewJobStatus::Queued {
            return Err(invalid_job());
        }
        if job.expires_at() <= self.clock.now() {
            return Err(expired_job());
        }
        let id = job.id().to_owned();
        let attempt = job.attempt();
        let (sender, receiver) = oneshot::channel();
        let cancellation = Arc::new(self.shared.shutdown.child_token());

        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(closed());
        }
        if state.admitted.contains_key(&id) {
            return Err(duplicate());
        }
        if state.admitted.len() >= MAX_ADMITTED_ATTEMPTS {
            return Err(busy());
        }
        state.admitted.insert(
            id.clone(),
            Admission {
                attempt,
                expires_at: job.expires_at(),
                cancellation: cancellation.clone(),
            },
        );

        let registration = Registration {
            shared: Arc::downgrade(&self.shared),
            id: id.clone(),
            cancellation: cancellation.clone(),
        };
        let runner = self.runner.clone();
        let transport = self.transport.clone();
        let identity = self.identity.clone();
        let active = self.active.clone();
        self.tracker.spawn_on(
            async move {
                let _drop_guard = (*cancellation).clone().drop_guard();
                let result = supervise_attempt(
                    runner,
                    transport,
                    identity,
                    active,
                    cancellation,
                    id,
                    attempt,
                )
                .await;
                drop(registration);
                let _ = sender.send(result);
            },
            &handle,
        );
        drop(state);

        Ok(PreviewAttemptObserver { receiver })
    }

    pub fn submit(
        &self,
        submission: PreviewSubmission,
    ) -> Result<PreviewAdmissionObserver, ApiError> {
        let now = self.clock.now();
        if now < submission.created_at() || now >= submission.expires_at() {
            return Err(invalid_job());
        }
        let id = submission.id().to_owned();
        let expected = SubmittedJob {
            id: id.clone(),
            attempt: 1,
            created_at: submission.created_at(),
            expires_at: submission.expires_at(),
            observation_count: submission.observations().len() as u32,
            profile_id: submission.profile().id.clone(),
            profile_version: submission.profile().version,
            binding: submission.binding().clone(),
        };
        self.prepare(
            id,
            1,
            now,
            expected.expires_at,
            PrepareOperation::Submit {
                submission: Box::new(submission),
                expected,
            },
        )
    }

    pub fn resume(&self, job: &PreviewJob) -> Result<PreviewAdmissionObserver, ApiError> {
        if !matches!(
            job.status(),
            PreviewJobStatus::Interrupted | PreviewJobStatus::Failed | PreviewJobStatus::Cancelled
        ) {
            return Err(invalid_resume());
        }
        let now = self.clock.now();
        if now < job.created_at() || now < job.updated_at() || now >= job.expires_at() {
            return Err(expired_job());
        }
        let attempt = job.attempt().checked_add(1).ok_or_else(attempt_exhausted)?;
        self.prepare(
            job.id().to_owned(),
            attempt,
            now,
            job.expires_at(),
            PrepareOperation::Resume {
                previous_attempt: job.attempt(),
            },
        )
    }

    fn prepare(
        &self,
        id: String,
        attempt: u32,
        admitted_at: i64,
        expires_at: i64,
        operation: PrepareOperation,
    ) -> Result<PreviewAdmissionObserver, ApiError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| runtime_unavailable())?;
        let (sender, receiver) = oneshot::channel();
        let cancellation = Arc::new(self.shared.shutdown.child_token());
        let closed_error: fn() -> ApiError = match &operation {
            PrepareOperation::Submit { .. } => submit_failed,
            PrepareOperation::Resume { .. } => resume_failed,
        };

        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(closed());
        }
        if state.admitted.contains_key(&id) {
            return Err(duplicate());
        }
        if state.admitted.len() >= MAX_ADMITTED_ATTEMPTS {
            return Err(busy());
        }
        state.admitted.insert(
            id.clone(),
            Admission {
                attempt,
                expires_at,
                cancellation: cancellation.clone(),
            },
        );

        let registration = Registration {
            shared: Arc::downgrade(&self.shared),
            id: id.clone(),
            cancellation: cancellation.clone(),
        };
        let runner = self.runner.clone();
        let jobs = self.jobs.clone();
        let clock = self.clock.clone();
        let transport = self.transport.clone();
        let identity = self.identity.clone();
        let active = self.active.clone();
        self.tracker.spawn_on(
            async move {
                let _drop_guard = (*cancellation).clone().drop_guard();
                let prepare_id = id.clone();
                let prepare_cancellation = cancellation.clone();
                let prepare = tokio::spawn(async move {
                    prepare_attempt(
                        jobs,
                        clock,
                        prepare_cancellation,
                        &prepare_id,
                        operation,
                        admitted_at,
                        expires_at,
                    )
                    .await
                });
                let prepared = prepare.await.unwrap_or_else(|_| Err((closed_error)()));
                let Err(error) = prepared else {
                    let (completion_sender, completion_receiver) = oneshot::channel();
                    let receipt = PreviewAcceptedAttempt {
                        job_id: id.clone(),
                        attempt,
                        completion: PreviewAttemptObserver {
                            receiver: completion_receiver,
                        },
                    };
                    let _ = sender.send(Ok(receipt));
                    let result = supervise_attempt(
                        runner,
                        transport,
                        identity,
                        active,
                        cancellation,
                        id,
                        attempt,
                    )
                    .await;
                    drop(registration);
                    let _ = completion_sender.send(result);
                    return;
                };
                drop(registration);
                let _ = sender.send(Err(error));
            },
            &handle,
        );
        drop(state);

        Ok(PreviewAdmissionObserver {
            receiver,
            closed_error,
        })
    }

    pub async fn cancel(&self, id: &str, expected_attempt: u32) -> Result<bool, ApiError> {
        {
            let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(admission) = state.admitted.get(id)
                && admission.attempt == expected_attempt
            {
                admission.cancellation.cancel();
            }
        }
        let jobs = self.jobs.clone();
        let id = id.to_owned();
        let now = self.clock.now();
        let token = CancellationToken::new();
        let _guard = token.clone().drop_guard();
        let control = TokioResolutionControl::new(token);
        match control
            .run(MANAGEMENT_BUDGET, jobs.cancel(id, expected_attempt, now))
            .await
        {
            Ok(cancelled) => Ok(cancelled),
            Err(_) => Err(cancel_failed()),
        }
    }

    pub async fn delete(&self, id: &str) -> Result<bool, ApiError> {
        {
            let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(admission) = state.admitted.get(id) {
                admission.cancellation.cancel();
            }
        }
        let jobs = self.jobs.clone();
        let id = id.to_owned();
        let token = CancellationToken::new();
        let _guard = token.clone().drop_guard();
        let control = TokioResolutionControl::new(token);
        match control.run(MANAGEMENT_BUDGET, jobs.delete(id)).await {
            Ok(deleted) => Ok(deleted),
            Err(_) => Err(delete_failed()),
        }
    }

    pub async fn shutdown(&self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            self.tracker.close();
        }
        self.shared.shutdown.cancel();
        self.tracker.wait().await;
    }
}

struct SubmittedJob {
    id: String,
    attempt: u32,
    created_at: i64,
    expires_at: i64,
    observation_count: u32,
    profile_id: String,
    profile_version: u32,
    binding: collector_core::access::Endpoint,
}

enum PrepareOperation {
    Submit {
        submission: Box<PreviewSubmission>,
        expected: SubmittedJob,
    },
    Resume {
        previous_attempt: u32,
    },
}

impl PrepareOperation {
    fn failure(&self) -> ApiError {
        match self {
            Self::Submit { .. } => submit_failed(),
            Self::Resume { .. } => resume_failed(),
        }
    }
}

async fn prepare_attempt(
    jobs: Arc<dyn PreviewJobRepository>,
    clock: Arc<dyn Clock>,
    cancellation: Arc<CancellationToken>,
    id: &str,
    operation: PrepareOperation,
    admitted_at: i64,
    expires_at: i64,
) -> Result<(), ApiError> {
    let now = clock.now();
    if now < admitted_at {
        return Err(operation.failure());
    }
    let remaining = expires_at.checked_sub(now).filter(|value| *value > 0);
    let Some(remaining) = remaining.and_then(|value| u64::try_from(value).ok()) else {
        return Err(operation.failure());
    };
    let control = TokioResolutionControl::new((*cancellation).clone());
    let deadline = MANAGEMENT_BUDGET.min(Duration::from_secs(remaining));
    match operation {
        PrepareOperation::Submit {
            submission,
            expected,
        } => control
            .run(deadline, jobs.submit(*submission))
            .await
            .map_err(sanitize_submit_error)
            .and_then(|job| {
                let completed_at = clock.now();
                if submitted_job_matches(&job, &expected)
                    && completed_at >= now
                    && completed_at < expires_at
                {
                    Ok(())
                } else {
                    Err(submit_failed())
                }
            }),
        PrepareOperation::Resume { previous_attempt } => {
            let expected_attempt = previous_attempt
                .checked_add(1)
                .ok_or_else(attempt_exhausted)?;
            control
                .run(deadline, jobs.resume(id.to_owned(), previous_attempt, now))
                .await
                .map_err(sanitize_resume_error)
                .and_then(|attempt| {
                    let completed_at = clock.now();
                    if attempt == Some(expected_attempt)
                        && completed_at >= now
                        && completed_at < expires_at
                    {
                        Ok(())
                    } else {
                        Err(resume_failed())
                    }
                })
        }
    }
}

fn submitted_job_matches(job: &PreviewJob, expected: &SubmittedJob) -> bool {
    job.id() == expected.id
        && job.status() == PreviewJobStatus::Queued
        && job.attempt() == expected.attempt
        && job.created_at() == expected.created_at
        && job.updated_at() == expected.created_at
        && job.expires_at() == expected.expires_at
        && job.observation_count() == expected.observation_count
        && job.profile_id() == expected.profile_id
        && job.profile_version() == expected.profile_version
        && job.is_bound_to(&expected.binding)
}

async fn execute_attempt<T, F>(
    runner: Arc<PreviewAttemptRunner>,
    transport: Arc<T>,
    identity: Arc<F>,
    active: Arc<Semaphore>,
    cancellation: Arc<CancellationToken>,
    id: String,
    attempt: u32,
) -> Result<PreviewAttemptOutcome, ApiError>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport + Send + Sync + 'static,
    F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync + 'static,
{
    let permit = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(cancelled()),
        permit = active.acquire_owned() => permit.map_err(|_| worker_failed())?,
    };
    let control = TokioResolutionControl::new((*cancellation).clone());
    let result = runner
        .run(
            &id,
            attempt,
            transport.as_ref(),
            &control,
            identity.as_ref(),
        )
        .await;
    drop(permit);
    result
}

async fn supervise_attempt<T, F>(
    runner: Arc<PreviewAttemptRunner>,
    transport: Arc<T>,
    identity: Arc<F>,
    active: Arc<Semaphore>,
    cancellation: Arc<CancellationToken>,
    id: String,
    attempt: u32,
) -> Result<PreviewAttemptOutcome, ApiError>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport + Send + Sync + 'static,
    F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync + 'static,
{
    tokio::spawn(execute_attempt(
        runner,
        transport,
        identity,
        active,
        cancellation,
        id,
        attempt,
    ))
    .await
    .unwrap_or_else(|_| Err(worker_failed()))
}

pub(crate) fn sanitize_submit_error(error: ApiError) -> ApiError {
    match error.code.as_str() {
        "preview_job_invalid" => invalid_job(),
        "preview_observation_invalid" => ApiError::new(
            "preview_observation_invalid",
            "Observation is outside the supported normalized preview subset.",
        ),
        "preview_observation_too_large" => ApiError::new(
            "preview_observation_too_large",
            "A normalized preview observation exceeds the encoded size limit.",
        ),
        "preview_submission_too_large" => ApiError::new(
            "preview_submission_too_large",
            "Preview submission exceeds its bounded storage limit.",
        ),
        "preview_binding_changed" => ApiError::new(
            "preview_binding_changed",
            "The configured NeoDB account binding changed.",
        ),
        "preview_profile_changed" => ApiError::new(
            "preview_profile_changed",
            "The selected mapping profile changed.",
        ),
        _ => submit_failed(),
    }
}

pub(crate) fn sanitize_resume_error(error: ApiError) -> ApiError {
    match error.code.as_str() {
        "preview_attempt_exhausted" => attempt_exhausted(),
        "preview_transition_invalid" => ApiError::new(
            "preview_transition_invalid",
            "Preview transition metadata is invalid.",
        ),
        "preview_binding_changed" => ApiError::new(
            "preview_binding_changed",
            "The configured NeoDB account binding changed.",
        ),
        "preview_profile_changed" => ApiError::new(
            "preview_profile_changed",
            "The selected mapping profile changed.",
        ),
        _ => resume_failed(),
    }
}

impl<T, F> PreviewManagement for PreviewRuntime<T, F>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport + Send + Sync + 'static,
    F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync + 'static,
{
    fn cancel<'a>(&'a self, id: &'a str, expected_attempt: u32) -> PortFuture<'a, bool> {
        Box::pin(async move { PreviewRuntime::cancel(self, id, expected_attempt).await })
    }

    fn delete<'a>(&'a self, id: &'a str) -> PortFuture<'a, bool> {
        Box::pin(async move { PreviewRuntime::delete(self, id).await })
    }
}

impl<T, F> PreviewSubmissionRuntime for PreviewRuntime<T, F>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport + Send + Sync + 'static,
    F: Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync + 'static,
{
    fn retention(&self) -> PreviewRetention {
        PreviewRuntime::retention(self)
    }

    fn submit(&self, submission: PreviewSubmission) -> Result<PreviewAdmissionObserver, ApiError> {
        PreviewRuntime::submit(self, submission)
    }

    fn resume(&self, job: &PreviewJob) -> Result<PreviewAdmissionObserver, ApiError> {
        PreviewRuntime::resume(self, job)
    }
}

fn signal_expired(shared: &Shared, now: i64) {
    let state = shared
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for admission in state.admitted.values() {
        if admission.expires_at <= now {
            admission.cancellation.cancel();
        }
    }
}

fn close_and_cancel(shared: &Shared) {
    {
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.closed = true;
    }
    shared.shutdown.cancel();
}

impl<T, F> Drop for PreviewRuntime<T, F> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.closed = true;
            self.tracker.close();
        }
        if let Some(guard) = self.shutdown_guard.take() {
            drop(guard);
        }
    }
}

struct Registration {
    shared: Weak<Shared>,
    id: String,
    cancellation: Arc<CancellationToken>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .admitted
            .get(&self.id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.cancellation, &self.cancellation))
        {
            state.admitted.remove(&self.id);
        }
    }
}

fn invalid_job() -> ApiError {
    ApiError::new(
        "preview_runtime_job_invalid",
        "Preview job metadata is not eligible for runtime admission.",
    )
}

fn invalid_resume() -> ApiError {
    ApiError::new(
        "preview_runtime_job_invalid",
        "Only an interrupted, failed, or cancelled preview attempt can be resumed.",
    )
}

fn attempt_exhausted() -> ApiError {
    ApiError::new(
        "preview_attempt_exhausted",
        "The preview attempt generation is exhausted.",
    )
}

fn submit_failed() -> ApiError {
    ApiError::new(
        "preview_submit_failed",
        "Preview submission could not be persisted; query job status.",
    )
}

fn resume_failed() -> ApiError {
    ApiError::new(
        "preview_resume_failed",
        "Preview resume could not be persisted; query job status.",
    )
}

fn expired_job() -> ApiError {
    ApiError::new(
        "preview_runtime_job_expired",
        "The preview job has expired and cannot be admitted.",
    )
}

fn busy() -> ApiError {
    ApiError::new(
        "preview_runtime_busy",
        "The preview runtime admission queue is full.",
    )
}

fn duplicate() -> ApiError {
    ApiError::new(
        "preview_runtime_duplicate",
        "This preview job is already admitted locally.",
    )
}

fn closed() -> ApiError {
    ApiError::new(
        "preview_runtime_closed",
        "The preview runtime is shutting down.",
    )
}

fn worker_failed() -> ApiError {
    ApiError::new(
        "preview_runtime_worker_failed",
        "The preview worker stopped unexpectedly; query job status.",
    )
}

fn runtime_unavailable() -> ApiError {
    ApiError::new(
        "preview_runtime_unavailable",
        "The preview runtime requires an active Tokio executor.",
    )
}

fn cancelled() -> ApiError {
    ApiError::new(
        "catalog_resolution_cancelled",
        "Catalog resolution was cancelled.",
    )
}

fn cancel_failed() -> ApiError {
    ApiError::new(
        "preview_cancel_failed",
        "Preview cancellation could not be persisted; query job status.",
    )
}

fn delete_failed() -> ApiError {
    ApiError::new(
        "preview_delete_failed",
        "Preview deletion could not be persisted; query job status.",
    )
}

fn startup_recovery_failed() -> ApiError {
    ApiError::new(
        "preview_startup_recovery_failed",
        "Preview startup recovery could not be completed.",
    )
}

fn startup_cleanup_failed() -> ApiError {
    ApiError::new(
        "preview_startup_cleanup_failed",
        "Expired preview cleanup could not be completed during startup.",
    )
}

fn maintenance_failed() -> ApiError {
    ApiError::new(
        "preview_maintenance_failed",
        "Preview retention maintenance stopped unexpectedly.",
    )
}

#[cfg(test)]
pub(crate) mod tests;
