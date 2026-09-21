# Resumable preview design

Status: approved for implementation by user continuation on 2026-09-18.
Product scope is already approved in the parent and `prd.md`.

## Boundaries and existing owners

- Reuse `collector-connectors/src/imdb.rs` observations (`imdb:title` identity),
  the Core mapping evaluator, immutable profile revisions and singleton endpoint.
- Core owns resolution, comparison and job transitions through ports. It gains
  no HTTP, JSON, SQLite or asynchronous runtime dependency.
- NeoDB transport stays in `collector-connectors`; source adapters never invoke
  it. Separate catalog operations from OAuth and authenticated mark reads.
- Storage implements operational repositories; Server owns runtime scheduling,
  cancellation, authentication and HTTP composition. CLI and WebUI share the
  versioned API and generated contracts.
- No personal-state writer, activation or schedule-enablement endpoint is added.

### Internal secret-value follow-up

Library-reuse follow-up: migrate credential-store values, OAuth app secrets/access
tokens and internal authorization codes to `secrecy::SecretString`; keep API DTOs,
database metadata and state-machine semantics unchanged. Reexport the reviewed
types from Core instead of adding a wrapper. Deserialize upstream secret fields
directly; borrow tokens for identity reads, move them into storage, and expose only
at consumers. Acceptance includes Debug redaction and compile-fail serialization/
Display tests plus all existing rollback paths. See `internal-secrets.md` in the
backend specs. OAuth protocol replacement remains a separate evaluation.

## 1. Native catalog-fetch boundary (first checkpoint)

Add an internal catalog transport port with a single-attempt fetch operation and
a detail-read operation. Return normalized outcomes, not reqwest responses or
arbitrary redirect URLs. The adapter validates an IMDb canonical title link
against the supported built-in identity rule before any request. Additional
platform validators can be registered later without a closed Core platform enum.
Client-supplied observations are not trusted merely because they name a connector.

`GET /api/catalog/fetch?url=...` is public and receives no Bearer token. It may
create a public catalog entry; there is no force-refetch operation. Disable
automatic redirects. A 302 is accepted only after resolving and validating its
Location against the configured instance and an allowlisted catalog-detail path.
Reject userinfo, query/fragment, scheme/origin changes, encoded path escapes and
non-detail endpoints. Detail redirects receive the same checks, a bounded hop
limit and cycle detection. Bound response bytes and request duration; sanitize
upstream error bodies and URLs out of diagnostics.

Normalize 202 as pending, 302 as a validated detail reference, 422 as unsupported,
429 as rate-limited and 404 as unavailable. Transport failure, malformed replies
and missing identity are never evidence of item absence. Do not silently switch
to search after transient failures; preserve an actionable reason.

Keep scheduling separate from single-attempt transport: a Core resolver uses an
injected monotonic timing/wait/cancellation port. Pending polls are at least 15
seconds apart, with a total 120-second deadline including I/O. Runtime cancellation
or deadline expiry drops in-flight work and prevents further requests. Cancellation
does not undo catalog creation already queued remotely. No busy-loop retry on 429.

Implementation refinement approved on 2026-09-18: reuse Tokio timeout/select/sleep
and tokio-util CancellationToken/drop_guard in the runtime adapter. Core keeps
only a thin execution-control port and business rules, not a custom future race.

The first checkpoint contains only the internal port, restrictive transport and
mock tests; polling/resolution follows as a second checkpoint. Neither exposes a
public Core route, so neither requires speculative OpenAPI DTOs.

## 2. Identity resolution

Read detail fields needed for evidence only: canonical NeoDB item identity,
category/model granularity and supported external identities/URLs. Core compares
that evidence with the observation. Exact `imdb:title` plus compatible movie/show
granularity can bind automatically; a TV season cannot satisfy a TV-show record.
Contradictory IDs/types, missing evidence or multiple candidates require review.
Titles are hints, never uniqueness proofs.

Search is a bounded fallback for unsupported resolution, not a second unlimited
crawler. A title-only hit remains unconfirmed. Explicit manual selection must
validate the target on the same instance, retain its evidence and identify the
decision as manual; it cannot bypass supported media/field constraints.

Internal manual-selection checkpoint: `resolve_manual` accepts a validated source identity and
an explicitly selected same-instance catalog **API detail reference**, never cached
candidate metadata. It reads fresh detail under the existing cancellation/deadline
control and does not call fetch/search. Reuse the native classifier: exact identity
or missing expected-scheme identity can become `ManuallySelected`, but contradictory
expected-scheme IDs and media/model mismatches remain `NeedsReview`. Manual selection
must never be reported as an automatic `Matched` result. The entire normalized
CatalogItem is retained as evidence. No UI action, stored manual mapping, public
route or canonical-page-URL conversion is claimed in this checkpoint; those belong
to later integration. Bounded search fallback is defined below.

### Bounded search refinement

The official catalog API searches the local index without URL fetching or external
results. Request only page 1 with the expected movie/TV category and a nonblank,
control-free query of at most 512 UTF-8 bytes. A separate search port returns at
most 20 normalized candidates (title hint plus validated CatalogItem evidence),
and an advisory more-available flag from estimated pagination. No pagination loop.
Reuse same-instance detail validation and the 64 KiB / ten-second transport caps;
reject malformed/oversized pages rather than silently accepting partial evidence.

An opt-in `resolve_with_search` composition shares a single 120-second budget
across native polling/detail and fallback. Only Unsupported permits one search;
rate limits, unavailable, errors and NeedsReview never silently trigger it.
All candidates, even an exact-ID singleton, remain review-only in this slice.
An empty page is not proof of catalog or personal-mark absence. Explicit selection
still calls `resolve_manual` for fresh evidence. Existing native/manual calls keep
their semantics. No public API or persistence is introduced.

## 3. Comparison and snapshots

After resolution, read current personal state using the connected account's
credential through the existing secret-store port. Verify exact mark-read routes,
absence semantics and response shapes against primary API evidence before adding
that transport. Never convert forbidden, missing-item or network errors into an
empty personal mark.

Run the existing mapping evaluator, then compare accepted suggestions with a
normalized sink snapshot: equal => skipped, unequal nonempty => needs_review,
confirmed empty => suggestion. A rating without a valid existing or suggested
shelf is not ready. Results contain field-level source/sink values and reasons;
none is executable and no successful preview enables a profile.

Bind each job to endpoint/account/credential generation and an immutable mapping
profile ID/version. Never expose secret references to clients. Endpoint/account
replacement invalidates current work before results can be treated as current.
Snapshot timestamps explicitly distinguish historical review data from live state.

## 4. Jobs and minimal persistence

Proposed lifecycle: queued -> running -> completed/failed/cancelled; startup marks
unfinished queued/running work interrupted. Resume is explicit and creates a new
comparison attempt, retaining the original binding and re-reading remote state.
If the endpoint/account or chosen profile changes, require a new preview instead
of silently rebinding old results. Completed rows from an earlier attempt are not
silently reused as fresh comparisons.

Use ordered migrations for jobs and normalized rows/results. Persist only the
supported observation fields needed to resume, identity evidence, profile version,
account/endpoint snapshot, timestamps and bounded status/reason metadata. Never
persist CSV bytes, raw response JSON, unsupported values or plaintext secrets.
Insert validated imports atomically after enforcing existing CSV limits; failed
imports must not leave partial jobs. Conditional job transitions prevent two
workers from processing one attempt and prevent late completion after cancellation.

Keep worker concurrency bounded (initially one job at a time), with cancellable
backpressure, not a general workflow engine. Use stable row ordering and bounded
pagination. Deletion cancels local work before removing job data and prevents a
late worker from recreating it. Startup/periodic/manual cleanup removes expired
preview rows/jobs (default 30 days, configurable), never profiles or credentials.

## 5. API and clients (later checkpoints)

Add authenticated import/preview creation, status, paginated results, cancel,
resume and delete operations under API v1. Reuse `imports.submit` for mutations
and `imports.read` for inspection, with existing session Origin/CSRF and Bearer
guards. Final route/DTO signatures belong in the executable contract when these
operations are implemented; do not advertise unfinished capabilities.

WebUI uploads CSV and selects a saved profile revision, shows progress and field
diffs, and offers explicit review/cancel/resume/delete. CLI supplies equivalent
commands through the same API. Neither client owns parsing, matching or planning.
Show that preview may create public catalog entries but changes no personal marks.

## Compatibility and rollback

No migration is needed for catalog transport or pure resolver/comparison slices.
Later migrations ship with compatible repositories and upgrade/reopen tests;
newer database versions retain the existing fail-closed startup behavior. Revert
code by follow-up commits, not destructive resets or automatic down-migrations.
Do not advertise the IMDb connector as runnable until the end-to-end job path is
available. OAuth read-only scope remains unchanged.

## Authority and evidence

Read the owning backend contracts before each slice: `connector-contract.md`,
`imdb-csv.md`, `mapping-evaluator.md`, `mapping-storage.md`,
`state-and-pairing.md`, `repository-layout.md`, and relevant HTTP/client contracts.
Product authority: `.trellis/spec/product/collector-spec.yaml`, especially
`neodb_native_fetch` and preview retention. Native-fetch evidence is pinned there
as `neodb_native_fetch_api` and `neodb_native_fetch_web`; the API and web share
the resolver/queue, not the entire browser confirmation flow.
