# Catalog identity resolvers

## 1. Scope / Trigger

Implemented in `collector-core/src/catalog_resolver.rs`; runtime adapter in
`collector-server/src/resolution_control.rs`; IMDb expectation validation in
`collector-connectors/src/imdb.rs`. These are internal components, not HTTP routes
or a running job service. Explicit manual selection validation shares this module;
[bounded search fallback](./catalog-search.md) returns review-only candidates.
Cross-job manual mappings and user-facing selection remain pending. Immutable
per-job manual choices are owned by [Preview Jobs](./preview-jobs.md); they invoke
this same fresh-detail resolver and do not change matching policy.

## 2. Signatures

- `resolve_native(&dyn NeoDbCatalogTransport, &impl ResolutionControl, base: &str,
  expected: &CatalogIdentity) -> Result<Resolution, ApiError>`
- `resolve_manual(&dyn NeoDbCatalogTransport, &impl ResolutionControl, base: &str,
  expected: &CatalogIdentity, reference: &str) -> Result<Resolution, ApiError>`
- `ResolutionControl::now() -> Duration` and
  `wait_until(at: Duration) -> PortFuture<()>` use one monotonic epoch.
- `ResolutionControl::run<T>(deadline, PortFuture<T>) -> PortFuture<T>` owns
  runtime cancellation/deadline enforcement without exposing runtime types to Core.
- `TokioResolutionControl::new(CancellationToken) -> Self`; the caller retains
  a token clone and its standard `drop_guard()` for owner-drop cancellation.
- `imdb::catalog_identity(&ObservedItem) -> Result<CatalogIdentity, ApiError>`
  validates the expected source URL/ID/type without invoking any sink.

CatalogIdentity contains source_url, media_type, model, scheme and external_id.
It is trusted adapter output, not an externally deserializable authorization DTO.
Resolution is Matched(item), ManuallySelected(item), NeedsReview(item, reason),
Unsupported, RateLimited, Unavailable or SearchCandidates(page). None authorizes
a personal-state write or activates a mapping. ManuallySelected records selection provenance, not verified
external-ID equivalence or permission to execute.

## 3. Contracts

- Resolver logic uses domain/Core ports and std; reviewed pure dependencies elsewhere
  in Core follow the repository allowlist. Runtime waits/cancellation remain behind the
  control port; no wall-clock timestamps or Tokio types enter matching logic.
  This slice uses Tokio `timeout_at`, `sleep_until`, `select!` and tokio-util
  `CancellationToken`, not a custom production future race or watch-based token.
  Pre/post deadline checks preserve the stricter cutoff for immediately-ready work.
- The 120-second budget starts on entry and includes fetch, waits and detail I/O.
  Race each operation against the deadline/cancellation waiter. Check interruption
  before polling I/O and after a ready response: late success cannot start detail.
- Pending waits at least 15 seconds after receipt, not request start. With
  instantaneous responses this allows at most eight fetches (0..105s), never one
  at the 120s boundary. Terminal 429/404/422 and transport errors never retry.
- A control waiter must wake on cancellation, return Ok only at/after its target,
  and keep cancellation sticky. Concurrent ready cancellation/deadline beats I/O.
  Dropping the guarded future drops in-flight work; remote creation is not undone.
- IMDb expectations require valid `imdb:title`, its exact canonical URL and a
  supported movie/TV-show observation. The adapter supplies Movie/TVShow; Core
  compares open string values rather than branching on platform names.
- Automatic matching requires the expected media/model and at least one exact
  ID in the expected scheme, with no contradictory ID in that scheme. No title
  matching. Missing IDs and show/season/episode differences become NeedsReview.
- Transport still validates same-instance detail identity/path before Core sees
  the evidence. Unsupported and Unavailable do not mean a personal mark is absent.
- `resolve_manual` is called only for a trusted, explicit user selection; it is not
  an automatic response to missing identity or a transport failure. It accepts an
  API detail reference (for example `/api/movie/Ab123`), not an arbitrary browser
  page URL or previously fetched candidate payload. The existing transport rejects
  foreign origins and unsafe/non-detail paths before network access.
- Manual resolution validates the same source expectation, rejects an empty/blank
  reference, and makes one `detail` call under a checked 120-second deadline and
  sticky cancellation control. Adapter redirect, byte and ten-second I/O limits
  still apply. It never calls native fetch, polls, searches or retries a failure.
- Fresh exact identity or missing expected-scheme identity may return
  ManuallySelected. Contradictory expected-scheme IDs or different media/model
  return NeedsReview with the existing reason. Reuse the native classifier, not
  a separate platform-specific manual matching implementation. Native resolution
  still treats missing IDs as NeedsReview and never emits ManuallySelected.
- Preserve the entire normalized CatalogItem evidence, including final detail and
  canonical URLs, UUID/parent, model and identity claims. This is not persistence
  of a cross-job manual mapping. The job integration binds the compact selection
  to its ordinal/source/full endpoint/profile/expiry and rereads detail each attempt.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Empty expectation or inconsistent IMDb input | catalog_identity_invalid, no I/O |
| Budget reached/overflow | catalog_resolution_timeout |
| Explicit token cancellation or dropped owner guard | catalog_resolution_cancelled |
| Model/media mismatch | NeedsReview: item_granularity_mismatch |
| Expected scheme has contradictory IDs | NeedsReview: item_identity_conflict |
| Native resolution: expected scheme has no matching ID | NeedsReview: item_identity_missing |
| Native entrypoint terminal outcome or adapter error | Preserve outcome/error; no retry/fallback |
| Opt-in resolve_with_search receives Unsupported | One search under the existing deadline; see search contract |
| Empty/blank manual reference | neodb_catalog_reference_invalid before I/O |
| Manual reference rejected by transport | Preserve sanitized adapter error; no fallback |
| Manual exact/missing ID with compatible media/model | ManuallySelected, not Matched |
| Manual contradictory ID or media/model mismatch | NeedsReview, same reason as native |

## 5. Good / Base / Bad Cases

Good: pending response at 3s, next fetch no earlier than 18s, exact show detail
matches. Base: unsupported link returns Unsupported for later explicit fallback.
Bad: accept a season because it carries the show's IMDb ID; or timeout only
between requests while a network future can remain pending forever.

Manual good case: an explicitly chosen movie lacks IMDb evidence; fresh compatible
detail becomes ManuallySelected. Bad: selecting a season to satisfy a whole-show
observation or converting failed identity lookup into a manual success automatically.

## 6. Tests Required

`collector-core/src/catalog_resolver/tests.rs` manually drives fake monotonic time:
poll spacing, eight-attempt exhaustion, cancellation before/during fetch/detail/
wait, dropping hung futures, late response, terminal outcomes and evidence cases.
Server paused-time tests verify timer advancement, actual cancellation wakeups,
dropping hung work and cancellation/deadline precedence over ready responses.
IMDb tests reject inconsistent URL/ID/type before producing expectations.
Core manual tests cover explicit reference forwarding/evidence preservation,
exact/missing IDs, contradictions and granularity, no fetch, invalid input before
I/O, adapter failure, cancellation and timeout/hung/late detail. Existing adapter
tests continue to prove same-instance reference rejection before network access.
Run `pnpm verify`; no real-account evidence is implied by mocks.

## 7. Wrong vs Correct

Wrong: await fetch, then check elapsed time, allowing hung I/O past the deadline.
Correct: let the runtime adapter use Tokio timeout/select and CancellationToken,
dropping unfinished work. Keep the same absolute deadline across all phases.

Wrong: reinterpret NeedsReview as Matched when a candidate has a similar title.
Correct: only a separate explicit selection can invoke resolve_manual; retain its
ManuallySelected provenance and refuse conflicting identity/granularity.
