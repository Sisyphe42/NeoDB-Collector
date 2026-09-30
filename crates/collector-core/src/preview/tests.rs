use super::*;
use crate::{
    access::{AccessRecord, ChallengeMatch, PortFuture},
    marks::MarkSnapshot,
    oauth::{OAuthApp, OAuthIdentity, OAuthToken},
    secrets::{ExposeSecret, SecretString},
};
use collector_domain::{
    access::PairedClient,
    mapping::{MappingRule, Operation, Predicate},
    observation::ObservedItem,
};
use std::{
    collections::{BTreeMap, VecDeque},
    future::pending,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
    },
};

fn endpoint(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn item() -> CatalogItem {
    CatalogItem {
        uuid: "FullEvidence123".into(),
        canonical_url: "https://neodb.example/tv/FullEvidence123".into(),
        detail_url: "https://neodb.example/api/tv/FullEvidence123".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        parent_uuid: Some("PreservedParent123".into()),
        external_ids: vec![
            ("imdb:title".into(), "tt1234567".into()),
            ("tmdb:tv".into(), "123".into()),
        ],
    }
}

struct Repo {
    current: Mutex<Option<Endpoint>>,
    endpoint_calls: AtomicUsize,
    replies: Mutex<VecDeque<Result<Option<Endpoint>, ApiError>>>,
}

impl Repo {
    fn new(current: Option<Endpoint>) -> Self {
        Self {
            current: Mutex::new(current),
            endpoint_calls: AtomicUsize::new(0),
            replies: Mutex::new(VecDeque::new()),
        }
    }

    fn queue(&self, replies: impl IntoIterator<Item = Result<Option<Endpoint>, ApiError>>) {
        self.replies.lock().unwrap().extend(replies);
    }
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
            self.endpoint_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(reply) = self.replies.lock().unwrap().pop_front() {
                reply
            } else {
                Ok(self.current.lock().unwrap().clone())
            }
        })
    }
    fn set_endpoint(&self, _: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
}

#[derive(Default)]
struct Secrets {
    get_calls: AtomicUsize,
    set_calls: AtomicUsize,
    delete_calls: AtomicUsize,
    fail: AtomicBool,
    hang: AtomicBool,
    dropped: AtomicBool,
}

struct DropFlag<'a>(&'a AtomicBool);
impl Drop for DropFlag<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl SecretStore for Secrets {
    fn set(&self, _: String, _: SecretString) -> PortFuture<'_, ()> {
        self.set_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }
    fn get(&self, reference: String) -> PortFuture<'_, SecretString> {
        Box::pin(async move {
            assert_eq!(reference, "credential-a");
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(ApiError::new(
                    "raw_secret_backend_error",
                    "reference should not escape",
                ));
            }
            if self.hang.load(Ordering::SeqCst) {
                let _guard = DropFlag(&self.dropped);
                return pending().await;
            }
            Ok("access-token".into())
        })
    }
    fn delete(&self, _: String) -> PortFuture<'_, ()> {
        self.delete_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { unreachable!() })
    }
}

struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

enum MarkReply {
    Present {
        owner: String,
        shelf: String,
        rating: Option<u8>,
    },
    Missing,
    Error,
    Hang,
}

struct Marks {
    reply: Mutex<Option<MarkReply>>,
    calls: AtomicUsize,
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
            assert_eq!(expected_item, &item());
            self.calls.fetch_add(1, Ordering::SeqCst);
            let reply = self.reply.lock().unwrap().take().unwrap();
            match reply {
                MarkReply::Present {
                    owner,
                    shelf,
                    rating,
                } => Ok(MarkRead::Present(Box::new(MarkSnapshot {
                    // Deliberately reduced embedded evidence: C2 must retain the
                    // caller's full resolved CatalogItem instead.
                    item: CatalogItem {
                        uuid: "FullEvidence123".into(),
                        canonical_url: "https://neodb.example/tv/FullEvidence123".into(),
                        detail_url: "https://neodb.example/api/tv/FullEvidence123".into(),
                        media_type: "tv".into(),
                        model: "TVShow".into(),
                        parent_uuid: Some("PreservedParent123".into()),
                        external_ids: vec![],
                    },
                    owner,
                    shelf_type: shelf,
                    rating_grade: rating,
                }))),
                MarkReply::Missing => Ok(MarkRead::MissingShelf),
                MarkReply::Error => Err(ApiError::new(
                    "neodb_mark_unavailable",
                    "Mark is unavailable.",
                )),
                MarkReply::Hang => {
                    let _guard = DropFlag(&self.dropped);
                    pending().await
                }
            }
        })
    }
}

struct Identity {
    username: Mutex<Option<Result<String, ApiError>>>,
    calls: AtomicUsize,
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
            self.username
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Ok("alice".into()))
                .map(|username| OAuthIdentity { username })
        })
    }
}

struct Fixture {
    service: PreviewSnapshotService,
    repo: Arc<Repo>,
    secrets: Arc<Secrets>,
    clock: Arc<TestClock>,
    identity: Arc<Identity>,
    marks: Arc<Marks>,
}

fn fixture(reply: MarkReply) -> Fixture {
    let repo = Arc::new(Repo::new(Some(endpoint("credential-a"))));
    let secrets = Arc::new(Secrets::default());
    let clock = Arc::new(TestClock(AtomicI64::new(1_234)));
    let identity = Arc::new(Identity {
        username: Mutex::new(None),
        calls: AtomicUsize::new(0),
    });
    let marks = Arc::new(Marks {
        reply: Mutex::new(Some(reply)),
        calls: AtomicUsize::new(0),
        dropped: AtomicBool::new(false),
    });
    let service = PreviewSnapshotService::new(
        repo.clone(),
        secrets.clone(),
        clock.clone(),
        identity.clone(),
        marks.clone(),
    );
    Fixture {
        service,
        repo,
        secrets,
        clock,
        identity,
        marks,
    }
}

fn present(shelf: &str, rating: Option<u8>) -> MarkReply {
    MarkReply::Present {
        owner: "alice".into(),
        shelf: shelf.into(),
        rating,
    }
}

#[tokio::test]
async fn present_snapshot_is_bound_timestamped_and_preserves_resolved_evidence() {
    let f = fixture(present("complete", Some(8)));
    f.clock.0.store(5_678, Ordering::SeqCst);
    let snapshot = f
        .service
        .read(endpoint("credential-a"), item())
        .await
        .unwrap();
    assert_eq!(snapshot.base_url(), "https://neodb.example");
    assert_eq!(snapshot.account(), "alice");
    assert_eq!(snapshot.observed_at(), 5_678);
    assert_eq!(
        snapshot.item().parent_uuid.as_deref(),
        Some("PreservedParent123")
    );
    assert_eq!(snapshot.item().external_ids.len(), 2);
    assert_eq!(f.repo.endpoint_calls.load(Ordering::SeqCst), 3);
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.set_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.delete_calls.load(Ordering::SeqCst), 0);
    f.service.ensure_current_binding(&snapshot).await.unwrap();
}

#[tokio::test]
async fn missing_shelf_requires_matching_identity_and_one_extra_freshness_check() {
    let f = fixture(MarkReply::Missing);
    let snapshot = f
        .service
        .read(endpoint("credential-a"), item())
        .await
        .unwrap();
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.repo.endpoint_calls.load(Ordering::SeqCst), 4);
    assert!(matches!(snapshot.sink("shelf_type"), SinkValue::Empty));
    assert!(matches!(snapshot.sink("rating_grade"), SinkValue::Unknown));

    for (identity, expected_code) in [
        (Ok("mallory".into()), "preview_account_changed"),
        (
            Err(ApiError::new(
                "neodb_oauth_unavailable",
                "Identity unavailable.",
            )),
            "neodb_oauth_unavailable",
        ),
    ] {
        let f = fixture(MarkReply::Missing);
        *f.identity.username.lock().unwrap() = Some(identity);
        let error = f
            .service
            .read(endpoint("credential-a"), item())
            .await
            .err()
            .expect("missing shelf must not produce a snapshot");
        assert_eq!(error.code, expected_code);
    }
}

#[tokio::test]
async fn missing_shelf_binding_change_before_identity_starts_no_identity_call() {
    let f = fixture(MarkReply::Missing);
    f.repo.queue([
        Ok(Some(endpoint("credential-a"))),
        Ok(Some(endpoint("credential-a"))),
        Ok(Some(endpoint("credential-b"))),
    ]);
    assert_eq!(
        f.service
            .read(endpoint("credential-a"), item())
            .await
            .err()
            .unwrap()
            .code,
        "preview_binding_changed"
    );
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn missing_shelf_binding_change_after_identity_returns_no_snapshot() {
    let f = fixture(MarkReply::Missing);
    f.repo.queue([
        Ok(Some(endpoint("credential-a"))),
        Ok(Some(endpoint("credential-a"))),
        Ok(Some(endpoint("credential-a"))),
        Ok(None),
    ]);
    assert_eq!(
        f.service
            .read(endpoint("credential-a"), item())
            .await
            .err()
            .unwrap()
            .code,
        "preview_account_disconnected"
    );
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn current_binding_check_detects_later_rotation_and_disconnect() {
    let f = fixture(present("complete", Some(8)));
    let snapshot = f
        .service
        .read(endpoint("credential-a"), item())
        .await
        .unwrap();
    *f.repo.current.lock().unwrap() = Some(endpoint("credential-b"));
    assert_eq!(
        f.service
            .ensure_current_binding(&snapshot)
            .await
            .unwrap_err()
            .code,
        "preview_binding_changed"
    );
    *f.repo.current.lock().unwrap() = None;
    assert_eq!(
        f.service
            .ensure_current_binding(&snapshot)
            .await
            .unwrap_err()
            .code,
        "preview_account_disconnected"
    );
}

#[tokio::test]
async fn present_owner_is_checked_defensively() {
    let f = fixture(MarkReply::Present {
        owner: "mallory".into(),
        shelf: "complete".into(),
        rating: Some(8),
    });
    assert_eq!(
        f.service
            .read(endpoint("credential-a"), item())
            .await
            .err()
            .unwrap()
            .code,
        "preview_account_changed"
    );
}

#[tokio::test]
async fn every_binding_phase_rejects_disconnect_or_full_tuple_change() {
    let variants = [
        None,
        Some(Endpoint {
            base_url: "https://other.example".into(),
            ..endpoint("credential-a")
        }),
        Some(Endpoint {
            account: "bob".into(),
            ..endpoint("credential-a")
        }),
        Some(endpoint("credential-b")),
    ];
    for changed in variants {
        for phase in 0..3 {
            let f = fixture(present("complete", Some(8)));
            let mut replies = vec![Ok(Some(endpoint("credential-a"))); 3];
            replies[phase] = Ok(changed.clone());
            f.repo.queue(replies);
            let error = f
                .service
                .read(endpoint("credential-a"), item())
                .await
                .err()
                .unwrap();
            assert_eq!(
                error.code,
                if changed.is_none() {
                    "preview_account_disconnected"
                } else {
                    "preview_binding_changed"
                }
            );
            assert_eq!(
                f.secrets.get_calls.load(Ordering::SeqCst),
                usize::from(phase > 0)
            );
            assert_eq!(f.marks.calls.load(Ordering::SeqCst), usize::from(phase > 1));
        }
    }
}

#[tokio::test]
async fn repository_secret_and_mark_failures_are_sanitized_and_stop() {
    let f = fixture(present("complete", Some(8)));
    f.repo
        .queue([Err(ApiError::new("database_unavailable", "raw db path"))]);
    let error = f
        .service
        .read(endpoint("credential-a"), item())
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "preview_binding_unavailable");
    assert!(!error.message.contains("path"));

    let f = fixture(present("complete", Some(8)));
    f.secrets.fail.store(true, Ordering::SeqCst);
    let error = f
        .service
        .read(endpoint("credential-a"), item())
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "preview_credential_unavailable");
    assert!(!error.message.contains("reference"));
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);

    let f = fixture(MarkReply::Error);
    assert_eq!(
        f.service
            .read(endpoint("credential-a"), item())
            .await
            .err()
            .unwrap()
            .code,
        "neodb_mark_unavailable"
    );
    assert_eq!(f.repo.endpoint_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn dropping_during_secret_read_starts_no_remote_calls_or_mutations() {
    let f = fixture(present("complete", Some(8)));
    f.secrets.hang.store(true, Ordering::SeqCst);
    let mut read = Box::pin(f.service.read(endpoint("credential-a"), item()));
    tokio::select! {
        _ = &mut read => panic!("secret read should remain pending"),
        _ = tokio::task::yield_now() => {}
    }
    assert_eq!(f.secrets.get_calls.load(Ordering::SeqCst), 1);
    drop(read);
    assert!(f.secrets.dropped.load(Ordering::SeqCst));
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.set_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.delete_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn dropping_pending_mark_read_stops_all_subsequent_calls() {
    let f = fixture(MarkReply::Hang);
    let mut read = Box::pin(f.service.read(endpoint("credential-a"), item()));
    tokio::select! {
        _ = &mut read => panic!("mark read should remain pending"),
        _ = tokio::task::yield_now() => {}
    }
    assert_eq!(f.marks.calls.load(Ordering::SeqCst), 1);
    drop(read);
    assert!(f.marks.dropped.load(Ordering::SeqCst));
    assert_eq!(f.repo.endpoint_calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.identity.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.set_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.secrets.delete_calls.load(Ordering::SeqCst), 0);
}

fn snapshot(state: SnapshotState) -> AccountMarkSnapshot {
    AccountMarkSnapshot {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        observed_at: 1_234,
        item: item(),
        state,
        binding: endpoint("credential-a"),
    }
}

fn observation(value: ScalarValue) -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: "row-1".into(),
        observed_at: "2026-09-30T00:00:00Z".into(),
        idempotency_key: "row-1".into(),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: "https://www.imdb.com/title/tt1234567/".into(),
            external_ids: BTreeMap::new(),
            title_hint: None,
        },
        state: BTreeMap::from([("rating".into(), value)]),
    }
}

fn rating_profile() -> MappingProfile {
    MappingProfile {
        schema_version: 1,
        id: "profile".into(),
        version: 7,
        source_connector_id: "imdb:ratings_csv".into(),
        sink_connector_id: "neodb".into(),
        enabled: false,
        rules: vec![MappingRule {
            id: "rating".into(),
            enabled: true,
            target: "rating_grade".into(),
            operation: Operation::Direct {
                from: "rating".into(),
            },
        }],
    }
}

fn field<'a>(comparison: &'a Comparison, name: &str) -> &'a ComparedField {
    comparison
        .fields
        .iter()
        .find(|field| field.field == name)
        .unwrap()
}

#[test]
fn present_rating_comparison_distinguishes_equal_empty_and_conflict() {
    for (rating, source, expected_status, expected_reason) in [
        (Some(8), 8.0, ComparisonStatus::Skipped, "already_equal"),
        (
            Some(7),
            8.0,
            ComparisonStatus::NeedsReview,
            "sink_conflict_needs_review",
        ),
        (
            None,
            0.0,
            ComparisonStatus::Accepted,
            "sink_empty_accepts_suggestion",
        ),
        (
            None,
            8.0,
            ComparisonStatus::Accepted,
            "sink_empty_accepts_suggestion",
        ),
        (
            None,
            8.5,
            ComparisonStatus::Accepted,
            "sink_empty_accepts_suggestion",
        ),
    ] {
        let snapshot = snapshot(SnapshotState::Present {
            shelf_type: "complete".into(),
            rating_grade: rating,
        });
        let result = compare(
            &rating_profile(),
            &observation(ScalarValue::Number(source)),
            &snapshot,
        )
        .unwrap();
        assert_eq!(result.profile_id, "profile");
        assert_eq!(result.profile_version, 7);
        assert_eq!(result.fields[0].status, expected_status);
        assert_eq!(result.fields[0].reason, expected_reason);
        assert!(result.fields[0].suggestion == Some(ScalarValue::Number(source)));
        match rating {
            Some(current) => assert!(matches!(
                &result.fields[0].sink,
                SinkValue::Known(ScalarValue::Number(actual)) if *actual == f64::from(current)
            )),
            None => assert!(matches!(&result.fields[0].sink, SinkValue::Empty)),
        }
    }
}

#[test]
fn every_supported_present_shelf_can_compare_equal() {
    for shelf in ["wishlist", "progress", "complete", "dropped"] {
        let profile = MappingProfile {
            rules: vec![MappingRule {
                id: "shelf".into(),
                enabled: true,
                target: "shelf_type".into(),
                operation: Operation::Condition {
                    from: "rating".into(),
                    test: Predicate::Exists,
                    value: ScalarValue::Text(shelf.into()),
                },
            }],
            ..rating_profile()
        };
        let snapshot = snapshot(SnapshotState::Present {
            shelf_type: shelf.into(),
            rating_grade: None,
        });
        let result = compare(&profile, &observation(ScalarValue::Number(8.0)), &snapshot).unwrap();
        assert_eq!(result.fields[0].status, ComparisonStatus::Skipped);
        assert_eq!(result.fields[0].reason, "already_equal");
    }
}

#[test]
fn missing_shelf_is_empty_only_for_shelf_and_rating_stays_unknown() {
    let without_shelf = compare(
        &rating_profile(),
        &observation(ScalarValue::Number(8.0)),
        &snapshot(SnapshotState::MissingShelf),
    )
    .unwrap();
    assert!(matches!(
        field(&without_shelf, "rating_grade").sink,
        SinkValue::Unknown
    ));
    assert_eq!(
        field(&without_shelf, "rating_grade").reason,
        "rating_requires_shelf"
    );

    // Put the shelf rule after rating to prove readiness is order-independent.
    let mut profile = rating_profile();
    profile.rules.push(MappingRule {
        id: "completion".into(),
        enabled: true,
        target: "shelf_type".into(),
        operation: Operation::Condition {
            from: "rating".into(),
            test: Predicate::Exists,
            value: ScalarValue::Text("complete".into()),
        },
    });
    let result = compare(
        &profile,
        &observation(ScalarValue::Number(8.0)),
        &snapshot(SnapshotState::MissingShelf),
    )
    .unwrap();
    assert_eq!(
        field(&result, "shelf_type").status,
        ComparisonStatus::Accepted
    );
    assert!(matches!(
        &field(&result, "shelf_type").sink,
        SinkValue::Empty
    ));
    assert_eq!(
        field(&result, "rating_grade").status,
        ComparisonStatus::NeedsReview
    );
    assert_eq!(field(&result, "rating_grade").reason, "sink_state_unknown");
    assert!(matches!(
        &field(&result, "rating_grade").sink,
        SinkValue::Unknown
    ));

    let mut reversed = profile.clone();
    reversed.rules.reverse();
    let reversed_result = compare(
        &reversed,
        &observation(ScalarValue::Number(8.0)),
        &snapshot(SnapshotState::MissingShelf),
    )
    .unwrap();
    for target in ["rating_grade", "shelf_type"] {
        assert_eq!(
            field(&result, target).status,
            field(&reversed_result, target).status
        );
        assert_eq!(
            field(&result, target).reason,
            field(&reversed_result, target).reason
        );
    }
}

#[test]
fn evaluator_validation_skip_and_rejection_are_preserved_without_fake_values() {
    let snapshot = snapshot(SnapshotState::Present {
        shelf_type: "complete".into(),
        rating_grade: Some(8),
    });
    let mut invalid = rating_profile();
    invalid.version = 0;
    assert_eq!(
        compare(&invalid, &observation(ScalarValue::Number(8.0)), &snapshot)
            .err()
            .unwrap()
            .code,
        "mapping_version_invalid"
    );

    let mut skipped = rating_profile();
    skipped.rules[0].enabled = false;
    let result = compare(&skipped, &observation(ScalarValue::Number(8.0)), &snapshot).unwrap();
    assert_eq!(result.fields[0].status, ComparisonStatus::Skipped);
    assert_eq!(result.fields[0].reason, "rule_disabled");
    assert!(result.fields[0].suggestion.is_none());

    let rejected = compare(
        &rating_profile(),
        &observation(ScalarValue::Text("eight".into())),
        &snapshot,
    )
    .unwrap();
    assert_eq!(rejected.fields[0].status, ComparisonStatus::Rejected);
    assert_eq!(rejected.fields[0].reason, "mapped_value_invalid");
    assert!(rejected.fields[0].suggestion.is_none());

    let mut missing = observation(ScalarValue::Number(8.0));
    missing.state.clear();
    let skipped = compare(&rating_profile(), &missing, &snapshot).unwrap();
    assert_eq!(skipped.fields[0].status, ComparisonStatus::Skipped);
    assert_eq!(skipped.fields[0].reason, "source_missing");
    assert!(skipped.fields[0].suggestion.is_none());
    assert!(matches!(
        &skipped.fields[0].sink,
        SinkValue::Known(ScalarValue::Number(value)) if *value == 8.0
    ));
}
