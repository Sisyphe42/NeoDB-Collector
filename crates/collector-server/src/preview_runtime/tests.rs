use super::*;
use collector_core::{
    access::{AccessRepository, Endpoint, PortFuture, SecretStore},
    catalog::{CatalogFetch, CatalogItem, CatalogSearchPage},
    mapping::{MappingRepository, imdb_default},
    marks::{MarkRead, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    preview::PreviewSnapshotService,
    preview_jobs::{PreviewJobIdentity, PreviewJobLifecycle, PreviewSubmission},
    preview_records::PreviewRecordService,
    preview_results::PreviewResultRepository,
    secrets::SecretString,
};
use collector_domain::observation::{Observation, ScalarValue};
use collector_storage::Storage;
use std::{
    collections::BTreeMap,
    future::pending,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicUsize, Ordering},
    },
};

struct FixedClock(AtomicI64);

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Secrets;

impl SecretStore for Secrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }

    fn get(&self, _: String) -> PortFuture<'_, SecretString> {
        Box::pin(async { Ok("token".into()) })
    }

    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

struct Identity;

impl NeoDbOAuthTransport for Identity {
    fn canonical_base(&self, _: String) -> PortFuture<'_, String> {
        Box::pin(async { unreachable!() })
    }

    fn register(&self, _: String, _: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async { unreachable!() })
    }

    fn authorization_url(
        &self,
        _: String,
        _: String,
        _: String,
        _: String,
    ) -> PortFuture<'_, String> {
        Box::pin(async { unreachable!() })
    }

    fn exchange(
        &self,
        _: String,
        _: OAuthApp,
        _: SecretString,
        _: String,
    ) -> PortFuture<'_, OAuthToken> {
        Box::pin(async { unreachable!() })
    }

    fn identity<'a>(&'a self, _: String, _: &'a SecretString) -> PortFuture<'a, OAuthIdentity> {
        Box::pin(async { unreachable!() })
    }
}

struct Marks;

impl NeoDbMarkTransport for Marks {
    fn read_mark<'a>(
        &'a self,
        _: String,
        _: &'a SecretString,
        _: String,
        _: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async { unreachable!() })
    }
}

struct FetchGuard<'a> {
    transport: &'a Transport,
}

impl Drop for FetchGuard<'_> {
    fn drop(&mut self) {
        self.transport.active.fetch_sub(1, Ordering::SeqCst);
        self.transport.fetch_dropped.store(true, Ordering::SeqCst);
    }
}

struct Transport {
    mode: AtomicU8,
    gate: tokio::sync::Semaphore,
    entered: tokio::sync::Notify,
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    fetch_dropped: AtomicBool,
    fail_on_call: AtomicUsize,
}

impl Transport {
    fn held() -> Self {
        Self {
            mode: AtomicU8::new(0),
            gate: tokio::sync::Semaphore::new(0),
            entered: tokio::sync::Notify::new(),
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            fetch_dropped: AtomicBool::new(false),
            fail_on_call: AtomicUsize::new(0),
        }
    }

    fn release(&self) {
        self.gate.add_permits(1);
    }
}

impl NeoDbCatalogTransport for Transport {
    fn fetch(&self, _: String, _: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.entered.notify_waiters();
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            let _guard = FetchGuard { transport: self };
            match self.mode.load(Ordering::SeqCst) {
                0 => {
                    let permit = self.gate.acquire().await.unwrap();
                    permit.forget();
                }
                1 => panic!("deliberate transport panic"),
                _ => {}
            }
            if self.fail_on_call.load(Ordering::SeqCst) == call {
                return Err(ApiError::new("catalog_unavailable", "Catalog unavailable."));
            }
            Ok(CatalogFetch::Unsupported)
        })
    }

    fn detail(&self, _: String, _: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async { unreachable!() })
    }
}

impl NeoDbCatalogSearchTransport for Transport {
    fn search(&self, _: String, _: String, _: String) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async { unreachable!() })
    }
}

type IdentityFn = fn(&ObservedItem) -> Result<CatalogIdentity, ApiError>;

struct Fixture {
    _dir: tempfile::TempDir,
    storage: Storage,
    runtime: PreviewRuntime<Transport, IdentityFn>,
    transport: Arc<Transport>,
    jobs: Vec<PreviewJob>,
}

async fn fixture(job_count: usize) -> Fixture {
    fixture_with_observations(job_count, 1).await
}

async fn fixture_with_observations(job_count: usize, observations_per_job: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let endpoint = endpoint();
    storage.set_endpoint(Some(endpoint.clone())).await.unwrap();
    let profile = imdb_default();
    storage.save(profile.clone(), 0).await.unwrap();
    let clock = Arc::new(FixedClock(AtomicI64::new(110)));
    let snapshots = Arc::new(PreviewSnapshotService::new(
        Arc::new(storage.clone()),
        Arc::new(Secrets),
        clock.clone(),
        Arc::new(Identity),
        Arc::new(Marks),
    ));
    let producer = Arc::new(PreviewRecordService::new(snapshots));
    let runner = Arc::new(PreviewAttemptRunner::new(
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        clock.clone(),
        producer,
    ));
    let mut jobs = Vec::new();
    for index in 0..job_count {
        jobs.push(
            storage
                .submit(
                    PreviewSubmission::new(
                        format!("job:{index}"),
                        endpoint.clone(),
                        profile.clone(),
                        (0..observations_per_job)
                            .map(|offset| observation(index * observations_per_job + offset))
                            .collect(),
                        100,
                        1_000,
                    )
                    .unwrap(),
                )
                .await
                .unwrap(),
        );
    }
    let transport = Arc::new(Transport::held());
    let runtime = PreviewRuntime::new(
        runner,
        Arc::new(storage.clone()),
        clock,
        transport.clone(),
        catalog_identity as IdentityFn,
    );
    Fixture {
        _dir: dir,
        storage,
        runtime,
        transport,
        jobs,
    }
}

fn endpoint() -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: "credential-a".into(),
    }
}

fn observation(index: usize) -> Observation {
    let id = format!("tt{:07}", index + 1);
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: id.clone(),
        observed_at: "2026-10-01T00:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{:064x}", index + 1),
        item: ObservedItem {
            media_type: "movie".into(),
            canonical_url: format!("https://www.imdb.com/title/{id}/"),
            external_ids: BTreeMap::from([("imdb:title".into(), id)]),
            title_hint: Some(format!("Title {index}")),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(8.0))]),
    }
}

fn catalog_identity(item: &ObservedItem) -> Result<CatalogIdentity, ApiError> {
    Ok(CatalogIdentity {
        source_url: item.canonical_url.clone(),
        media_type: item.media_type.clone(),
        model: "Movie".into(),
        scheme: "imdb:title".into(),
        external_id: item.external_ids["imdb:title"].clone(),
    })
}

async fn wait_for_calls(transport: &Transport, expected: usize) {
    loop {
        let entered = transport.entered.notified();
        if transport.calls.load(Ordering::SeqCst) >= expected {
            return;
        }
        entered.await;
    }
}

fn queued_generation(job: &PreviewJob, attempt: u32) -> PreviewJob {
    PreviewJob::from_stored(
        PreviewJobIdentity {
            id: job.id().to_owned(),
            profile_id: job.profile_id().to_owned(),
            profile_version: job.profile_version(),
            observation_count: job.observation_count(),
        },
        PreviewJobLifecycle {
            status: PreviewJobStatus::Queued,
            attempt,
            created_at: job.created_at(),
            updated_at: job.updated_at(),
            expires_at: job.expires_at(),
        },
        endpoint(),
    )
    .unwrap()
}

#[tokio::test]
async fn admission_is_single_bounded_and_duplicate_safe() {
    let f = fixture(10).await;
    let first = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    assert_eq!(
        f.runtime.enqueue(&f.jobs[0]).unwrap_err().code,
        "preview_runtime_duplicate"
    );
    let different_generation = queued_generation(&f.jobs[0], 2);
    assert_eq!(
        f.runtime.enqueue(&different_generation).unwrap_err().code,
        "preview_runtime_duplicate"
    );
    let mut waiting = Vec::new();
    for job in &f.jobs[1..9] {
        waiting.push((job.id().to_owned(), f.runtime.enqueue(job).unwrap()));
    }
    assert_eq!(
        f.runtime.enqueue(&f.jobs[9]).unwrap_err().code,
        "preview_runtime_busy"
    );
    assert_eq!(f.transport.max_active.load(Ordering::SeqCst), 1);

    for (id, observer) in waiting {
        assert!(f.runtime.cancel(&id, 1).await.unwrap());
        assert_eq!(
            observer.wait().await.unwrap_err().code,
            "catalog_resolution_cancelled"
        );
    }
    let replacement = f.runtime.enqueue(&f.jobs[9]).unwrap();
    assert!(f.runtime.cancel(f.jobs[9].id(), 1).await.unwrap());
    assert!(replacement.wait().await.is_err());

    f.transport.release();
    assert_eq!(
        first.wait().await.unwrap(),
        PreviewAttemptOutcome::Completed
    );
    let stale = queued_generation(&f.jobs[0], 2);
    assert_eq!(
        f.runtime.enqueue(&stale).unwrap().wait().await.unwrap(),
        PreviewAttemptOutcome::NotClaimed
    );
}

#[tokio::test]
async fn distinct_jobs_complete_sequentially_and_reuse_the_permit() {
    let f = fixture(2).await;
    let first = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    let second = f.runtime.enqueue(&f.jobs[1]).unwrap();
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[1].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Queued
    );
    f.transport.release();
    assert_eq!(
        first.wait().await.unwrap(),
        PreviewAttemptOutcome::Completed
    );
    wait_for_calls(&f.transport, 2).await;
    assert_eq!(f.transport.max_active.load(Ordering::SeqCst), 1);
    f.transport.release();
    assert_eq!(
        second.wait().await.unwrap(),
        PreviewAttemptOutcome::Completed
    );
}

#[tokio::test]
async fn cancellation_delete_and_wrong_attempt_are_truthful() {
    let f = fixture(2).await;
    let first = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    assert!(!f.runtime.cancel(f.jobs[0].id(), 2).await.unwrap());
    assert_eq!(f.transport.active.load(Ordering::SeqCst), 1);
    assert!(f.runtime.cancel(f.jobs[0].id(), 1).await.unwrap());
    assert_eq!(
        first.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert!(f.transport.fetch_dropped.load(Ordering::SeqCst));

    assert_eq!(
        f.storage
            .resume(f.jobs[0].id().into(), 1, 110)
            .await
            .unwrap(),
        Some(2)
    );
    let resumed = PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
        .await
        .unwrap()
        .unwrap();
    let resumed_observer = f.runtime.enqueue(&resumed).unwrap();
    wait_for_calls(&f.transport, 2).await;
    assert!(!f.runtime.cancel(f.jobs[0].id(), 1).await.unwrap());
    assert_eq!(f.transport.active.load(Ordering::SeqCst), 1);
    assert!(f.runtime.cancel(f.jobs[0].id(), 2).await.unwrap());
    assert_eq!(
        resumed_observer.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );

    let second = f.runtime.enqueue(&f.jobs[1]).unwrap();
    wait_for_calls(&f.transport, 3).await;
    assert!(f.runtime.delete(f.jobs[1].id()).await.unwrap());
    assert_eq!(
        second.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert!(
        PreviewJobRepository::load(&f.storage, f.jobs[1].id().into())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn panic_observer_is_sanitized_and_registration_is_released() {
    let f = fixture(1).await;
    f.transport.mode.store(1, Ordering::SeqCst);
    let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "preview_runtime_worker_failed"
    );
    let current = PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.status(), PreviewJobStatus::Running);
    assert_eq!(
        f.runtime.enqueue(&current).unwrap_err().code,
        "preview_runtime_job_invalid"
    );
    let stale = queued_generation(&f.jobs[0], 2);
    assert!(f.runtime.enqueue(&stale).is_ok());
    f.runtime.shutdown().await;
}

#[tokio::test]
async fn observer_drop_does_not_cancel_accepted_work() {
    let f = fixture(2).await;
    drop(f.runtime.enqueue(&f.jobs[0]).unwrap());
    wait_for_calls(&f.transport, 1).await;
    f.transport.release();
    f.runtime.tracker.close();
    f.runtime.tracker.wait().await;
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Completed
    );
}

#[tokio::test]
async fn shutdown_signals_active_and_queued_without_persisted_cancel() {
    let f = fixture(2).await;
    let active = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    let pending = f.runtime.enqueue(&f.jobs[1]).unwrap();
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
    f.runtime.shutdown().await;
    assert!(f.runtime.shared.state.lock().unwrap().admitted.is_empty());
    assert_eq!(f.runtime.active.available_permits(), 1);
    assert_eq!(
        active.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        pending.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Running
    );
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[1].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Queued
    );
    assert_eq!(
        f.runtime.enqueue(&f.jobs[1]).unwrap_err().code,
        "preview_runtime_closed"
    );
}

#[tokio::test]
async fn owner_drop_interrupts_without_persisting_user_cancel() {
    let f = fixture(1).await;
    let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    drop(f.runtime);
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Running
    );
}

#[tokio::test]
async fn runner_failure_releases_admission_and_preserves_partial_results() {
    let f = fixture_with_observations(1, 2).await;
    f.transport.mode.store(2, Ordering::SeqCst);
    f.transport.fail_on_call.store(2, Ordering::SeqCst);
    let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "catalog_unavailable"
    );
    let current = PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.status(), PreviewJobStatus::Failed);
    assert_eq!(
        PreviewResultRepository::results(&f.storage, f.jobs[0].id().into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    let stale = queued_generation(&f.jobs[0], 2);
    assert!(f.runtime.enqueue(&stale).is_ok());
    f.runtime.shutdown().await;
}

#[test]
fn enqueue_without_executor_is_static_and_does_not_register() {
    let initial = tokio::runtime::Runtime::new().unwrap();
    let f = initial.block_on(fixture(1));
    drop(initial);
    assert_eq!(
        f.runtime.enqueue(&f.jobs[0]).unwrap_err().code,
        "preview_runtime_unavailable"
    );
    assert!(f.runtime.shared.state.lock().unwrap().admitted.is_empty());
    let executor = tokio::runtime::Runtime::new().unwrap();
    executor.block_on(async {
        let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
        f.runtime.shutdown().await;
        assert_eq!(
            observer.wait().await.unwrap_err().code,
            "catalog_resolution_cancelled"
        );
    });
}

struct FailingManagementJobs {
    mode: AtomicU8,
    calls: Mutex<Vec<&'static str>>,
    gate: tokio::sync::Semaphore,
}

impl PreviewJobRepository for FailingManagementJobs {
    fn submit(&self, _: PreviewSubmission) -> PortFuture<'_, PreviewJob> {
        Box::pin(async { unreachable!() })
    }
    fn load(&self, _: String) -> PortFuture<'_, Option<PreviewJob>> {
        Box::pin(async { unreachable!() })
    }
    fn observations(&self, _: String, _: u32, _: u32) -> PortFuture<'_, Vec<Observation>> {
        Box::pin(async { unreachable!() })
    }
    fn claim(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn cancel(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("cancel");
            if self.mode.load(Ordering::SeqCst) == 0 {
                pending().await
            } else if self.mode.load(Ordering::SeqCst) == 2 {
                let permit = self.gate.acquire().await.unwrap();
                permit.forget();
                Ok(true)
            } else {
                Err(ApiError::new("raw_database_detail", "must be sanitized"))
            }
        })
    }
    fn interrupt_running(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }
    fn fail(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn resume(&self, _: String, _: u32, _: i64) -> PortFuture<'_, Option<u32>> {
        Box::pin(async { unreachable!() })
    }
    fn delete(&self, _: String) -> PortFuture<'_, bool> {
        Box::pin(async move {
            self.calls.lock().unwrap().push("delete");
            if self.mode.load(Ordering::SeqCst) == 0 {
                pending().await
            } else if self.mode.load(Ordering::SeqCst) == 2 {
                let permit = self.gate.acquire().await.unwrap();
                permit.forget();
                Ok(true)
            } else {
                Err(ApiError::new("raw_database_detail", "must be sanitized"))
            }
        })
    }
    fn cleanup_expired(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }
}

#[tokio::test]
async fn management_timeout_and_errors_are_bounded_and_sanitized() {
    let mut f = fixture(1).await;
    let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    let management = Arc::new(FailingManagementJobs {
        mode: AtomicU8::new(1),
        calls: Mutex::new(Vec::new()),
        gate: tokio::sync::Semaphore::new(0),
    });
    f.runtime.jobs = management.clone();
    assert_eq!(
        f.runtime.cancel(f.jobs[0].id(), 1).await.unwrap_err().code,
        "preview_cancel_failed"
    );
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Running
    );
    assert_eq!(
        f.runtime.delete(f.jobs[0].id()).await.unwrap_err().code,
        "preview_delete_failed"
    );
    assert!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .is_some()
    );

    tokio::time::pause();
    management.mode.store(0, Ordering::SeqCst);
    let cancel = f.runtime.cancel("job:missing", 1);
    tokio::pin!(cancel);
    assert!(
        std::future::poll_fn(|cx| {
            if cancel.as_mut().poll(cx).is_pending() {
                std::task::Poll::Ready(true)
            } else {
                std::task::Poll::Ready(false)
            }
        })
        .await
    );
    tokio::time::advance(MANAGEMENT_BUDGET).await;
    assert_eq!(cancel.await.unwrap_err().code, "preview_cancel_failed");

    let delete = f.runtime.delete("job:missing");
    tokio::pin!(delete);
    assert!(
        std::future::poll_fn(|cx| {
            if delete.as_mut().poll(cx).is_pending() {
                std::task::Poll::Ready(true)
            } else {
                std::task::Poll::Ready(false)
            }
        })
        .await
    );
    tokio::time::advance(MANAGEMENT_BUDGET).await;
    assert_eq!(delete.await.unwrap_err().code, "preview_delete_failed");

    management.mode.store(2, Ordering::SeqCst);
    let exact_cancel = f.runtime.cancel("job:missing", 1);
    tokio::pin!(exact_cancel);
    assert!(
        std::future::poll_fn(|cx| {
            if exact_cancel.as_mut().poll(cx).is_pending() {
                std::task::Poll::Ready(true)
            } else {
                std::task::Poll::Ready(false)
            }
        })
        .await
    );
    tokio::time::advance(MANAGEMENT_BUDGET).await;
    management.gate.add_permits(1);
    assert_eq!(
        exact_cancel.await.unwrap_err().code,
        "preview_cancel_failed"
    );

    let exact_delete = f.runtime.delete("job:missing");
    tokio::pin!(exact_delete);
    assert!(
        std::future::poll_fn(|cx| {
            if exact_delete.as_mut().poll(cx).is_pending() {
                std::task::Poll::Ready(true)
            } else {
                std::task::Poll::Ready(false)
            }
        })
        .await
    );
    tokio::time::advance(MANAGEMENT_BUDGET).await;
    management.gate.add_permits(1);
    assert_eq!(
        exact_delete.await.unwrap_err().code,
        "preview_delete_failed"
    );
}
