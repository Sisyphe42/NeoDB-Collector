# Persistent preview job repository

## 1. Scope / Trigger

Checkpoint D1 owns the internal Core repository and SQLite migration/adapter.
Submission, ordered reads and guarded lifecycle changes work without a worker.
Runtime startup/configuration is owned by Server/CLI; historical HTTP inspection
is owned by [Preview HTTP](./preview-http.md); cancel/delete dispatch is owned by
[Preview Management HTTP](./preview-management-http.md). Submission/resume is owned
by [Preview Submission HTTP](./preview-submission-http.md); client screens are
owned by [Historical Preview WebUI](../frontend/preview-panel.md).
No completed job can be produced by D1 alone; the compatible
[D2b1 result repository](./preview-results.md) owns insertion and completion.
[D2a single-record production](./preview-records.md) composes resolution and
comparison without changing this repository or completing jobs. Persistent result
transactions are owned by D2b1; runtime startup/configuration is wired through Server/CLI.
[D2b2a execution](./preview-runner.md) joins these ports for one explicit attempt;
it does not enable automatic scheduling or startup recovery.
Server's [owned runtime](./preview-runtime.md) manages explicitly selected jobs;
startup recovery/retention and authenticated public submission/resume are implemented.

## 2. Signatures

Core types/ports live in `crates/collector-core/src/preview_jobs.rs`; Storage
implements them in `crates/collector-storage/src/preview_jobs.rs`.

- `PreviewSubmission::new(id, Endpoint, MappingProfile, Vec<Observation>,
  created_at, expires_at) -> Result<PreviewSubmission, ApiError>` validates input.
- `with_search_queries(self, Vec<(u32, String)>) -> Result<Self, ApiError>` attaches
  an explicit sparse ordinal/query plan; `search_query(ordinal) -> Option<&str>`
  exposes normalized internal input. The ordinary constructor remains native.
- `with_manual_selections(self, Vec<(u32, CatalogReference)>) -> Result<Self, ApiError>`
  attaches explicit sparse choices, mutually exclusive with search on an ordinal.
  `CatalogReference` lives in Core `catalog.rs`: `from_item(base, &CatalogItem)`
  and `from_stored(uuid, media_type, model)` validate supported route/UUID evidence;
  getters and `detail_path()` expose a safe derived relative detail path only.
- `PreviewJobRepository::submit(PreviewSubmission) -> PortFuture<PreviewJob>`.
- `load(id: String) -> PortFuture<Option<PreviewJob>>`.
- `observations(id: String, offset: u32, limit: u32) -> PortFuture<Vec<Observation>>`.
- `resolution_input(id: String, ordinal: u32) -> PortFuture<PreviewResolutionInput>`
  reads one complete validated input: `Native`, `Search(String)` or
  `Manual(CatalogReference)`. E4b replaces the E4a query-only port, avoiding
  separate awaited reads that could select an inconsistent input mode.
- `claim/cancel/fail(id: String, attempt: u32, now: i64) -> PortFuture<bool>`.
- `resume(id: String, attempt: u32, now: i64) -> PortFuture<Option<u32>>`.
- `interrupt_running(now: i64) -> PortFuture<u64>` interrupts queued **and** running.
- `delete(id: String) -> PortFuture<bool>` and
  `cleanup_expired(now: i64) -> PortFuture<u64>` remove job data only.

`PreviewRetention` in the same Core owner is pure configuration: default 30 days,
`new(u32)`, `days()` and checked `expires_at(created_at: i64)`. Zero days, negative
Unix time and overflow return static `preview_retention_invalid`. No serializer/
runtime dependency and no retroactive change to saved jobs.

`PreviewJob` has read-only metadata getters and `is_bound_to(&Endpoint)` for exact
base/account/credential-reference comparison without exposing its stored reference;
it has no Debug/serde implementation. Status is queued/running/
interrupted/failed/cancelled/completed. D1 has no completed transition; D2b1 adds
conditional completion only after every result exists. Timestamps are Unix
seconds except the existing trusted observation ingestion metadata.

`0004_preview_jobs.sql` adds:

```text
preview_jobs(id, profile_id, profile_version, base_url, account, credential_ref,
             status, attempt, created_at, updated_at, expires_at, observation_count)
preview_observations(job_id, ordinal, observation_json)
```

`0005_preview_results.sql` transactionally rebuilds the two v4 tables to permit
completed jobs and adds the normalized result table; see the result contract.
`0006_preview_search_inputs.sql` adds nullable `search_query` to each observation
row. Existing rows default to NULL. Queries are bounded to 512 UTF-8 bytes and
remain normalized operational input, not raw source metadata or workflow code.
`0007_preview_manual_inputs.sql` adds nullable `manual_selection`, a bounded strict
private JSON reference (at most 512 UTF-8 bytes) with `schema_version: 1`, `uuid`,
`media_type` and `model`. Core `catalog::MAX_CATALOG_REFERENCE_BYTES` owns the cap.
NULL query and selection mean Native. Existing v6 rows retain inputs/history.
No URL, title, external-ID array or previous candidate metadata is stored here.
Insert/update triggers refuse simultaneously non-NULL query and selection;
strict shared read decoding also rejects collisions if database constraints were
bypassed. Downgrade fixtures must remove these triggers before dropping the column.

Rows are keyed by `(job_id, ordinal)` with zero-based stable ordering and a job
foreign key; profile references target `(id, version)`. Observation JSON encodes the
existing strict Domain schema and is never a raw import/response document.

## 3. Contracts

- Store only normalized operational observations under an explicit built-in
  capability validator. Initial source is `imdb:ratings_csv`, media movie/tv,
  identity imdb:title + its derived canonical URL and integer personal rating
  1..10. Reject extra identity/state fields; never keep a raw CSV row, API payload
  or unsupported values. Preserve bounded title hints including valid multiline
  titles and trusted ingestion metadata. Connector/media IDs remain open strings.
  The IMDb title-ID predicate is owned by Core `src/catalog.rs` and reused by the
  connector parser/catalog adapter and this validator; do not duplicate its bounds
  or route Core through a connector dependency.
- Bound submission at 10,000 observations, 16 KiB per encoded observation and
  32 MiB total encoded observation bytes. Pagination accepts at most 100 records.
  Core declares bounds without acquiring a JSON/runtime/database dependency.
- Explicit search plans reject duplicate/out-of-range ordinals and invalid queries
  using `catalog_resolver::validate_catalog_search_query`; retain trimmed strings
  only. At most one 512-byte query per observation bounds additional storage.
  Query plans cannot be changed through repository methods. A different query
  requires a new submission; resume retains the original queries, profile,
  account binding and fixed expiry. Do not derive queries from title hints.
  Read-side query validation requires exact normalization and rejects corruption
  with `database_unavailable`, never silently trimming or returning native mode.
- Manual plans have the same ordinal/count/immutability limits. A compact validated
  reference stores only supported UUID/media/model; derive its relative detail
  path from the same pure route/UUID owner used for normalized item validation.
  Do not add a Core URL parser or retain arbitrary user-supplied links. Reject
  duplicates/range errors and search/manual collisions in either builder order.
  Reopen/resume retain choices, exact binding/revision and fixed expiry. Shared
  strict input decoding rejects malformed/oversized/unknown-field selection JSON
  and noncanonical bytes (re-encoding must equal the stored JSON), as well as
  collisions on observation, resolution-input, history, store and completion
  paths; never downgrade corrupt Manual to Native or Search.
- Insert metadata and all ordered rows in one transaction. The singleton endpoint
  must still equal base/account/credential reference and the exact selected profile
  must exist and remain disabled/valid. Failure leaves no partial job or rows.
- Validate the submitted profile against the stored revision's full normalized
  content, not just its ID/version. Use SQLx-managed `BEGIN IMMEDIATE` for mutations
  with read-before-write validation; it serializes writers before the validation
  read. Conditional updates still enforce lifecycle/attempt predicates. No custom
  transaction framework, application busy loop or lock across remote I/O.
- Each attempt has a positive u32 generation. Claim is a conditional queued-to-
  running write; concurrent workers cannot both claim the same attempt. Binding
  replacement or expired retention prevents claim/resume. Stored credential
  references are internal metadata and absent from future client DTOs/diagnostics.
- IDs are generated by trusted Core callers and never intentionally reused after
  deletion. D2 will reuse the existing cryptographic TokenSource for new IDs;
  clients do not choose IDs. Attempt guards identify attempts of that unique job,
  not a durable tombstone registry for arbitrary caller-reused identifiers.
- Worker transitions require both the expected attempt and running status. Cancel
  and recovery invalidate active status; explicit resume from interrupted/failed/
  cancelled increments attempt and requeues all original observations, deleting
  prior results in that same transaction. Late workers
  from prior attempts cannot update the new attempt or recreate deleted data.
- Preserve the selected immutable profile ID/version. Never rebind to a newer
  latest revision on resume. Selecting a different revision requires a new job.
  The runtime must freshly resolve/read/compare on every attempt; saved historical
  results cannot be used as fresh producer records. D1 stores no old result.
- Recovery marks unfinished queued/running jobs interrupted. Production runtime startup
  calls recovery under an exclusive host serve lease; doctor never does. Recovery
  never resumes/enqueues work. Even after clock regression recovery interrupts all
  unfinished jobs and preserves `updated_at` with `MAX(updated_at, now)`; ordinary
  transitions refuse timestamp regression. D1 has no automatic resume.
- Expiry is fixed for a job, not extended by resume. Expired jobs cannot run.
  Manual delete and expiry cleanup atomically delete rows and metadata only;
  profiles, account metadata, auth hashes and OS credentials remain untouched.
- Read-side validation rejects corrupt metadata and normalized JSON. Database
  failures have static diagnostics. No secret-store/network port is used.
- Metadata/profile/count/page reads share a read transaction, so concurrent deletion
  cannot produce mixed snapshots. Validate row count and contiguous zero-based
  ordinals and aggregate encoded bytes, not just JSON. Build the submission return
  value inside its transaction; perform no fallible database read after a
  successful commit.
- Migration ships with compatible repository operations and upgrade/reopen tests,
  never alone. Update prior-schema fixtures to remove every later table/ledger
  entry before simulating that committed version. Future-schema preflight continues
  to reject without changing database bytes.

## 4. Validation & Error Matrix

| Condition | Required outcome |
|---|---|
| Invalid submission / observation | `preview_job_invalid` / `preview_observation_invalid`; no insertion |
| Duplicate/out-of-range search ordinal | `preview_search_plan_invalid`; no submission accepted |
| Blank/control-containing/over-512-byte query | `catalog_search_query_invalid`; no submission accepted |
| Invalid compact catalog reference | `neodb_catalog_reference_invalid`; no submission accepted |
| Duplicate/out-of-range manual ordinal | `preview_manual_plan_invalid`; no submission accepted |
| Search/manual conflict | `preview_resolution_plan_invalid`; no submission accepted |
| Count / record bytes / total bytes over bounds | `preview_submission_too_large` / `preview_observation_too_large`; no insertion |
| Invalid page size / transition metadata | `preview_page_invalid` / `preview_transition_invalid` |
| Stale or missing endpoint/profile | `preview_binding_changed` / `preview_profile_changed`; no job or rows accepted |
| Insert fails midway | Whole transaction rolls back |
| Two claims for queued attempt | Exactly one winner |
| Cancel/interruption/deletion before late worker | Late conditional write affects zero rows |
| Resume with same retained binding | New attempt, all original observations |
| Attempt generation overflows | `preview_attempt_exhausted`; no transition |
| Expired job | No claim/resume |
| Delete / cleanup | Job and observations removed atomically, unrelated state kept |
| Corrupt stored metadata/profile/JSON | `database_unavailable`, never partial accepted evidence |
| Missing ordinal in resolution-input read | `database_unavailable`, never implicit Native |
| Newer database migration | database_version_unsupported, bytes unchanged |

## 5. Good / Base / Bad Cases

Good: an interrupted attempt 1 is explicitly resumed as attempt 2; a late attempt-1
worker cannot change it. Base: submitting an empty valid CSV batch stores a bounded
queued job with zero observations for later runtime handling. Bad: inserting rows
outside the job transaction, replacing the selected profile with latest, or treating
the existence of a queued record as successful comparison/completion.

## 6. Tests Required

`crates/collector-core/src/preview_jobs/tests.rs` covers normalized-record/metadata
validation; `crates/collector-storage/src/preview_jobs/tests.rs` covers repository
transactions/lifecycle. Migration fixtures live in Storage `src/lib.rs` and
`src/mapping.rs`. Required cases:
ordered bounded pagination, atomic rollback, duplicate claims, stale binding and
attempts, cancel/interruption/resume, delete/expiry, corruption, persistence/reopen
and migration from all committed prior versions. Audit stored schema/payload fields
and prove profiles/auth/account metadata survive deletion and cleanup.
E4a adds sparse-plan validation, atomic query persistence/rollback, native defaults,
query corruption, fresh resume/reopen preservation and populated exact-v5 upgrade.
Core `explicit_search_plan_is_sparse_normalized_bounded_and_unique` covers ordinal
and ASCII/UTF-8 byte boundaries. Storage
`search_inputs_survive_reopen_and_resume_without_changing_native_slots` preserves
v1/full binding/fixed expiry despite v2, while
`corrupt_search_input_fails_closed_instead_of_becoming_native` rejects padded or
invalid query inputs without publishing a normalized observation page.
E4b adds compact-reference hostile input/route tests, mutually exclusive sparse
plans, preserved mixed-mode inputs on reopen/resume, exact manual-request proof,
fresh detail with distinct manual provenance, corruption/rollback checks and
populated exact-v6 migration preserving native/search inputs and old manual history.
Concrete tests: Core `catalog.rs::tests::compact_reference_validates_item_and_derives_only_safe_relative_paths`
and `preview_jobs/tests.rs::explicit_manual_plan_is_sparse_bounded_unique_and_conflicts_in_both_builder_orders`;
Storage `manual_inputs_survive_reopen_resume_and_corruption_never_becomes_native`
and the mixed-input `failed_row_insert_rolls_back_job_and_all_observations`.
Run scoped Core/Storage tests, full `pnpm verify`, YAML and `git diff --check`.

## 7. Wrong vs Correct

Wrong: read queued status and then unconditionally set running.
Correct: one conditional write claims only the expected live queued attempt.

Wrong: old worker inserts output after cancellation/deletion/resume.
Correct: every future D2 result commit must be conditional on live running attempt
and account/profile binding in the same transaction; D2b1 provides that guarded
insertion port, not an unguarded row write.

Wrong: recover a missing/invalid search query from `title_hint`.
Correct: only an explicit validated stored query opts that ordinal into search;
corrupt input fails closed and original native jobs stay native.

Wrong: save a candidate's complete metadata or arbitrary URL as a manual job input.
Correct: save a bounded validated reference; each attempt freshly reads detail,
then preserves current identity checks and manual provenance.
