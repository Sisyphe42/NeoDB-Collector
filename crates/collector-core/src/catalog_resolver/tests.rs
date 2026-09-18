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
    fn detail(&self, _: String, _: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(("detail".into(), self.control.now().as_secs()));
            if self.hang_detail {
                let _guard = DropGuard(&self.dropped);
                return pending().await;
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
