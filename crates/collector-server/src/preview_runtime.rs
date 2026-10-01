//! Bounded ownership for explicitly selected preview attempts.

use crate::resolution_control::TokioResolutionControl;
use collector_core::{
    access::Clock,
    catalog::{NeoDbCatalogSearchTransport, NeoDbCatalogTransport},
    catalog_resolver::{CatalogIdentity, ResolutionControl},
    preview_jobs::{PreviewJob, PreviewJobRepository, PreviewJobStatus},
    preview_runner::{PreviewAttemptOutcome, PreviewAttemptRunner},
};
use collector_domain::{ApiError, observation::ObservedItem};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::sync::{Semaphore, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

const MAX_ADMITTED_ATTEMPTS: usize = 9;
const MANAGEMENT_BUDGET: Duration = Duration::from_secs(10);

pub struct PreviewRuntime<T, F> {
    runner: Arc<PreviewAttemptRunner>,
    jobs: Arc<dyn PreviewJobRepository>,
    clock: Arc<dyn Clock>,
    transport: Arc<T>,
    identity: Arc<F>,
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
    cancellation: Arc<CancellationToken>,
}

#[derive(Debug)]
pub struct PreviewAttemptObserver {
    receiver: oneshot::Receiver<Result<PreviewAttemptOutcome, ApiError>>,
}

impl PreviewAttemptObserver {
    pub async fn wait(self) -> Result<PreviewAttemptOutcome, ApiError> {
        self.receiver.await.unwrap_or_else(|_| Err(worker_failed()))
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

    pub fn enqueue(&self, job: &PreviewJob) -> Result<PreviewAttemptObserver, ApiError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| runtime_unavailable())?;
        if job.status() != PreviewJobStatus::Queued {
            return Err(invalid_job());
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
                let task_cancellation = cancellation.clone();
                let task = tokio::spawn(async move {
                    let _drop_guard = (*task_cancellation).clone().drop_guard();
                    let permit = tokio::select! {
                        biased;
                        _ = task_cancellation.cancelled() => return Err(cancelled()),
                        permit = active.acquire_owned() => permit.map_err(|_| worker_failed())?,
                    };
                    let control = TokioResolutionControl::new((*task_cancellation).clone());
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
                });
                let result = task.await.unwrap_or_else(|_| Err(worker_failed()));
                drop(registration);
                let _ = sender.send(result);
            },
            &handle,
        );
        drop(state);

        Ok(PreviewAttemptObserver { receiver })
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
        "Only a queued preview attempt can be admitted.",
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

#[cfg(test)]
mod tests;
