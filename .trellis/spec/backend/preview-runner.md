# Single-attempt preview execution

## 1. Scope / Trigger

D2b2a under `09-16-preview-jobs` composes existing internal repositories and the
real record producer in `crates/collector-core/src/preview_runner.rs`. It executes
one explicitly invoked existing attempt, not an automatic background scheduler.
Server [owned runtime](./preview-runtime.md) provides internal admission/cancellation;
startup/recovery and retention configuration are wired through Server/CLI;
public API/thin clients, [CLI commands](./cli-preview.md) and
[screens](../frontend/preview-panel.md) are implemented; final end-to-end acceptance
remains pending. No new dependency or public capability. E4a's compatible search
input migration and E4b manual inputs are owned by [preview jobs](./preview-jobs.md).
Tests live in `src/preview_runner/tests.rs`.

## 2. Signatures

`PreviewAttemptRunner::new(jobs, results, mappings, access, clock, producer)` owns
Arc references to existing job/result/mapping/access/clock ports and
PreviewRecordService. `run<T,C,F>(&self, job_id: &str, attempt: u32,
transport: &T, control: &C, identity: &F)` returns
`Result<PreviewAttemptOutcome, ApiError>` with Completed / NotClaimed / Stopped.
Transport implements existing catalog/search ports; ResolutionControl owns
runtime timeout/cancellation. The pure trusted callback accepts `&ObservedItem`
and returns `Result<CatalogIdentity, ApiError>`; the existing built-in IMDb
`catalog_identity` function can be passed directly by the runtime composer.

There is no new processor framework or public DTO. Core never acquires a connector,
runtime or SQLite dependency. Native remains the default; E4a consumes immutable
explicit search queries or validated manual references from original job inputs.
It never infers queries/manual
choices from display hints or persisted historical results.

## 3. Contracts

- Validate positive attempt/job ID, wall-clock validity and cancellation before
  ownership or remote work. Initial queued status and retained full endpoint tuple
  must agree. Load only the exact disabled mapping profile ID/version.
- Atomic `PreviewJobRepository::claim` is the sole ownership grant. A losing
  invocation cannot fail another worker's attempt or access catalog/credentials.
- Read original observations in pages of at most MAX_PREVIEW_PAGE (100), checking
  expected page sizes and normalized inputs. Process each sequentially at stable
  ordinal; save its immutable produced record immediately. Do not buffer all results.
- Reuse the existing mapping evaluator, PreviewRecordService and native resolver.
  Identity/model derivation stays in the trusted adapter callback, not a second
  IMDb mapper in Core. Preserve all Compared/unresolved outcomes unchanged.
- Read the current ordinal's complete resolution input through
  `PreviewJobRepository::resolution_input`
  within its existing whole-record deadline. Revalidate the query and recheck
  live job/binding/time guards after that awaited read before catalog work.
  Native is the default; a valid explicit query selects Search, which still permits
  fallback only after native Unsupported. Manual derives a safe relative detail
  reference and calls only fresh explicit manual resolution. It never invokes
  native fetch/search or replays stored candidate evidence. Input errors/corruption/cancellation
  never silently select Native. Search candidates remain review-only and make
  no credential/mark read; the runner never auto-selects a candidate.
- Before each record, persistence and completion, check running status, expected
  attempt, original and freshly loaded job bindings against the full endpoint tuple
  retained at claim, and live endpoint equality. Recheck wall-clock bounds after
  awaited endpoint reads. Atomic
  result storage/completion still decides races: false stops the run without
  further remote work or overwriting another attempt/terminal state.
- A claimed empty batch may complete. Completion is conditional and only follows
  processing all original ordinals. It never grants write/activation authority.
- Reuse ResolutionControl.run for repository and producer I/O, including hung
  ports. Each record's checked monotonic deadline is capped by RESOLUTION_BUDGET
  (120 seconds) and remaining wall-clock retention. Setup/claim, page reads and
  completion are also bounded. Initial load and claim share one setup deadline;
  profile/endpoint reads cannot restart its budget. Never apply a 120-second total-job limit to a large
  batch or reset the record deadline between guards/production/persistence.
- On ordinary error after a known successful claim, request only attempt-guarded
  fail; retain partial history. Cleanup has its own maximum 10-second budget capped
  by remaining retention. If fail returns false, reload under that same deadline:
  missing/terminal/different-attempt state is supersession, so preserve the original
  error. A still-running same attempt or failed cleanup I/O returns the static
  `preview_failure_cleanup_failed` error, not success. Cancellation during cleanup
  remains the cancellation error. Cancellation is not automatically user cancellation:
  return the interruption error without marking failed/cancelled. Runtime owners
  distinguish user cancel from shutdown. Dropped work stays recoverable.
- Dropping an in-flight mutation does not undo an already committed database
  change. A completion may commit before interruption is observed: subsequent
  status is authoritative. Do not promise rollback across asynchronous boundaries.
- Explicit repository resume creates a new attempt and clears old results. Running
  that attempt starts from ordinal zero and reads fresh catalog/account state;
  this runner never consumes saved review output as execution input.
- Runtime global one-job admission, queue backpressure and local cancellation
  registration are not implemented by this callable service. No auto retry/resume,
  token mutation, source write, profile enablement or general workflow engine.

## 4. Validation & Error Matrix

| Condition | Required outcome |
|---|---|
| Invalid ID/attempt or negative initial clock | Existing preview validation error before remote I/O |
| Missing/nonqueued/wrong-attempt/expired job or claim race lost | NotClaimed; no failure/result/network calls |
| Missing initial binding or exact profile unavailable | NotClaimed; never implicit rebind/latest revision |
| Malformed exact profile or setup port failure | Existing error; no failure transition without known claim |
| Status/attempt/binding/expiry changes after claim | Stop before further work; conditional writes refuse late result |
| Unexpected empty/short/oversized page | preview_observation_page_invalid; no partial completion |
| Producer/port error after ownership | Guarded fail; partial results remain |
| Resolution-input read invalid/hung/interrupted or binding changes during read | Fail/stop under existing controls; no native fallback or subsequent catalog call |
| Failure transition cannot be persisted while same attempt runs | preview_failure_cleanup_failed; query persisted status |
| Failure transition refused after cancel/delete/resume | Preserve original error; never overwrite new/terminal state |
| Control cancellation | Existing cancellation error; no invented terminal status |
| Port timeout while attempt remains live | Guarded fail and timeout error; cleanup failure stays explicit |
| Retention expiry or wall-clock regression after claim | Stopped; no subsequent remote work/completion |
| Result/complete refused | Stopped; no next record |
| All rows including empty batch processed | Completed only when atomic completion succeeds |

## 5. Good / Base / Bad Cases

Good: explicit resume makes attempt 2; run rereads and compares every original
record, not old attempt-1 results. Base: an Unsupported result is saved as-is and
processing may finish without granting any write permission. Bad: a losing claim
calls fail, cancelling the legitimate owner, or a shutdown token is recorded as
user cancellation.

## 6. Tests Required

Use the real producer with fake repositories, clock, control and transports; no
live NeoDB or OS credentials. Assert empty/multipage completion, ordered ordinals,
<=100 pages and sequential calls; exact revision, no history reads, fresh resume;
claim races, terminal/attempt/binding/delete/expiry changes at phase boundaries;
partial failure, fail cleanup errors, refused result/completion, invalid pages;
cancellation/timeout/drop during pending ports and retention-limited work. Check
that a batch may exceed 120 seconds while each record stays within its own budget.
E4a tests mixed native/search ordinals, explicit query forwarding, preserved
terminal native outcomes, review-only candidates, pending query reads, and
fresh attempts retaining the immutable query plan without reading old results.
Concrete tests: `mixed_native_and_explicit_search_inputs_run_in_ordinal_order`
keeps even an exact-ID candidate review-only and re-forwards the query on a fresh
attempt; `saved_search_query_never_overrides_a_native_terminal_result` avoids
search after a native match; `resolution_input_read_is_bounded_and_followed_by_a_fresh_live_guard`
drops hung input reads on cancel/timeout and stops after rebind/delete before I/O.
E4b `mixed_manual_search_and_native_inputs_use_fresh_manual_detail_on_every_attempt`
asserts mixed-mode ordinal/attempt order, fresh selected detail and account reads
on each attempt, explicit manual provenance and unchanged review-only search.
Run Core tests/fmt/Clippy, full pnpm verify, YAML/evidence checks and git diff --check.

Concrete regression tests in `crates/collector-core/src/preview_runner/tests.rs`:
`empty_and_multipage_attempts_complete_in_stable_order` (empty and 205 records),
`matched_record_uses_real_snapshot_comparison_and_partial_failure_is_retained`,
`changed_binding_before_first_record_stops_without_network_or_fail` (rotation,
same-ID job rebind and expiry during endpoint read),
`a_second_run_loses_the_real_claim_while_the_owner_is_pending`,
`cancellation_drops_every_pending_repository_phase_without_inventing_terminal_state`,
`timeout_guards_pending_phases_and_fail_cleanup_timeout_is_static` (load, claim,
page, store, complete and fail), and
`malformed_page_fails_claimed_attempt_and_cleanup_failure_is_static` (short/oversized
pages, malformed observation and unchanged/cancelled/resumed/deleted cleanup races).

## 7. Wrong vs Correct

Wrong: claim, read all rows, run unbounded concurrent record previews, then bulk
insert results and set completed. Correct: conditional ownership, bounded ordered
pages, sequential fresh production, guarded per-record commits and exhaustive
conditional completion.

Wrong: every cancellation changes the job to cancelled. Correct: cancellation
interrupts local execution; explicit user cancel and shutdown/recovery are distinct
runtime responsibilities, with persisted status as the eventual source of truth.
