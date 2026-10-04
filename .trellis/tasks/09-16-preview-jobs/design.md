# Resumable preview design

Status: approved for implementation by user continuation on 2026-09-18.
Product scope is already approved in the parent and `prd.md`.

## E5b: Real adapter failure-path acceptance (2026-10-04)

Extend E5a's test-only composition through the same real Rust CLI client, Core
HTTP, SQLite/runtime/runner and production NeoDbClient. Extract only the concrete
fixture setup/shutdown already needed by both suites; preserve E5a assertions.
Keep fault injection exclusively in synthetic upstream HTTP responses/gates,
never substitute catalog/mark/identity ports or shorten production timeouts.

Each submitted batch first commits an equal comparison, then encounters the
faulting second observation. Native fetch 422/429 are terminal Unsupported/
RateLimited historical outcomes and may complete the job without a personal read
for that row. Fetch timeout/unsafe 302, detail 429 and mark 401/302/non-missing
404/timeout are errors: persist Failed with the original first result only;
never invent an Empty/Unknown comparison for the failed row or erase history.
Check the full singleton tuple, exact disabled v1/profile retention, unchanged
credential entries, historical/non-executable output and no raw diagnostic/token
persistence. Status is authoritative; its existing shape exposes no last-error
code, so do not assert a new public diagnostic contract.

Record exact GET order/auth/cookie/refetch behavior. A second-origin loopback
trap must observe zero requests for foreign catalog redirects; same-instance
noncatalog/mark redirect targets also must not be followed. No retry, implicit
search, identity lookup after a failed mark or credential read after unresolved
catalog work. Upstream hangs use release gates, actual ten-second adapter timeout
and bounded terminal status reconciliation, not sleeps or a timeout override.
Release held handlers and await their actual responded-or-dropped lifecycle
before checking unchanged history and gracefully draining all runtime/HTTP/storage
owners. A cancelled handler does not prove delivery of a late response.
Main independently reviews/repeats and runs the full workspace gate.

No production/dependency/API/schema/migration/capability/write changes, real
account/keychain access, restart/migration/browser or polling/search/manual scope.
Keep child and parent active after this bounded checkpoint.

## E5a: Real CLI client and native adapter integration acceptance (2026-10-04)

Test-only composition uses actual PreviewClient pairing/import/status/results,
the protected loopback router, temporary SQLite, PreviewRuntime::start and the
real runner/record/snapshot services. Production NeoDbClient supports literal
loopback HTTP, so a second Axum server supplies synthetic upstream responses
without a new dependency, TLS exception or fake transport port.

One raw BOM/UTF-8 CSV submission produces three normalized native rows: equal
rating, conflict preserving the sink, and exact missing-shelf 404 followed by
authenticated identity confirmation (unknown rating, not empty). Include skipped
and duplicate inputs and discarded raw-column/upstream markers. Save changed v2
before submitting explicit disabled v1; prove exact revision, stable ordinals,
pagination, durable completed attempt 1, historical/non-executable output and
absence of raw payload/credential values in persisted normalized evidence.

Record every upstream request: public fetch 302 and fresh detail are unauthenticated,
mark and identity use only the fixture NeoDB Bearer token; all requests are GET,
with no Cookie, refetch or source interaction. Use bounded event/status waits,
then drain runtime and shut down HTTP servers/SQLite. Fake only secret store and
clock; no real account or OS credential access. This proves the Rust CLI client
chain, not a shipped-process/native-keychain or browser acceptance result.

Pending polling/search/manual, integrated timeout/429/auth/redirect, restart,
migration and browser-to-real-adapter acceptance remain later bounded slices.
Keep this child and parent active, capability IDs empty and writes disabled.

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

### E2b1 admission before durable submission/resume

Before exposing CSV upload or resume HTTP routes, extend the existing single
Server runtime with internal `submit(PreviewSubmission)` and `resume(&PreviewJob)`.
Both synchronously reserve the existing bounded admission slot and job ID before
any repository mutation, then return an admission observer. Its awaited receipt
contains only job ID, accepted attempt and the existing completion observer; it
does not invent running/completed status. Submission requires live validated
metadata. Resume accepts only retained interrupted/failed/cancelled metadata,
checks generation overflow and reserves the next generation. The existing atomic
repository remains authoritative about current status, binding and exact revision.

The runtime owns persistence and execution as one tracked operation. Dropping
either observer, including an HTTP caller that will later await admission, does
not strand an accepted mutation by abandoning its future. Use existing
Tokio/tokio-util task tracking, cancellation and strict ten-second control for
the mutation; do not hold the registration mutex across I/O or add a processor
port, scheduler, library, migration or public DTO. A confirmed mutation continues
on the same registration into the real runner without re-enqueueing or a second
capacity check. Only runner execution acquires the single execution permit.

Refused/failed persistence releases admission without catalog/keychain access.
Resume refusal leaves prior results intact; successful resume runs all original
observations freshly and retains the exact profile/binding and fixed expiry.
Cancellation, expiry and shutdown signal preparation as well as execution;
shutdown drains their tracked futures before storage closes. A timeout, panic or
shutdown may follow a SQL commit: return a static query-status error, never assert
rollback, auto-retry, delete a possibly committed job, or report fabricated success.
Registration cleanup happens before normal failure notification or execution
completion, while a successful admission receipt is sent before execution finishes.
No worker may retain the owner's shutdown guard in an Arc cycle.

Tests use the real runner and temporary SQLite/fake ports. Cover full/closed/
duplicate admission before submit/resume mutation, concurrent resume, immutable
profile/account binding, empty batches, fresh resume, dropped observers during
pending mutation, hung/error/panic/refused/late mutation, expiry and shutdown,
truthful ambiguous commits, registration reuse and no transport before persistence.
Public CSV submission/resume routes and generated clients remain E2b2; no API
capability is advertised in E2b1.

### E2b2 authenticated upload and explicit resume

Expose POST `/api/v1/imports/imdb/ratings?profileId=...&profileVersion=N` with
raw UTF-8 `text/csv` (optional UTF-8 charset), not JSON/multipart. Authenticate
before reading the body: session mutations require exact Origin/CSRF; paired
clients require `imports.submit`. Strict query DTO selects an existing immutable
disabled revision; no default/latest/save is implicit. Bound the raw stream to
the existing parser's 10 MiB, reuse `collector_connectors::imdb::parse_ratings`,
and persist only its validated observations. Trusted observedAt is ingestion Unix
seconds as a decimal string. Receipt reports bounded observation/skipped-row/
skipped-column/duplicate-row counts, never raw unsupported values or header lists.

Use a fresh cryptographic TokenSource ID and the shared runtime retention policy.
Endpoint/profile reads, body collection and admission observation share one strict
ten-second post-auth budget. The parser is a synchronous bounded pure adapter;
check the control before/after it rather than spawn unbounded detached parse work.
Core validates the normalized submission; the existing atomic repository checks
binding/profile again. Compose a narrow Server submission view of the same CLI
runtime Arc; no new Core parser port, scheduler, migration or third-party crate.
Server may depend on the existing Connectors crate for this composition boundary;
Core remains connector-independent and source adapters never call NeoDB.

Dependency review: reuse the existing locked Hyperium
[http-body project](https://github.com/hyperium/http-body), not a new streaming
framework. `http-body-util` 0.1.5 is a direct Server dependency only to identify
LengthLimitError through standard error sources (ordinary stream failure is not
413); its default feature set is empty and this declaration enables neither
channel nor full. `http-body` 1.1.0 is Server dev-only for a small controllable
streaming test body. Locked published manifests declare MIT and Rust 1.61 for
both; this is below the repository toolchain and does not introduce OS-specific
code. Cargo.lock retains all previous versions/checksums; Core's allowlist is
unchanged. Typed error classification and direct body-poll assertions justify
these focused dependencies instead of error-string inspection or another parser/
stream abstraction. Cross-platform CI remains separate from local Windows proof.

Expose POST `/api/v1/previews/{id}/resume` with a strict 4 KiB JSON envelope
`{expectedAttempt}`. Load retained metadata under the same bounded control,
reject mismatched attempts and ineligible states; reserve through runtime.resume.
The repository remains authoritative about exact endpoint/revision and lifecycle.
Resume never extends expiry, rebinds an account, selects a newer profile, restores
old results as fresh evidence or retries automatically.

Domain owns PreviewImportSelection, PreviewResumeRequest, PreviewAccepted,
PreviewImportAccepted and PreviewAdmissionFailure. Successful durable admission
returns 202 with only jobId/attempt and executable=false, not execution status.
After reservation, ambiguous failure includes `{jobId, attempt, error: ApiError}`
so even a new submission can be reconciled through existing GET routes; it never
asserts rollback. Known safe failures retain reconstructed static codes/messages.
Pre-admission errors use ApiError; full admission is 429, local duplicate/conflict
409 and closed/unavailable runtime 503. No accepted receipt is retried by clients.

Regenerate contracts and add thin TS submitImdbRatings/resumePreview methods,
raw-body serialization, generated response validation and a typed sanitized
PreviewAdmissionClientError carrying reconciliation metadata. Preserve timeout,
caller cancellation, in-memory CSRF and no automatic retry. These two admissions
default to a bounded 30-second client timeout (explicit timeoutMs still overrides),
so the ordinary five-second GET timeout does not cut short the Server's ten-second
admission budget. Network loss/abort can still lose the entire receipt, with no
recoverable ID; never interpret that as safe rollback or retry. Advertise delivered
route keys only; connectorIds remains empty until E3 end-to-end delivery. No new
CLI commands, upload/resume screens or personal-state writes in this checkpoint.

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

### D2b1 result repository before runtime composition

Ship `0005_preview_results.sql` together with compatible Core/Storage operations.
Add `completed` to the job lifecycle by rebuilding the v4 jobs/observations tables
transactionally while preserving all rows, bindings and foreign keys. Never edit
the committed v4 migration or disable foreign keys opportunistically. Add a result
table keyed by job/ordinal, storing current attempt, commit time and strict bounded
normalized review JSON; it references the original observation. No worker or route
is enabled by this repository slice.

The write port consumes a service-produced PreviewRecord, not deserialized review
data. Bind it to the exact normalized observation and full disabled profile used
by the producer, not merely an ID/version or user-supplied fingerprint. An internal
private input proof may retain bounded normalized inputs in memory for equality;
never serialize that proof or persist a second profile/observation payload per
result. Keep historical decoded review types separate from service snapshots and
records: loading saved evidence must not recreate fresh-read or write authority.

Result insertion uses SQLx-managed BEGIN IMMEDIATE and checks, in that transaction:
existing running job, exact positive attempt, non-regressing now before expiry,
current full endpoint tuple, selected valid immutable profile and exact observation
at the ordinal, agreeing with the producer's inputs. Reject stale/late work without
insertion; duplicate ordinal writes never overwrite existing evidence. Only a
validated normalized result may be encoded. Keep identity, automatic/manual
provenance, account/profile metadata, snapshot time and comparison's source/sink
Known/Empty/Unknown/status/reason; unresolved outcomes never acquire invented sink
values. Permit at most 64 KiB per result and 32 MiB aggregate result JSON per job,
independent of the existing observation byte limits; reject without truncation.

Completion is conditional on the same running attempt/time/account/profile guard
and one valid result for every original zero-based ordinal. Zero-row jobs can
complete only after claim. Completed means preview processing finished, not that
every item matched or any write is ready. Partial failed/cancelled/interrupted
attempts may retain historical review rows. Explicit successful resume deletes
those rows in the same transaction as incrementing/requeuing the attempt: no old
comparison is reused. Delete/expiry removes observations/results/job atomically
and preserves profiles/account/auth/credentials. Late writes cannot recreate rows.

Read current-attempt results in stable ordinal pages (1..100) within a single read
transaction. Revalidate normalized encoding, bounded values, metadata, profile and
observation agreement; corrupted rows fail closed rather than returning a partial
page. Missing jobs or an obsolete requested attempt return no rows. Historical
reads remain possible after an account replacement but are never labeled fresh.
Encoding stays in Storage using existing serde/serde_json; Core gains no serializer,
database/runtime or connector dependency and no general persistence framework.

Library reuse review (2026-10-01): reuse the already locked serde 1.0.229 and its
workspace-enabled derive/std features directly in Storage. The local registry
manifest declares MIT OR Apache-2.0, Rust 1.56 and serde_core/optional serde_derive;
this does not change the workspace's supported Rust version or prove other OSes.
Strict private DTO derives replace hand-written JSON field/variant decoding;
serde_json remains the bounded codec. No new version, feature set, transitive
capability or Core dependency is required. Record the direct Cargo/lock metadata
change with implementation and tests, not a broader dependency-policy relaxation.

Tests produce records using the real Core producer with fake transports/secrets.
Cover each resolution/comparison variant, reopen, current-attempt ordering,
same-ID altered profile/observation rejection, duplicate writers, every late-write
boundary (cancel/fail/recovery/resume/delete/expiry/rotation), incomplete/empty
completion, rollback of insertion/resume/delete, malformed/oversized result JSON,
prior v1-v4 upgrade with populated v4 rows and preservation of unrelated state.
Do not call real NeoDB or the OS credential store.

### D2b2a single-attempt runner before runtime ownership

Compose the existing job/result/mapping/access/clock ports and PreviewRecordService
in `collector-core/src/preview_runner.rs`. This is callable internal orchestration,
not an automatically scheduled worker. Server's single-job admission, owned
cancellation, startup interruption and retention configuration follow separately.
No public DTO/route, migration, new dependency or advertised capability belongs here.

An invocation identifies one existing job and expected positive attempt. Validate
metadata and cancellation before work; load the queued job, current endpoint and
its exact disabled profile revision, never latest. Require retained full binding,
status/attempt, non-regressing wall clock and unexpired retention before claim.
The existing atomic claim is the only ownership grant. A losing/stale invocation
performs no catalog/credential/mark reads, result write or fail transition.

Traverse original observations in stable pages of at most 100, process sequentially
and retain no batch-wide result buffer. A trusted pure adapter callback accepts
ObservedItem and returns CatalogIdentity; callers can pass existing IMDb
`catalog_identity` directly. Core must not depend on connectors or reconstruct a
second source identity/model mapper. Validate page count and normalized observations,
reuse the production evaluator and record producer. Native mode only in this slice:
no implicit search query, selected item or old stored result is execution input.

Before each record and before publication/completion, recheck live job status,
attempt, both original/fresh job bindings against the initial claimed full endpoint
tuple, and wall-clock bounds after awaited endpoint reads. The result repository remains
the atomic authority against concurrent cancellation/deletion/resume/rotation.
Store immediately at the original ordinal; stop on a refused write and never
process a subsequent record. Call conditional complete only after every ordinal
was processed, including a claimed empty batch. A completed preview remains
non-executable regardless of identity/review outcome.

Reuse ResolutionControl for cancellable bounded repository/producer operations;
no custom future race, busy loop or Core runtime dependency. Each record gets a
checked deadline capped at 120 seconds and its remaining retention; a large batch
does not receive an accidental 120-second total-job deadline. Include per-record
guards and persistence in that outer budget. Initial load/setup/claim share one
deadline; page reads and completion also use bounded control. Wall-clock regression
or exact expiry stops remote work.
Dropping a mutation future cannot undo an already committed SQLite operation;
callers inspect status, and later startup recovery interrupts unfinished attempts.

After a known successful claim, ordinary producer/repository failures request only
the existing attempt-guarded fail transition, preserving partial history. Surface
failure to persist that transition with a static cleanup diagnostic; never declare
success or overwrite another attempt/terminal state. Cleanup has a separate
10-second bound capped by remaining retention. If fail returns false, reload under
that deadline: disappearance, terminal state or another attempt preserves the
original error; an unchanged running attempt yields the cleanup error. Runtime cancellation returns
its existing interruption error without inventing cancelled/failed status: user
cancel and shutdown have different owners. Dropped runs remain unfinished for
explicit later recovery; no automatic retry/resume or expiry extension.

Tests use real Core producer plus memory job/result repositories, fake transports,
credentials/clocks and deterministic control. Cover empty/multiple-page processing,
stable ordinals and bounded sequential calls, losing/concurrent claims, fresh rerun
after explicit resume, exact revision and all tuple changes, terminal/deleted/expired
jobs at phase boundaries, refused stores/completion, partial failure/cleanup failure,
invalid/holey pages, cancellation/timeout/drop during every pending port phase and
retention-limited work. Keep runtime composition and public integration unchecked.

### D2b2b owned runtime admission and lifecycle

Add an internal Server runtime owner around the real PreviewAttemptRunner, existing
transport ports and trusted identity callback. No HTTP/CLI composition, database
scan, automatic restart/resume, retention setting or public capability in this slice.
Use existing Tokio Semaphore/CancellationToken and tokio-util TaskTracker; no new
library/version or general workflow framework. One owner is created by the later
server composer; independent owners do not promise process-wide exclusion.

Accept explicitly supplied validated PreviewJob metadata, retaining only its ID
and attempt for execution. Admission is nonblocking and bounded: at most one active
attempt plus a fixed small number of waiting attempts. Return a static busy error
when full; do not spawn unlimited waiters or hold source payloads in the queue.
Reject duplicate local job IDs, including a different generation while an older
local task still exists. Waiting tasks acquire the shared single permit with a
cancellation-aware select. Claim remains Core/Storage's atomic authority, not the
semaphore or an in-memory running label.

The runtime owns spawned tasks. Returning/dropping a result observer does not cancel
accepted work; cancellation is explicit. Each task has a child token and drop guard,
and a scoped registration guard removes its own admission entry on all exits,
including panic/drop. Keep registration/closed-state mutex sections short and never
hold them across I/O. Observer failure after panic must be sanitized and must not
invent a persisted status. Persisted status remains authoritative if an async
mutation committed before interruption was observed.

User cancel first signals only the matching local job/attempt token, then requests
the existing attempt-guarded repository cancel under a separate bounded management
budget, reusing fresh independent TokioResolutionControl with strict 10-second
pre/post deadline checks rather than a bare timeout. Wrong attempts cannot stop newer work. Delete signals any local generation
for that job before bounded repository deletion; late stores cannot recreate it.
Management repository errors/timeouts are explicit: local interruption is not proof
that cancelled/deleted state persisted. Dropping a management future cannot roll
back already committed SQL. Do not erase partial history except through existing
delete/resume operations, and never touch credentials/profiles.

Shutdown closes admission before signalling all work and waiting for tracked tasks.
It does not classify shutdown as user cancel or mutate jobs to failed/cancelled;
unfinished queued/running work is left for D2b2c startup interruption. Dropping the
runtime owner signals children without detaching uncancelled work; workers must not
retain the owning shutdown guard in an Arc cycle. Caller-owned tickets are observers,
not task owners. No automatic retries or a total-batch 120-second timeout.

Tests use real runner/producer with fake transport/secrets and memory or temporary
SQLite repositories. Prove one active job across distinct job IDs, bounded waiters,
duplicate rejection, cancellation before permit and during hung catalog I/O,
wrong-generation isolation, deletion/late writes, partial history, admission release
after failure/panic, dropped observer vs dropped runtime, shutdown races and bounded
management failure. No live NeoDB/keychain; full gate before checkpoint commit.

### D2b2c startup composition and retention maintenance

Compose exactly one runtime in CLI `serve`, reusing the same Storage, clock,
OS-secret adapter and Arc<NeoDbClient> for OAuth and preview services. Construct
the real snapshot -> record producer -> attempt runner and pass the existing
IMDb identity function. Startup binds first, opens/migrates storage, then performs
bounded queued/running interruption and expiry cleanup before publishing the
listener/bootstrap or accepting work. No scan/resume/enqueue of saved jobs and
no keychain/network access occurs merely from composing these services. `doctor`
only checks/migrates storage and never performs preview recovery/maintenance.

Before opening/migrating/recovering, `serve` holds a nonblocking exclusive data-
directory lease for its entire lifetime (including worker drain and pool close).
Different ports do not permit multiple hosts for the same operational database.
Use a separate `collector.serve.lock` file opened read/write/create without
truncation, and std File::try_lock; leave the file in place and rely on handle
drop/process exit to release ownership, not file deletion or a stale PID protocol.
Refuse static `core_data_directory_in_use` / `core_data_directory_lock_failed`
before DB changes; doctor remains lease-free and nonrecovering. This coordinates
cooperating Core hosts, not arbitrary external tools removing/replacing the lock.
Standard-library review: local rustc 1.98.0 and stable toolchain are compatible;
try_lock is stable since 1.89, uses Unix flock/Windows LockFileEx and releases when
the last owning handle closes. Evidence: https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock.
No new dependency/license/features; local tests are not three-OS verification.

Add a pure Core PreviewRetention value: default 30 days, nonzero u32 days,
checked Unix-second expiry calculation (reject negative time/overflow). CLI
configuration schema_version remains 1; add YAML `preview.retention_days`, env
`NEODB_COLLECTOR_PREVIEW_RETENTION_DAYS` and `--preview-retention-days`, preserving
CLI > env > YAML > default and strict unknown-field handling. The shared runtime
retains this validated policy for future submission; it never rewrites existing
job expiry or extends retention on resume. No public API DTO/route is added.

Server's production async `PreviewRuntime::start` performs recovery then cleanup
under one independent strict ten-second control budget and starts one tracked
maintenance task. Keep the existing raw constructor for internal focused tests;
production composition must use start. Use Tokio interval_at every 60 seconds
with MissedTickBehavior::Skip; the first tick is after startup, not an immediate
duplicate cleanup. Each tick has a fresh cancellation-aware strict ten-second
budget, samples wall time once, signals expired registered attempts before calling
job-only repository cleanup. Admission keeps only ID/attempt/expiry plus token;
reject already-expired admission. No payload scan, busy loop or generic scheduler.

The maintenance observer is not an owner: dropping it does not stop maintenance.
Static startup/cleanup/panic errors must be explicit. A periodic failure closes
admission, signals work and notifies the host, which gracefully stops serving and
returns failure rather than silently running without retention. No automatic
retry policy is introduced. Shutdown signals/drains maintenance and attempts
before Storage::close on normal signal, maintenance failure and HTTP server error.
Dropping the owner cancels maintenance without a guard/Arc cycle. Reuse existing
Tokio/tokio-util control/tracking; no library version, schema migration or source
write. SQLite remains authoritative if a mutation committed before cancellation.

Tests use fake time/ports, paused Tokio and temporary SQLite: startup interrupts
queued/running while preserving terminal/partial history, deletes expired rows
only, never resumes or calls transport/keychain, repeat startup/reopen works;
ticks do not run early/catch up; expiry stops active/waiting work and prevents late
results; startup/periodic errors, hung/late ports and owner-drop/shutdown are bounded
and sanitized. Test configuration defaults/precedence/unknown/zero/negative/
overflow values, fixed expiry, doctor non-recovery, and real serve composition with
injected shutdown/fakes. Run the full gate and keep public delivery E pending.

## 5. API and clients (later checkpoints)

### E3b historical WebUI preview

Add a session-only PreviewPanel through the existing shared AccessClient. Select
a local UTF-8 CSV (10 MiB maximum, fatal decoding preserving BOM) and explicitly
enter a saved profile ID/version. Core validates that exact immutable revision;
the latest-profile GET must not silently replace a requested older version. No
CSV parser, persistence, default selection, model, dependency or API change.

Preserve the existing blue-grey console tokens (#243341 text, #607384 secondary,
#f3f6f8 canvas, #ffffff paper, #dbe3e9 borders, #26775f connection), Bahnschrift
headings, Segoe UI body and Cascadia Code data. Use stacked import/inspection
sections and per-record evidence/field-diff cards rather than a mobile-wide table.
The signature is explicit suggestion vs Known/Empty/Unknown sink comparisons;
no new hero, color scheme, font download, decorative animation or redesign.

Keep drafts, current job ID and CSV transient. Users can reopen a known job ID;
there is no list API. Poll queued/running jobs at bounded nonoverlapping cadence,
stop on error/terminal/unmount, and guard abort/stale responses. Read one explicit
attempt-bound ordinal page at a time; empty/sparse pages do not prove completion
or hasMore, and progress means actual lifecycle/counts rather than percentage.
Every result remains historical/non-executable even with current binding. Show
all outcome kinds, identity claims, provenance, times and every field decision;
review is inspection only because manual selection has no public API yet.

Mutations occur only on user action, serialized and attempt-bound; no effect or
StrictMode may submit/resume/cancel/delete. Reconcile applied=false and uncertain
mutations with status, never fabricate state or retry. Admission errors with a
validated ID retain it for inspection; lost receipts without ID remain unknown.
Deletion requires explicit confirmation and only removes operational history.
Warn that public catalog creation is possible but personal/source state is never
written. Logout unmounts and aborts observation, not accepted Core work.

Sol/medium owns PreviewPanel, its focused tests, minimal AccessPanel integration
and preview-only CSS. Main owns task/spec changes, mock Chromium smoke, review,
full validation and commit. No recursive delegation or agent commits. This
checkpoint does not advertise connectorIds or close final mock restart/resume
acceptance; all tests avoid actual accounts, keychains and native catalog fetch.

### E3a thin CLI preview commands

Deliver CLI before WebUI as an independently verifiable checkpoint. Reuse the
existing CLI literal-loopback origin, health compatibility preflight, OS-stored
per-origin Bearer credential and bounded no-proxy/no-redirect reqwest client.
No direct database reads, source parsing, matching, planning or new dependency.

Library review: the already locked reqwest 0.12.28 implements default retry of
protocol nacks; its published source `src/retry.rs` documents
`reqwest::retry::never()` as the disabling policy. Set that on the existing shared
CLI HTTP builder instead of adding a custom retry wrapper. No new version,
feature, license or platform capability; the existing bounded response collection
and no-proxy/no-redirect guards remain. Primary source:
https://docs.rs/reqwest/0.12.28/reqwest/retry/fn.never.html.

Commands are `import imdb-ratings FILE --profile-id ID --profile-version N`
and `preview status|results|cancel|resume|delete ID`, with `--core-url` and
`--format human|json`. Results require `--attempt`, use explicit ordinal
`--offset`/`--limit` (1..100), and fetch exactly one page, never infer completion
from an empty page. Cancel/resume require explicit `--expected-attempt`.
No automatic polling, resume or mutation retry. Status reports the server's
lifecycle/binding/counts, not a fabricated percentage or fresh snapshot.

FILE is a path or `-` for noninteractive stdin. Collect at most 10 MiB plus one
sentinel byte using standard bounded Read, send original bytes as text/csv;
Core owns UTF-8/CSV validation. Do not persist/copy/log raw data or its path.
Use 30-second bounded preview requests (Server has a strict ten-second budget),
64 KiB response caps except results (existing 8 MiB HTTP cap). Deserialize Domain
wire DTOs with strict serde shapes and check semantic safety/echoed request
identity, positive generations, limits and historical/non-executable invariants.
Never hand-maintain parallel wire types or a general schema parser.
Reject any composed URL whose normalized path differs from the intended route;
an ASCII-valid `.`/`..` job ID is not a safe URL path segment. This is a thin HTTP
guard, not a change to shared Domain identifier grammar or profile query IDs.

Successful commands return the actual Domain receipt/view/page/mutation DTO,
exit 0; 202 means durable admission only and applied=false is not an exception.
Admission failures preserve validated job ID/attempt in the existing Domain
failure envelope, exit 1; errors have static sanitized diagnostics and never echo
upstream messages/bodies/credentials. Lost responses do not prove rollback or
permit retry. Human output states historical/non-executable and public catalog
creation boundaries; render data through JSON escaping to avoid terminal control
injection, retain every field diff, Known/Empty/Unknown and evidence claim.

Tests use injected fake credential stores and loopback HTTP only: exact raw bytes,
immutable query revision, all command methods/envelopes/one-page queries,
authenticated reads, success/refusal/error/ambiguous admission, invalid DTOs,
identity/attempt mismatches, strict CLI flags, input and response bounds,
redirect/version/auth denial and sanitized output. No native keychain/account.
WebUI screens, mock restart/resume end-to-end and runnable connectorIds remain
pending until E3b/final acceptance; no public API/schema migration is required.

### E1 read-only job inspection before submission/client screens

Expose GET `/api/v1/previews/{id}` and GET
`/api/v1/previews/{id}/results?attempt=N&offset=0&limit=50`. Both require a
WebUI session or paired Bearer `imports.read`, using existing Host/Origin,
Authorization-presence precedence and no-store boundaries. GET is nonmutating,
so no CSRF is required; no credential, catalog or mark read is performed.
Missing/expired jobs return static 404; an obsolete requested attempt returns
409, not an empty current page. Query fields are strict, duplicate/unknown or
invalid positive attempt/page limit are rejected without echo. Limit is 1..100;
offset is an original zero-based observation ordinal, not a SQL row offset.

Domain owns strict generated wire DTOs (camelCase metadata, snake_case tagged
outcome/sink/status variants). Job views include ID/status/attempt, fixed expiry,
observation count, immutable profile revision, base/account and a point-in-time
binding state `current|changed|disconnected`. This is not snapshot freshness or
write authority: every returned result is explicitly historical/non-executable.
No credential reference, source payload or token enters these DTOs. Preserve all
normalized outcomes, identity evidence including distinct external-ID claims,
automatic/manual provenance, snapshot/commit times and complete field comparison
suggestion/sink Known/Empty/Unknown/status/reason. Reuse Domain ScalarValue.

Result pages carry job ID/attempt/offset/limit, historical=true and ordered items.
There is no fabricated total/remaining count or progress percentage. Empty pages
while queued/running do not prove completion or absence; clients poll job status
and reread pages after terminal transitions. Fetch job before and after the result
repository read; refuse attempt change/deletion/expiry without partial publication.
Validate returned rows against job metadata/attempt and ordered bounded ordinal
range. Use one existing strict ten-second ResolutionControl budget for repository
reads and endpoint inspection, with static 503 errors on corrupt/hung/late ports.
Serialize bounded pages with existing serde_json; cap encoded response at 8 MiB
and reject overflow rather than truncate. No new database schema or dependency.

Compose optional inspection job/result ports in AccessState and wire real Storage
in CLI serve. Advertise only the two delivered read routes, keep connectorIds empty
and import/resume/cancel/delete/UI capabilities pending. Thin shared TS client uses
generated paths/validators, bounded/caller-abortable GETs and sanitized errors;
it does not cast results or implement matching/pagination policy.

Tests use real temporary SQLite plus fake clock/ports: authentication and malformed/
revoked/duplicate Bearer cannot fall back to cookies, method denial, strict query,
all statuses/outcomes and unknown/empty/conflicting field evidence, sparse pages,
current vs rotated/disconnected binding, expiry, attempt/deletion races, corrupt
storage, hung/late reads and response bounds/redaction. No actual account/keychain
or native-fetch operation. Full gate before an independent E1 checkpoint; E2
submission/management and E3 CLI/WebUI remain pending.

### E2a explicit cancel/delete before submission and resume

Expose POST `/api/v1/previews/{id}/cancel` with strict
`PreviewCancelRequest { expectedAttempt: positive u32 }`, and DELETE
`/api/v1/previews/{id}` without a body. Both return a strict
`PreviewMutationResult { applied: boolean }`: true reports the repository's
successful/idempotent transition, false reports its refusal/absence, never a
fabricated cancelled/deleted status. Clients use the existing GET to reconcile
ambiguous outcomes; no automatic mutation retry. A wrong cancellation attempt
must not signal newer work; deletion signals any locally registered generation.

Session mutations require exact Origin and CSRF; Bearer mutations require the
already fixed `imports.submit` scope. Authorization presence excludes cookie
fallback. Reuse the existing job ID grammar, 4 KiB JSON cap, no-store/Host/Origin
boundary and static errors. Invalid IDs/envelopes/zero attempts are 400, over-cap
JSON is 413, absent management composition is 503 preview_unavailable. Failures,
hung or exact-deadline work return static 503 preview_cancel_failed or
preview_delete_failed, not transport/database diagnostics. Each request has a
strict ten-second management budget; a committed mutation cannot be rolled back
merely because its future was dropped or its response timed out.

Use an object-safe narrow Server management view of the existing generic runtime
to compose the exact same CLI-owned Arc into AccessState. Its cancel/delete
methods delegate directly to the runtime, not repositories or a second scheduler.
No Core port, new runtime owner, migration, dependency, enqueue/resume, network,
credential operation or personal-state write belongs here. Runtime shutdown/drain
and the serve lease retain their existing ownership. Thin TS methods consume the
generated request/result/path contract and mutation CSRF without UI controls.

Tests use temporary SQLite, real runtime/runner and fake catalog/secret ports to
prove active/queued cancellation, wrong-generation isolation, partial-history
preservation and deletion preventing late publication; retain profiles/account/auth.
Also cover session Origin/CSRF, Bearer fallback/revocation/duplicates, strict input,
optional composition, static failures and strict hung/late management bounds.
Update real-process smoke for delivered methods/capabilities. Full gate before
checkpoint commit; CSV submission, explicit resume and screens remain pending,
connectorIds stays empty and no personal-state write capability is advertised.

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

### E4e: explicit CLI resolution inputs (2026-10-03)

Extend `import imdb-ratings FILE --profile-id ID --profile-version N` with
optional `--resolution-inputs PATH`. PATH is a UTF-8 JSON array of the existing
Domain `PreviewResolutionChoice`; it is a file only (`-` is refused), so CSV can
continue using piped stdin without competing readers. Missing flag keeps the
existing raw byte transport unchanged. Do not parse CSV, infer choices from
titles/history, mutate old jobs or create a separate request schema/dependency.

The structured path reads bounded CSV/choice inputs before health/credential I/O,
strictly deserializes existing closed Domain shapes, preserves CSV UTF-8/BOM, and
serializes `PreviewImdbResolutionRequest` once. Bound both input files and the
actual encoded request to 10 MiB, and array count to 10,000. Standard bounded Read,
strict serde and existing preview transport/error/receipt guards remain owners.
Core remains authoritative for normalized ordinals/source IDs, duplicate inputs,
query semantics and supported reference media/model pairs; do not duplicate its
CSV parser or identity policy in CLI. Local errors must not echo paths, CSV,
queries or credentials. No health/token/POST after a local structured input error.

Use the same exact saved profile revision, literal-loopback origin, no proxy,
redirect or retry, health-before-token, 30-second request/64 KiB receipt bounds and
truthful ambiguous-admission behavior. Every successful structured POST admits
a NEW immutable job. No client schema/API/Core/Server/Storage change, capability,
personal/source write or persistence of the local input is added.

Sol/medium owns CLI argument parsing, bounded input/transport and Rust tests;
main owns existing actual-process smoke/help/early refusal evidence, README,
task/spec updates, independent review, full gate and checkpoint commit. Tests
cover mixed Search/Manual/Native-default payloads, exact BOM/UTF-8 and revision,
closed nested shapes/numeric errors, file/encoded exact/+1 bounds, count cap,
file-only choices, raw compatibility and inherited authenticated/ambiguous receipt
handling without retries. Only fake credentials and loopback endpoints are used.
Broader real-client/simulated-NeoDB-adapter acceptance remains pending.

### E4d: explicit WebUI resolution resubmission (2026-10-03)

Extend the existing PreviewPanel and blue-grey evidence cards, not the API or a
new application. Use `submitImdbRatingsWithResolution` and generated choice types.
Main adds narrow generated choice/reference validator re-exports from the shared
client facade; WebUI gains no direct dependency or handwritten wire validator.
Keep ordinary raw CSV upload unchanged. Scope is session WebUI only; CLI choices,
arbitrary catalog URL/UUID entry, cross-job saved mappings and a standalone catalog
search route remain separate. No dependency, schema, Core/Server/Storage change,
capability advertisement, activation or personal/source write.

Only a completed, currently bound job submitted by this mounted panel from a
retained local File and exact saved profile revision can stage choices. Associate
the file/selection with the confirmed receipt's job ID/attempt, and use matching
job/page/row metadata to bind zero-based ordinals and sourceRecordIds. Reopened
historical jobs remain inspection-only: raw CSV is never recovered from storage
or parsed in the browser. File/profile edits, explicit job opening, attempt change,
binding change, deletion or unmount invalidate that association and sparse draft.
Keep unrelated import form drafts and historical evidence useful. Do not infer
selections from previous outcomes or auto-select a singleton candidate.

For Unsupported or SearchCandidates rows offer an explicit bounded query form;
explain that native resolution still precedes Unsupported-only search. For actual
returned candidates or NeedsReview item evidence, offer explicit manual staging
from `{uuid, mediaType, model}` only; generated reference/choice validation rejects
malformed legacy evidence. Display identity/granularity and explain that manual
choice is not automatic IMDb identity proof and may still need review. Choices
are sparse, one per ordinal, replaceable/resettable, at most 10,000, and remain
transient across explicit pagination of the same job/attempt. Draft-level reset
remains available for off-page or now-Compared ordinals. Auth expiry/in-flight
actions disable staging/clearing too. Staging/clearing does not read CSV or send
any request. Draft summaries never imply changed history.

A separate explicit "generate new preview" action snapshots the association and
choices, serializes through the existing action lock, reads the retained File with
existing exact byte/fatal UTF-8/BOM guards, and validates the ENCODED JSON body's
10 MiB limit using TextEncoder before upload. Do not enlarge limits or implement
a CSV parser. Recheck the original job ID/attempt/completed/current binding,
profile and public base/account metadata through the existing job-read API just
before submission; this is a point-in-time client guard, not atomic authorization.
Core remains the current singleton binding and fresh identity/read authority.
Sort sparse choices by ordinal and use the original saved revision, never latest.
An accepted request opens a NEW job, preserves explicitly submitted choices for
further refinement in this mounted lineage, and never mutates old job/results.

Preserve action/inspection abort, mount epochs, selection/file generation and
same-tick double-action guards. Stale fulfilled/rejected requests must not publish
into a changed file/profile/job/attempt, clear loading, or expire a newer session.
Do not submit on effects, polling, StrictMode replay, staging or candidate click.
Ambiguous admission uses existing known-ID reconciliation without auto-retry;
unknown/ambiguous receipt does not establish a new file-to-job association. Stop
mutations after auth expiry. Keep historical evidence and useful form/error context.

Sol/medium owns panel/components, focused unit tests and preview-only CSS; main
owns the narrow client validator re-exports, task/specs, schema-validated mock
Chromium smoke, screenshots, independent review, full gate and checkpoint commit.
Tests cover raw compatibility, explicit
Search -> candidate -> Manual -> fresh NEW job, sparse drafts/pagination/reset,
full frame invalidation, generated unsafe-reference and query bounds, JSON encoded
cap, no staging I/O, StrictMode/double clicks, stale File/status/submit promises,
auth expiry and ambiguous receipts without retry. Browser smoke validates JSON
requests and new IDs against generated contracts, desktop/mobile controls,
overflow, keyboard access and empty local/session storage. All upstream/account
responses stay isolated mocks; neither UI smoke nor this slice closes the child.

### E4c: structured resolution submission HTTP and thin client (2026-10-03)

Add `application/json` to the existing authenticated POST
`/api/v1/imports/imdb/ratings?profileId=ID&profileVersion=N`. Keep the existing
UTF-8 `text/csv` request unchanged. JSON is a strict envelope `{csv,
resolutionInputs}`; choices are tagged Search or Manual, never implicit native
fallback overrides. Ordinal refers to the normalized post-skip/dedup observation
order and each choice must also match that observation's `sourceRecordId`.
Search carries an explicit query; Manual carries only `{uuid, mediaType, model}`
in an `item` reference. Unspecified ordinals remain Native. Changing a choice
means explicitly resubmitting the CSV as a new job, not modifying prior history.

Wire DTOs belong to Domain; Server converts them through existing query/reference
validators and immutable Core submission builders. Reuse the same parser, full
endpoint/profile binding, retention, ID source, one owned runtime, admission
deadline and truthful failure envelope. Authorize before reading/parsing body.
Both raw and encoded JSON bodies are capped at 10 MiB, including streaming;
decoded CSV is also capped by the existing parser. Escaping/plan overhead means
a raw CSV near the cap may not fit JSON; never silently raise the raw limit.
Bound choices to 10,000 and no more than normalized observation count; reject
unknown fields/kinds, malformed integers, duplicates/collisions, source/ordinal
mismatch and unsafe refs before admission or catalog/credential I/O. No raw CSV
or request JSON persists. Do not infer choices from old historical results.

Generate OpenAPI/JSON Schema/TS from Domain. Extend the existing focused tagged
enum closure fix only for this new choice enum; no generic schema processor.
Add a thin shared client method with generated request/response validators,
existing CSRF/Bearer/request timeout/abort/error reconciliation and no retry.
No UI/CLI command, separate catalog search endpoint, job cloning, migration,
dependency, connector advertisement, profile activation or personal/source write.

Router tests use actual runtime/SQLite/producer and fake transport/credential/time
ports: mixed Native/Search/Manual inputs, review-only candidates, fresh manual
provenance, strict source/ordinal binding and query/reference validation, raw
compatibility, auth before body polling, encoded/streamed limits and unchanged
capacity/admission failure behavior. Client/schema tests cover generated closed
variants, request bounds, exact wire body/query/headers, single request, sanitized
failure metadata, timeout/abort and malformed payload refusal. Run scoped Server
tests/fmt/Clippy, generate/check contracts, TS lint/typecheck/tests and full gate.
Public submission becomes usable through API/shared client only; interactive
search/selection UX and broader integrated acceptance remain pending.

### E4b: immutable explicit manual selections (2026-10-03)

Extend the E4a input boundary with sparse per-ordinal explicit manual choices.
Persist only a validated compact catalog reference (UUID, media type and model),
not arbitrary URLs, previous candidate metadata or historical comparisons. Reuse
the existing normalized item route/UUID validation owner in Core; derive a safe
relative detail path from the validated reference. URL parsing and same-instance
network validation remain in the transport. No new dependency is required.

Search and manual inputs for the same ordinal conflict; reject them, duplicates,
out-of-range ordinals and oversized plans before persistence. Choices are immutable
and bound to the original job's complete endpoint tuple, disabled profile revision
and expiry. Changing a choice requires a new submission, not rewriting history.
Use one repository resolution-input read (Native / Search / Manual) for the runner,
with corruption rejection and the existing whole-record deadline/live recheck.
Manual executes fresh detail only, never native fetch/search or saved-result replay;
identity/model contradictions still require review and personal writes remain off.

Ship a compatible v7 migration, strict bounded private serde storage and shared
input decoding for observation/history/store/completion paths. Retain the exact
requested manual detail reference in the private producer proof, not the final
possibly redirected item URL. Atomic result insertion must match the complete
stored input mode: Native, exact Search, or exact Manual. Historical v6 manual
results remain readable; older NULL slots default to Native for new production.
Adapt synthetic manual-write fixtures to explicitly submit a choice, never weaken
the new guard for tests. Historical result JSON remains version 1 and cannot
reconstruct a fresh record or authorize writes.

Validate fake-transport success/provenance, conflicts and unsafe references before
storage; mixed modes and fresh resume; request-reference vs redirected evidence;
corrupt stored selections; exact mode/proof refusal; rollback/reopen/retention;
populated exact-v6 migration preserving search/native inputs and old manual
history. Scope is Core/Storage/runner plus minimal Server test-port updates only.
Public DTOs, routes, UI/CLI actions and capability advertising remain pending.

### E4a: immutable explicit search inputs (2026-10-03)

Before exposing per-record search/review actions, extend internal submission with
an optional sparse ordinal -> explicit query plan. Existing submissions and saved
v5 rows remain native-only. Validate each ordinal against observation count,
reject duplicates, and reuse `validate_catalog_search_query` before persistence;
retain its trimmed, bounded query, never infer one from an observation title.
The plan is immutable for a job. Choosing another query requires a new submission;
resume preserves queries, original observations, binding, revision and expiry.

Ship migration v6 with compatible Core/Storage/runner behavior. Store nullable
`search_query` on the existing observation row, not another scheduler or generic
workflow document. NULL means native; non-NULL must be normalized and valid.
Use the existing atomic submission/read/lifecycle transactions and validate query
data on reads. Core gains no runtime/serializer/database dependency. Add a narrow
repository query read for one ordinal; the runner reads it within the same record
deadline, then rechecks live guards before any catalog operation. Query failure,
corruption or cancellation must not silently fall back to Native.

The private produced-record proof also retains the explicit normalized search
query (if any). Result insertion checks it against the stored ordinal input in
the existing guarded transaction, refusing a native result for a search input or
a result produced with another query. Historical result JSON remains unchanged
and cannot reconstruct fresh producer authority. Manual resolution remains the
existing internal producer operation; persistent manual selection is not delivered
or inferred by this search-only checkpoint.

Use fake ports/clock/credentials and temporary SQLite only. Cover validation,
native defaults, mixed native/search ordinal order, Unsupported-only fallback,
review-only candidates with zero secrets, fresh resume/reopen preserving queries,
query mismatch proof refusal, corrupt queries and bounded pending query reads.
Upgrade populated exact-v5 and all existing prior-schema fixtures, preserve
historical evidence/config/auth and prove atomic rollback. No public DTO/route,
client screen, live NeoDB/native fetch, capability or personal/source write change.
Public search/manual actions and broader acceptance remain pending.

### E3c: backend restart acceptance (2026-10-03)

Add a test-only authenticated router integration using the real IMDb CSV parser,
SQLite repositories, Core snapshot/record producer/runner and production
`PreviewRuntime::start`. Only upstream catalog/mark/identity, credentials and
clock are fake ports; this is backend integration evidence, not a live NeoDB or
real-process browser-to-upstream test. Reuse existing test helpers/dependencies.

Import at least two normalized ratings through HTTP. Gate the second upstream
operation after the first comparison is committed, then shutdown/drain and close
SQLite. Reopen the same data directory and start a new owner: unfinished work
becomes interrupted, partial history survives and startup accesses neither
transport nor credentials. Existing session/client authentication remains valid.

Create a newer disabled profile revision, then explicitly resume through HTTP.
The attempt increments, old results clear, original observations restart from
ordinal zero, original revision and expiry remain fixed, and changed mock marks
produce fresh snapshot times/diffs. Delete through HTTP and prove job-only removal
with profiles, endpoint and authentication preserved; fake secrets are read-only.
Use deterministic notifications/semaphores and bounded waits, not timing sleeps.

No API/schema, production bypass, dependency, migration, capability advertisement
or personal/source write is added by this checkpoint. Broader PRD search/manual
selection and negative-path acceptance must be audited separately before enabling
connectorIds or archiving the child; isolated/internal evidence is not public
delivery. Main owns that audit, specification alignment and final verification.

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
