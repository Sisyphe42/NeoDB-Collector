use super::*;
use collector_core::{
    access::{
        AccessRecord, AccessRepository, AccessService, ChallengeMatch, Clock, Endpoint, PortFuture,
        SecretStore,
    },
    catalog::{
        CatalogFetch, CatalogItem, CatalogSearchPage, NeoDbCatalogSearchTransport,
        NeoDbCatalogTransport,
    },
    catalog_resolver::{CatalogIdentity, ResolutionControl},
    mapping::{MappingRepository, imdb_default},
    marks::{MarkRead, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    preview::PreviewSnapshotService,
    preview_jobs::{
        PreviewJob, PreviewJobIdentity, PreviewJobLifecycle, PreviewJobRepository,
        PreviewJobStatus, PreviewResolutionInput, PreviewSubmission,
    },
    preview_records::{PreviewRecordRequest, PreviewRecordService, PreviewResolutionMode},
    preview_results::{
        PreviewResultRepository, PreviewResultWrite, StoredPreviewMetadata, StoredPreviewOutcome,
        StoredPreviewResult,
    },
    secrets::SecretString,
};
use collector_domain::{
    ApiError,
    access::{PairedClient, PairingExchange, PairingRequest},
    observation::{Observation, ObservedItem, ScalarValue},
};
use collector_storage::{Storage, credentials::SecureTokens};
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone, Copy)]
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

struct AtomicClock(AtomicI64);
impl Clock for AtomicClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct PanicSecrets;
impl SecretStore for PanicSecrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async { panic!("unsupported production must not write credentials") })
    }
    fn get(&self, _: String) -> PortFuture<'_, SecretString> {
        Box::pin(async { panic!("unsupported production must not read credentials") })
    }
    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { panic!("unsupported production must not delete credentials") })
    }
}

struct PanicIdentity;
impl NeoDbOAuthTransport for PanicIdentity {
    fn canonical_base(&self, _: String) -> PortFuture<'_, String> {
        Box::pin(async { panic!("unsupported production must not inspect identity") })
    }
    fn register(&self, _: String, _: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async { panic!("unsupported production must not register OAuth") })
    }
    fn authorization_url(
        &self,
        _: String,
        _: String,
        _: String,
        _: String,
    ) -> PortFuture<'_, String> {
        Box::pin(async { panic!("unsupported production must not authorize OAuth") })
    }
    fn exchange(
        &self,
        _: String,
        _: OAuthApp,
        _: SecretString,
        _: String,
    ) -> PortFuture<'_, OAuthToken> {
        Box::pin(async { panic!("unsupported production must not exchange OAuth") })
    }
    fn identity<'a>(&'a self, _: String, _: &'a SecretString) -> PortFuture<'a, OAuthIdentity> {
        Box::pin(async { panic!("unsupported production must not inspect account identity") })
    }
}

struct PanicMarks;
impl NeoDbMarkTransport for PanicMarks {
    fn read_mark<'a>(
        &'a self,
        _: String,
        _: &'a SecretString,
        _: String,
        _: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async { panic!("unsupported production must not read personal marks") })
    }
}

struct UnsupportedTransport;
impl NeoDbCatalogTransport for UnsupportedTransport {
    fn fetch(&self, _: String, _: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async { Ok(CatalogFetch::Unsupported) })
    }
    fn detail(&self, _: String, _: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async { panic!("unsupported production must not fetch item detail") })
    }
}
impl NeoDbCatalogSearchTransport for UnsupportedTransport {
    fn search(&self, _: String, _: String, _: String) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async { panic!("native unsupported production must not search") })
    }
}

struct ImmediateControl;
impl ResolutionControl for ImmediateControl {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
    fn wait_until(&self, _: Duration) -> PortFuture<'_, ()> {
        Box::pin(async { panic!("unsupported production must not poll") })
    }
    fn run<'a, T: Send + 'a>(&'a self, _: Duration, work: PortFuture<'a, T>) -> PortFuture<'a, T> {
        work
    }
}

struct ExpiringRepository {
    storage: Storage,
    clock: Arc<AtomicClock>,
}

impl AccessRepository for ExpiringRepository {
    fn insert(&self, value: AccessRecord) -> PortFuture<'_, ()> {
        AccessRepository::insert(&self.storage, value)
    }
    fn exchange(&self, challenge: ChallengeMatch, replacement: AccessRecord) -> PortFuture<'_, ()> {
        AccessRepository::exchange(&self.storage, challenge, replacement)
    }
    fn find(
        &self,
        hash: String,
        kind: String,
        audience: String,
        now: i64,
    ) -> PortFuture<'_, Option<AccessRecord>> {
        AccessRepository::find(&self.storage, hash, kind, audience, now)
    }
    fn revoke(&self, id: String, kind: String) -> PortFuture<'_, ()> {
        AccessRepository::revoke(&self.storage, id, kind)
    }
    fn clients(&self) -> PortFuture<'_, Vec<PairedClient>> {
        AccessRepository::clients(&self.storage)
    }
    fn endpoint(&self) -> PortFuture<'_, Option<Endpoint>> {
        Box::pin(async move {
            self.clock.0.store(200, Ordering::SeqCst);
            AccessRepository::endpoint(&self.storage).await
        })
    }
    fn set_endpoint(&self, endpoint: Option<Endpoint>) -> PortFuture<'_, ()> {
        AccessRepository::set_endpoint(&self.storage, endpoint)
    }
}

#[derive(Clone, Copy)]
struct JobSpec {
    status: PreviewJobStatus,
    attempt: u32,
    expires_at: i64,
}

struct Jobs {
    sequence: Mutex<VecDeque<Option<JobSpec>>>,
    fallback: Option<JobSpec>,
    behavior: LoadBehavior,
}

#[derive(Clone, Copy)]
enum LoadBehavior {
    Ready,
    Failed,
    Pending,
}

impl Jobs {
    fn one(spec: Option<JobSpec>) -> Self {
        Self {
            sequence: Mutex::new(VecDeque::new()),
            fallback: spec,
            behavior: LoadBehavior::Ready,
        }
    }

    fn sequenced(sequence: Vec<Option<JobSpec>>, fallback: Option<JobSpec>) -> Self {
        Self {
            sequence: Mutex::new(sequence.into()),
            fallback,
            behavior: LoadBehavior::Ready,
        }
    }

    fn failed() -> Self {
        Self {
            sequence: Mutex::new(VecDeque::new()),
            fallback: None,
            behavior: LoadBehavior::Failed,
        }
    }

    fn pending() -> Self {
        Self {
            sequence: Mutex::new(VecDeque::new()),
            fallback: None,
            behavior: LoadBehavior::Pending,
        }
    }

    fn build(spec: JobSpec) -> PreviewJob {
        PreviewJob::from_stored(
            PreviewJobIdentity {
                id: "job:test".into(),
                profile_id: "imdb_ratings".into(),
                profile_version: 1,
                observation_count: 100,
            },
            PreviewJobLifecycle {
                status: spec.status,
                attempt: spec.attempt,
                created_at: 1,
                updated_at: 4,
                expires_at: spec.expires_at,
            },
            endpoint("credential-a"),
        )
        .unwrap()
    }
}

impl PreviewJobRepository for Jobs {
    fn submit(&self, _: PreviewSubmission) -> PortFuture<'_, PreviewJob> {
        Box::pin(async { unreachable!() })
    }

    fn load(&self, _: String) -> PortFuture<'_, Option<PreviewJob>> {
        match self.behavior {
            LoadBehavior::Failed => {
                return Box::pin(async { Err(ApiError::new("internal", "secret detail")) });
            }
            LoadBehavior::Pending => return Box::pin(std::future::pending()),
            LoadBehavior::Ready => {}
        }
        let value = self
            .sequence
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(self.fallback);
        Box::pin(async move { Ok(value.map(Self::build)) })
    }

    fn observations(
        &self,
        _: String,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<collector_domain::observation::Observation>> {
        Box::pin(async { unreachable!() })
    }

    fn resolution_input(&self, _: String, _: u32) -> PortFuture<'_, PreviewResolutionInput> {
        Box::pin(async { unreachable!() })
    }

    fn claim(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn cancel(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
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
        Box::pin(async { unreachable!() })
    }
    fn cleanup_expired(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }
}

struct TimedJobs {
    delay: Duration,
    signal: Arc<Notify>,
}
impl PreviewJobRepository for TimedJobs {
    fn submit(&self, _: PreviewSubmission) -> PortFuture<'_, PreviewJob> {
        Box::pin(async { unreachable!() })
    }
    fn load(&self, _: String) -> PortFuture<'_, Option<PreviewJob>> {
        Box::pin(async move {
            self.signal.notify_one();
            tokio::time::sleep(self.delay).await;
            Ok(Some(Jobs::build(live(PreviewJobStatus::Running))))
        })
    }
    fn observations(&self, _: String, _: u32, _: u32) -> PortFuture<'_, Vec<Observation>> {
        Box::pin(async { unreachable!() })
    }
    fn resolution_input(&self, _: String, _: u32) -> PortFuture<'_, PreviewResolutionInput> {
        Box::pin(async { unreachable!() })
    }
    fn claim(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn cancel(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
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
        Box::pin(async { unreachable!() })
    }
    fn cleanup_expired(&self, _: i64) -> PortFuture<'_, u64> {
        Box::pin(async { unreachable!() })
    }
}

struct Results {
    ordinals: Vec<u32>,
}

struct CorruptResults;
impl PreviewResultRepository for CorruptResults {
    fn store(&self, _: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn results(
        &self,
        _: String,
        _: u32,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>> {
        Box::pin(async {
            Err(ApiError::new(
                "database_unavailable",
                "raw corrupt result_json detail",
            ))
        })
    }
    fn complete(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
}

struct BadMetadataResults;
impl PreviewResultRepository for BadMetadataResults {
    fn store(&self, _: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn results(
        &self,
        _: String,
        attempt: u32,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>> {
        Box::pin(async move {
            Ok(vec![
                StoredPreviewResult::from_stored(
                    StoredPreviewMetadata {
                        ordinal: 0,
                        attempt,
                        committed_at: 3,
                        source_connector_id: "imdb:ratings_csv".into(),
                        source_record_id: "tt1234567".into(),
                        source_fingerprint: "fingerprint".into(),
                        profile_id: "wrong_profile".into(),
                        profile_version: 1,
                        base_url: "https://neodb.example".into(),
                        account: "alice".into(),
                    },
                    StoredPreviewOutcome::Unsupported,
                )
                .unwrap(),
            ])
        })
    }
    fn complete(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
}

impl PreviewResultRepository for Results {
    fn store(&self, _: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }

    fn results(
        &self,
        _: String,
        attempt: u32,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>> {
        let rows = self
            .ordinals
            .iter()
            .map(|ordinal| {
                StoredPreviewResult::from_stored(
                    StoredPreviewMetadata {
                        ordinal: *ordinal,
                        attempt,
                        committed_at: 3,
                        source_connector_id: "imdb:ratings_csv".into(),
                        source_record_id: "tt1234567".into(),
                        source_fingerprint: "fingerprint".into(),
                        profile_id: "imdb_ratings".into(),
                        profile_version: 1,
                        base_url: "https://neodb.example".into(),
                        account: "alice".into(),
                    },
                    StoredPreviewOutcome::Unsupported,
                )
                .unwrap()
            })
            .collect();
        Box::pin(async move { Ok(rows) })
    }

    fn complete(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
}

struct TimedResults {
    delay: Option<Duration>,
    signal: Arc<Notify>,
}
impl PreviewResultRepository for TimedResults {
    fn store(&self, _: PreviewResultWrite) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
    fn results(
        &self,
        _: String,
        _: u32,
        _: u32,
        _: u32,
    ) -> PortFuture<'_, Vec<StoredPreviewResult>> {
        Box::pin(async move {
            self.signal.notify_one();
            match self.delay {
                Some(delay) => {
                    tokio::time::sleep(delay).await;
                    Ok(vec![])
                }
                None => std::future::pending().await,
            }
        })
    }
    fn complete(&self, _: String, _: u32, _: i64) -> PortFuture<'_, bool> {
        Box::pin(async { unreachable!() })
    }
}

fn endpoint(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn observation() -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        observed_at: "2026-10-01T00:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{:0>64}", "1234567"),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::from([("imdb:title".into(), "tt1234567".into())]),
            title_hint: Some("Exact title".into()),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(8.0))]),
    }
}

async fn setup(
    jobs: Arc<dyn PreviewJobRepository>,
    results: Arc<dyn PreviewResultRepository>,
) -> (tempfile::TempDir, Storage, Arc<AccessService>, Router) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    storage
        .set_endpoint(Some(endpoint("credential-a")))
        .await
        .unwrap();
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(100)),
        audience: "http://127.0.0.1:19423".into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_jobs = Some(jobs);
    state.preview_results = Some(results);
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    (dir, storage, service, app)
}

fn live(status: PreviewJobStatus) -> JobSpec {
    JobSpec {
        status,
        attempt: 1,
        expires_at: 200,
    }
}

#[tokio::test]
async fn session_inspects_all_statuses_and_sparse_original_ordinals() {
    for (status, expected) in [
        (PreviewJobStatus::Queued, "queued"),
        (PreviewJobStatus::Running, "running"),
        (PreviewJobStatus::Interrupted, "interrupted"),
        (PreviewJobStatus::Failed, "failed"),
        (PreviewJobStatus::Cancelled, "cancelled"),
        (PreviewJobStatus::Completed, "completed"),
    ] {
        let (_dir, storage, service, app) = setup(
            Arc::new(Jobs::one(Some(live(status)))),
            Arc::new(Results { ordinals: vec![] }),
        )
        .await;
        let (cookie, _) = access_tests::login(&app, &service).await;
        let response = access_tests::request(
            &app,
            "GET",
            "/api/v1/previews/job:test",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await;
        assert_eq!(response.0, StatusCode::OK);
        assert_eq!(response.2["status"], expected);
        assert_eq!(response.2["bindingState"], "current");
        assert_eq!(response.2["historical"], true);
        assert_eq!(response.2["executable"], false);
        assert!(response.2.get("credentialRef").is_none());
        storage.close().await;
    }

    let jobs = Arc::new(Jobs::one(Some(live(PreviewJobStatus::Running))));
    let (_dir, storage, service, app) = setup(
        jobs,
        Arc::new(Results {
            ordinals: vec![1, 99],
        }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    let response = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test/results?attempt=1&offset=1&limit=2",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.2["items"][0]["ordinal"], 1);
    assert_eq!(response.2["items"][1]["ordinal"], 99);
    assert_eq!(response.2["items"][0]["outcome"]["kind"], "unsupported");
    assert!(response.2.get("total").is_none());
    storage.close().await;
}

#[tokio::test]
async fn real_producer_persisted_result_survives_endpoint_credential_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    storage
        .set_endpoint(Some(endpoint("credential-a")))
        .await
        .unwrap();
    let profile = imdb_default();
    let observed = observation();
    MappingRepository::save(&storage, profile.clone(), 0)
        .await
        .unwrap();
    PreviewJobRepository::submit(
        &storage,
        PreviewSubmission::new(
            "job:real".into(),
            endpoint("credential-a"),
            profile.clone(),
            vec![observed.clone()],
            100,
            200,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    assert!(
        PreviewJobRepository::claim(&storage, "job:real".into(), 1, 105)
            .await
            .unwrap()
    );
    let snapshots = Arc::new(PreviewSnapshotService::new(
        Arc::new(storage.clone()),
        Arc::new(PanicSecrets),
        Arc::new(FixedClock(110)),
        Arc::new(PanicIdentity),
        Arc::new(PanicMarks),
    ));
    let producer = PreviewRecordService::new(snapshots);
    let expected = endpoint("credential-a");
    let identity = CatalogIdentity {
        source_url: observed.item.canonical_url.clone(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        scheme: "imdb:title".into(),
        external_id: observed.source_record_id.clone(),
    };
    let record = producer
        .preview(
            &UnsupportedTransport,
            &ImmediateControl,
            PreviewRecordRequest {
                expected: &expected,
                profile: &profile,
                observation: &observed,
                identity: &identity,
                mode: PreviewResolutionMode::Native,
            },
        )
        .await
        .unwrap();
    assert!(
        PreviewResultRepository::store(
            &storage,
            PreviewResultWrite::new("job:real".into(), 0, 1, 110, record).unwrap(),
        )
        .await
        .unwrap()
    );
    assert!(
        PreviewResultRepository::complete(&storage, "job:real".into(), 1, 111)
            .await
            .unwrap()
    );
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(110)),
        audience: "http://127.0.0.1:19423".into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_jobs = Some(Arc::new(storage.clone()));
    state.preview_results = Some(Arc::new(storage.clone()));
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    let (cookie, _) = access_tests::login(&app, &service).await;
    let job = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:real",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(job.0, StatusCode::OK);
    assert_eq!(job.2["observationCount"], 1);
    assert_eq!(job.2["status"], "completed");
    storage
        .set_endpoint(Some(endpoint("credential-b")))
        .await
        .unwrap();
    let page = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:real/results?attempt=1",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(page.0, StatusCode::OK);
    assert_eq!(page.2["items"][0]["outcome"]["kind"], "unsupported");
    assert_eq!(page.2["items"][0]["historical"], true);
    assert_eq!(page.2["items"][0]["executable"], false);
    storage.close().await;
}

#[tokio::test]
async fn expiry_during_endpoint_inspection_is_not_published() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    storage
        .set_endpoint(Some(endpoint("credential-a")))
        .await
        .unwrap();
    let clock = Arc::new(AtomicClock(AtomicI64::new(100)));
    let repository = Arc::new(ExpiringRepository {
        storage: storage.clone(),
        clock: clock.clone(),
    });
    let service = Arc::new(AccessService {
        repository,
        tokens: Arc::new(SecureTokens),
        clock,
        audience: "http://127.0.0.1:19423".into(),
    });
    let mut state = access::AccessState::new(service.clone());
    state.preview_jobs = Some(Arc::new(Jobs::one(Some(live(PreviewJobStatus::Queued)))));
    state.preview_results = Some(Arc::new(Results { ordinals: vec![] }));
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    let (cookie, _) = access_tests::login(&app, &service).await;
    let response = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(response.0, StatusCode::NOT_FOUND);
    assert_eq!(response.2["code"], "preview_job_not_found");
    storage.close().await;
}

#[tokio::test]
async fn repository_failure_is_static_and_redacted() {
    let (_dir, storage, service, app) = setup(
        Arc::new(Jobs::failed()),
        Arc::new(Results { ordinals: vec![] }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    let response = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    assert!(!response.2.to_string().contains("secret detail"));
    storage.close().await;
}

#[tokio::test]
async fn absent_optional_read_ports_are_static_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let service = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(FixedClock(100)),
        audience: "http://127.0.0.1:19423".into(),
    });
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(access::AccessState::new(service.clone())),
    );
    let (cookie, _) = access_tests::login(&app, &service).await;
    for path in [
        "/api/v1/previews/job:test",
        "/api/v1/previews/job:test/results?attempt=1",
    ] {
        let response =
            access_tests::request(&app, "GET", path, json!({}), &[("cookie", &cookie)]).await;
        assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.2["code"], "preview_unavailable");
    }
    storage.close().await;
}

#[tokio::test]
async fn corrupt_result_port_is_static_without_partial_page() {
    let (_dir, storage, service, app) = setup(
        Arc::new(Jobs::one(Some(live(PreviewJobStatus::Running)))),
        Arc::new(CorruptResults),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    let response = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test/results?attempt=1",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    assert!(response.2.get("items").is_none());
    assert!(!response.2.to_string().contains("result_json"));
    storage.close().await;
}

#[tokio::test]
async fn invalid_returned_rows_never_publish_a_partial_page() {
    let ports: Vec<Arc<dyn PreviewResultRepository>> = vec![
        Arc::new(Results {
            ordinals: vec![2, 1],
        }),
        Arc::new(Results {
            ordinals: vec![1, 1],
        }),
        Arc::new(Results {
            ordinals: vec![100],
        }),
        Arc::new(BadMetadataResults),
    ];
    for results in ports {
        let (_dir, storage, service, app) = setup(
            Arc::new(Jobs::one(Some(live(PreviewJobStatus::Running)))),
            results,
        )
        .await;
        let (cookie, _) = access_tests::login(&app, &service).await;
        let response = access_tests::request(
            &app,
            "GET",
            "/api/v1/previews/job:test/results?attempt=1",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await;
        assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.2["code"], "preview_unavailable");
        assert!(response.2.get("items").is_none());
        storage.close().await;
    }
}

#[tokio::test]
async fn hung_repository_fails_at_the_strict_ten_second_deadline() {
    let (_dir, storage, service, app) = setup(
        Arc::new(Jobs::pending()),
        Arc::new(Results { ordinals: vec![] }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    let started = tokio::time::Instant::now();
    let response = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert!(started.elapsed() >= Duration::from_secs(10));
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    storage.close().await;
}

#[tokio::test]
async fn one_shared_budget_spans_job_and_result_reads() {
    let jobs_started = Arc::new(Notify::new());
    let results_started = Arc::new(Notify::new());
    let (_dir, storage, service, app) = setup(
        Arc::new(TimedJobs {
            delay: Duration::from_secs(9),
            signal: jobs_started.clone(),
        }),
        Arc::new(TimedResults {
            delay: Some(Duration::from_secs(2)),
            signal: results_started.clone(),
        }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    tokio::time::pause();
    let task = tokio::spawn(async move {
        access_tests::request(
            &app,
            "GET",
            "/api/v1/previews/job:test/results?attempt=1",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await
    });
    jobs_started.notified().await;
    tokio::time::advance(Duration::from_secs(9)).await;
    results_started.notified().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    let response = task.await.unwrap();
    tokio::time::resume();
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    storage.close().await;
}

#[tokio::test]
async fn exact_deadline_ready_job_reply_is_rejected() {
    let started = Arc::new(Notify::new());
    let (_dir, storage, service, app) = setup(
        Arc::new(TimedJobs {
            delay: Duration::from_secs(10),
            signal: started.clone(),
        }),
        Arc::new(Results { ordinals: vec![] }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    tokio::time::pause();
    let task = tokio::spawn(async move {
        access_tests::request(
            &app,
            "GET",
            "/api/v1/previews/job:test",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await
    });
    started.notified().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    let response = task.await.unwrap();
    tokio::time::resume();
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    storage.close().await;
}

#[tokio::test]
async fn hung_result_phase_is_bounded_and_static() {
    let started = Arc::new(Notify::new());
    let (_dir, storage, service, app) = setup(
        Arc::new(Jobs::one(Some(live(PreviewJobStatus::Running)))),
        Arc::new(TimedResults {
            delay: None,
            signal: started.clone(),
        }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    tokio::time::pause();
    let task = tokio::spawn(async move {
        access_tests::request(
            &app,
            "GET",
            "/api/v1/previews/job:test/results?attempt=1",
            json!({}),
            &[("cookie", &cookie)],
        )
        .await
    });
    started.notified().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    let response = task.await.unwrap();
    tokio::time::resume();
    assert_eq!(response.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.2["code"], "preview_unavailable");
    storage.close().await;
}

#[tokio::test]
async fn authentication_query_binding_expiry_and_read_races_fail_closed() {
    let (_dir, storage, service, app) = setup(
        Arc::new(Jobs::one(Some(live(PreviewJobStatus::Queued)))),
        Arc::new(Results { ordinals: vec![] }),
    )
    .await;
    let (cookie, _) = access_tests::login(&app, &service).await;
    for path in [
        "/api/v1/previews/job:test",
        "/api/v1/previews/job:test/results?attempt=1",
    ] {
        let missing = access_tests::request(&app, "GET", path, json!({}), &[]).await;
        assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    }
    let link = service
        .create_pairing(PairingRequest {
            client_name: "Preview test".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let one_time_key = link.uri.split("#key=").nth(1).unwrap().to_owned();
    let paired = service
        .pair(PairingExchange {
            one_time_key,
            client_name: "Preview test".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let authorization = format!("Bearer {}", paired.token);
    for path in [
        "/api/v1/previews/job:test",
        "/api/v1/previews/job:test/results?attempt=1",
    ] {
        let response = access_tests::request(
            &app,
            "GET",
            path,
            json!({}),
            &[("authorization", &authorization)],
        )
        .await;
        assert_eq!(response.0, StatusCode::OK, "{path}");
        let duplicate = access_tests::request(
            &app,
            "GET",
            path,
            json!({}),
            &[
                ("authorization", &authorization),
                ("authorization", &authorization),
            ],
        )
        .await;
        assert_eq!(duplicate.0, StatusCode::UNAUTHORIZED);
    }
    AccessRepository::revoke(&storage, paired.client_id, "client".into())
        .await
        .unwrap();
    for path in [
        "/api/v1/previews/job:test",
        "/api/v1/previews/job:test/results?attempt=1",
    ] {
        let revoked = access_tests::request(
            &app,
            "GET",
            path,
            json!({}),
            &[("authorization", &authorization), ("cookie", &cookie)],
        )
        .await;
        assert_eq!(revoked.0, StatusCode::UNAUTHORIZED);
    }
    for path in [
        "/api/v1/previews/job:test/results",
        "/api/v1/previews/job:test/results?attempt=0",
        "/api/v1/previews/job:test/results?attempt=1&limit=0",
        "/api/v1/previews/job:test/results?attempt=1&limit=101",
        "/api/v1/previews/job:test/results?attempt=1&attempt=1",
        "/api/v1/previews/job:test/results?attempt=1&unknown=1",
        "/api/v1/previews/job:test/results?attempt=-1",
        "/api/v1/previews/job:test/results?attempt=1.5",
        "/api/v1/previews/job:test/results?attempt=4294967296",
        "/api/v1/previews/job:test/results?attempt=1&offset=-1",
        "/api/v1/previews/job:test/results?attempt=1&offset=1.5",
        "/api/v1/previews/job:test/results?attempt=1&offset=4294967296",
        "/api/v1/previews/job:test/results?attempt=1&limit=-1",
        "/api/v1/previews/job:test/results?attempt=1&limit=1.5",
        "/api/v1/previews/job:test/results?attempt=1&limit=4294967296",
    ] {
        let response =
            access_tests::request(&app, "GET", path, json!({}), &[("cookie", &cookie)]).await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(response.2["code"], "preview_request_invalid");
    }
    let denied = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie), ("authorization", "Bearer malformed")],
    )
    .await;
    assert_eq!(denied.0, StatusCode::UNAUTHORIZED);
    storage
        .set_endpoint(Some(endpoint("credential-b")))
        .await
        .unwrap();
    let changed = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(changed.2["bindingState"], "changed");
    storage.set_endpoint(None).await.unwrap();
    let disconnected = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:test",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(disconnected.2["bindingState"], "disconnected");
    let mismatched = access_tests::request(
        &app,
        "GET",
        "/api/v1/previews/job:other",
        json!({}),
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(mismatched.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(mismatched.2["code"], "preview_unavailable");
    storage.close().await;

    for (jobs, path, code, status) in [
        (
            Jobs::one(None),
            "/api/v1/previews/job:test",
            "preview_job_not_found",
            StatusCode::NOT_FOUND,
        ),
        (
            Jobs::one(Some(JobSpec {
                expires_at: 100,
                ..live(PreviewJobStatus::Queued)
            })),
            "/api/v1/previews/job:test",
            "preview_job_not_found",
            StatusCode::NOT_FOUND,
        ),
        (
            Jobs::sequenced(
                vec![
                    Some(live(PreviewJobStatus::Running)),
                    Some(JobSpec {
                        attempt: 2,
                        ..live(PreviewJobStatus::Queued)
                    }),
                ],
                None,
            ),
            "/api/v1/previews/job:test/results?attempt=1",
            "preview_attempt_changed",
            StatusCode::CONFLICT,
        ),
        (
            Jobs::sequenced(vec![Some(live(PreviewJobStatus::Running)), None], None),
            "/api/v1/previews/job:test/results?attempt=1",
            "preview_job_not_found",
            StatusCode::NOT_FOUND,
        ),
        (
            Jobs::sequenced(
                vec![
                    Some(live(PreviewJobStatus::Running)),
                    Some(JobSpec {
                        expires_at: 100,
                        ..live(PreviewJobStatus::Running)
                    }),
                ],
                None,
            ),
            "/api/v1/previews/job:test/results?attempt=1",
            "preview_job_not_found",
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (_dir, storage, service, app) =
            setup(Arc::new(jobs), Arc::new(Results { ordinals: vec![] })).await;
        let (cookie, _) = access_tests::login(&app, &service).await;
        let response =
            access_tests::request(&app, "GET", path, json!({}), &[("cookie", &cookie)]).await;
        assert_eq!(response.0, status);
        assert_eq!(response.2["code"], code);
        storage.close().await;
    }
}
