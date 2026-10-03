use super::*;
use crate::{
    access::{AccessRecord, AccessRepository, ChallengeMatch, Clock, PortFuture, SecretStore},
    catalog::{CatalogFetch, CatalogSearchCandidate},
    marks::{MarkRead, MarkSnapshot, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    preview::{ComparisonStatus, SinkValue},
    secrets::{ExposeSecret, SecretString},
};
use collector_domain::{
    access::PairedClient,
    observation::{Observation, ObservedItem, ScalarValue},
};
use std::{
    collections::BTreeMap,
    future::{Future, pending, poll_fn},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};

#[derive(Default)]
struct Control {
    now: AtomicU64,
    cancelled: AtomicBool,
}

impl ResolutionControl for Control {
    fn now(&self) -> Duration {
        Duration::from_secs(self.now.load(Ordering::SeqCst))
    }

    fn wait_until(&self, at: Duration) -> PortFuture<'_, ()> {
        Box::pin(poll_fn(move |_| {
            if self.cancelled.load(Ordering::SeqCst) {
                Poll::Ready(Err(cancelled()))
            } else if self.now() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }

    fn run<'a, T: Send + 'a>(
        &'a self,
        deadline: Duration,
        mut work: PortFuture<'a, T>,
    ) -> PortFuture<'a, T> {
        Box::pin(poll_fn(move |cx| {
            let check = || {
                if self.cancelled.load(Ordering::SeqCst) {
                    Err(cancelled())
                } else if self.now() >= deadline {
                    Err(resolution_timeout())
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

struct DropFlag<'a>(&'a AtomicBool);
impl Drop for DropFlag<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct Repo {
    current: Mutex<Option<Endpoint>>,
    calls: AtomicUsize,
    hang_on: AtomicUsize,
    change_on: AtomicUsize,
    disconnect_on: AtomicUsize,
    fail_on: AtomicUsize,
    dropped: AtomicBool,
}

impl AccessRepository for Repo {
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
            if self.hang_on.load(Ordering::SeqCst) == call {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            if self.fail_on.load(Ordering::SeqCst) == call {
                return Err(ApiError::new("database_unavailable", "raw path"));
            }
            if self.disconnect_on.load(Ordering::SeqCst) == call {
                return Ok(None);
            }
            if self.change_on.load(Ordering::SeqCst) == call {
                return Ok(Some(endpoint("credential-b")));
            }
            Ok(self.current.lock().unwrap().clone())
        })
    }
    fn set_endpoint(&self, _: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

#[derive(Default)]
struct Secrets {
    calls: AtomicUsize,
    fail: AtomicBool,
    hang: AtomicBool,
    dropped: AtomicBool,
}

impl SecretStore for Secrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn get(&self, reference: String) -> PortFuture<'_, SecretString> {
        Box::pin(async move {
            assert_eq!(reference, "credential-a");
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(ApiError::new("secret_raw", "must be sanitized"));
            }
            if self.hang.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            Ok("access-token".into())
        })
    }
    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

enum MarkReply {
    Present(Option<u8>),
    Missing,
    Error,
}

struct Marks {
    reply: Mutex<Option<MarkReply>>,
    calls: AtomicUsize,
    hang: AtomicBool,
    dropped: AtomicBool,
}

impl NeoDbMarkTransport for Marks {
    fn read_mark<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
        account: String,
        expected_item: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async move {
            assert_eq!(base, "https://neodb.example");
            assert_eq!(token.expose_secret(), "access-token");
            assert_eq!(account, "alice");
            assert_eq!(expected_item.uuid, "Resolved123");
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.hang.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            match self.reply.lock().unwrap().take().unwrap() {
                MarkReply::Present(rating) => Ok(MarkRead::Present(Box::new(MarkSnapshot {
                    item: item(),
                    owner: "alice".into(),
                    shelf_type: "complete".into(),
                    rating_grade: rating,
                }))),
                MarkReply::Missing => Ok(MarkRead::MissingShelf),
                MarkReply::Error => {
                    Err(ApiError::new("neodb_mark_unavailable", "Mark unavailable."))
                }
            }
        })
    }
}

struct Identity {
    calls: AtomicUsize,
    hang: AtomicBool,
    fail: AtomicBool,
    dropped: AtomicBool,
}

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
    fn identity<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
    ) -> PortFuture<'a, OAuthIdentity> {
        Box::pin(async move {
            assert_eq!(base, "https://neodb.example");
            assert_eq!(token.expose_secret(), "access-token");
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.hang.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            if self.fail.load(Ordering::SeqCst) {
                return Err(ApiError::new("neodb_oauth_unavailable", "Unavailable."));
            }
            Ok(OAuthIdentity {
                username: "alice".into(),
            })
        })
    }
}

struct WallClock;
impl Clock for WallClock {
    fn now(&self) -> i64 {
        1_234
    }
}

struct Transport {
    fetch: Mutex<Option<Result<CatalogFetch, ApiError>>>,
    item: Mutex<Option<Result<CatalogItem, ApiError>>>,
    page: Mutex<Option<Result<CatalogSearchPage, ApiError>>>,
    fetch_calls: AtomicUsize,
    detail_calls: AtomicUsize,
    search_calls: AtomicUsize,
    hang_fetch: AtomicBool,
    hang_detail: AtomicBool,
    hang_search: AtomicBool,
    dropped: AtomicBool,
}

impl NeoDbCatalogTransport for Transport {
    fn fetch(&self, base: String, source_url: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            assert_eq!(base, "https://neodb.example");
            assert_eq!(source_url, "https://www.imdb.com/title/tt1234567/");
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            if self.hang_fetch.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            self.fetch.lock().unwrap().take().unwrap()
        })
    }

    fn detail(&self, base: String, reference: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move {
            assert_eq!(base, "https://neodb.example");
            assert_eq!(reference, "/api/tv/Resolved123");
            self.detail_calls.fetch_add(1, Ordering::SeqCst);
            if self.hang_detail.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            self.item.lock().unwrap().take().unwrap()
        })
    }
}

impl NeoDbCatalogSearchTransport for Transport {
    fn search(
        &self,
        base: String,
        query: String,
        category: String,
    ) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async move {
            assert_eq!(base, "https://neodb.example");
            assert_eq!(query, "Exact title");
            assert_eq!(category, "tv");
            self.search_calls.fetch_add(1, Ordering::SeqCst);
            if self.hang_search.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            self.page.lock().unwrap().take().unwrap()
        })
    }
}

struct Fixture {
    service: PreviewRecordService,
    repo: Arc<Repo>,
    secrets: Arc<Secrets>,
    marks: Arc<Marks>,
    identity: Arc<Identity>,
    control: Arc<Control>,
    transport: Transport,
}

fn endpoint(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn item() -> CatalogItem {
    CatalogItem {
        uuid: "Resolved123".into(),
        canonical_url: "https://neodb.example/tv/Resolved123".into(),
        detail_url: "https://neodb.example/api/tv/Resolved123".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        parent_uuid: Some("Parent123".into()),
        external_ids: vec![
            ("imdb:title".into(), "tt1234567".into()),
            ("tmdb:tv".into(), "456".into()),
        ],
    }
}

fn observation(rating: f64) -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "tt1234567".into(),
        observed_at: "2026-10-01T00:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{}", "a".repeat(64)),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::from([("imdb:title".into(), "tt1234567".into())]),
            title_hint: Some("Exact title".into()),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(rating))]),
    }
}

fn catalog_identity() -> CatalogIdentity {
    CatalogIdentity {
        source_url: "https://www.imdb.com/title/tt1234567/".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        scheme: "imdb:title".into(),
        external_id: "tt1234567".into(),
    }
}

fn fixture(mark: MarkReply) -> Fixture {
    let repo = Arc::new(Repo {
        current: Mutex::new(Some(endpoint("credential-a"))),
        calls: AtomicUsize::new(0),
        hang_on: AtomicUsize::new(0),
        change_on: AtomicUsize::new(0),
        disconnect_on: AtomicUsize::new(0),
        fail_on: AtomicUsize::new(0),
        dropped: AtomicBool::new(false),
    });
    let secrets = Arc::new(Secrets::default());
    let marks = Arc::new(Marks {
        reply: Mutex::new(Some(mark)),
        calls: AtomicUsize::new(0),
        hang: AtomicBool::new(false),
        dropped: AtomicBool::new(false),
    });
    let identity = Arc::new(Identity {
        calls: AtomicUsize::new(0),
        hang: AtomicBool::new(false),
        fail: AtomicBool::new(false),
        dropped: AtomicBool::new(false),
    });
    let snapshots = Arc::new(PreviewSnapshotService::new(
        repo.clone(),
        secrets.clone(),
        Arc::new(WallClock),
        identity.clone(),
        marks.clone(),
    ));
    Fixture {
        service: PreviewRecordService::new(snapshots),
        repo,
        secrets,
        marks,
        identity,
        control: Arc::new(Control::default()),
        transport: Transport {
            fetch: Mutex::new(Some(Ok(CatalogFetch::Detail("/api/tv/Resolved123".into())))),
            item: Mutex::new(Some(Ok(item()))),
            page: Mutex::new(Some(Ok(CatalogSearchPage {
                candidates: vec![],
                more_available: false,
            }))),
            fetch_calls: AtomicUsize::new(0),
            detail_calls: AtomicUsize::new(0),
            search_calls: AtomicUsize::new(0),
            hang_fetch: AtomicBool::new(false),
            hang_detail: AtomicBool::new(false),
            hang_search: AtomicBool::new(false),
            dropped: AtomicBool::new(false),
        },
    }
}

fn request<'a>(
    endpoint: &'a Endpoint,
    profile: &'a MappingProfile,
    observation: &'a Observation,
    identity: &'a CatalogIdentity,
    mode: PreviewResolutionMode<'a>,
) -> PreviewRecordRequest<'a> {
    PreviewRecordRequest {
        expected: endpoint,
        profile,
        observation,
        identity,
        mode,
    }
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
fn automatic_and_manual_results_preserve_metadata_evidence_and_provenance() {
    for manual in [false, true] {
        let f = fixture(MarkReply::Present(Some(8)));
        let endpoint = endpoint("credential-a");
        let profile = mapping::imdb_default();
        let observation = observation(8.0);
        let identity = catalog_identity();
        let mode = if manual {
            PreviewResolutionMode::Manual {
                reference: "/api/tv/Resolved123",
            }
        } else {
            PreviewResolutionMode::Native
        };
        let mut future = Box::pin(f.service.preview(
            &f.transport,
            f.control.as_ref(),
            request(&endpoint, &profile, &observation, &identity, mode),
        ));
        let record = ready(poll(future.as_mut())).unwrap();
        assert_eq!(record.source_connector_id(), "imdb:ratings_csv");
        assert_eq!(record.source_record_id(), "tt1234567");
        assert_eq!(record.source_fingerprint(), observation.idempotency_key);
        assert_eq!(record.profile_id(), "imdb_ratings");
        assert_eq!(record.profile_version(), 1);
        assert_eq!(record.base_url(), "https://neodb.example");
        assert_eq!(record.account(), "alice");
        assert!(record.is_bound_to(&endpoint));
        let rotated = Endpoint {
            credential_ref: "credential-b".into(),
            ..endpoint.clone()
        };
        assert!(!record.is_bound_to(&rotated));
        let PreviewRecordOutcome::Compared(compared) = record.outcome() else {
            panic!("expected comparison");
        };
        let ComparedPreviewRecord {
            snapshot,
            comparison,
            provenance,
        } = compared.as_ref();
        assert_eq!(
            *provenance,
            if manual {
                PreviewRecordProvenance::Manual
            } else {
                PreviewRecordProvenance::Automatic
            }
        );
        assert_eq!(snapshot.observed_at(), 1_234);
        assert_eq!(snapshot.item(), &item());
        assert_eq!(comparison.profile_id, "imdb_ratings");
        assert_eq!(comparison.fields[0].status, ComparisonStatus::Skipped);
        assert_eq!(comparison.fields[0].reason, "already_equal");
        assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 1);
        assert_eq!(f.marks.calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn comparison_preserves_empty_conflict_and_unknown_sink_semantics() {
    for (mark, status, reason, unknown) in [
        (
            MarkReply::Present(None),
            ComparisonStatus::Accepted,
            "sink_empty_accepts_suggestion",
            false,
        ),
        (
            MarkReply::Present(Some(7)),
            ComparisonStatus::NeedsReview,
            "sink_conflict_needs_review",
            false,
        ),
        (
            MarkReply::Missing,
            ComparisonStatus::NeedsReview,
            "rating_requires_shelf",
            true,
        ),
    ] {
        let f = fixture(mark);
        let endpoint = endpoint("credential-a");
        let profile = mapping::imdb_default();
        let observation = observation(8.0);
        let identity = catalog_identity();
        let mut future = Box::pin(f.service.preview(
            &f.transport,
            f.control.as_ref(),
            request(
                &endpoint,
                &profile,
                &observation,
                &identity,
                PreviewResolutionMode::Native,
            ),
        ));
        let record = ready(poll(future.as_mut())).unwrap();
        let PreviewRecordOutcome::Compared(compared) = record.outcome() else {
            panic!("expected comparison");
        };
        let comparison = &compared.comparison;
        assert_eq!(comparison.fields[0].status, status);
        assert_eq!(comparison.fields[0].reason, reason);
        assert_eq!(
            matches!(comparison.fields[0].sink, SinkValue::Unknown),
            unknown
        );
        assert_eq!(
            f.identity.calls.load(Ordering::SeqCst),
            usize::from(unknown)
        );
    }
}

#[test]
fn unresolved_outcomes_preserve_evidence_and_never_read_personal_state() {
    for case in 0..5 {
        let f = fixture(MarkReply::Present(Some(8)));
        match case {
            0 => *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unsupported)),
            1 => *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::RateLimited)),
            2 => *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unavailable)),
            3 => {
                f.transport
                    .item
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .external_ids = vec![];
            }
            4 => {
                *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unsupported));
                *f.transport.page.lock().unwrap() = Some(Ok(CatalogSearchPage {
                    candidates: vec![CatalogSearchCandidate {
                        title: "Exact title".into(),
                        item: item(),
                    }],
                    more_available: true,
                }));
            }
            _ => unreachable!(),
        }
        let endpoint = endpoint("credential-a");
        let profile = mapping::imdb_default();
        let observation = observation(8.0);
        let identity = catalog_identity();
        let mode = if case == 4 {
            PreviewResolutionMode::Search {
                query: "  Exact title  ",
            }
        } else {
            PreviewResolutionMode::Native
        };
        let mut future = Box::pin(f.service.preview(
            &f.transport,
            f.control.as_ref(),
            request(&endpoint, &profile, &observation, &identity, mode),
        ));
        let record = ready(poll(future.as_mut())).unwrap();
        match (case, record.outcome()) {
            (0, PreviewRecordOutcome::Unsupported)
            | (1, PreviewRecordOutcome::RateLimited)
            | (2, PreviewRecordOutcome::Unavailable) => {}
            (
                3,
                PreviewRecordOutcome::NeedsReview {
                    item: actual_item,
                    reason,
                },
            ) => {
                assert_eq!(*reason, "item_identity_missing");
                let mut expected_item = item();
                expected_item.external_ids.clear();
                assert_eq!(actual_item, &expected_item);
            }
            (4, PreviewRecordOutcome::SearchCandidates(page)) => {
                assert_eq!(page.candidates.len(), 1);
                assert_eq!(page.candidates[0].item, item());
                assert!(page.more_available);
            }
            _ => panic!("unexpected outcome"),
        }
        assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn invalid_inputs_fail_before_binding_or_catalog_io() {
    let oversized_reference = "x".repeat(2049);
    for case in 0..7 {
        let f = fixture(MarkReply::Present(Some(8)));
        let endpoint = endpoint("credential-a");
        let mut profile = mapping::imdb_default();
        let mut observation = observation(8.0);
        let mut identity = catalog_identity();
        let mode = match case {
            0 => {
                observation
                    .state
                    .insert("extra".into(), ScalarValue::Boolean(true));
                PreviewResolutionMode::Native
            }
            1 => {
                profile.enabled = true;
                PreviewResolutionMode::Native
            }
            2 => {
                identity.external_id = "tt7654321".into();
                PreviewResolutionMode::Native
            }
            3 => PreviewResolutionMode::Search { query: "\ninvalid" },
            4 => PreviewResolutionMode::Manual { reference: "   " },
            5 => {
                identity.model.clear();
                PreviewResolutionMode::Native
            }
            6 => PreviewResolutionMode::Manual {
                reference: &oversized_reference,
            },
            _ => unreachable!(),
        };
        let mut future = Box::pin(f.service.preview(
            &f.transport,
            f.control.as_ref(),
            request(&endpoint, &profile, &observation, &identity, mode),
        ));
        assert!(ready(poll(future.as_mut())).is_err());
        assert_eq!(f.repo.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.transport.fetch_calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.transport.detail_calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.transport.search_calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn every_binding_boundary_rejects_rotation_or_disconnect() {
    for boundary in 1..=6 {
        for disconnect in [false, true] {
            let f = fixture(MarkReply::Present(Some(8)));
            if disconnect {
                f.repo.disconnect_on.store(boundary, Ordering::SeqCst);
            } else {
                f.repo.change_on.store(boundary, Ordering::SeqCst);
            }
            let endpoint = endpoint("credential-a");
            let profile = mapping::imdb_default();
            let observation = observation(8.0);
            let identity = catalog_identity();
            let mut future = Box::pin(f.service.preview(
                &f.transport,
                f.control.as_ref(),
                request(
                    &endpoint,
                    &profile,
                    &observation,
                    &identity,
                    PreviewResolutionMode::Native,
                ),
            ));
            assert_eq!(
                error(ready(poll(future.as_mut()))).code,
                if disconnect {
                    "preview_account_disconnected"
                } else {
                    "preview_binding_changed"
                }
            );
            if boundary == 1 {
                assert_eq!(f.transport.fetch_calls.load(Ordering::SeqCst), 0);
            }
            if boundary <= 3 {
                assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
            }
        }
    }

    // Unresolved records still require both the post-resolution and final
    // publication checks, without ever opening the credential store.
    for boundary in [2, 3] {
        for disconnect in [false, true] {
            let f = fixture(MarkReply::Present(Some(8)));
            *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unsupported));
            if disconnect {
                f.repo.disconnect_on.store(boundary, Ordering::SeqCst);
            } else {
                f.repo.change_on.store(boundary, Ordering::SeqCst);
            }
            assert_eq!(
                error(run_native(&f)).code,
                if disconnect {
                    "preview_account_disconnected"
                } else {
                    "preview_binding_changed"
                }
            );
            assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
            assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);
            assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn missing_shelf_rotation_before_identity_stops_identity_and_publication() {
    let f = fixture(MarkReply::Missing);
    f.repo.change_on.store(5, Ordering::SeqCst);
    assert_eq!(error(run_native(&f)).code, "preview_binding_changed");
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.repo.calls.load(Ordering::SeqCst), 5);
}

#[test]
fn adapter_and_account_errors_return_no_record() {
    let f = fixture(MarkReply::Present(Some(8)));
    *f.transport.fetch.lock().unwrap() = Some(Err(ApiError::new(
        "neodb_catalog_unavailable",
        "Catalog unavailable.",
    )));
    assert_eq!(error(run_native(&f)).code, "neodb_catalog_unavailable");

    let f = fixture(MarkReply::Present(Some(8)));
    f.secrets.fail.store(true, Ordering::SeqCst);
    assert_eq!(error(run_native(&f)).code, "preview_credential_unavailable");
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);

    let f = fixture(MarkReply::Error);
    assert_eq!(error(run_native(&f)).code, "neodb_mark_unavailable");

    let f = fixture(MarkReply::Missing);
    f.identity.fail.store(true, Ordering::SeqCst);
    assert_eq!(error(run_native(&f)).code, "neodb_oauth_unavailable");

    let f = fixture(MarkReply::Present(Some(8)));
    f.repo.fail_on.store(1, Ordering::SeqCst);
    let error = error(run_native(&f));
    assert_eq!(error.code, "preview_binding_unavailable");
    assert!(!error.message.contains("path"));
}

fn run_native(f: &Fixture) -> Result<PreviewRecord, ApiError> {
    let endpoint = endpoint("credential-a");
    let profile = mapping::imdb_default();
    let observation = observation(8.0);
    let identity = catalog_identity();
    let mut future = Box::pin(f.service.preview(
        &f.transport,
        f.control.as_ref(),
        request(
            &endpoint,
            &profile,
            &observation,
            &identity,
            PreviewResolutionMode::Native,
        ),
    ));
    ready(poll(future.as_mut()))
}

fn error(result: Result<PreviewRecord, ApiError>) -> ApiError {
    match result {
        Ok(_) => panic!("expected error"),
        Err(error) => error,
    }
}

#[test]
fn cancellation_or_deadline_drops_each_pending_phase_and_stops_calls() {
    for phase in 0..8 {
        for timeout in [false, true] {
            let mark = if phase == 6 {
                MarkReply::Missing
            } else {
                MarkReply::Present(Some(8))
            };
            let f = fixture(mark);
            match phase {
                0 => f.repo.hang_on.store(1, Ordering::SeqCst),
                1 => f.transport.hang_fetch.store(true, Ordering::SeqCst),
                2 => f.transport.hang_detail.store(true, Ordering::SeqCst),
                3 => f.repo.hang_on.store(2, Ordering::SeqCst),
                4 => f.secrets.hang.store(true, Ordering::SeqCst),
                5 => f.marks.hang.store(true, Ordering::SeqCst),
                6 => f.identity.hang.store(true, Ordering::SeqCst),
                7 => f.repo.hang_on.store(6, Ordering::SeqCst),
                _ => unreachable!(),
            }
            let endpoint = endpoint("credential-a");
            let profile = mapping::imdb_default();
            let observation = observation(8.0);
            let identity = catalog_identity();
            let mut future = Box::pin(f.service.preview(
                &f.transport,
                f.control.as_ref(),
                request(
                    &endpoint,
                    &profile,
                    &observation,
                    &identity,
                    PreviewResolutionMode::Native,
                ),
            ));
            assert!(poll(future.as_mut()).is_pending());
            let before = (
                f.repo.calls.load(Ordering::SeqCst),
                f.transport.fetch_calls.load(Ordering::SeqCst),
                f.transport.detail_calls.load(Ordering::SeqCst),
                f.secrets.calls.load(Ordering::SeqCst),
                f.marks.calls.load(Ordering::SeqCst),
                f.identity.calls.load(Ordering::SeqCst),
            );
            if timeout {
                f.control.now.store(120, Ordering::SeqCst);
            } else {
                f.control.cancelled.store(true, Ordering::SeqCst);
            }
            assert_eq!(
                error(ready(poll(future.as_mut()))).code,
                if timeout {
                    "catalog_resolution_timeout"
                } else {
                    "catalog_resolution_cancelled"
                }
            );
            assert_eq!(
                before,
                (
                    f.repo.calls.load(Ordering::SeqCst),
                    f.transport.fetch_calls.load(Ordering::SeqCst),
                    f.transport.detail_calls.load(Ordering::SeqCst),
                    f.secrets.calls.load(Ordering::SeqCst),
                    f.marks.calls.load(Ordering::SeqCst),
                    f.identity.calls.load(Ordering::SeqCst),
                )
            );
            assert!(
                f.repo.dropped.load(Ordering::SeqCst)
                    || f.transport.dropped.load(Ordering::SeqCst)
                    || f.secrets.dropped.load(Ordering::SeqCst)
                    || f.marks.dropped.load(Ordering::SeqCst)
                    || f.identity.dropped.load(Ordering::SeqCst)
            );
        }
    }
}

#[test]
fn cancellation_before_work_and_dropping_service_future_start_nothing_later() {
    let f = fixture(MarkReply::Present(Some(8)));
    f.control.cancelled.store(true, Ordering::SeqCst);
    assert_eq!(error(run_native(&f)).code, "catalog_resolution_cancelled");
    assert_eq!(f.repo.calls.load(Ordering::SeqCst), 0);

    let f = fixture(MarkReply::Present(Some(8)));
    f.transport.hang_fetch.store(true, Ordering::SeqCst);
    let endpoint = endpoint("credential-a");
    let profile = mapping::imdb_default();
    let observation = observation(8.0);
    let identity = catalog_identity();
    let mut future = Box::pin(f.service.preview(
        &f.transport,
        f.control.as_ref(),
        request(
            &endpoint,
            &profile,
            &observation,
            &identity,
            PreviewResolutionMode::Native,
        ),
    ));
    assert!(poll(future.as_mut()).is_pending());
    drop(future);
    assert!(f.transport.dropped.load(Ordering::SeqCst));
    assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn outer_deadline_overflow_fails_before_any_io() {
    let f = fixture(MarkReply::Present(Some(8)));
    f.control.now.store(u64::MAX, Ordering::SeqCst);
    assert_eq!(error(run_native(&f)).code, "catalog_resolution_timeout");
    assert_eq!(f.repo.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.transport.fetch_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn search_is_unsupported_only_and_a_first_page_never_reads_marks() {
    let f = fixture(MarkReply::Present(Some(8)));
    *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unsupported));
    *f.transport.page.lock().unwrap() = Some(Ok(CatalogSearchPage {
        candidates: vec![CatalogSearchCandidate {
            title: "Exact title".into(),
            item: item(),
        }],
        more_available: false,
    }));
    let endpoint = endpoint("credential-a");
    let profile = mapping::imdb_default();
    let observation = observation(8.0);
    let identity = catalog_identity();
    let mut future = Box::pin(f.service.preview(
        &f.transport,
        f.control.as_ref(),
        request(
            &endpoint,
            &profile,
            &observation,
            &identity,
            PreviewResolutionMode::Search {
                query: "Exact title",
            },
        ),
    ));
    assert!(matches!(
        ready(poll(future.as_mut())).unwrap().outcome(),
        PreviewRecordOutcome::SearchCandidates(_)
    ));
    assert_eq!(f.transport.search_calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);

    let f = fixture(MarkReply::Present(Some(8)));
    let record = run_search(&f);
    assert!(matches!(
        record.outcome(),
        PreviewRecordOutcome::Compared(_)
    ));
    assert_eq!(f.transport.search_calls.load(Ordering::SeqCst), 0);

    for timeout in [false, true] {
        let f = fixture(MarkReply::Present(Some(8)));
        *f.transport.fetch.lock().unwrap() = Some(Ok(CatalogFetch::Unsupported));
        f.transport.hang_search.store(true, Ordering::SeqCst);
        let mut future = Box::pin(f.service.preview(
            &f.transport,
            f.control.as_ref(),
            request(
                &endpoint,
                &profile,
                &observation,
                &identity,
                PreviewResolutionMode::Search {
                    query: "Exact title",
                },
            ),
        ));
        assert!(poll(future.as_mut()).is_pending());
        assert_eq!(f.transport.search_calls.load(Ordering::SeqCst), 1);
        if timeout {
            f.control.now.store(120, Ordering::SeqCst);
        } else {
            f.control.cancelled.store(true, Ordering::SeqCst);
        }
        assert_eq!(
            error(ready(poll(future.as_mut()))).code,
            if timeout {
                "catalog_resolution_timeout"
            } else {
                "catalog_resolution_cancelled"
            }
        );
        assert!(f.transport.dropped.load(Ordering::SeqCst));
        assert_eq!(f.secrets.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    }
}

fn run_search(f: &Fixture) -> PreviewRecord {
    let endpoint = endpoint("credential-a");
    let profile = mapping::imdb_default();
    let observation = observation(8.0);
    let identity = catalog_identity();
    let mut future = Box::pin(f.service.preview(
        &f.transport,
        f.control.as_ref(),
        request(
            &endpoint,
            &profile,
            &observation,
            &identity,
            PreviewResolutionMode::Search {
                query: "Exact title",
            },
        ),
    ));
    ready(poll(future.as_mut())).unwrap()
}
