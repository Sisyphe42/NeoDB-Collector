use super::*;
use collector_core::{
    access::{AccessRecord, AccessRepository, ChallengeMatch, Clock, Endpoint, SecretStore},
    catalog::{CatalogFetch, NeoDbCatalogSearchTransport, NeoDbCatalogTransport},
    catalog_resolver::{CatalogIdentity, ResolutionControl},
    mapping::{MappingRepository, imdb_default},
    marks::{MarkRead, MarkSnapshot, NeoDbMarkTransport},
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
    preview::{PreviewSnapshotService, SinkValue},
    preview_jobs::{PreviewJobRepository, PreviewJobStatus, PreviewSubmission},
    preview_records::{
        PreviewRecordOutcome, PreviewRecordRequest, PreviewRecordService, PreviewResolutionMode,
    },
    preview_results::{
        PreviewResultRepository, PreviewResultWrite, StoredPreviewOutcome, StoredPreviewResult,
    },
    secrets::SecretString,
};
use collector_domain::{
    access::PairedClient,
    mapping::MappingProfile,
    observation::{Observation, ObservedItem, ScalarValue},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

fn binding(reference: &str) -> Endpoint {
    Endpoint {
        base_url: "https://neodb.example".into(),
        account: "alice".into(),
        credential_ref: reference.into(),
    }
}

fn observation(id: &str, rating: u8) -> Observation {
    Observation {
        schema_version: 1,
        source_connector_id: "imdb:ratings_csv".into(),
        source_record_id: id.into(),
        observed_at: "2026-10-01T00:00:00Z".into(),
        idempotency_key: format!("imdb:ratings_csv:{:0>64}", id.trim_start_matches("tt")),
        item: ObservedItem {
            media_type: "tv".into(),
            canonical_url: format!("https://www.imdb.com/title/{id}/"),
            external_ids: BTreeMap::from([("imdb:title".into(), id.into())]),
            title_hint: Some("Exact title".into()),
        },
        state: BTreeMap::from([("rating".into(), ScalarValue::Number(f64::from(rating)))]),
    }
}

fn item(ids: Vec<(String, String)>) -> CatalogItem {
    CatalogItem {
        uuid: "Resolved123".into(),
        canonical_url: "https://neodb.example/tv/Resolved123".into(),
        detail_url: "https://neodb.example/api/tv/Resolved123".into(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        parent_uuid: Some("Parent123".into()),
        external_ids: ids,
    }
}

struct Repo(Mutex<Option<Endpoint>>);
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
        Box::pin(async move { Ok(self.0.lock().unwrap().clone()) })
    }
    fn set_endpoint(&self, _: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
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
struct WallClock;
impl Clock for WallClock {
    fn now(&self) -> i64 {
        110
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
        Box::pin(async {
            Ok(OAuthIdentity {
                username: "alice".into(),
            })
        })
    }
}

enum Mark {
    Present(Option<u8>),
    Missing,
}
struct Marks(Mutex<Option<Mark>>);
impl NeoDbMarkTransport for Marks {
    fn read_mark<'a>(
        &'a self,
        _: String,
        _: &'a SecretString,
        _: String,
        _: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async move {
            Ok(match self.0.lock().unwrap().take().unwrap() {
                Mark::Present(rating) => MarkRead::Present(Box::new(MarkSnapshot {
                    item: item(vec![]),
                    owner: "alice".into(),
                    shelf_type: "complete".into(),
                    rating_grade: rating,
                })),
                Mark::Missing => MarkRead::MissingShelf,
            })
        })
    }
}

#[derive(Clone, Copy)]
enum ResolutionKind {
    Matched,
    NeedsReview,
    Search,
    Unsupported,
    RateLimited,
    Unavailable,
}
struct Transport {
    kind: ResolutionKind,
    evidence: Mutex<Option<CatalogItem>>,
}
impl NeoDbCatalogTransport for Transport {
    fn fetch(&self, _: String, _: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            Ok(match self.kind {
                ResolutionKind::Matched | ResolutionKind::NeedsReview => {
                    CatalogFetch::Detail("/api/tv/Resolved123".into())
                }
                ResolutionKind::Search | ResolutionKind::Unsupported => CatalogFetch::Unsupported,
                ResolutionKind::RateLimited => CatalogFetch::RateLimited,
                ResolutionKind::Unavailable => CatalogFetch::Unavailable,
            })
        })
    }
    fn detail(&self, _: String, _: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move { Ok(self.evidence.lock().unwrap().take().unwrap()) })
    }
}
impl NeoDbCatalogSearchTransport for Transport {
    fn search(&self, _: String, _: String, _: String) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async {
            Ok(CatalogSearchPage {
                candidates: vec![CatalogSearchCandidate {
                    title: "Exact title".into(),
                    item: item(vec![("imdb:title".into(), "tt1234567".into())]),
                }],
                more_available: false,
            })
        })
    }
}
struct Control;
impl ResolutionControl for Control {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
    fn wait_until(&self, _: Duration) -> PortFuture<'_, ()> {
        Box::pin(async { unreachable!() })
    }
    fn run<'a, T: Send + 'a>(&'a self, _: Duration, work: PortFuture<'a, T>) -> PortFuture<'a, T> {
        work
    }
}

async fn produce(
    profile: &MappingProfile,
    observation: &Observation,
    kind: ResolutionKind,
    mark: Mark,
    manual: bool,
) -> collector_core::preview_records::PreviewRecord {
    let ids = if matches!(kind, ResolutionKind::NeedsReview) {
        vec![]
    } else {
        vec![
            ("imdb:title".into(), "tt1234567".into()),
            ("tmdb:tv".into(), "456".into()),
        ]
    };
    produce_with_ids(profile, observation, kind, mark, manual, ids).await
}

async fn produce_with_ids(
    profile: &MappingProfile,
    observation: &Observation,
    kind: ResolutionKind,
    mark: Mark,
    manual: bool,
    ids: Vec<(String, String)>,
) -> collector_core::preview_records::PreviewRecord {
    let query = matches!(kind, ResolutionKind::Search).then_some("Exact title");
    produce_with_ids_and_query(profile, observation, kind, mark, manual, ids, query).await
}

async fn produce_with_ids_and_query(
    profile: &MappingProfile,
    observation: &Observation,
    kind: ResolutionKind,
    mark: Mark,
    manual: bool,
    ids: Vec<(String, String)>,
    search_query: Option<&str>,
) -> collector_core::preview_records::PreviewRecord {
    let snapshots = Arc::new(PreviewSnapshotService::new(
        Arc::new(Repo(Mutex::new(Some(binding("credential-a"))))),
        Arc::new(Secrets),
        Arc::new(WallClock),
        Arc::new(Identity),
        Arc::new(Marks(Mutex::new(Some(mark)))),
    ));
    let service = PreviewRecordService::new(snapshots);
    let transport = Transport {
        kind,
        evidence: Mutex::new(Some(item(ids))),
    };
    let identity = CatalogIdentity {
        source_url: observation.item.canonical_url.clone(),
        media_type: "tv".into(),
        model: "TVShow".into(),
        scheme: "imdb:title".into(),
        external_id: observation.source_record_id.clone(),
    };
    let expected = binding("credential-a");
    let mode = if manual {
        PreviewResolutionMode::Manual {
            reference: "/api/tv/Resolved123",
        }
    } else if let Some(query) = search_query {
        PreviewResolutionMode::Search { query }
    } else {
        PreviewResolutionMode::Native
    };
    service
        .preview(
            &transport,
            &Control,
            PreviewRecordRequest {
                expected: &expected,
                profile,
                observation,
                identity: &identity,
                mode,
            },
        )
        .await
        .unwrap()
}

async fn setup() -> (tempfile::TempDir, Storage, MappingProfile) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    storage
        .set_endpoint(Some(binding("credential-a")))
        .await
        .unwrap();
    let profile = imdb_default();
    storage.save(profile.clone(), 0).await.unwrap();
    (dir, storage, profile)
}

async fn running(storage: &Storage, id: &str, profile: MappingProfile, values: Vec<Observation>) {
    storage
        .submit(
            PreviewSubmission::new(
                id.into(),
                binding("credential-a"),
                profile,
                values,
                100,
                1_000,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(storage.claim(id.into(), 1, 105).await.unwrap());
}

async fn running_with_search(
    storage: &Storage,
    id: &str,
    profile: MappingProfile,
    values: Vec<Observation>,
    query: &str,
) {
    let submission = PreviewSubmission::new(
        id.into(),
        binding("credential-a"),
        profile,
        values,
        100,
        1_000,
    )
    .unwrap()
    .with_search_queries(vec![(0, query.into())])
    .unwrap();
    storage.submit(submission).await.unwrap();
    assert!(storage.claim(id.into(), 1, 105).await.unwrap());
}

#[tokio::test]
async fn real_producer_results_round_trip_complete_and_reopen() {
    let (dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running(
        &storage,
        "job:roundtrip",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    let first = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    let duplicate = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    let (first, duplicate) = tokio::join!(
        storage.store(PreviewResultWrite::new("job:roundtrip".into(), 0, 1, 120, first).unwrap()),
        storage
            .store(PreviewResultWrite::new("job:roundtrip".into(), 0, 1, 120, duplicate).unwrap()),
    );
    assert_ne!(first.unwrap(), duplicate.unwrap());
    let (first, second) = tokio::join!(
        storage.complete("job:roundtrip".into(), 1, 130),
        storage.complete("job:roundtrip".into(), 1, 130),
    );
    assert_ne!(first.unwrap(), second.unwrap());
    assert_eq!(
        PreviewJobRepository::load(&storage, "job:roundtrip".into())
            .await
            .unwrap()
            .unwrap()
            .status(),
        PreviewJobStatus::Completed
    );
    storage.close().await;
    let storage = Storage::open(dir.path()).await.unwrap();
    let values = storage
        .results("job:roundtrip".into(), 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(values.len(), 1);
    assert!(matches!(
        values[0].outcome(),
        StoredPreviewOutcome::Compared {
            provenance: PreviewRecordProvenance::Automatic,
            ..
        }
    ));
    assert!(
        storage
            .results("job:roundtrip".into(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let json: String =
        sqlx::query_scalar("SELECT result_json FROM preview_results WHERE job_id='job:roundtrip'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    for forbidden in [
        "credential-a",
        "idempotencyKey",
        "observation",
        "rules",
        "operation",
        "token",
    ] {
        assert!(!json.contains(forbidden));
    }
    storage.close().await;
}

#[tokio::test]
async fn every_outcome_and_comparison_state_remains_normalized_history() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    let cases = [
        (
            "equal",
            ResolutionKind::Matched,
            Mark::Present(Some(8)),
            false,
        ),
        ("empty", ResolutionKind::Matched, Mark::Present(None), false),
        (
            "conflict",
            ResolutionKind::Matched,
            Mark::Present(Some(7)),
            false,
        ),
        ("unknown", ResolutionKind::Matched, Mark::Missing, false),
        (
            "manual",
            ResolutionKind::Matched,
            Mark::Present(Some(8)),
            true,
        ),
        (
            "review",
            ResolutionKind::NeedsReview,
            Mark::Present(Some(8)),
            false,
        ),
        (
            "search",
            ResolutionKind::Search,
            Mark::Present(Some(8)),
            false,
        ),
        (
            "unsupported",
            ResolutionKind::Unsupported,
            Mark::Present(Some(8)),
            false,
        ),
        (
            "rate",
            ResolutionKind::RateLimited,
            Mark::Present(Some(8)),
            false,
        ),
        (
            "unavailable",
            ResolutionKind::Unavailable,
            Mark::Present(Some(8)),
            false,
        ),
    ];
    for (name, kind, mark, manual) in cases {
        let id = format!("job:{name}");
        if matches!(kind, ResolutionKind::Search) {
            running_with_search(
                &storage,
                &id,
                profile.clone(),
                vec![observed.clone()],
                "Exact title",
            )
            .await;
        } else {
            running(&storage, &id, profile.clone(), vec![observed.clone()]).await;
        }
        let record = produce(&profile, &observed, kind, mark, manual).await;
        let expected_compared = match record.outcome() {
            PreviewRecordOutcome::Compared(value) => Some((
                value.snapshot.item().uuid.clone(),
                value.snapshot.item().canonical_url.clone(),
                value.snapshot.item().detail_url.clone(),
                value.snapshot.item().parent_uuid.clone(),
                value.snapshot.item().external_ids.clone(),
                value.snapshot.observed_at(),
                value.comparison.clone(),
                value.provenance,
            )),
            _ => None,
        };
        let expected_review = match record.outcome() {
            PreviewRecordOutcome::NeedsReview { item, reason } => Some((
                item.uuid.clone(),
                item.canonical_url.clone(),
                item.detail_url.clone(),
                item.parent_uuid.clone(),
                item.external_ids.clone(),
                *reason,
            )),
            _ => None,
        };
        let expected_search = match record.outcome() {
            PreviewRecordOutcome::SearchCandidates(page) => Some((
                page.candidates[0].title.clone(),
                page.candidates[0].item.uuid.clone(),
                page.candidates[0].item.external_ids.clone(),
                page.more_available,
            )),
            _ => None,
        };
        assert!(
            storage
                .store(PreviewResultWrite::new(id.clone(), 0, 1, 120, record).unwrap())
                .await
                .unwrap()
        );
        let rows = storage.results(id, 1, 0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ordinal(), 0);
        assert_eq!(rows[0].attempt(), 1);
        assert_eq!(rows[0].committed_at(), 120);
        assert_eq!(rows[0].source_connector_id(), "imdb:ratings_csv");
        assert_eq!(rows[0].source_record_id(), "tt1234567");
        assert_eq!(rows[0].source_fingerprint(), observed.idempotency_key);
        assert_eq!(rows[0].profile_id(), profile.id);
        assert_eq!(rows[0].profile_version(), profile.version);
        assert_eq!(rows[0].base_url(), "https://neodb.example");
        assert_eq!(rows[0].account(), "alice");
        if let (
            Some(expected),
            StoredPreviewOutcome::Compared {
                item,
                observed_at,
                comparison,
                provenance,
            },
        ) = (&expected_compared, rows[0].outcome())
        {
            assert!(
                (
                    &item.uuid,
                    &item.canonical_url,
                    &item.detail_url,
                    &item.parent_uuid,
                    &item.external_ids,
                    observed_at,
                    comparison,
                    provenance
                ) == (
                    &expected.0,
                    &expected.1,
                    &expected.2,
                    &expected.3,
                    &expected.4,
                    &expected.5,
                    &expected.6,
                    &expected.7
                )
            );
        }
        if let (Some(expected), StoredPreviewOutcome::NeedsReview { item, reason }) =
            (&expected_review, rows[0].outcome())
        {
            assert_eq!(
                (
                    &item.uuid,
                    &item.canonical_url,
                    &item.detail_url,
                    &item.parent_uuid,
                    &item.external_ids,
                    reason.as_str()
                ),
                (
                    &expected.0,
                    &expected.1,
                    &expected.2,
                    &expected.3,
                    &expected.4,
                    expected.5
                )
            );
        }
        if let (Some(expected), StoredPreviewOutcome::SearchCandidates(page)) =
            (&expected_search, rows[0].outcome())
        {
            assert_eq!(
                (
                    &page.candidates[0].title,
                    &page.candidates[0].item.uuid,
                    &page.candidates[0].item.external_ids,
                    page.more_available
                ),
                (&expected.0, &expected.1, &expected.2, expected.3)
            );
        }
        match name {
            "equal" => assert!(
                matches!(rows[0].outcome(), StoredPreviewOutcome::Compared { comparison, .. } if comparison.fields[0].reason == "already_equal")
            ),
            "empty" => assert!(
                matches!(rows[0].outcome(), StoredPreviewOutcome::Compared { comparison, .. } if matches!(comparison.fields[0].sink, SinkValue::Empty))
            ),
            "conflict" => assert!(
                matches!(rows[0].outcome(), StoredPreviewOutcome::Compared { comparison, .. } if comparison.fields[0].reason == "sink_conflict_needs_review")
            ),
            "unknown" => assert!(
                matches!(rows[0].outcome(), StoredPreviewOutcome::Compared { comparison, .. } if matches!(comparison.fields[0].sink, SinkValue::Unknown))
            ),
            "manual" => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::Compared {
                    provenance: PreviewRecordProvenance::Manual,
                    ..
                }
            )),
            "review" => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::NeedsReview { .. }
            )),
            "search" => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::SearchCandidates(_)
            )),
            "unsupported" => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::Unsupported
            )),
            "rate" => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::RateLimited
            )),
            _ => assert!(matches!(
                rows[0].outcome(),
                StoredPreviewOutcome::Unavailable
            )),
        }
    }
    storage.close().await;
}

#[tokio::test]
async fn exact_input_and_live_binding_guards_reject_forged_or_late_records() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running(
        &storage,
        "job:proof",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;

    let mut changed_observation = observed.clone();
    changed_observation
        .state
        .insert("rating".into(), ScalarValue::Number(9.0));
    let record = produce(
        &profile,
        &changed_observation,
        ResolutionKind::Matched,
        Mark::Present(Some(9)),
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:proof".into(), 0, 1, 120, record).unwrap())
            .await
            .unwrap()
    );

    let mut changed_profile = profile.clone();
    changed_profile.rules[0].enabled = false;
    let record = produce(
        &changed_profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:proof".into(), 0, 1, 120, record).unwrap())
            .await
            .unwrap()
    );

    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    storage
        .set_endpoint(Some(binding("credential-b")))
        .await
        .unwrap();
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:proof".into(), 0, 1, 120, record).unwrap())
            .await
            .unwrap()
    );

    storage
        .set_endpoint(Some(binding("credential-a")))
        .await
        .unwrap();
    for (index, endpoint) in [
        None,
        Some(Endpoint {
            base_url: "https://other.example".into(),
            ..binding("credential-a")
        }),
        Some(Endpoint {
            account: "bob".into(),
            ..binding("credential-a")
        }),
        Some(binding("credential-b")),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("job:complete-binding-{index}");
        running(&storage, &id, profile.clone(), vec![observed.clone()]).await;
        let record = produce(
            &profile,
            &observed,
            ResolutionKind::Unsupported,
            Mark::Missing,
            false,
        )
        .await;
        assert!(
            storage
                .store(PreviewResultWrite::new(id.clone(), 0, 1, 120, record).unwrap())
                .await
                .unwrap()
        );
        storage.set_endpoint(endpoint).await.unwrap();
        assert!(!storage.complete(id, 1, 130).await.unwrap());
        storage
            .set_endpoint(Some(binding("credential-a")))
            .await
            .unwrap();
    }
    storage.close().await;
}

#[tokio::test]
async fn result_write_requires_the_exact_stored_search_query_proof() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running_with_search(
        &storage,
        "job:search-proof",
        profile.clone(),
        vec![observed.clone()],
        "Exact title",
    )
    .await;

    let native = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:search-proof".into(), 0, 1, 110, native).unwrap())
            .await
            .unwrap()
    );

    let manual = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        true,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:search-proof".into(), 0, 1, 110, manual).unwrap())
            .await
            .unwrap()
    );

    let different = produce_with_ids_and_query(
        &profile,
        &observed,
        ResolutionKind::Search,
        Mark::Missing,
        false,
        vec![("imdb:title".into(), "tt1234567".into())],
        Some("Different title"),
    )
    .await;
    assert!(
        !storage
            .store(
                PreviewResultWrite::new("job:search-proof".into(), 0, 1, 110, different).unwrap()
            )
            .await
            .unwrap()
    );

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:search-proof'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    let matching = produce_with_ids_and_query(
        &profile,
        &observed,
        ResolutionKind::Search,
        Mark::Missing,
        false,
        vec![("imdb:title".into(), "tt1234567".into())],
        Some(" Exact title "),
    )
    .await;
    let matching = PreviewResultWrite::new("job:search-proof".into(), 0, 1, 110, matching).unwrap();
    assert_eq!(matching.search_query_proof(), Some("Exact title"));
    assert!(storage.store(matching).await.unwrap());
    let json: String = sqlx::query_scalar(
        "SELECT result_json FROM preview_results WHERE job_id='job:search-proof'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert!(!json.contains("searchQuery"));
    assert!(!json.contains("search_query"));
    sqlx::query(
        "UPDATE preview_observations SET search_query=' padded ' WHERE job_id='job:search-proof'",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        storage
            .results("job:search-proof".into(), 1, 0, 100)
            .await
            .err()
            .unwrap()
            .code,
        "database_unavailable"
    );
    let late = produce(
        &profile,
        &observed,
        ResolutionKind::Search,
        Mark::Missing,
        false,
    )
    .await;
    assert_eq!(
        storage
            .store(PreviewResultWrite::new("job:search-proof".into(), 0, 1, 111, late).unwrap())
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    assert_eq!(
        storage
            .complete("job:search-proof".into(), 1, 120)
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    let job = PreviewJobRepository::load(&storage, "job:search-proof".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Running);
    assert_eq!(job.updated_at(), 110);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:search-proof'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    storage.close().await;
}

#[tokio::test]
async fn incomplete_empty_resume_and_late_attempt_transitions_are_guarded() {
    let (_dir, storage, profile) = setup().await;
    let first = observation("tt1234567", 8);
    let second = observation("tt1234568", 9);
    running(
        &storage,
        "job:partial",
        profile.clone(),
        vec![first.clone(), second.clone()],
    )
    .await;
    let record = produce(
        &profile,
        &first,
        ResolutionKind::Unsupported,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert!(
        storage
            .store(PreviewResultWrite::new("job:partial".into(), 0, 1, 120, record).unwrap())
            .await
            .unwrap()
    );
    assert!(
        !storage
            .complete("job:partial".into(), 1, 130)
            .await
            .unwrap()
    );
    assert!(storage.fail("job:partial".into(), 1, 130).await.unwrap());
    assert_eq!(
        storage
            .results("job:partial".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    let late = produce(
        &profile,
        &second,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:partial".into(), 1, 1, 135, late).unwrap())
            .await
            .unwrap()
    );
    assert!(
        !storage
            .complete("job:partial".into(), 1, 135)
            .await
            .unwrap()
    );
    assert_eq!(
        storage
            .results("job:partial".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage.resume("job:partial".into(), 1, 140).await.unwrap(),
        Some(2)
    );
    assert!(
        storage
            .results("job:partial".into(), 2, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:partial'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    let late = produce(
        &profile,
        &first,
        ResolutionKind::Unsupported,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:partial".into(), 0, 1, 150, late).unwrap())
            .await
            .unwrap()
    );

    running(&storage, "job:empty", profile, vec![]).await;
    assert!(storage.complete("job:empty".into(), 1, 120).await.unwrap());

    let profile = imdb_default();
    running(&storage, "job:time", profile.clone(), vec![first.clone()]).await;
    let early = produce(
        &profile,
        &first,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:time".into(), 0, 1, 104, early).unwrap())
            .await
            .unwrap()
    );
    let expired = produce(
        &profile,
        &first,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:time".into(), 0, 1, 1_000, expired).unwrap())
            .await
            .unwrap()
    );
    let valid = produce(
        &profile,
        &first,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(PreviewResultWrite::new("job:time".into(), 0, 1, 120, valid).unwrap())
            .await
            .unwrap()
    );
    assert!(!storage.complete("job:time".into(), 1, 119).await.unwrap());
    assert!(!storage.complete("job:time".into(), 1, 1_000).await.unwrap());
    storage.close().await;
}

#[tokio::test]
async fn corrupt_rows_and_failed_insert_fail_closed_without_advancing_job() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running(
        &storage,
        "job:rollback-result",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    sqlx::raw_sql("CREATE TRIGGER fail_result BEFORE INSERT ON preview_results BEGIN SELECT RAISE(ABORT, 'test'); END;").execute(&storage.pool).await.unwrap();
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert_eq!(
        storage
            .store(
                PreviewResultWrite::new("job:rollback-result".into(), 0, 1, 120, record).unwrap()
            )
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    let job = PreviewJobRepository::load(&storage, "job:rollback-result".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.updated_at(), 105);
    sqlx::query("DROP TRIGGER fail_result")
        .execute(&storage.pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TRIGGER fail_result_timestamp BEFORE UPDATE OF updated_at ON preview_jobs WHEN NEW.id='job:rollback-result' AND NEW.updated_at=120 BEGIN SELECT RAISE(ABORT, 'test'); END;").execute(&storage.pool).await.unwrap();
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert_eq!(
        storage
            .store(
                PreviewResultWrite::new("job:rollback-result".into(), 0, 1, 120, record).unwrap()
            )
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_results WHERE job_id='job:rollback-result'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    let job = PreviewJobRepository::load(&storage, "job:rollback-result".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.updated_at(), 105);
    sqlx::query("DROP TRIGGER fail_result_timestamp")
        .execute(&storage.pool)
        .await
        .unwrap();
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
    )
    .await;
    assert!(
        storage
            .store(
                PreviewResultWrite::new("job:rollback-result".into(), 0, 1, 120, record).unwrap()
            )
            .await
            .unwrap()
    );
    sqlx::query("UPDATE preview_results SET result_json='{}' WHERE job_id='job:rollback-result'")
        .execute(&storage.pool)
        .await
        .unwrap();
    let error = match storage
        .results("job:rollback-result".into(), 1, 0, 100)
        .await
    {
        Ok(_) => panic!("corrupt result must fail closed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "database_unavailable");
    assert_eq!(
        storage
            .complete("job:rollback-result".into(), 1, 130)
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );

    let huge_ids = (0..17)
        .map(|index| (format!("scheme:{index:02}"), "x".repeat(4096)))
        .collect();
    running(
        &storage,
        "job:oversize",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    let huge = produce_with_ids(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
        huge_ids,
    )
    .await;
    assert_eq!(
        storage
            .store(PreviewResultWrite::new("job:oversize".into(), 0, 1, 120, huge).unwrap())
            .await
            .unwrap_err()
            .code,
        "preview_result_too_large"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:oversize'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);

    running(
        &storage,
        "job:escape-oversize",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    let escaped_ids = (0..9)
        .map(|index| (format!("escaped:{index:02}"), "\\\"".repeat(2048)))
        .collect();
    let escaped = produce_with_ids(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
        escaped_ids,
    )
    .await;
    assert_eq!(
        storage
            .store(
                PreviewResultWrite::new("job:escape-oversize".into(), 0, 1, 120, escaped).unwrap()
            )
            .await
            .unwrap_err()
            .code,
        "preview_result_too_large"
    );
    let escaped_job = PreviewJobRepository::load(&storage, "job:escape-oversize".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(escaped_job.updated_at(), 105);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_results WHERE job_id='job:escape-oversize'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);

    running(
        &storage,
        "job:extent-corrupt",
        profile.clone(),
        vec![observed.clone(), observed.clone()],
    )
    .await;
    let first = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(PreviewResultWrite::new("job:extent-corrupt".into(), 0, 1, 120, first).unwrap())
            .await
            .unwrap()
    );
    sqlx::query("UPDATE preview_results SET attempt=2 WHERE job_id='job:extent-corrupt'")
        .execute(&storage.pool)
        .await
        .unwrap();
    let second = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert_eq!(
        storage
            .store(PreviewResultWrite::new("job:extent-corrupt".into(), 1, 1, 121, second).unwrap())
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    sqlx::query("UPDATE preview_results SET attempt=1 WHERE job_id='job:extent-corrupt'")
        .execute(&storage.pool)
        .await
        .unwrap();
    let mut conn = storage.pool.acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("UPDATE preview_results SET ordinal=999 WHERE job_id='job:extent-corrupt'")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    let second = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert_eq!(
        storage
            .store(PreviewResultWrite::new("job:extent-corrupt".into(), 1, 1, 121, second).unwrap())
            .await
            .unwrap_err()
            .code,
        "database_unavailable"
    );
    storage.close().await;
}

#[tokio::test]
async fn terminal_delete_and_expiry_transitions_never_accept_late_results() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    for id in ["job:cancel", "job:interrupt", "job:delete", "job:expire"] {
        running(
            &storage,
            id,
            profile.clone(),
            vec![observed.clone(), observed.clone()],
        )
        .await;
        let record = produce(
            &profile,
            &observed,
            ResolutionKind::Unsupported,
            Mark::Missing,
            false,
        )
        .await;
        assert!(
            storage
                .store(PreviewResultWrite::new(id.into(), 0, 1, 120, record).unwrap())
                .await
                .unwrap()
        );
    }
    assert!(storage.cancel("job:cancel".into(), 1, 130).await.unwrap());
    let late = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        !storage
            .store(PreviewResultWrite::new("job:cancel".into(), 1, 1, 131, late).unwrap())
            .await
            .unwrap()
    );
    assert!(!storage.complete("job:cancel".into(), 1, 131).await.unwrap());
    assert_eq!(
        storage
            .results("job:cancel".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(storage.interrupt_running(130).await.unwrap(), 3);
    for id in ["job:interrupt", "job:delete"] {
        let late = produce(
            &profile,
            &observed,
            ResolutionKind::Unsupported,
            Mark::Missing,
            false,
        )
        .await;
        assert!(
            !storage
                .store(PreviewResultWrite::new(id.into(), 1, 1, 131, late).unwrap())
                .await
                .unwrap()
        );
        assert!(!storage.complete(id.into(), 1, 131).await.unwrap());
        assert_eq!(
            storage.results(id.into(), 1, 0, 100).await.unwrap().len(),
            1
        );
    }
    assert_eq!(
        storage
            .results("job:interrupt".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(storage.delete("job:delete".into()).await.unwrap());
    assert_eq!(storage.cleanup_expired(1_000).await.unwrap(), 3);
    for id in ["job:cancel", "job:interrupt", "job:delete", "job:expire"] {
        let record = produce(
            &profile,
            &observed,
            ResolutionKind::Unsupported,
            Mark::Missing,
            false,
        )
        .await;
        assert!(
            !storage
                .store(PreviewResultWrite::new(id.into(), 0, 1, 140, record).unwrap())
                .await
                .unwrap()
        );
    }
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_results WHERE job_id IN ('job:delete','job:expire')",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
    storage.close().await;
}

#[tokio::test]
async fn sparse_pagination_is_ordered_and_never_returns_a_partial_corrupt_page() {
    let (_dir, storage, profile) = setup().await;
    let observations = vec![
        observation("tt1234567", 8),
        observation("tt1234568", 9),
        observation("tt1234569", 10),
    ];
    running(&storage, "job:pages", profile.clone(), observations.clone()).await;
    for (ordinal, committed_at) in [(2, 120), (0, 121)] {
        let record = produce(
            &profile,
            &observations[ordinal as usize],
            ResolutionKind::Unsupported,
            Mark::Missing,
            false,
        )
        .await;
        assert!(
            storage
                .store(
                    PreviewResultWrite::new("job:pages".into(), ordinal, 1, committed_at, record)
                        .unwrap()
                )
                .await
                .unwrap()
        );
    }
    let first = storage.results("job:pages".into(), 1, 0, 1).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].ordinal(), 0);
    let second = storage
        .results("job:pages".into(), 1, 1, 100)
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].ordinal(), 2);
    let record = produce(
        &profile,
        &observations[1],
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(PreviewResultWrite::new("job:pages".into(), 1, 1, 122, record).unwrap())
            .await
            .unwrap()
    );
    let all = storage
        .results("job:pages".into(), 1, 0, 100)
        .await
        .unwrap();
    assert_eq!(
        all.iter()
            .map(StoredPreviewResult::ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    sqlx::query(
        "UPDATE preview_results SET result_json='{}' WHERE job_id='job:pages' AND ordinal=2",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    assert!(
        storage
            .results("job:pages".into(), 1, 0, 100)
            .await
            .is_err()
    );
    storage.close().await;
}

#[tokio::test]
async fn transition_failures_roll_back_results_and_lifecycle() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running(
        &storage,
        "job:resume-rollback",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(
                PreviewResultWrite::new("job:resume-rollback".into(), 0, 1, 120, record).unwrap()
            )
            .await
            .unwrap()
    );
    assert!(
        storage
            .fail("job:resume-rollback".into(), 1, 130)
            .await
            .unwrap()
    );
    sqlx::raw_sql("CREATE TRIGGER fail_result_delete BEFORE DELETE ON preview_results BEGIN SELECT RAISE(ABORT, 'test'); END;").execute(&storage.pool).await.unwrap();
    assert!(
        storage
            .resume("job:resume-rollback".into(), 1, 140)
            .await
            .is_err()
    );
    let job = PreviewJobRepository::load(&storage, "job:resume-rollback".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Failed);
    assert_eq!(job.attempt(), 1);
    assert_eq!(
        storage
            .results("job:resume-rollback".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(storage.delete("job:resume-rollback".into()).await.is_err());
    assert!(
        PreviewJobRepository::load(&storage, "job:resume-rollback".into())
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("DROP TRIGGER fail_result_delete")
        .execute(&storage.pool)
        .await
        .unwrap();

    running(
        &storage,
        "job:complete-rollback",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(
                PreviewResultWrite::new("job:complete-rollback".into(), 0, 1, 120, record).unwrap()
            )
            .await
            .unwrap()
    );
    sqlx::raw_sql("CREATE TRIGGER fail_completion BEFORE UPDATE OF status ON preview_jobs WHEN NEW.status='completed' BEGIN SELECT RAISE(ABORT, 'test'); END;").execute(&storage.pool).await.unwrap();
    assert!(
        storage
            .complete("job:complete-rollback".into(), 1, 130)
            .await
            .is_err()
    );
    let job = PreviewJobRepository::load(&storage, "job:complete-rollback".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Running);
    assert_eq!(job.updated_at(), 120);
    assert_eq!(
        storage
            .results("job:complete-rollback".into(), 1, 0, 100)
            .await
            .unwrap()
            .len(),
        1
    );
    storage.close().await;
}

#[tokio::test]
async fn exact_per_result_and_aggregate_byte_limits_are_enforced() {
    let (_dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    let mut ids: Vec<(String, String)> = (0..15)
        .map(|index| (format!("scheme:{index:02}"), "x".repeat(4096)))
        .collect();
    ids.push(("scheme:99".into(), "x".into()));
    let base = produce_with_ids(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
        ids.clone(),
    )
    .await;
    let base_write = PreviewResultWrite::new("job:size-probe".into(), 0, 1, 120, base).unwrap();
    let base_len = encode(&base_write).unwrap().len();
    let padding = MAX_PREVIEW_RESULT_BYTES.checked_sub(base_len).unwrap();
    assert!((1..4096).contains(&padding));
    ids.last_mut().unwrap().1 = "x".repeat(1 + padding);

    let observations = vec![observed.clone(); 513];
    running(&storage, "job:aggregate", profile.clone(), observations).await;
    let exact = produce_with_ids(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
        ids.clone(),
    )
    .await;
    let exact = PreviewResultWrite::new("job:aggregate".into(), 0, 1, 120, exact).unwrap();
    let exact_json = encode(&exact).unwrap();
    assert_eq!(exact_json.len(), MAX_PREVIEW_RESULT_BYTES);
    assert!(storage.store(exact).await.unwrap());
    let mut tx = storage.pool.begin().await.unwrap();
    for ordinal in 1..512_u32 {
        sqlx::query("INSERT INTO preview_results(job_id,ordinal,attempt,committed_at,result_json) VALUES('job:aggregate',?,1,120,?)")
            .bind(ordinal).bind(&exact_json).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:aggregate'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 512);
    let final_record = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    let error = storage
        .store(PreviewResultWrite::new("job:aggregate".into(), 512, 1, 121, final_record).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.code, "preview_results_too_large");
    let count_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:aggregate'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count_after, 512);

    running(
        &storage,
        "job:one-byte-over",
        profile.clone(),
        vec![observed.clone()],
    )
    .await;
    ids.last_mut().unwrap().1.push('x');
    let oversized = produce_with_ids(
        &profile,
        &observed,
        ResolutionKind::Matched,
        Mark::Present(Some(8)),
        false,
        ids,
    )
    .await;
    let error = storage
        .store(PreviewResultWrite::new("job:one-byte-over".into(), 0, 1, 120, oversized).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.code, "preview_result_too_large");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:one-byte-over'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    storage.close().await;
}

#[tokio::test]
async fn populated_exact_v4_schema_upgrades_without_losing_operational_state() {
    let (dir, storage, profile) = setup().await;
    storage
        .insert(AccessRecord {
            id: "client-v4".into(),
            hash: "hash-v4".into(),
            kind: "client".into(),
            audience: "http://127.0.0.1:19424".into(),
            name: "Preserved client".into(),
            client_kind: "cli".into(),
            expires_at: 9_999,
        })
        .await
        .unwrap();
    sqlx::raw_sql(
        "DROP TABLE preview_results; DROP TABLE preview_observations; DROP TABLE preview_jobs; DELETE FROM _sqlx_migrations WHERE version>=5;",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0004_preview_jobs.sql"))
        .execute(&storage.pool)
        .await
        .unwrap();
    let observed = observation("tt1234567", 8);
    let observation_json = serde_json::to_string(&observed).unwrap();
    sqlx::query("INSERT INTO preview_jobs(id,profile_id,profile_version,base_url,account,credential_ref,status,attempt,created_at,updated_at,expires_at,observation_count) VALUES(?,?,?,?,?,'credential-a','running',1,100,105,1000,1)")
        .bind("job:v4").bind(&profile.id).bind(profile.version).bind("https://neodb.example").bind("alice")
        .execute(&storage.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO preview_observations(job_id,ordinal,observation_json) VALUES('job:v4',0,?)",
    )
    .bind(&observation_json)
    .execute(&storage.pool)
    .await
    .unwrap();
    storage.close().await;

    let storage = Storage::open(dir.path()).await.unwrap();
    let latest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(latest, 6);
    let job = PreviewJobRepository::load(&storage, "job:v4".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Running);
    assert_eq!(job.profile_id(), profile.id);
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.created_at(), 100);
    assert_eq!(job.updated_at(), 105);
    assert_eq!(job.expires_at(), 1_000);
    assert_eq!(job.observation_count(), 1);
    assert!(job.is_bound_to(&binding("credential-a")));
    assert!(storage.observations("job:v4".into(), 0, 100).await.unwrap() == vec![observed]);
    assert!(
        MappingRepository::load(&storage, profile.id.clone(), Some(1))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .find(
                "hash-v4".into(),
                "client".into(),
                "http://127.0.0.1:19424".into(),
                200
            )
            .await
            .unwrap()
            .is_some()
    );
    assert!(storage.endpoint().await.unwrap().is_some());
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(foreign_keys, 1);
    let result_parent: String = sqlx::query_scalar(
        "SELECT \"table\" FROM pragma_foreign_key_list('preview_results') LIMIT 1",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(result_parent, "preview_observations");
    let observation_parent: String = sqlx::query_scalar(
        "SELECT \"table\" FROM pragma_foreign_key_list('preview_observations') LIMIT 1",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(observation_parent, "preview_jobs");
    let job_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='preview_jobs'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert!(job_sql.contains("'completed'"));
    let indexes: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='index' AND name IN ('preview_jobs_status_expiry','preview_results_attempt_ordinal') ORDER BY name").fetch_all(&storage.pool).await.unwrap();
    assert_eq!(
        indexes,
        vec![
            "preview_jobs_status_expiry",
            "preview_results_attempt_ordinal"
        ]
    );
    sqlx::query("INSERT INTO preview_results(job_id,ordinal,attempt,committed_at,result_json) VALUES('job:v4',0,1,110,'{}')").execute(&storage.pool).await.unwrap();
    sqlx::query("DELETE FROM preview_observations WHERE job_id='job:v4' AND ordinal=0")
        .execute(&storage.pool)
        .await
        .unwrap();
    let result_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM preview_results WHERE job_id='job:v4'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(result_count, 0);
    storage.close().await;
}

#[tokio::test]
async fn populated_exact_v5_schema_upgrades_native_rows_with_null_search_query() {
    let (dir, storage, profile) = setup().await;
    let observed = observation("tt1234567", 8);
    running(&storage, "job:v5", profile.clone(), vec![observed.clone()]).await;
    let record = produce(
        &profile,
        &observed,
        ResolutionKind::Unsupported,
        Mark::Missing,
        false,
    )
    .await;
    assert!(
        storage
            .store(PreviewResultWrite::new("job:v5".into(), 0, 1, 110, record).unwrap())
            .await
            .unwrap()
    );
    sqlx::raw_sql(
        "ALTER TABLE preview_observations DROP COLUMN search_query; DELETE FROM _sqlx_migrations WHERE version>=6;",
    )
    .execute(&storage.pool)
    .await
    .unwrap();
    storage.close().await;

    let reopened = Storage::open(dir.path()).await.unwrap();
    let latest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(latest, 6);
    let job = PreviewJobRepository::load(&reopened, "job:v5".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status(), PreviewJobStatus::Running);
    assert_eq!(job.profile_version(), 1);
    assert_eq!(job.expires_at(), 1_000);
    assert_eq!(
        reopened.search_query("job:v5".into(), 0).await.unwrap(),
        None
    );
    let rows = reopened.results("job:v5".into(), 1, 0, 100).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].outcome(),
        StoredPreviewOutcome::Unsupported
    ));
    reopened.close().await;
}
