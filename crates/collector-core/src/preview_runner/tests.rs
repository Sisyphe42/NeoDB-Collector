use super::*;
use crate::{
    access::{AccessRecord, ChallengeMatch, PortFuture, SecretStore},
    catalog::{CatalogFetch, CatalogItem, CatalogSearchPage},
    marks::{MarkRead, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    secrets::SecretString,
};
use collector_domain::{access::PairedClient, observation::ScalarValue};
use std::{
    collections::BTreeMap,
    future::{Future, pending, poll_fn},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Control {
    now: AtomicU64,
    cancelled: AtomicBool,
    deadlines: Mutex<Vec<Duration>>,
}

impl ResolutionControl for Control {
    fn now(&self) -> Duration {
        Duration::from_secs(self.now.load(Ordering::SeqCst))
    }

    fn wait_until(&self, _: Duration) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn run<'a, T: Send + 'a>(
        &'a self,
        deadline: Duration,
        mut work: PortFuture<'a, T>,
    ) -> PortFuture<'a, T> {
        self.deadlines.lock().unwrap().push(deadline);
        Box::pin(poll_fn(move |cx| {
            let check = || {
                if self.cancelled.load(Ordering::SeqCst) {
                    Err(cancelled())
                } else if self.now() >= deadline {
                    Err(ApiError::new(
                        "catalog_resolution_timeout",
                        "Catalog resolution exceeded its time budget.",
                    ))
                } else {
                    Ok(())
                }
            };
            check()?;
            match work.as_mut().poll(cx) {
                Poll::Ready(result) => Poll::Ready(check().and(result)),
                Poll::Pending => Poll::Pending,
            }
        }))
    }
}

fn cancelled() -> ApiError {
    ApiError::new("catalog_resolution_cancelled", "Cancelled.")
}

struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct JobState {
    present: bool,
    status: PreviewJobStatus,
    attempt: u32,
    observations: Vec<Observation>,
    claim_allowed: bool,
    fail_allowed: bool,
    fail_calls: usize,
    claim_calls: usize,
    page_calls: Vec<(u32, u32)>,
    short_page: bool,
    expires_at: i64,
    binding_ref: String,
    rebind_after_claim: bool,
    fail_race: Option<(PreviewJobStatus, u32)>,
    load_calls: usize,
    advance_first_load: u64,
    pending_load_call: usize,
    pending_claim: bool,
    pending_page: bool,
    pending_fail: bool,
    pending_dropped: Arc<AtomicBool>,
    oversized_page: bool,
    fail_deletes: bool,
}

struct Jobs {
    state: Arc<Mutex<JobState>>,
    control: Arc<Control>,
}

impl Jobs {
    fn job(state: &JobState) -> PreviewJob {
        PreviewJob::from_stored(
            crate::preview_jobs::PreviewJobIdentity {
                id: "job:one".into(),
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                observation_count: state.observations.len() as u32,
            },
            crate::preview_jobs::PreviewJobLifecycle {
                status: state.status,
                attempt: state.attempt,
                created_at: 10,
                updated_at: 100,
                expires_at: state.expires_at,
            },
            endpoint(&state.binding_ref),
        )
        .unwrap()
    }
}

impl PreviewJobRepository for Jobs {
    fn submit(&self, _: crate::preview_jobs::PreviewSubmission) -> PortFuture<'_, PreviewJob> {
        Box::pin(async { unreachable!() })
    }

    fn load(&self, id: String) -> PortFuture<'_, Option<PreviewJob>> {
        Box::pin(async move {
            assert_eq!(id, "job:one");
            let pending_drop = {
                let mut state = self.state.lock().unwrap();
                state.load_calls += 1;
                if state.load_calls == 1 {
                    self.control
                        .now
                        .fetch_add(state.advance_first_load, Ordering::SeqCst);
                }
                (state.pending_load_call == state.load_calls).then(|| state.pending_dropped.clone())
            };
            if let Some(dropped) = pending_drop {
                let _flag = DropFlag(dropped);
                return pending().await;
            }
            let state = self.state.lock().unwrap();
            Ok(state.present.then(|| Self::job(&state)))
        })
    }

    fn observations(&self, _: String, offset: u32, limit: u32) -> PortFuture<'_, Vec<Observation>> {
        Box::pin(async move {
            let pending_drop = {
                let mut state = self.state.lock().unwrap();
                state.page_calls.push((offset, limit));
                state.pending_page.then(|| state.pending_dropped.clone())
            };
            if let Some(dropped) = pending_drop {
                let _flag = DropFlag(dropped);
                return pending().await;
            }
            let state = self.state.lock().unwrap();
            let start = offset as usize;
            let mut end = start
                .saturating_add(limit as usize)
                .min(state.observations.len());
            if state.short_page && end > start {
                end -= 1;
            }
            let mut observations = state.observations[start..end].to_vec();
            if state.oversized_page && !observations.is_empty() {
                observations.push(observations[0].clone());
            }
            Ok(observations)
        })
    }

    fn claim(&self, _: String, attempt: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            let pending_drop = {
                let mut state = self.state.lock().unwrap();
                state.claim_calls += 1;
                state.pending_claim.then(|| state.pending_dropped.clone())
            };
            if let Some(dropped) = pending_drop {
                let _flag = DropFlag(dropped);
                return pending().await;
            }
            let mut state = self.state.lock().unwrap();
            if state.claim_allowed
                && state.status == PreviewJobStatus::Queued
                && state.attempt == attempt
            {
                state.status = PreviewJobStatus::Running;
                if state.rebind_after_claim {
                    state.binding_ref = "credential-b".into();
                }
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    fn cancel(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }

    fn interrupt_running(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }

    fn fail(&self, _: String, attempt: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            let pending_drop = {
                let mut state = self.state.lock().unwrap();
                state.fail_calls += 1;
                state.pending_fail.then(|| state.pending_dropped.clone())
            };
            if let Some(dropped) = pending_drop {
                let _flag = DropFlag(dropped);
                return pending().await;
            }
            let mut state = self.state.lock().unwrap();
            if state.fail_deletes {
                state.present = false;
                return Ok(false);
            }
            if let Some((status, next_attempt)) = state.fail_race.take() {
                state.status = status;
                state.attempt = next_attempt;
                return Ok(false);
            }
            if state.fail_allowed
                && state.status == PreviewJobStatus::Running
                && state.attempt == attempt
            {
                state.status = PreviewJobStatus::Failed;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    fn resume(&self, _: String, _: u32, _: i64) -> PortFuture<'_, Option<u32>> {
        Box::pin(async { unreachable!() })
    }
    fn delete(&self, _: String) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn cleanup_expired(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }
}

struct Results {
    state: Arc<Mutex<JobState>>,
    stored: Mutex<Vec<(u32, u32)>>,
    refuse_ordinal: Mutex<Option<u32>>,
    complete_allowed: AtomicBool,
    compared: AtomicUsize,
    pending_store: AtomicBool,
    pending_complete: AtomicBool,
    pending_dropped: Arc<AtomicBool>,
    complete_calls: AtomicUsize,
}

impl PreviewResultRepository for Results {
    fn store(&self, write: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async move {
            if self.pending_store.load(Ordering::SeqCst) {
                let _flag = DropFlag(self.pending_dropped.clone());
                return pending().await;
            }
            if *self.refuse_ordinal.lock().unwrap() == Some(write.ordinal()) {
                return Ok(false);
            }
            if matches!(
                write.record().outcome(),
                crate::preview_records::PreviewRecordOutcome::Compared(_)
            ) {
                self.compared.fetch_add(1, Ordering::SeqCst);
            }
            self.stored
                .lock()
                .unwrap()
                .push((write.ordinal(), write.attempt()));
            Ok(true)
        })
    }

    fn results(
        &self,
        _: String,
        _: u32,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<crate::preview_results::StoredPreviewResult>> {
        Box::pin(async { panic!("runner must not read historical results") })
    }

    fn complete(&self, _: String, attempt: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async move {
            self.complete_calls.fetch_add(1, Ordering::SeqCst);
            if self.pending_complete.load(Ordering::SeqCst) {
                let _flag = DropFlag(self.pending_dropped.clone());
                return pending().await;
            }
            if !self.complete_allowed.load(Ordering::SeqCst) {
                return Ok(false);
            }
            let mut state = self.state.lock().unwrap();
            if state.status == PreviewJobStatus::Running && state.attempt == attempt {
                state.status = PreviewJobStatus::Completed;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }
}

struct Mappings(Mutex<Option<MappingProfile>>);
impl MappingRepository for Mappings {
    fn load(&self, id: String, version: Option<u32>) -> PortFuture<'_, Option<MappingProfile>> {
        Box::pin(async move {
            assert_eq!(id, "imdb_ratings");
            assert_eq!(version, Some(1));
            Ok(self.0.lock().unwrap().clone())
        })
    }
    fn save(&self, _: MappingProfile, _: u32) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

struct Access {
    endpoint: Mutex<Option<Endpoint>>,
    calls: AtomicUsize,
    rotate_on: AtomicUsize,
    expire_on: AtomicUsize,
    clock: Arc<TestClock>,
    expiry: i64,
}

impl AccessRepository for Access {
    fn insert(&self, _: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn exchange(&self, _: ChallengeMatch, _: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn find(
        &self,
        _: String,
        _: String,
        _: String,
        _: i64,
    ) -> PortFuture<'_, Option<AccessRecord>> {
        Box::pin(async { unreachable!() })
    }
    fn revoke(&self, _: String, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn clients(&self) -> PortFuture<'_, Vec<PairedClient>> {
        Box::pin(async { unreachable!() })
    }
    fn endpoint(&self) -> PortFuture<'_, Option<Endpoint>> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.expire_on.load(Ordering::SeqCst) == call {
                self.clock.0.store(self.expiry, Ordering::SeqCst);
            }
            if self.rotate_on.load(Ordering::SeqCst) == call {
                return Ok(Some(endpoint("credential-b")));
            }
            Ok(self.endpoint.lock().unwrap().clone())
        })
    }
    fn set_endpoint(&self, _: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

#[derive(Default)]
struct TestSecrets(AtomicUsize);
impl SecretStore for TestSecrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn get(&self, reference: String) -> PortFuture<'_, SecretString> {
        Box::pin(async move {
            assert_eq!(reference, "credential-a");
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok("access-token".into())
        })
    }
    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

#[derive(Default)]
struct TestMarks(AtomicUsize);
impl NeoDbMarkTransport for TestMarks {
    fn read_mark<'a>(
        &'a self,
        _: String,
        _: &'a SecretString,
        _: String,
        _: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(MarkRead::Present(Box::new(crate::marks::MarkSnapshot {
                item: catalog_item(),
                owner: "alice".into(),
                shelf_type: "complete".into(),
                rating_grade: Some(8),
            })))
        })
    }
}

struct NoIdentity;
impl NeoDbOAuthTransport for NoIdentity {
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
        Box::pin(async { panic!("unsupported resolution must not read identity") })
    }
}

struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct Transport {
    calls: AtomicUsize,
    advance: u64,
    control: Arc<Control>,
    cancel_on_fetch: bool,
    hang: bool,
    dropped: Arc<AtomicBool>,
    matched: bool,
    fail_on_call: usize,
}

impl NeoDbCatalogTransport for Transport {
    fn fetch(&self, _: String, _: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.control.now.fetch_add(self.advance, Ordering::SeqCst);
            if self.cancel_on_fetch {
                self.control.cancelled.store(true, Ordering::SeqCst);
            }
            if self.hang {
                let _flag = DropFlag(self.dropped.clone());
                return pending().await;
            }
            if self.fail_on_call == call {
                return Err(ApiError::new("catalog_unavailable", "Catalog unavailable."));
            }
            if self.matched {
                Ok(CatalogFetch::Detail("/api/movie/Resolved123".into()))
            } else {
                Ok(CatalogFetch::Unsupported)
            }
        })
    }
    fn detail(&self, _: String, reference: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move {
            assert_eq!(reference, "/api/movie/Resolved123");
            Ok(catalog_item())
        })
    }
}

impl NeoDbCatalogSearchTransport for Transport {
    fn search(&self, _: String, _: String, _: String) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async { unreachable!() })
    }
}

struct Fixture {
    runner: PreviewAttemptRunner,
    state: Arc<Mutex<JobState>>,
    results: Arc<Results>,
    access: Arc<Access>,
    mappings: Arc<Mappings>,
    clock: Arc<TestClock>,
    control: Arc<Control>,
    transport: Transport,
    secrets: Arc<TestSecrets>,
    marks: Arc<TestMarks>,
}

fn fixture(count: usize) -> Fixture {
    let observations = (0..count).map(observation).collect();
    let state = Arc::new(Mutex::new(JobState {
        present: true,
        status: PreviewJobStatus::Queued,
        attempt: 1,
        observations,
        claim_allowed: true,
        fail_allowed: true,
        fail_calls: 0,
        claim_calls: 0,
        page_calls: vec![],
        short_page: false,
        expires_at: 1_000,
        binding_ref: "credential-a".into(),
        rebind_after_claim: false,
        fail_race: None,
        load_calls: 0,
        advance_first_load: 0,
        pending_load_call: 0,
        pending_claim: false,
        pending_page: false,
        pending_fail: false,
        pending_dropped: Arc::new(AtomicBool::new(false)),
        oversized_page: false,
        fail_deletes: false,
    }));
    let control = Arc::new(Control::default());
    let jobs = Arc::new(Jobs {
        state: state.clone(),
        control: control.clone(),
    });
    let results = Arc::new(Results {
        state: state.clone(),
        stored: Mutex::new(vec![]),
        refuse_ordinal: Mutex::new(None),
        complete_allowed: AtomicBool::new(true),
        compared: AtomicUsize::new(0),
        pending_store: AtomicBool::new(false),
        pending_complete: AtomicBool::new(false),
        pending_dropped: Arc::new(AtomicBool::new(false)),
        complete_calls: AtomicUsize::new(0),
    });
    let clock = Arc::new(TestClock(AtomicI64::new(100)));
    let access = Arc::new(Access {
        endpoint: Mutex::new(Some(endpoint("credential-a"))),
        calls: AtomicUsize::new(0),
        rotate_on: AtomicUsize::new(0),
        expire_on: AtomicUsize::new(0),
        clock: clock.clone(),
        expiry: 1_000,
    });
    let secrets = Arc::new(TestSecrets::default());
    let marks = Arc::new(TestMarks::default());
    let snapshots = Arc::new(crate::preview::PreviewSnapshotService::new(
        access.clone(),
        secrets.clone(),
        clock.clone(),
        Arc::new(NoIdentity),
        marks.clone(),
    ));
    let producer = Arc::new(PreviewRecordService::new(snapshots));
    let mappings = Arc::new(Mappings(Mutex::new(Some(mapping::imdb_default()))));
    let runner = PreviewAttemptRunner::new(
        jobs,
        results.clone(),
        mappings.clone(),
        access.clone(),
        clock.clone(),
        producer,
    );
    let dropped = Arc::new(AtomicBool::new(false));
    let transport = Transport {
        calls: AtomicUsize::new(0),
        advance: 0,
        control: control.clone(),
        cancel_on_fetch: false,
        hang: false,
        dropped,
        matched: false,
        fail_on_call: 0,
    };
    Fixture {
        runner,
        state,
        results,
        access,
        mappings,
        clock,
        control,
        transport,
        secrets,
        marks,
    }
}

fn endpoint(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn catalog_item() -> CatalogItem {
    CatalogItem {
        uuid: "Resolved123".into(),
        canonical_url: "https://neodb.example/movie/Resolved123".into(),
        detail_url: "https://neodb.example/api/movie/Resolved123".into(),
        media_type: "movie".into(),
        model: "Movie".into(),
        parent_uuid: None,
        external_ids: vec![("imdb:title".into(), "tt0000001".into())],
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
    let external_id = item.external_ids["imdb:title"].clone();
    Ok(CatalogIdentity {
        source_url: item.canonical_url.clone(),
        media_type: item.media_type.clone(),
        model: "Movie".into(),
        scheme: "imdb:title".into(),
        external_id,
    })
}

fn broken_identity(_: &ObservedItem) -> Result<CatalogIdentity, ApiError> {
    Err(ApiError::new("adapter_failed", "Adapter failed."))
}

fn poll<T>(future: Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

fn ready<T>(poll: Poll<T>) -> T {
    match poll {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected completion"),
    }
}

#[test]
fn empty_and_multipage_attempts_complete_in_stable_order() {
    for count in [0, 205] {
        let f = fixture(count);
        let mut future = Box::pin(f.runner.run(
            "job:one",
            1,
            &f.transport,
            f.control.as_ref(),
            &catalog_identity,
        ));
        assert_eq!(
            ready(poll(future.as_mut())).unwrap(),
            PreviewAttemptOutcome::Completed
        );
        let state = f.state.lock().unwrap();
        assert_eq!(state.status, PreviewJobStatus::Completed);
        assert_eq!(
            state.page_calls,
            if count == 0 {
                vec![]
            } else {
                vec![(0, 100), (100, 100), (200, 5)]
            }
        );
        drop(state);
        let stored = f.results.stored.lock().unwrap();
        assert_eq!(stored.len(), count);
        assert!(
            stored
                .iter()
                .enumerate()
                .all(|(i, value)| *value == (i as u32, 1))
        );
        assert_eq!(f.transport.calls.load(Ordering::SeqCst), count);
    }
}

#[test]
fn losing_claim_and_missing_exact_profile_do_no_work_or_failure_transition() {
    let f = fixture(1);
    f.state.lock().unwrap().claim_allowed = false;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::NotClaimed
    );
    let state = f.state.lock().unwrap();
    assert_eq!(state.fail_calls, 0);
    assert!(state.page_calls.is_empty());
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);

    let f = fixture(1);
    *f.mappings.0.lock().unwrap() = None;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::NotClaimed
    );
    assert_eq!(f.state.lock().unwrap().claim_calls, 0);
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn changed_binding_before_first_record_stops_without_network_or_fail() {
    let f = fixture(1);
    f.access.rotate_on.store(3, Ordering::SeqCst);
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Stopped
    );
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.state.lock().unwrap().fail_calls, 0);

    let f = fixture(1);
    f.state.lock().unwrap().rebind_after_claim = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Stopped
    );
    assert_eq!(f.state.lock().unwrap().claim_calls, 1);
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);

    let f = fixture(1);
    f.access.expire_on.store(2, Ordering::SeqCst);
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Stopped
    );
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.results.stored.lock().unwrap().len(), 0);
}

#[test]
fn refused_store_or_completion_stops_without_later_records() {
    let f = fixture(3);
    *f.results.refuse_ordinal.lock().unwrap() = Some(1);
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Stopped
    );
    assert_eq!(*f.results.stored.lock().unwrap(), vec![(0, 1)]);
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 2);

    let f = fixture(0);
    f.results.complete_allowed.store(false, Ordering::SeqCst);
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Stopped
    );
}

#[test]
fn malformed_page_fails_claimed_attempt_and_cleanup_failure_is_static() {
    let f = fixture(2);
    f.state.lock().unwrap().short_page = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "preview_observation_page_invalid"
    );
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Failed);
    assert_eq!(f.results.complete_calls.load(Ordering::SeqCst), 0);

    let f = fixture(2);
    f.state.lock().unwrap().oversized_page = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "preview_observation_page_invalid"
    );
    assert_eq!(f.results.complete_calls.load(Ordering::SeqCst), 0);

    let f = fixture(1);
    f.state.lock().unwrap().observations[0].schema_version = 2;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "preview_observation_invalid"
    );
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.results.complete_calls.load(Ordering::SeqCst), 0);

    let f = fixture(1);
    f.state.lock().unwrap().fail_allowed = false;
    let broken = |_: &ObservedItem| Err(ApiError::new("adapter_failed", "Adapter failed."));
    let mut future =
        Box::pin(
            f.runner
                .run("job:one", 1, &f.transport, f.control.as_ref(), &broken),
        );
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "preview_failure_cleanup_failed"
    );
    assert_eq!(f.state.lock().unwrap().fail_calls, 1);

    let f = fixture(1);
    f.state.lock().unwrap().fail_race = Some((PreviewJobStatus::Cancelled, 1));
    let broken = |_: &ObservedItem| Err(ApiError::new("adapter_failed", "Adapter failed."));
    let mut future =
        Box::pin(
            f.runner
                .run("job:one", 1, &f.transport, f.control.as_ref(), &broken),
        );
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "adapter_failed"
    );
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Cancelled);

    let f = fixture(1);
    f.state.lock().unwrap().fail_race = Some((PreviewJobStatus::Queued, 2));
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &broken_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "adapter_failed"
    );
    assert_eq!(f.state.lock().unwrap().attempt, 2);

    let f = fixture(1);
    f.state.lock().unwrap().fail_deletes = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &broken_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "adapter_failed"
    );
    assert!(!f.state.lock().unwrap().present);
}

#[derive(Clone, Copy, Debug)]
enum PendingPhase {
    Load,
    Claim,
    Page,
    Store,
    Complete,
    Fail,
}

fn pending_fixture(phase: PendingPhase) -> Fixture {
    let f = fixture(if matches!(phase, PendingPhase::Complete) {
        0
    } else {
        1
    });
    match phase {
        PendingPhase::Load => f.state.lock().unwrap().pending_load_call = 1,
        PendingPhase::Claim => f.state.lock().unwrap().pending_claim = true,
        PendingPhase::Page => f.state.lock().unwrap().pending_page = true,
        PendingPhase::Store => f.results.pending_store.store(true, Ordering::SeqCst),
        PendingPhase::Complete => f.results.pending_complete.store(true, Ordering::SeqCst),
        PendingPhase::Fail => f.state.lock().unwrap().pending_fail = true,
    }
    f
}

#[test]
fn cancellation_drops_every_pending_repository_phase_without_inventing_terminal_state() {
    for phase in [
        PendingPhase::Load,
        PendingPhase::Claim,
        PendingPhase::Page,
        PendingPhase::Store,
        PendingPhase::Complete,
        PendingPhase::Fail,
    ] {
        let f = pending_fixture(phase);
        let callback: fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> =
            if matches!(phase, PendingPhase::Fail) {
                broken_identity
            } else {
                catalog_identity
            };
        let mut future =
            Box::pin(
                f.runner
                    .run("job:one", 1, &f.transport, f.control.as_ref(), &callback),
            );
        assert!(poll(future.as_mut()).is_pending(), "phase {phase:?}");
        f.control.cancelled.store(true, Ordering::SeqCst);
        assert_eq!(
            ready(poll(future.as_mut())).unwrap_err().code,
            "catalog_resolution_cancelled",
            "phase {phase:?}"
        );
        let state = f.state.lock().unwrap();
        assert_eq!(
            state.status,
            if matches!(phase, PendingPhase::Load | PendingPhase::Claim) {
                PreviewJobStatus::Queued
            } else {
                PreviewJobStatus::Running
            },
            "phase {phase:?}"
        );
        assert!(
            state.pending_dropped.load(Ordering::SeqCst)
                || matches!(phase, PendingPhase::Store | PendingPhase::Complete)
        );
        drop(state);
        if matches!(phase, PendingPhase::Store | PendingPhase::Complete) {
            assert!(f.results.pending_dropped.load(Ordering::SeqCst));
        }
    }
}

#[test]
fn timeout_guards_pending_phases_and_fail_cleanup_timeout_is_static() {
    for phase in [
        PendingPhase::Load,
        PendingPhase::Claim,
        PendingPhase::Page,
        PendingPhase::Store,
        PendingPhase::Complete,
        PendingPhase::Fail,
    ] {
        let f = pending_fixture(phase);
        let callback: fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> =
            if matches!(phase, PendingPhase::Fail) {
                broken_identity
            } else {
                catalog_identity
            };
        let mut future =
            Box::pin(
                f.runner
                    .run("job:one", 1, &f.transport, f.control.as_ref(), &callback),
            );
        assert!(poll(future.as_mut()).is_pending(), "phase {phase:?}");
        f.control.now.store(
            if matches!(phase, PendingPhase::Fail) {
                11
            } else {
                121
            },
            Ordering::SeqCst,
        );
        let error = ready(poll(future.as_mut())).unwrap_err();
        assert_eq!(
            error.code,
            if matches!(phase, PendingPhase::Fail) {
                "preview_failure_cleanup_failed"
            } else {
                "catalog_resolution_timeout"
            },
            "phase {phase:?}"
        );
        let state = f.state.lock().unwrap();
        assert_eq!(
            state.status,
            match phase {
                PendingPhase::Load | PendingPhase::Claim => PreviewJobStatus::Queued,
                PendingPhase::Fail => PreviewJobStatus::Running,
                _ => PreviewJobStatus::Failed,
            },
            "phase {phase:?}"
        );
    }
}

#[test]
fn matched_record_uses_real_snapshot_comparison_and_partial_failure_is_retained() {
    let mut f = fixture(1);
    f.transport.matched = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    assert_eq!(f.results.compared.load(Ordering::SeqCst), 1);
    assert_eq!(f.secrets.0.load(Ordering::SeqCst), 1);
    assert_eq!(f.marks.0.load(Ordering::SeqCst), 1);

    let mut f = fixture(2);
    f.transport.matched = true;
    f.transport.fail_on_call = 2;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_unavailable"
    );
    assert_eq!(*f.results.stored.lock().unwrap(), vec![(0, 1)]);
    let state = f.state.lock().unwrap();
    assert_eq!(state.status, PreviewJobStatus::Failed);
    assert_eq!(state.fail_calls, 1);
}

#[test]
fn cancellation_and_dropped_work_leave_claimed_attempt_running() {
    let mut f = fixture(1);
    f.transport.cancel_on_fetch = true;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Running);
    assert_eq!(f.state.lock().unwrap().fail_calls, 0);

    let mut f = fixture(1);
    f.transport.hang = true;
    let dropped = f.transport.dropped.clone();
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert!(poll(future.as_mut()).is_pending());
    drop(future);
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Running);

    let mut f = fixture(1);
    f.transport.hang = true;
    let dropped = f.transport.dropped.clone();
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert!(poll(future.as_mut()).is_pending());
    f.control.cancelled.store(true, Ordering::SeqCst);
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_cancelled"
    );
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Running);

    let mut f = fixture(1);
    f.transport.hang = true;
    let dropped = f.transport.dropped.clone();
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert!(poll(future.as_mut()).is_pending());
    f.control.now.store(121, Ordering::SeqCst);
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_timeout"
    );
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Failed);
}

#[test]
fn a_second_run_loses_the_real_claim_while_the_owner_is_pending() {
    let mut f = fixture(1);
    f.transport.hang = true;
    let mut owner = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert!(poll(owner.as_mut()).is_pending());
    assert_eq!(f.state.lock().unwrap().status, PreviewJobStatus::Running);
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);

    let mut contender = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(contender.as_mut())).unwrap(),
        PreviewAttemptOutcome::NotClaimed
    );
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.state.lock().unwrap().fail_calls, 0);
    drop(owner);
}

#[test]
fn each_record_has_a_fresh_budget_and_retention_caps_deadlines() {
    let mut f = fixture(3);
    f.transport.advance = 70;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    assert_eq!(f.control.now.load(Ordering::SeqCst), 210);

    let f = fixture(0);
    f.state.lock().unwrap().expires_at = 130;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    assert!(
        f.control
            .deadlines
            .lock()
            .unwrap()
            .iter()
            .any(|deadline| *deadline == Duration::from_secs(30))
    );

    let f = fixture(0);
    f.state.lock().unwrap().advance_first_load = 50;
    let mut future = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    let deadlines = f.control.deadlines.lock().unwrap();
    assert_eq!(deadlines[0], Duration::from_secs(120));
    assert_eq!(deadlines[1], Duration::from_secs(120));
}

#[test]
fn explicit_new_attempt_reads_every_original_record_again() {
    let f = fixture(2);
    let mut first = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(first.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    {
        let mut state = f.state.lock().unwrap();
        state.status = PreviewJobStatus::Queued;
        state.attempt = 2;
    }
    f.results.stored.lock().unwrap().clear();
    let mut second = Box::pin(f.runner.run(
        "job:one",
        2,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(second.as_mut())).unwrap(),
        PreviewAttemptOutcome::Completed
    );
    assert_eq!(*f.results.stored.lock().unwrap(), vec![(0, 2), (1, 2)]);
    assert_eq!(f.transport.calls.load(Ordering::SeqCst), 4);
}

#[test]
fn invalid_metadata_and_expiry_do_not_claim() {
    let f = fixture(1);
    let mut invalid = Box::pin(f.runner.run(
        "bad id",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(invalid.as_mut())).unwrap_err().code,
        "preview_job_invalid"
    );

    f.clock.0.store(1_000, Ordering::SeqCst);
    let mut expired = Box::pin(f.runner.run(
        "job:one",
        1,
        &f.transport,
        f.control.as_ref(),
        &catalog_identity,
    ));
    assert_eq!(
        ready(poll(expired.as_mut())).unwrap(),
        PreviewAttemptOutcome::NotClaimed
    );
    assert_eq!(f.state.lock().unwrap().claim_calls, 0);
}
