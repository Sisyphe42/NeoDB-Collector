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
    clock: Arc<FixedClock>,
    jobs: Vec<PreviewJob>,
}

pub(crate) struct HttpRuntimeFixture {
    _dir: tempfile::TempDir,
    pub(crate) storage: Storage,
    runtime: Arc<PreviewRuntime<Transport, IdentityFn>>,
    transport: Arc<Transport>,
    jobs: Vec<PreviewJob>,
}

impl HttpRuntimeFixture {
    pub(crate) fn management(&self) -> Arc<dyn PreviewManagement> {
        self.runtime.clone()
    }

    pub(crate) fn enqueue(&self, index: usize) -> PreviewAttemptObserver {
        self.runtime.enqueue(&self.jobs[index]).unwrap()
    }

    pub(crate) fn job_id(&self, index: usize) -> &str {
        self.jobs[index].id()
    }

    pub(crate) async fn wait_for_calls(&self, expected: usize) {
        wait_for_calls(&self.transport, expected).await;
    }

    pub(crate) fn release(&self) {
        self.transport.release();
    }

    pub(crate) fn active_calls(&self) -> usize {
        self.transport.active.load(Ordering::SeqCst)
    }

    pub(crate) async fn shutdown(&self) {
        self.runtime.shutdown().await;
    }
}

pub(crate) async fn http_fixture(
    job_count: usize,
    observations_per_job: usize,
) -> HttpRuntimeFixture {
    let fixture = fixture_with_observations(job_count, observations_per_job).await;
    HttpRuntimeFixture {
        _dir: fixture._dir,
        storage: fixture.storage,
        runtime: Arc::new(fixture.runtime),
        transport: fixture.transport,
        jobs: fixture.jobs,
    }
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
        clock.clone(),
        transport.clone(),
        catalog_identity as IdentityFn,
    );
    Fixture {
        _dir: dir,
        storage,
        runtime,
        transport,
        clock,
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

struct MaintenanceJobs {
    mode: AtomicU8,
    interrupts: AtomicUsize,
    cleanups: AtomicUsize,
    periodic_entries: AtomicUsize,
    cleaned: tokio::sync::Notify,
    periodic_entered: tokio::sync::Notify,
    gate: tokio::sync::Semaphore,
    storage: Storage,
}

impl MaintenanceJobs {
    fn new(storage: &Storage, mode: u8) -> Arc<Self> {
        Arc::new(Self {
            mode: AtomicU8::new(mode),
            interrupts: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            periodic_entries: AtomicUsize::new(0),
            cleaned: tokio::sync::Notify::new(),
            periodic_entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            storage: storage.clone(),
        })
    }
}

impl PreviewJobRepository for MaintenanceJobs {
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
        Box::pin(async { unreachable!() })
    }
    fn interrupt_running(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async move {
            match self.mode.load(Ordering::SeqCst) {
                4 => return Err(ApiError::new("raw_startup_error", "must be sanitized")),
                6 => return pending().await,
                7 | 10 => {
                    let permit = self.gate.acquire().await.unwrap();
                    permit.forget();
                }
                _ => {}
            }
            self.interrupts.fetch_add(1, Ordering::SeqCst);
            Ok(0)
        })
    }
    fn fail(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn resume(&self, _: String, _: u32, _: i64) -> PortFuture<'_, Option<u32>> {
        Box::pin(async { unreachable!() })
    }
    fn delete(&self, _: String) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn cleanup_expired(&self, now: i64) -> PortFuture<'_, u64> {
        Box::pin(async move {
            let periodic = self.cleanups.load(Ordering::SeqCst) > 0;
            if periodic {
                self.periodic_entries.fetch_add(1, Ordering::SeqCst);
                self.periodic_entered.notify_waiters();
            }
            match (self.mode.load(Ordering::SeqCst), periodic) {
                (9, true) => return Err(cancelled()),
                (10, false) => {
                    let permit = self.gate.acquire().await.unwrap();
                    permit.forget();
                    return Ok(0);
                }
                (11, false) => return pending().await,
                (1, true) => {
                    return Err(ApiError::new("raw_cleanup_error", "must be sanitized"));
                }
                (2, true) => panic!("deliberate maintenance panic"),
                (3, true) => return pending().await,
                (8, true) => {
                    let permit = self.gate.acquire().await.unwrap();
                    permit.forget();
                }
                (5, false) => {
                    return Err(ApiError::new("raw_startup_cleanup", "must be sanitized"));
                }
                _ => {}
            }
            let deleted = PreviewJobRepository::cleanup_expired(&self.storage, now).await?;
            self.cleanups.fetch_add(1, Ordering::SeqCst);
            self.cleaned.notify_waiters();
            Ok(deleted)
        })
    }
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

#[tokio::test]
async fn production_start_interrupts_unfinished_and_cleans_expired_without_resuming() {
    let f = fixture_with_observations(1, 2).await;
    let credential = collector_core::access::AccessRecord {
        id: "client:retained".into(),
        hash: "a".repeat(64),
        kind: "client".into(),
        audience: "http://127.0.0.1:19423".into(),
        name: "Retained client".into(),
        client_kind: "cli".into(),
        expires_at: 1_000,
    };
    f.storage.insert(credential.clone()).await.unwrap();
    let expired = f
        .storage
        .submit(
            PreviewSubmission::new(
                "job:expired".into(),
                endpoint(),
                imdb_default(),
                vec![observation(99)],
                50,
                100,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let terminal = f
        .storage
        .submit(
            PreviewSubmission::new(
                "job:terminal".into(),
                endpoint(),
                imdb_default(),
                vec![],
                100,
                1_000,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        f.storage
            .cancel(terminal.id().into(), terminal.attempt(), 110)
            .await
            .unwrap()
    );
    let observer = f.runtime.enqueue(&f.jobs[0]).unwrap();
    wait_for_calls(&f.transport, 1).await;
    f.transport.release();
    wait_for_calls(&f.transport, 2).await;
    let runner = f.runtime.runner.clone();
    let clock: Arc<dyn Clock> = f.clock.clone();
    let transport = f.transport.clone();
    drop(f.runtime);
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        f.storage
            .results(f.jobs[0].id().into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );

    let (runtime, maintenance) = PreviewRuntime::start(
        runner,
        Arc::new(f.storage.clone()),
        clock,
        transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::new(45).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(runtime.retention().days(), 45);
    assert_eq!(
        f.storage
            .find(
                credential.hash.clone(),
                credential.kind.clone(),
                credential.audience.clone(),
                110
            )
            .await
            .unwrap()
            .unwrap()
            .id,
        credential.id
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Interrupted
    );
    assert!(
        PreviewJobRepository::load(&f.storage, expired.id().into())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        PreviewJobRepository::load(&f.storage, terminal.id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Cancelled
    );
    assert_eq!(
        f.storage
            .results(f.jobs[0].id().into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(f.storage.endpoint().await.unwrap().is_some());
    assert!(
        MappingRepository::load(
            &f.storage,
            f.jobs[0].profile_id().into(),
            Some(f.jobs[0].profile_version()),
        )
        .await
        .unwrap()
        .is_some()
    );

    let runner = runtime.runner.clone();
    let clock = runtime.clock.clone();
    let transport = runtime.transport.clone();
    runtime.shutdown().await;
    maintenance.wait().await.unwrap();
    let (runtime, maintenance) = PreviewRuntime::start(
        runner,
        Arc::new(f.storage.clone()),
        clock,
        transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::new(45).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        PreviewJobRepository::load(&f.storage, f.jobs[0].id().into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Interrupted
    );
    assert_eq!(
        f.storage
            .results(f.jobs[0].id().into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    runtime.shutdown().await;
    maintenance.wait().await.unwrap();
}

#[tokio::test]
async fn maintenance_waits_for_first_tick_then_signals_expired_active_work() {
    let f = fixture(0).await;
    let runner = f.runtime.runner.clone();
    let clock: Arc<dyn Clock> = f.clock.clone();
    let transport = f.transport.clone();
    drop(f.runtime);
    let maintenance_jobs = Arc::new(MaintenanceJobs {
        mode: AtomicU8::new(0),
        interrupts: AtomicUsize::new(0),
        cleanups: AtomicUsize::new(0),
        periodic_entries: AtomicUsize::new(0),
        cleaned: tokio::sync::Notify::new(),
        periodic_entered: tokio::sync::Notify::new(),
        gate: tokio::sync::Semaphore::new(0),
        storage: f.storage.clone(),
    });
    let (runtime, maintenance) = PreviewRuntime::start(
        runner,
        maintenance_jobs.clone(),
        clock.clone(),
        transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    assert_eq!(maintenance_jobs.interrupts.load(Ordering::SeqCst), 1);
    assert_eq!(maintenance_jobs.cleanups.load(Ordering::SeqCst), 1);
    let active_job = f
        .storage
        .submit(
            PreviewSubmission::new(
                "job:tick".into(),
                endpoint(),
                imdb_default(),
                vec![observation(100)],
                100,
                1_000,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let waiting_job = f
        .storage
        .submit(
            PreviewSubmission::new(
                "job:tick-waiting".into(),
                endpoint(),
                imdb_default(),
                vec![observation(101)],
                100,
                1_000,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let active = runtime.enqueue(&active_job).unwrap();
    wait_for_calls(&transport, 1).await;
    let waiting = runtime.enqueue(&waiting_job).unwrap();
    tokio::time::pause();
    f.clock.0.store(1_000, Ordering::SeqCst);
    tokio::time::advance(Duration::from_secs(59)).await;
    tokio::task::yield_now().await;
    assert_eq!(maintenance_jobs.cleanups.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::time::resume();
    loop {
        let cleaned = maintenance_jobs.cleaned.notified();
        if maintenance_jobs.cleanups.load(Ordering::SeqCst) >= 2 {
            break;
        }
        cleaned.await;
    }
    assert_eq!(
        active.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(
        waiting.wait().await.unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    runtime.shutdown().await;
    maintenance.wait().await.unwrap();
    assert_eq!(maintenance_jobs.cleanups.load(Ordering::SeqCst), 2);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert!(transport.fetch_dropped.load(Ordering::SeqCst));
    assert!(
        PreviewJobRepository::load(&f.storage, active_job.id().into())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        PreviewJobRepository::load(&f.storage, waiting_job.id().into())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn startup_and_periodic_failures_are_static_and_close_admission() {
    for (mode, expected) in [
        (4, "preview_startup_recovery_failed"),
        (5, "preview_startup_cleanup_failed"),
    ] {
        let f = fixture(0).await;
        let jobs = Arc::new(MaintenanceJobs {
            mode: AtomicU8::new(mode),
            interrupts: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            periodic_entries: AtomicUsize::new(0),
            cleaned: tokio::sync::Notify::new(),
            periodic_entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            storage: f.storage.clone(),
        });
        let error = PreviewRuntime::start(
            f.runtime.runner.clone(),
            jobs,
            f.clock.clone(),
            f.transport.clone(),
            catalog_identity as IdentityFn,
            PreviewRetention::default(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.code, expected);
        assert!(!error.message.contains("raw_"));
    }

    for mode in [1, 2] {
        let f = fixture(1).await;
        let jobs = Arc::new(MaintenanceJobs {
            mode: AtomicU8::new(0),
            interrupts: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            periodic_entries: AtomicUsize::new(0),
            cleaned: tokio::sync::Notify::new(),
            periodic_entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            storage: f.storage.clone(),
        });
        let (runtime, maintenance) = PreviewRuntime::start(
            f.runtime.runner.clone(),
            jobs.clone(),
            f.clock.clone(),
            f.transport.clone(),
            catalog_identity as IdentityFn,
            PreviewRetention::default(),
        )
        .await
        .unwrap();
        jobs.mode.store(mode, Ordering::SeqCst);
        tokio::time::pause();
        tokio::time::advance(MAINTENANCE_INTERVAL).await;
        assert_eq!(
            maintenance.wait().await.unwrap_err().code,
            "preview_maintenance_failed"
        );
        tokio::time::resume();
        assert_eq!(
            runtime.enqueue(&f.jobs[0]).unwrap_err().code,
            "preview_runtime_closed"
        );
        runtime.shutdown().await;
    }
}

#[tokio::test]
async fn startup_and_periodic_hung_or_exact_deadline_work_fail_strictly() {
    for mode in [6, 7] {
        let f = fixture(0).await;
        let jobs = Arc::new(MaintenanceJobs {
            mode: AtomicU8::new(mode),
            interrupts: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            periodic_entries: AtomicUsize::new(0),
            cleaned: tokio::sync::Notify::new(),
            periodic_entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            storage: f.storage.clone(),
        });
        tokio::time::pause();
        let start = tokio::spawn(PreviewRuntime::start(
            f.runtime.runner.clone(),
            jobs.clone(),
            f.clock.clone(),
            f.transport.clone(),
            catalog_identity as IdentityFn,
            PreviewRetention::default(),
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(MANAGEMENT_BUDGET).await;
        if mode == 7 {
            jobs.gate.add_permits(1);
        }
        assert_eq!(
            start.await.unwrap().err().unwrap().code,
            "preview_startup_recovery_failed"
        );
        tokio::time::resume();
    }

    for mode in [3, 8] {
        let f = fixture(0).await;
        let jobs = Arc::new(MaintenanceJobs {
            mode: AtomicU8::new(0),
            interrupts: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            periodic_entries: AtomicUsize::new(0),
            cleaned: tokio::sync::Notify::new(),
            periodic_entered: tokio::sync::Notify::new(),
            gate: tokio::sync::Semaphore::new(0),
            storage: f.storage.clone(),
        });
        let (runtime, maintenance) = PreviewRuntime::start(
            f.runtime.runner.clone(),
            jobs.clone(),
            f.clock.clone(),
            f.transport.clone(),
            catalog_identity as IdentityFn,
            PreviewRetention::default(),
        )
        .await
        .unwrap();
        jobs.mode.store(mode, Ordering::SeqCst);
        tokio::time::pause();
        tokio::time::advance(MAINTENANCE_INTERVAL).await;
        loop {
            let entered = jobs.periodic_entered.notified();
            if jobs.periodic_entries.load(Ordering::SeqCst) > 0 {
                break;
            }
            entered.await;
        }
        tokio::time::advance(MANAGEMENT_BUDGET).await;
        if mode == 8 {
            jobs.gate.add_permits(1);
        }
        assert_eq!(
            maintenance.wait().await.unwrap_err().code,
            "preview_maintenance_failed"
        );
        tokio::time::resume();
        runtime.shutdown().await;
    }
}

#[tokio::test]
async fn skipped_interval_does_not_catch_up_after_a_large_time_jump() {
    let f = fixture(0).await;
    let jobs = Arc::new(MaintenanceJobs {
        mode: AtomicU8::new(0),
        interrupts: AtomicUsize::new(0),
        cleanups: AtomicUsize::new(0),
        periodic_entries: AtomicUsize::new(0),
        cleaned: tokio::sync::Notify::new(),
        periodic_entered: tokio::sync::Notify::new(),
        gate: tokio::sync::Semaphore::new(0),
        storage: f.storage.clone(),
    });
    let (runtime, maintenance) = PreviewRuntime::start(
        f.runtime.runner.clone(),
        jobs.clone(),
        f.clock.clone(),
        f.transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(MAINTENANCE_INTERVAL * 3).await;
    tokio::time::resume();
    loop {
        let cleaned = jobs.cleaned.notified();
        if jobs.cleanups.load(Ordering::SeqCst) >= 2 {
            break;
        }
        cleaned.await;
    }
    assert_eq!(jobs.periodic_entries.load(Ordering::SeqCst), 1);
    runtime.shutdown().await;
    maintenance.wait().await.unwrap();
}

#[tokio::test]
async fn expired_job_is_rejected_before_registration() {
    let f = fixture(1).await;
    f.clock.0.store(1_000, Ordering::SeqCst);
    assert_eq!(
        f.runtime.enqueue(&f.jobs[0]).unwrap_err().code,
        "preview_runtime_job_expired"
    );
    assert!(f.runtime.shared.state.lock().unwrap().admitted.is_empty());
}

#[tokio::test]
async fn maintenance_observer_drop_keeps_cleanup_owned_and_owner_drop_drains() {
    let f = fixture(0).await;
    let jobs = MaintenanceJobs::new(&f.storage, 0);
    let (runtime, observer) = PreviewRuntime::start(
        f.runtime.runner.clone(),
        jobs.clone(),
        f.clock.clone(),
        f.transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    drop(observer);
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(MAINTENANCE_INTERVAL).await;
    tokio::time::resume();
    loop {
        let cleaned = jobs.cleaned.notified();
        if jobs.cleanups.load(Ordering::SeqCst) == 2 {
            break;
        }
        cleaned.await;
    }
    let tracker = runtime.tracker.clone();
    drop(runtime);
    tracker.wait().await;
    assert!(tracker.is_empty());
    tokio::time::pause();
    tokio::time::advance(MAINTENANCE_INTERVAL * 3).await;
    tokio::time::resume();
    assert_eq!(jobs.cleanups.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_repository_cancellation_code_does_not_fabricate_host_shutdown() {
    let f = fixture(0).await;
    let jobs = MaintenanceJobs::new(&f.storage, 9);
    let (runtime, observer) = PreviewRuntime::start(
        f.runtime.runner.clone(),
        jobs,
        f.clock.clone(),
        f.transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::default(),
    )
    .await
    .unwrap();
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(MAINTENANCE_INTERVAL).await;
    assert_eq!(
        observer.wait().await.unwrap_err().code,
        "preview_maintenance_failed"
    );
    tokio::time::resume();
    assert!(runtime.shared.state.lock().unwrap().closed);
    runtime.shutdown().await;
}

#[tokio::test]
async fn startup_cleanup_shares_recovery_budget_and_rejects_hung_or_late_replies() {
    use std::{future::Future, task::Poll};
    for mode in [10, 11] {
        let f = fixture(0).await;
        let jobs = MaintenanceJobs::new(&f.storage, mode);
        tokio::time::pause();
        let start = PreviewRuntime::start(
            f.runtime.runner.clone(),
            jobs.clone(),
            f.clock.clone(),
            f.transport.clone(),
            catalog_identity as IdentityFn,
            PreviewRetention::default(),
        );
        tokio::pin!(start);
        assert!(std::future::poll_fn(|cx| Poll::Ready(start.as_mut().poll(cx).is_pending())).await);
        if mode == 10 {
            tokio::time::advance(Duration::from_secs(9)).await;
            jobs.gate.add_permits(1);
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(start.as_mut().poll(cx).is_pending())).await
            );
            assert_eq!(jobs.interrupts.load(Ordering::SeqCst), 1);
            tokio::time::advance(Duration::from_secs(1)).await;
            jobs.gate.add_permits(1);
        } else {
            tokio::time::advance(MANAGEMENT_BUDGET).await;
        }
        assert_eq!(
            start.await.err().unwrap().code,
            "preview_startup_cleanup_failed"
        );
        assert_eq!(jobs.cleanups.load(Ordering::SeqCst), 0);
        tokio::time::resume();
    }
}

#[test]
fn production_start_without_executor_performs_no_recovery_or_cleanup() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let executor = tokio::runtime::Runtime::new().unwrap();
    let f = executor.block_on(fixture(0));
    drop(executor);
    let jobs = MaintenanceJobs::new(&f.storage, 0);
    let mut start = Box::pin(PreviewRuntime::start(
        f.runtime.runner.clone(),
        jobs.clone(),
        f.clock.clone(),
        f.transport.clone(),
        catalog_identity as IdentityFn,
        PreviewRetention::default(),
    ));
    let mut context = Context::from_waker(Waker::noop());
    let Poll::Ready(Err(error)) = start.as_mut().poll(&mut context) else {
        panic!("Missing executor must reject before port calls.");
    };
    assert_eq!(error.code, "preview_runtime_unavailable");
    assert_eq!(jobs.interrupts.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.cleanups.load(Ordering::SeqCst), 0);
}
