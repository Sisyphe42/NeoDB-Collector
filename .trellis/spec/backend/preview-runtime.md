# Owned preview runtime

## 1. Scope / Trigger

D2b2b/c and E2b1 under `09-16-preview-jobs` add internal runtime admission and lifecycle in
`crates/collector-server/src/preview_runtime.rs`, around the real Core
[single-attempt runner](./preview-runner.md), plus CLI startup composition,
restart interruption and periodic retention cleanup. Public API/client delivery
is owned by [Preview HTTP](./preview-http.md) and
[Preview Management HTTP](./preview-management-http.md). E2b1 adds owned internal
submission/resume; public CSV upload/resume is owned by
[Preview Submission HTTP](./preview-submission-http.md); screens are owned by
[Historical Preview WebUI](../frontend/preview-panel.md). Final end-to-end acceptance
remains pending beyond the verified mock-port backend restart chain. No new runtime
dependency or migration.

## 2. Signatures

`PreviewRuntime<T,F>::new(runner: Arc<PreviewAttemptRunner>,
jobs: Arc<dyn PreviewJobRepository>, clock: Arc<dyn Clock>, transport: Arc<T>,
identity: F)` requires the existing catalog/search ports and a pure trusted
`Fn(&ObservedItem) -> Result<CatalogIdentity, ApiError> + Send + Sync` callback;
T/F are Send/Sync/static. Runtime resolution uses this trusted callback;
Server's HTTP composition also reuses the existing Connectors CSV parser.

- `enqueue(&self, &PreviewJob) -> Result<PreviewAttemptObserver, ApiError>` accepts
  validated queued/unexpired metadata; copy only ID/attempt/expiry, not payloads/credentials.
- `submit(&self, PreviewSubmission)` and `resume(&self, &PreviewJob)` return
  `Result<PreviewAdmissionObserver, ApiError>`, reserving capacity synchronously
  before starting a repository mutation. Submit consumes validated normalized
  inputs; resume accepts only interrupted/failed/cancelled metadata.
- `PreviewAdmissionObserver::wait(self) -> Result<PreviewAcceptedAttempt, ApiError>`
  reports confirmed mutation acceptance. Receipt fields are `job_id: String`,
  `attempt: u32`, `completion: PreviewAttemptObserver`; not persisted running or
  completed status. Receipt types are internal and not serialized.
- `PreviewAttemptObserver::wait(self) -> Result<PreviewAttemptOutcome, ApiError>`
  observes completion; dropping it does not cancel accepted runtime-owned work.
- `cancel(&self, id: &str, expected_attempt: u32) -> Result<bool, ApiError>` and
  `delete(&self, id: &str) -> Result<bool, ApiError>` are async management methods.
- `shutdown(&self)` is async and closes/signals/drains the owner.
- Async `start(runner, jobs, clock, transport, identity, PreviewRetention)` returns
  `Result<(Self, PreviewMaintenanceObserver), ApiError>` after recovery and initial
  cleanup. Production hosts use start; raw new remains focused internal construction.
- `retention()` returns the validated pure Core policy for future submission.
- `PreviewMaintenanceObserver::wait(self) -> Result<(), ApiError>` observes owned
  maintenance. Observer drop does not stop it. Explicit cancellation returns Ok;
  unexpected task/channel failure is static `preview_maintenance_failed`.

All are internal Rust APIs, not HTTP routes or public DTOs. A narrow Server
management view type-erases cancel/delete for authenticated HTTP dispatch;
a separate narrow submission view exposes submit/resume/retention. CLI composes
both from the same runtime Arc, never another owner or repository bypass.
CLI serve shares exactly one owner; recovery/cleanup never enqueue or resume saved jobs.

## 3. Contracts

- One owner shared by the production composer has one execution permit across
  all jobs. Independent owners do not promise process-wide admission exclusion;
  the repository still prevents duplicate claim of the same attempt.
- Bound accepted attempts at 9 (one permit and at most 8 additional waiters) before
  spawning. Reject duplicate local job IDs, including
  an older locally registered generation, and reject full admission without
  an unbounded list of futures. A cancellable single semaphore serializes work.
- Use existing Tokio/tokio-util primitives, not a second processor port, generic
  workflow engine, polling database scheduler or custom future race. The runner
  retains exact binding/profile and per-record deadlines, using native resolution
  by default and only immutable explicit search inputs from the job repository.
  Runtime ownership/admission is unchanged by E4a; no public query action exists.
- Serialize registration/spawn with the closed-state check. TaskTracker close
  alone does not prohibit later spawns. Release task registration and permits
  before delivering a normal result; panic/drop must release them through RAII.
- Check for an active Tokio handle before registration/spawn, so missing runtime
  context cannot panic while the registration mutex is held. A tracked outer task
  awaits its standard inner JoinHandle to sanitize panic failure; tracker shutdown
  therefore waits for inner task destruction too, without catch-unwind machinery.
- Workers own child cancellation guards but never retain the runtime owner's
  shutdown guard in an Arc cycle. Observer drop leaves accepted work running.
  Owner drop signals all work, including queued children; it does not promise
  synchronous task destruction. Explicit shutdown waits for tracked destruction.
- Submit/resume reserve the same nine-slot capacity and ID exclusion as enqueue
  before database I/O; full/closed/duplicate admission performs no mutation.
  Resume reserves the checked next attempt and retains the existing fixed expiry.
  Submit requires `created_at <= now < expires_at`; resume also refuses time
  regression before mutation. Core validation and atomic Storage binding/profile/
  lifecycle guards remain authoritative. No new processor port or scheduler.
- A single tracked task owns preparation and execution. Prepare uses existing
  strict ten-second ResolutionControl, capped by remaining retention, with the
  registration's child cancellation token. Do not hold the state mutex over I/O.
  Confirmed persistence continues directly into the real runner on the same
  registration; never persist then call enqueue, recheck capacity or reload solely
  to create an acceptance response. Only execution waits for the shared permit.
- Dropping an admission or completion observer never aborts already accepted
  preparation. Error/refusal/panic/cancellation releases registration before error
  notification and performs no catalog/keychain operation. SQL may commit before
  timeout/shutdown/panic is observed: static failures instruct status inspection,
  never promise rollback, erase a possibly committed job or automatically retry.
  A successful receipt reports only persistence acceptance; later execution may
  stop or fail. Fresh resume always reads all original observations again.
- User cancellation signals only the matching local generation before the
  repository's attempt-guarded cancel. Deletion signals the local job before
  repository deletion. Use a separate bounded management budget, not the already
  cancelled record token. Reuse a fresh independent TokioResolutionControl for
  strict 10-second pre/post deadline checks; bare Tokio timeout may accept ready
  work after its deadline. No network or credential write belongs to management.
- Repository false/error/timeout is not proof cancellation/deletion persisted.
  Return truthful results. Completion may have committed before interruption:
  persisted status is authoritative. Never roll back async commits by assertion.
- Shutdown closes admission then signals/drains local work without marking it as
  failed or user-cancelled. Startup interrupts unfinished attempts; no automatic resume.
  Preserve partial review history and unrelated profiles/account/credentials.

Production start interrupts queued/running then deletes expired job data under
one independent strict ten-second control budget, before publishing HTTP/bootstrap.
Static errors prevent startup; cancellation cannot undo already committed SQL.

A tracked task uses Tokio interval_at every 60 seconds, MissedTickBehavior::Skip,
first tick delayed. Each tick samples wall time once and uses fresh cancellation-
aware strict ten-second control. Signal expired registered attempts before job-only
cleanup; transaction guards prevent late results. No scan/auto-enqueue/retry.
Retention defaults to 30 positive u32 days and remains fixed per saved job.
Changing configuration never rewrites existing expiry or extends it on resume.
Recovery/cleanup perform no transport/keychain operation. Periodic failure/panic
closes admission, cancels children and notifies the host, which gracefully stops
serving with a static error, not silent maintenance loss. Shutdown drains attempts
and maintenance before pool close; the serve lease is released last.

## 4. Validation & Error Matrix

| Condition | Required outcome |
|---|---|
| Nonqueued metadata | preview_runtime_job_invalid; no task |
| Expired enqueue/resume metadata | preview_runtime_job_expired; no task |
| No active Tokio runtime | preview_runtime_unavailable; no registration |
| Duplicate locally registered job | preview_runtime_duplicate; no second task |
| Admission full | preview_runtime_busy; no unbounded spawned waiter |
| Closed runtime | preview_runtime_closed; no registration or remote work |
| Submit future-created/expired metadata | preview_runtime_job_invalid; no mutation |
| Resume time regression | preview_runtime_job_expired; no mutation |
| Clock regresses before/after preparation I/O | preview_submit_failed / preview_resume_failed; no execution, do not assert rollback |
| Ineligible resume status / generation overflow | preview_runtime_job_invalid / preview_attempt_exhausted; no mutation |
| Unknown submit/resume error, refusal, hang, panic or ambiguous commit | preview_submit_failed / preview_resume_failed; query status, no fabricated rollback/success |
| Known submit validation/binding/profile/storage-size or resume transition/binding/profile error | Preserve stable code with a reconstructed static message, never echo port diagnostics |
| Admission observer dropped during mutation | Owned preparation continues under its deadline |
| Cancelled while waiting | No claim/catalog/credential/result calls |
| Wrong cancellation attempt | Newer local generation remains running; repository guard decides mutation |
| User cancellation / deletion | Signal matching work before bounded repository request |
| Cancel/delete repository failure/timeout | preview_cancel_failed / preview_delete_failed; status is authoritative |
| Task panic / observer channel closed | preview_runtime_worker_failed; no fabricated terminal status |
| Observer dropped | Accepted runtime-owned work continues |
| Runtime owner dropped | Children signalled; no implicit persisted cancel/fail |
| Explicit shutdown | No later admission; tracked tasks drained |
| Startup interruption/cleanup error or deadline | preview_startup_recovery_failed / preview_startup_cleanup_failed; no published host |
| Periodic failure/deadline/panic/channel failure | preview_maintenance_failed; close admission, cancel work, stop host |

## 5. Good / Base / Bad Cases

Good: task A holds the single permit; B waits and can be explicitly cancelled
without a catalog request. Base: an observer is discarded and the accepted job
finishes normally. Bad: a shutdown sets jobs cancelled, or a duplicate request
cancels the legitimate owner rather than reporting local duplicate admission.
Good: full admission rejects resume without deleting its historical results.
Base: accepted header-only normalized input completes without catalog calls.
Bad: create a durable job before asking a runtime with no capacity to execute it.

## 6. Tests Required

`crates/collector-server/src/preview_runtime/tests.rs` uses the real Core runner
and record producer with fake transports/secrets, deterministic Tokio scheduling
and memory/temporary SQLite state. Prove global single execution across IDs,
bounded admission, duplicate exclusion, queued/active cancellation, wrong-attempt
isolation, late delete/results, partial failure, permit/registration release on
panic/errors, observer drop, owner drop, closed-state/shutdown races and bounded
management failures. No live NeoDB or OS credentials. Run scoped Server tests,
fmt/Clippy, full `pnpm verify`, YAML/evidence and `git diff --check`.

Concrete test assertions include `distinct_jobs_complete_sequentially_and_reuse_the_permit`,
`cancellation_delete_and_wrong_attempt_are_truthful` (real resume and fresh fetch),
`observer_drop_does_not_cancel_accepted_work`,
`shutdown_signals_active_and_queued_without_persisted_cancel`,
`panic_observer_is_sanitized_and_registration_is_released`,
`enqueue_without_executor_is_static_and_does_not_register`, and
`management_timeout_and_errors_are_bounded_and_sanitized` (local interruption
does not fabricate persisted cancellation, and gated ready results at exactly
10 seconds still fail the deadline). Owner-drop and partial-result failure
tests check the same lifecycle using real runner/SQLite operations.

D2b2c tests prove queued/running interruption and terminal/partial-history
preservation across reopen, expired data removal without profile/account/auth loss,
no automatic transport/secret reads, delayed/skipped periodic ticks, active/waiter
expiry cancellation, bounded startup/periodic static errors and late/hung ports,
observer/owner drop and shutdown drain. CLI composition tests use injected
shutdown/fakes; smoke rejects an ephemeral-port second host on the same database
and verifies doctor remains usable without recovery ownership.

Independent-review regressions include
`production_start_without_executor_performs_no_recovery_or_cleanup`,
`startup_cleanup_shares_recovery_budget_and_rejects_hung_or_late_replies`,
`a_repository_cancellation_code_does_not_fabricate_host_shutdown`, and
`maintenance_observer_drop_keeps_cleanup_owned_and_owner_drop_drains`.
The partial-history startup test also retains a live hashed client credential.

E2b1 tests in the same runtime test owner additionally prove bounded/duplicate/
closed admission before submit/resume I/O, real normalized submission and fresh
resume, exact revision/full binding/fixed expiry, refused persistence retaining
history, observer drop during pending persistence, static error/panic/hang/late
commit behavior, cancellation and shutdown drain, and capacity reuse. All prepare
failure paths assert no catalog/keychain use; ambiguous commit tests inspect real
persisted status rather than asserting rollback. Public wire schemas stay unchanged.
Concrete regressions include
`submit_admission_is_bounded_duplicate_safe_and_closed_before_mutation`,
`resume_admission_is_bounded_and_closed_before_mutation`,
`dropped_admission_observer_does_not_abandon_pending_submission`,
`preparation_rejects_clock_regression_before_and_after_commit`,
`cancellation_after_resume_commit_is_truthful_and_skips_catalog`,
`retention_caps_the_preparation_deadline`,
`shutdown_drains_held_preparation_before_and_after_commit`, and
`accepted_execution_panic_is_sanitized_and_releases_registration`.
The extended `runner_failure_releases_admission_and_preserves_partial_results`
proves changed-binding refusal keeps attempt-1 history, and successful resume after
profile v2 exists still uses v1, restarts from ordinal zero and obtains fresh results.

E3c `crates/collector-server/src/preview_restart_tests.rs::authenticated_import_restart_resume_and_delete_preserve_unrelated_state`
composes authenticated routes, the actual CSV/identity adapter, SQLite, real
snapshot/producer/runner and production `start`, with fake upstream/credential/time
ports. It commits one equal comparison before holding the next fetch, drains and
drops old owners, closes/reopens SQLite and proves startup interruption preserves
the exact historical HTTP item with no upstream/secret access or automatic resume.
Explicit resume refetches both original ordinals, clears prior history and retains
v1/fixed expiry despite a semantically changed disabled v2; changed marks yield
fresh conflicting comparisons. HTTP deletion removes job data only, preserving
the full endpoint tuple, both revisions, session and paired-client authentication.
This is backend integration evidence, not CLI/browser-to-real-NeoDB-client
end-to-end or live-account evidence. Public search/manual integration remains pending.

## 7. Wrong vs Correct

Wrong: spawn every request, then acquire a permit and forget its task handle.
Correct: bounded registration before spawn, cancellation-aware permit acquisition,
scoped task cleanup and explicit tracked shutdown.

Wrong: dropping TaskTracker cancels workers, or its close forbids spawn.
Correct: owner-held cancellation guards signal work; an explicit closed-state
lock serializes admission and shutdown, and tracker wait confirms destruction.

Wrong: recover another live host's database by choosing a different port, or close
SQLite while maintenance still owns a cleanup future.
Correct: hold the serve lease before recovery, drain all work before pool close,
and release the lease last. See [Runnable Foundation](./foundation-runtime.md).

Wrong: `jobs.submit(input).await?; runtime.enqueue(&job)?` may leave an unowned
queued job when admission is full, and caller disconnect can abandon preparation.
Correct: reserve capacity before mutation; a tracked prepare-and-execute operation
keeps the reservation through execution while observers are disposable.
