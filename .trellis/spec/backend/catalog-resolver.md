# Native catalog resolver

## 1. Scope / Trigger

Implemented in `collector-core/src/catalog_resolver.rs`; runtime adapter in
`collector-server/src/resolution_control.rs`; IMDb expectation validation in
`collector-connectors/src/imdb.rs`. These are internal components, not HTTP routes
or a running job service. Search fallback and manual selection remain pending.

## 2. Signatures

- `resolve_native(&dyn NeoDbCatalogTransport, &impl ResolutionControl, base: &str,
  expected: &CatalogIdentity) -> Result<Resolution, ApiError>`
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
Resolution is Matched(item), NeedsReview(item, reason), Unsupported, RateLimited
or Unavailable. None authorizes a personal-state write or activates a mapping.

## 3. Contracts

- Core depends only on domain/std. Runtime waits/cancellation remain behind the
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

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Empty expectation or inconsistent IMDb input | catalog_identity_invalid, no I/O |
| Budget reached/overflow | catalog_resolution_timeout |
| Explicit token cancellation or dropped owner guard | catalog_resolution_cancelled |
| Model/media mismatch | NeedsReview: item_granularity_mismatch |
| Expected scheme has contradictory IDs | NeedsReview: item_identity_conflict |
| Expected scheme has no matching ID | NeedsReview: item_identity_missing |
| Terminal fetch outcome or adapter error | Preserve outcome/error; no retry/fallback |

## 5. Good / Base / Bad Cases

Good: pending response at 3s, next fetch no earlier than 18s, exact show detail
matches. Base: unsupported link returns Unsupported for later explicit fallback.
Bad: accept a season because it carries the show's IMDb ID; or timeout only
between requests while a network future can remain pending forever.

## 6. Tests Required

`collector-core/src/catalog_resolver/tests.rs` manually drives fake monotonic time:
poll spacing, eight-attempt exhaustion, cancellation before/during fetch/detail/
wait, dropping hung futures, late response, terminal outcomes and evidence cases.
Server paused-time tests verify timer advancement, actual cancellation wakeups,
dropping hung work and cancellation/deadline precedence over ready responses.
IMDb tests reject inconsistent URL/ID/type before producing expectations.
Run `pnpm verify`; no real-account evidence is implied by mocks.

## 7. Wrong vs Correct

Wrong: await fetch, then check elapsed time, allowing hung I/O past the deadline.
Correct: let the runtime adapter use Tokio timeout/select and CancellationToken,
dropping unfinished work. Keep the same absolute deadline across all phases.
