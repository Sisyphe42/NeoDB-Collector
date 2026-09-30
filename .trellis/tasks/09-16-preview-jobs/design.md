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

### Personal-state read refinement (C1 before comparison)

Use authenticated GET `/api/me/shelf/item/{uuid}` only, never batch omission as
absence evidence. A 200 result must have matching normalized catalog identity,
owner username equal to the expected connected account, one supported shelf type,
and a required nullable integer rating (null or 1..10). Retain only those normalized
fields. Borrow SecretString; validate the expected same-instance movie/TV detail
identity and nonempty account/token before sending. Reuse existing URL, catalog
normalization, HTTP client and bounded parsing mechanisms. No redirect following,
retry or secondary requests: 302 indicates changed identity requiring re-resolution.

Only a bounded, valid 404 JSON result whose message is exactly `Mark not found`
returns MissingShelf. Item not found, unknown 404s, auth failures, rate limits,
timeouts and malformed replies remain distinct sanitized errors. MissingShelf is
not an empty rating: upstream models store ratings separately from shelf members.
Thus C2 must preserve unknown field state rather than invent an empty sink value.
Transport has a ten-second request/body budget and 64 KiB response cap. C1 adds
only an internal read port/adapter and mocks, not credential lookup/account-replace
orchestration, comparison, persistence or HTTP/UI; those remain later checkpoints.

### Account-bound snapshots and pure comparison (C2)

Compose existing endpoint, secret, clock, OAuth identity and mark-read ports in
Core. Callers supply the expected endpoint tuple (base/account/credential reference)
from their resolved-item work. Check that tuple before credential lookup, after
lookup before remote I/O, and after remote I/O before returning a snapshot. A new
OAuth token has a fresh random reference even for the same account, so reference
equality is the existing generation boundary; do not add a schema migration here.
Mismatch, disconnection or read failure returns no snapshot. Verify `/api/me`
identity for owner-less MissingShelf; successful marks must match the expected owner.

Keep credential binding private and non-serializable. Expose normalized evidence,
base/account and observation time, never a credential reference in a client DTO or
diagnostic. Provide a current-binding check for later consumers. These are
point-in-time checks, not a transaction across network and SQLite or authorization
to execute later. Dropping the read future stops subsequent local operations;
bounded runtime worker cancellation/commit checks remain checkpoint D.

Pure comparison reuses `mapping::evaluate`; it does not reimplement transformations.
Preserve skipped/rejected mapping reasons and bind results to profile ID/version.
Model sink values as known value, confirmed empty or unknown: equal is skipped,
unequal nonempty needs review preserving the sink, empty permits a suggestion,
and unknown always needs review. MissingShelf confirms only empty shelf and unknown
rating, even when an explicit completion suggestion exists. Without a valid existing
or accepted suggested shelf, rating is not ready. Mapping numeric semantics remain
unchanged (no implicit rounding or write encoding). No result is executable, enables
a profile or proves identity matching; callers still own prior resolution binding.

Use private snapshot construction so comparison receives service-normalized state,
not caller-supplied JSON. Tests use fake repositories, credentials, clocks and
transports and cover errors, replacement races, unknown/empty distinction, mapping
validation and field-order independence. No new dependency or public contract.

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

### D1 job repository before D2 runtime/results

Implement the internal job repository and `0004_preview_jobs.sql` together before
adding the runtime worker. Core owns job metadata, lifecycle values, bounds and
repository ports; Storage owns SQL and encoding of existing normalized Observation
values. No serde/runtime dependency is added to Core. D1 has no result-commit or
completed-job operation: normalized identity/snapshot/comparison result persistence
and completion belong to D2 with their producer and validation.

Submission is atomic: insert job and ordered observations only if the singleton
endpoint still equals the supplied base/account/credential-reference tuple and the
exact disabled profile revision exists. Reject unsupported raw-like observation
state rather than persisting it. This checkpoint accepts the implemented IMDb CSV
subset only; future connectors extend explicit normalized-record validation behind
the same open-string connector boundary. Keep source/media IDs as strings.
Bound submissions at 10,000 observations, 16 KiB per encoded observation and
32 MiB total normalized observation bytes; pagination is at most 100 records.
Records retain only validated supported fields and display/identity metadata.

Each job has a positive attempt number. Claim changes queued to running once using
an atomic conditional write for ID/attempt/current binding/unexpired retention.
Cancel/interrupt/fail transitions cannot update a different attempt. Explicit resume
from interrupted/failed/cancelled increments attempt and requeues all observations;
no old worker result can be accepted under the new attempt. There is no implicit
startup resume. Later D2 results must additionally check endpoint/profile binding
in the result transaction. Repository recovery/cleanup methods are internal here;
startup scheduling and configurable retention stay D2.

Profiles are immutable revisions: preserve the selected exact ID/version and
never load a different latest revision on resume. Adding another revision does
not silently change which revision an existing historical job selected; callers
choosing a different revision must create a new job. Expired jobs cannot claim or
resume. Manual deletion and expiry cleanup remove observations and job atomically;
late transitions update zero rows and cannot recreate a deleted job. Keep profiles,
endpoint metadata and credentials. No HTTP/UI or source/NeoDB operation is added.

### D2a single-record producer before persistence/runtime composition

First compose a usable internal Core record-preview service. Reuse the existing
native/manual/search resolvers, account-bound snapshot service and pure comparison;
do not introduce a second matcher, comparison implementation or future-racing loop.
Result storage and conditional job completion follow with their compatible adapter;
the worker/startup/configuration integration remains a subsequent slice of D2.

Input binds one validated IMDb Observation, disabled MappingProfile, expected
Endpoint and trusted adapter CatalogIdentity. Before I/O, apply the existing
normalized-observation validator and mapping evaluator, and require the expectation
to agree with the observation's canonical URL, media type and scheme/ID. The model
is trusted built-in adapter output, not a client-supplied matching override.
Resolution choice is explicit: native, native-with-search(query), or manual(detail
reference). Do not auto-select a candidate or fabricate a search query from a
multiline/oversized title hint. Manual mode requires a later caller's explicit user
selection; this internal method is not a new UI action or saved manual mapping.

Use the existing ResolutionControl for one outer 120-second deadline covering
binding reads, catalog resolution, credential lookup, mark/identity reads and final
publication. Existing inner resolver budgets and 15-second polling still apply,
but cannot extend the outer deadline. Cancellation/expiry drops pending work and
prevents later calls; it cannot undo remotely queued catalog creation or an OS
credential operation already running in a blocking adapter.

Expose the snapshot service's existing expected-binding check for reuse. Require
the exact base/account/credential reference before catalog I/O, after resolution
before account reads, and before returning any record result (including unresolved
outcomes). No mutex/SQLite transaction is held across I/O. Only Matched and
ManuallySelected can read marks and compare; NeedsReview, SearchCandidates,
Unsupported, RateLimited and Unavailable retain their evidence/outcomes without
credential or personal-state reads. Errors return no partial record or empty sink.

The output has private construction, read-only source fingerprint/profile/base/
account metadata, an exact-binding predicate and an immutable outcome accessor.
Compared output owns the original AccountMarkSnapshot and Comparison, recording
automatic versus manual provenance separately. Do not flatten away evidence,
unknown/empty distinctions, snapshot time or field reasons. No Debug/serde or
credential-reference accessor on the record. Publication is historical evidence,
not job completion or write/activation authority. This slice adds no migration,
worker, HTTP/client contract, dependency, actual account access or source write.

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
