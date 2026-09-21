use super::*;
use std::{
    collections::VecDeque,
    future::{Future, pending, poll_fn},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Control {
    now: AtomicU64,
    cancelled: AtomicBool,
}
impl ResolutionControl for Control {
    fn run<'a, T: Send + 'a>(
        &'a self,
        deadline: Duration,
        mut work: PortFuture<'a, T>,
    ) -> PortFuture<'a, T> {
        Box::pin(poll_fn(move |cx| {
            let check = || {
                if self.cancelled.load(Ordering::SeqCst) {
                    Err(ApiError::new("catalog_resolution_cancelled", "Cancelled."))
                } else if self.now() >= deadline {
                    Err(timeout())
                } else {
                    Ok(())
                }
            };
            if let Err(error) = check() {
                return Poll::Ready(Err(error));
            }
            match work.as_mut().poll(cx) {
                Poll::Ready(result) => Poll::Ready(check().and(result)),
                Poll::Pending => Poll::Pending,
            }
        }))
    }
    fn now(&self) -> Duration {
        Duration::from_secs(self.now.load(Ordering::SeqCst))
    }
    fn wait_until(&self, at: Duration) -> PortFuture<'_, ()> {
        Box::pin(poll_fn(move |_| {
            if self.cancelled.load(Ordering::SeqCst) {
                Poll::Ready(Err(ApiError::new(
                    "catalog_resolution_cancelled",
                    "Cancelled.",
                )))
            } else if self.now() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
}
enum Fetch {
    Reply(CatalogFetch, u64),
    Fail,
    Hang,
}
struct Transport {
    control: Arc<Control>,
    fetches: Mutex<VecDeque<Fetch>>,
    calls: Mutex<Vec<(String, u64)>>,
    item: Mutex<Option<CatalogItem>>,
    hang_detail: bool,
    detail_elapsed: u64,
    detail_error: Option<ApiError>,
    detail_args: Mutex<Vec<(String, String)>>,
    dropped: AtomicBool,
}
struct DropGuard<'a>(&'a AtomicBool);
impl Drop for DropGuard<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
impl NeoDbCatalogTransport for Transport {
    fn fetch(&self, _: String, _: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(("fetch".into(), self.control.now().as_secs()));
            let next = self.fetches.lock().unwrap().pop_front().unwrap();
            match next {
                Fetch::Reply(reply, elapsed) => {
                    self.control.now.fetch_add(elapsed, Ordering::SeqCst);
                    Ok(reply)
                }
                Fetch::Fail => Err(ApiError::new("neodb_catalog_unavailable", "Unavailable.")),
                Fetch::Hang => {
                    let _guard = DropGuard(&self.dropped);
                    pending().await
                }
            }
        })
    }
    fn detail(&self, base: String, reference: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move {
            self.detail_args.lock().unwrap().push((base, reference));
            self.calls
                .lock()
                .unwrap()
                .push(("detail".into(), self.control.now().as_secs()));
            if self.hang_detail {
                let _guard = DropGuard(&self.dropped);
                return pending().await;
            }
            self.control
                .now
                .fetch_add(self.detail_elapsed, Ordering::SeqCst);
            if let Some(error) = &self.detail_error {
                return Err(error.clone());
            }
            Ok(self.item.lock().unwrap().take().unwrap())
        })
    }
}
fn identity() -> CatalogIdentity {
    CatalogIdentity {
        source_url: "https://www.imdb.com/title/tt1234567/".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        scheme: "imdb:title".into(),
        external_id: "tt1234567".into(),
    }
}
fn item() -> CatalogItem {
    CatalogItem {
        uuid: "abc".into(),
        canonical_url: "https://neodb.example/tv/abc".into(),
        detail_url: "https://neodb.example/api/tv/abc".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        parent_uuid: None,
        external_ids: vec![("imdb:title".into(), "tt1234567".into())],
    }
}
fn setup(fetches: Vec<Fetch>) -> (Arc<Control>, Transport) {
    let control = Arc::new(Control::default());
    let transport = Transport {
        control: control.clone(),
        fetches: Mutex::new(fetches.into()),
        calls: Mutex::new(vec![]),
        item: Mutex::new(Some(item())),
        hang_detail: false,
        detail_elapsed: 0,
        detail_error: None,
        detail_args: Mutex::new(vec![]),
        dropped: AtomicBool::new(false),
    };
    (control, transport)
}
fn poll<T>(future: Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn ready<T>(result: Poll<T>) -> T {
    match result {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected completion"),
    }
}

#[test]
fn pending_spacing_includes_request_duration_and_exact_identity_matches() {
    let (clock, transport) = setup(vec![
        Fetch::Reply(CatalogFetch::Pending, 3),
        Fetch::Reply(CatalogFetch::Detail("ref".into()), 1),
    ]);
    let expected = identity();
    let mut future = Box::pin(resolve_native(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
    ));
    assert!(poll(future.as_mut()).is_pending());
    clock.now.store(17, Ordering::SeqCst);
    assert!(poll(future.as_mut()).is_pending());
    clock.now.store(18, Ordering::SeqCst);
    assert!(matches!(
        ready(poll(future.as_mut())).unwrap(),
        Resolution::Matched(_)
    ));
    assert_eq!(
        *transport.calls.lock().unwrap(),
        [
            ("fetch".into(), 0),
            ("fetch".into(), 18),
            ("detail".into(), 19)
        ]
    );
}

#[test]
fn repeated_pending_stops_at_deadline_without_ninth_fetch() {
    let (clock, transport) = setup(
        (0..8)
            .map(|_| Fetch::Reply(CatalogFetch::Pending, 0))
            .collect(),
    );
    let expected = identity();
    let mut future = Box::pin(resolve_native(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
    ));
    for now in (0..120).step_by(15) {
        clock.now.store(now, Ordering::SeqCst);
        assert!(poll(future.as_mut()).is_pending());
    }
    clock.now.store(120, Ordering::SeqCst);
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_timeout"
    );
    assert_eq!(transport.calls.lock().unwrap().len(), 8);
}

#[test]
fn timeout_drops_hung_fetch_and_detail() {
    for detail in [false, true] {
        let (clock, mut transport) = setup(vec![if detail {
            Fetch::Reply(CatalogFetch::Detail("ref".into()), 0)
        } else {
            Fetch::Hang
        }]);
        transport.hang_detail = detail;
        let expected = identity();
        let mut future = Box::pin(resolve_native(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
        ));
        assert!(poll(future.as_mut()).is_pending());
        clock.now.store(120, Ordering::SeqCst);
        assert_eq!(
            ready(poll(future.as_mut())).unwrap_err().code,
            "catalog_resolution_timeout"
        );
        assert!(transport.dropped.load(Ordering::SeqCst));
    }
}

#[test]
fn cancellation_before_during_io_or_wait_never_retries() {
    for mode in 0..4 {
        let (clock, mut transport) = setup(vec![match mode {
            1 => Fetch::Hang,
            2 => Fetch::Reply(CatalogFetch::Detail("ref".into()), 0),
            _ => Fetch::Reply(CatalogFetch::Pending, 0),
        }]);
        transport.hang_detail = mode == 2;
        let expected = identity();
        let mut future = Box::pin(resolve_native(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
        ));
        if mode != 0 {
            assert!(poll(future.as_mut()).is_pending());
        }
        let count = transport.calls.lock().unwrap().len();
        clock.cancelled.store(true, Ordering::SeqCst);
        clock.now.store(120, Ordering::SeqCst);
        assert_eq!(
            ready(poll(future.as_mut())).unwrap_err().code,
            "catalog_resolution_cancelled"
        );
        assert_eq!(transport.calls.lock().unwrap().len(), count);
        if mode == 1 || mode == 2 {
            assert!(transport.dropped.load(Ordering::SeqCst));
        }
    }
}

#[test]
fn late_response_does_not_start_detail() {
    let (clock, transport) = setup(vec![Fetch::Reply(CatalogFetch::Detail("ref".into()), 120)]);
    let expected = identity();
    let mut future = Box::pin(resolve_native(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_timeout"
    );
    assert_eq!(transport.calls.lock().unwrap().len(), 1);
}

#[test]
fn terminal_results_and_errors_are_not_retried_or_treated_as_absence() {
    for (fetch, expected) in [
        (CatalogFetch::RateLimited, Resolution::RateLimited),
        (CatalogFetch::Unsupported, Resolution::Unsupported),
        (CatalogFetch::Unavailable, Resolution::Unavailable),
    ] {
        let (clock, transport) = setup(vec![Fetch::Reply(fetch, 0)]);
        let identity = identity();
        let mut future = Box::pin(resolve_native(
            &transport,
            clock.as_ref(),
            "base",
            &identity,
        ));
        assert_eq!(ready(poll(future.as_mut())).unwrap(), expected);
        assert_eq!(transport.calls.lock().unwrap().len(), 1);
    }
    let (clock, transport) = setup(vec![Fetch::Fail]);
    let expected = identity();
    let mut future = Box::pin(resolve_native(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "neodb_catalog_unavailable"
    );
}

#[test]
fn granularity_missing_and_conflicting_claims_require_review() {
    for (model, ids, reason) in [
        ("TVSeason", vec!["tt1234567"], "item_granularity_mismatch"),
        ("TVEpisode", vec!["tt1234567"], "item_granularity_mismatch"),
        ("Movie", vec!["tt1234567"], "item_granularity_mismatch"),
        ("TVShow", vec![], "item_identity_missing"),
        ("TVShow", vec!["tt7654321"], "item_identity_conflict"),
        (
            "TVShow",
            vec!["tt1234567", "tt7654321"],
            "item_identity_conflict",
        ),
    ] {
        let mut evidence = item();
        evidence.model = model.into();
        evidence.external_ids = ids
            .into_iter()
            .map(|id| ("imdb:title".into(), id.into()))
            .collect();
        assert!(
            matches!(classify(&identity(), evidence), Resolution::NeedsReview { reason: actual, .. } if actual == reason)
        );
    }
    let mut expected = identity();
    expected.media_type = "movie".into();
    expected.model = "Movie".into();
    let mut evidence = item();
    evidence.media_type = "movie".into();
    evidence.model = "Movie".into();
    assert!(matches!(
        classify(&expected, evidence),
        Resolution::Matched(_)
    ));
}

#[test]
fn invalid_expectations_fail_before_io() {
    let (clock, transport) = setup(vec![]);
    let mut expected = identity();
    expected.scheme.clear();
    let mut future = Box::pin(resolve_native(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_identity_invalid"
    );
    assert!(transport.calls.lock().unwrap().is_empty());
}

#[test]
fn manual_exact_or_missing_identity_is_distinct_and_preserves_full_evidence() {
    for ids in [
        vec![("imdb:title".into(), "tt1234567".into())],
        vec![],
        vec![("tmdb:tv".into(), "987".into())],
    ] {
        let (clock, transport) = setup(vec![]);
        let mut evidence = item();
        evidence.uuid = "FullEvidence".into();
        evidence.canonical_url = "https://neodb.example/tv/FullEvidence".into();
        evidence.detail_url = "https://neodb.example/api/tv/FullEvidence".into();
        evidence.parent_uuid = Some("parent".into());
        evidence.external_ids = ids.clone();
        *transport.item.lock().unwrap() = Some(evidence);
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "https://neodb.example",
            &expected,
            "/api/tv/FullEvidence",
        ));
        let result = ready(poll(future.as_mut())).unwrap();
        let Resolution::ManuallySelected(actual) = result else {
            panic!("manual choice must have a distinct outcome");
        };
        assert_eq!(
            actual,
            CatalogItem {
                uuid: "FullEvidence".into(),
                canonical_url: "https://neodb.example/tv/FullEvidence".into(),
                detail_url: "https://neodb.example/api/tv/FullEvidence".into(),
                media_type: "tv".into(),
                model: "TVShow".into(),
                parent_uuid: Some("parent".into()),
                external_ids: ids,
            }
        );
        assert_eq!(
            *transport.detail_args.lock().unwrap(),
            [(
                "https://neodb.example".into(),
                "/api/tv/FullEvidence".into()
            )]
        );
        assert_eq!(*transport.calls.lock().unwrap(), [("detail".into(), 0)]);
    }
}

#[test]
fn manual_conflicts_and_granularity_are_refused_without_fetch() {
    for (media_type, model, ids, expected_reason) in [
        (
            "tv",
            "TVShow",
            vec![("imdb:title".into(), "tt7654321".into())],
            "item_identity_conflict",
        ),
        (
            "tv",
            "TVShow",
            vec![
                ("imdb:title".into(), "tt1234567".into()),
                ("imdb:title".into(), "tt7654321".into()),
            ],
            "item_identity_conflict",
        ),
        (
            "tv",
            "TVSeason",
            vec![("imdb:title".into(), "tt1234567".into())],
            "item_granularity_mismatch",
        ),
        (
            "movie",
            "TVShow",
            vec![("imdb:title".into(), "tt1234567".into())],
            "item_granularity_mismatch",
        ),
    ] {
        let (clock, transport) = setup(vec![]);
        let mut evidence = item();
        evidence.media_type = media_type.into();
        evidence.model = model.into();
        evidence.external_ids = ids;
        *transport.item.lock().unwrap() = Some(evidence);
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
            "/api/tv/abc",
        ));
        assert!(matches!(
            ready(poll(future.as_mut())).unwrap(),
            Resolution::NeedsReview { reason, .. } if reason == expected_reason
        ));
        assert_eq!(*transport.calls.lock().unwrap(), [("detail".into(), 0)]);
    }
}

#[test]
fn manual_invalid_inputs_fail_before_io() {
    for reference in ["", "   "] {
        let (clock, transport) = setup(vec![]);
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
            reference,
        ));
        let error = ready(poll(future.as_mut())).unwrap_err();
        assert_eq!(error.code, "neodb_catalog_reference_invalid");
        assert_eq!(
            error.message,
            "Expected a supported NeoDB catalog detail reference."
        );
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    let (clock, transport) = setup(vec![]);
    let mut expected = identity();
    expected.external_id.clear();
    let mut future = Box::pin(resolve_manual(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
        "/api/tv/abc",
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_identity_invalid"
    );
    assert!(transport.calls.lock().unwrap().is_empty());

    let (clock, transport) = setup(vec![]);
    clock.now.store(u64::MAX, Ordering::SeqCst);
    let expected = identity();
    let mut future = Box::pin(resolve_manual(
        &transport,
        clock.as_ref(),
        "base",
        &expected,
        "/api/tv/abc",
    ));
    assert_eq!(
        ready(poll(future.as_mut())).unwrap_err().code,
        "catalog_resolution_timeout"
    );
    assert!(transport.calls.lock().unwrap().is_empty());
}

#[test]
fn manual_propagates_sanitized_detail_error_once() {
    for (code, message) in [
        (
            "neodb_catalog_reference_invalid",
            "Catalog detail reference is invalid.",
        ),
        (
            "neodb_catalog_rate_limited",
            "Catalog detail is rate limited.",
        ),
        (
            "neodb_catalog_unavailable",
            "Catalog detail is unavailable.",
        ),
    ] {
        let (clock, mut transport) = setup(vec![]);
        transport.detail_error = Some(ApiError::new(code, message));
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
            "/api/tv/abc",
        ));
        let error = ready(poll(future.as_mut())).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.message, message);
        assert_eq!(*transport.calls.lock().unwrap(), [("detail".into(), 0)]);
    }
}

#[test]
fn manual_cancellation_before_or_during_detail_drops_work() {
    for cancel_before in [true, false] {
        let (clock, mut transport) = setup(vec![]);
        transport.hang_detail = true;
        if cancel_before {
            clock.cancelled.store(true, Ordering::SeqCst);
        }
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
            "/api/tv/abc",
        ));
        if !cancel_before {
            assert!(poll(future.as_mut()).is_pending());
            clock.cancelled.store(true, Ordering::SeqCst);
        }
        assert_eq!(
            ready(poll(future.as_mut())).unwrap_err().code,
            "catalog_resolution_cancelled"
        );
        assert_eq!(
            transport.calls.lock().unwrap().len(),
            usize::from(!cancel_before)
        );
        assert_eq!(transport.dropped.load(Ordering::SeqCst), !cancel_before);
    }
}

#[test]
fn manual_deadline_drops_hung_detail_and_rejects_late_result() {
    for late in [false, true] {
        let (clock, mut transport) = setup(vec![]);
        transport.hang_detail = !late;
        transport.detail_elapsed = if late { 120 } else { 0 };
        let expected = identity();
        let mut future = Box::pin(resolve_manual(
            &transport,
            clock.as_ref(),
            "base",
            &expected,
            "/api/tv/abc",
        ));
        if !late {
            assert!(poll(future.as_mut()).is_pending());
            clock.now.store(120, Ordering::SeqCst);
        }
        assert_eq!(
            ready(poll(future.as_mut())).unwrap_err().code,
            "catalog_resolution_timeout"
        );
        assert_eq!(transport.dropped.load(Ordering::SeqCst), !late);
        assert_eq!(*transport.calls.lock().unwrap(), [("detail".into(), 0)]);
    }
}
