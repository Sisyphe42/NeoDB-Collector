# Preview implementation checkpoints

Status: approved and activated on 2026-09-18.
Dependencies: preview-auth and preview-mapping are completed and archived.

## Delegation for remaining work (user request, 2026-09-21)

The main agent owns planning, boundaries, acceptance and checkpoint commits.
Delegate bounded local/mechanical implementation to `gpt-5.6-luna` / `max`, and
cross-module or authentication work to `gpt-5.6-sol` / `medium`. Supply the active
task path and exact scope/specs explicitly, split file ownership and prohibit
recursive delegation. Secret-value and manual-resolution checkpoints use Sol.
Sub-agents report evidence and do not commit; the main agent reviews the integrated
diff and verification results. Model availability failures must be reported rather
than silently substituting a different model. This does not remove review gates.

## Activation gate

- [x] Inspect parent approval, implemented adapters and owning product contracts.
- [x] Complete child design and ordered execution plan without expanding product scope.
- [x] User reviews these artifacts; then run `task.py start 09-16-preview-jobs`.
- [x] Load `trellis-before-dev` and each affected layer's contracts before editing.

## Ordered independently verified commits

### A. Restrictive catalog transport

- [x] Define internal Core catalog port/outcomes and implement NeoDB adapter.
- [x] Validate supported canonical source URLs before I/O; no generic URL proxy.
- [x] Handle pending, detail redirects, unsupported/rate-limit/missing responses;
  validate same-instance detail routes, limit redirects/bytes/time and sanitize errors.
- [x] Prove no Authorization header, force-refetch or personal-state mutation.
- [x] Test 202/302/404/422/429, bad/foreign Location, redirect cycles, oversized or
  malformed detail responses and upstream body redaction with loopback mocks.
- [x] Update implemented backend contract with real code/test paths and commit.

Checkpoint A verification (2026-09-18): `pnpm verify` passed, including 63 Rust
tests (10 new catalog tests), 25 TS/UI tests, Clippy, contract drift, dependency
boundaries, lint/typecheck, builds and CLI/HTTP/browser smoke. `git diff --check`
passed. Catalog transport has no public route; checkpoints B-E remain pending.

### B. Bounded resolver and identity evidence

- [x] Add runtime-independent polling/cancellation ports and fake-clock tests.
- [x] Enforce >=15-second poll spacing and <=120-second overall budget, including
  slow requests; cancellation produces no subsequent poll.
- [x] Normalize and compare exact IDs, canonical links and media granularity.
- [x] Add bounded search fallback; title-only/contradictory
  candidates remain review-required, unavailable is not absent.
- [x] Validate explicit manual selection with fresh evidence and distinct provenance;
  user-facing selection and saved mapping integration remain pending.
- [x] Test movie/show/season distinctions, pending exhaustion, cancellation,
  rate-limit behavior and identity mismatches; verify and commit.

Checkpoint B1 verification (2026-09-18): native resolver, IMDb expectation
validation and Tokio/CancellationToken adapter passed `pnpm verify`: 76 Rust
tests, 25 TS/UI tests, Clippy, contracts, boundaries, builds and browser smoke.
No live catalog fetch or personal-state write. Search/manual fallback is still
unchecked above; B is not complete and the child remains active.

### B2. Explicit manual selection validation

- [x] Add an internal `resolve_manual` operation with a distinct manual result.
- [x] Re-read the selected detail reference through existing restrictive transport;
  reuse identity/granularity rules, timeout/cancellation and sanitized failures.
- [x] Allow absent identity evidence only through explicit manual selection;
  contradictory IDs and type/granularity mismatch remain review-required.
- [x] Prove zero native-fetch/search calls, evidence preservation, invalid-input
  rejection, cancellation/timeout including hung I/O, and unchanged native matching.
- [x] Run full verification and update owning specs/status for a separate checkpoint.

Implementation: Sol/medium owns Core code/tests; main owns planning/specs, integration
review and commit. Search, manual mapping persistence and HTTP/UI integration stay pending.

Verification (2026-09-21): Sol/medium implemented B2 and the main agent reviewed
the code, tests and aligned specs. Scoped Core 30 unit + 2 compile-fail doctests
and catalog transport 10 tests passed. Full `pnpm verify` with `CARGO_BUILD_JOBS=1`
passed: 85 Rust unit tests, 2 doctests, 25 TS/UI tests, fmt/Clippy, contracts,
dependency boundaries, lint/typecheck, builds and CLI/HTTP/browser smoke.
Product YAML parsing and `git diff --check` passed. No public schema, migration,
new dependency or personal-state write was added. B remains incomplete until search
fallback is delivered; do not archive the preview child.

### Library reuse follow-up (approved 2026-09-18)

- [x] Promote the standard-library/maintained-library reuse principle to the
  repository dependency contract; keep explicit Core allowlist review.
- [x] Replace Server's handwritten CSRF comparison with `subtle::ConstantTimeEq`.
- [x] Cover absent/empty/short/long/equal-length mismatch and cross-session CSRF;
  preserve Origin/session/Bearer behavior and allow the valid token after denials.

Verification: Server's 17 tests passed offline. Full `pnpm verify` passed with
77 Rust tests, 25 TS/UI tests, Clippy, contracts/boundaries, lint/typecheck, builds
and CLI/HTTP/browser smoke. Initial parallel compilation exhausted Windows
commit memory (1455/LNK1102); rerun used process-local `CARGO_BUILD_JOBS=1`, without
system changes or skipped checks. `cargo tree -i subtle -e features --offline`
confirmed reuse of existing 2.6.1; no new crate version or Core dependency.
The secret-type follow-up is completed under user continuation; OAuth
protocol migration remains a separate assessment in `research/core-library-review.md`.
The preview child remains unfinished/active.

### Internal secret values (library reuse follow-up)

- [x] Review `secrecy` features/license/transitive dependencies; add exact allowlist entry.
- [x] Migrate credential/OAuth internal values and consuming adapters without wire changes.
- [x] Add Deserialize/Debug tests and compile-fail Serialize/Display tests.
- [x] Complete independent code review and adapter regression coverage.
- [x] Run full workspace verification and sync specs for a separate checkpoint.

Verification (2026-09-21): Sol/medium completed the code review and added mock
form/invalid-secret/scope-redaction coverage. Main agent reviewed the integrated
code/spec diff and resolved dependency features. `pnpm verify` passed with
`CARGO_BUILD_JOBS=1`: 79 Rust unit tests, 2 compile-fail doctests, 25 TS/UI tests,
Clippy, generated contracts, dependency boundaries, lint/typecheck, WebUI/CLI
builds and actual CLI/local HTTP/desktop-mobile browser smoke. No real keychain
or NeoDB account operation. Public API DTOs and database schemas are unchanged.

### B3. Bounded search fallback

- [x] Add a separate internal catalog-search port and first-page candidate result.
- [x] Compose native resolution with search only after Unsupported, sharing the
  same 120-second deadline; preserve all other native outcomes/errors unchanged.
- [x] Request one movie/TV search page only; cap query at 512 UTF-8 bytes, response
  at 64 KiB and candidates at 20. Validate references/evidence using detail rules.
- [x] Return review-only candidates (including an empty list), never automatic
  identity or personal-mark absence from search. Manual selection re-reads detail.
- [x] Mock success/limits/unsafe replies, no retry/redirect/fetch from search,
  cancellation/timeout and native outcome gating; run full verification and commit.

Sol/medium owns Core and connector code/tests; main owns specs and acceptance.
No new dependency, HTTP/UI surface, persistent mapping or job is included.

Verification (2026-09-22): Sol/medium implemented Core and adapter code/tests;
main reviewed production logic and strengthened outcome-preservation, byte bounds,
atomic-page rejection and distinct 20/21-candidate tests. Scoped Core 36 unit + 2
compile-fail doctests and catalog 17 tests passed. Final `pnpm verify`, rerun after
the last test refinement with `CARGO_BUILD_JOBS=1`, passed: 98 Rust unit tests,
2 doctests, 25 TS/UI tests, fmt/Clippy, contracts, dependency boundaries,
lint/typecheck, builds and actual CLI/local HTTP/desktop-mobile browser smoke.
Product YAML/evidence references and `git diff --check` passed. B is complete;
comparison, persisted jobs and public client integration (C-E) remain pending.
Do not archive the preview child.

### C. Personal-state reads and pure comparison

Deliver C1 (authenticated single-item transport) before C2 (account-bound snapshot
orchestration and pure mapping comparison). This keeps absence semantics separately
verifiable; neither checkpoint enables writes. C1 uses Sol/medium for code/tests,
main for upstream evidence, contracts, review and commit.

- [x] C1: Add borrowed-secret mark-read port, normalized present/MissingShelf outcomes,
  strict item/owner binding, status/size/timeout/redirect tests and full verification.
- [x] C2: Obtain credentials through existing ports, detect replaced endpoint/account,
  produce timestamped snapshots and compare mapped fields without treating unknown
  rating as absent. MissingShelf confirms only shelf absence, not rating emptiness.
  Verify token identity with `/api/me` before binding owner-less MissingShelf to
  the expected account; a stored reference alone is not response-owner evidence.

- [x] Verify mark-read API and absence semantics with primary evidence; persist
  references in canonical specs before relying on platform behavior.
- [x] Implement restrictive authenticated read port/adapter and normalized transport snapshots.
- [x] Reuse mapping evaluator; test equal, empty, conflict, missing shelf, stale
  account and unreachable state. No writer or activation path.
- [x] Verify and commit the complete internal comparison slice.

C2 implementation ownership: Sol/medium implements Core service/comparison and
fake-port tests; main owns contract updates, independent review, full verification
and commit. Reuse existing ports without a new dependency, schema or public route.
The full endpoint tuple is checked before/after secret lookup and after remote
reads; tests replace the base, account and same-account credential reference at
each boundary. MissingShelf requires matching `/api/me`; Present avoids that extra
read. Test snapshot time, retained resolution evidence, later stale-binding checks,
credential/repository/transport failures and dropped-future short-circuiting.
Comparison tests cover equal/empty/conflicting/unknown values, existing shelves,
explicit shelf suggestions independent of rule order, disabled/missing/rejected
mapping outputs and preserved mapping validation. No test uses a real account.

C1 verification (2026-09-30): Sol/medium implemented code/tests; main reviewed
the integrated diff, shared parser/identity reuse, owner binding and absence semantics.
Main reran full `pnpm verify` after the interrupted verification session, with
`CARGO_BUILD_JOBS=1`: 108 Rust unit tests, 2 compile-fail doctests, 25 TS/UI tests,
fmt/Clippy, generated contracts, dependency boundaries, lint/typecheck, builds and
actual CLI/local HTTP/desktop-mobile browser smoke passed. Product YAML/evidence
references and `git diff --check` passed. Only loopback mocks were used; no live
NeoDB account, credential store, public API or persistence change. Account-bound
snapshots and pure comparison remain C2; C-E and the preview child remain unfinished.

C2 verification (2026-09-30): Sol/medium implemented the internal snapshot service,
pure comparison and 14 fake-port tests. Main reviewed the integrated code and
strengthened exact error assertions, valid identity fixtures, preserved sink/evidence,
numeric behavior and rule-order checks. Scoped Core 50 unit + 2 doctests passed.
Main's full `pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 122 Rust unit tests,
2 compile-fail doctests, 25 TS/UI tests, fmt/Clippy, generated contract drift,
dependency boundaries, lint/typecheck, builds and actual CLI/local HTTP/desktop-mobile
browser smoke. Product YAML/evidence references and `git diff --check` passed.
Snapshot/comparison remains internal and historical, not execution authority; no
public DTO, migration, new dependency, real account or OS credential access.
Checkpoint C is complete; persistent jobs and public integration (D-E) remain
pending. Keep the preview child active rather than archiving it.

### D. Persistent resumable jobs

- [x] D1: Internal repository + migration, normalized bounded submission, ordered
  reads, atomic claim/attempt guards, cancellation/interruption/explicit resume,
  deletion/expiry primitives and prior-schema migration/rollback/reopen tests.
- [ ] D2: Persist validated identity/snapshot/comparison results with conditional
  completion, connect the bounded runtime worker, startup recovery and retention
  configuration/scheduling. Run fresh comparison after explicit resume.
  - [x] D2a: Internal single-record producer joining resolution/snapshot/comparison,
    explicit provenance, whole-record cancellation/deadline and binding checks.
  - [ ] D2b: Compatible result persistence and conditional completion, followed by
    runtime worker/startup/configuration composition; do not mark D2 complete at D2a.
    - [x] D2b1: Consume produced records through an atomic bounded result repository,
      current-attempt pagination and conditional completion; compatible v5 migration.
    - [ ] D2b2: Compose bounded worker, startup recovery and retention configuration.
      - [x] D2b2a: Internal sequential single-attempt runner using existing ports
        and real record production; no automatic runtime/startup/public surface.
      - [x] D2b2b: Server-owned single-job admission/cancellation and lifecycle.
      - [ ] D2b2c: Startup recovery and retention configuration/periodic cleanup.

Main owns design/contracts/acceptance/commit; Sol/medium implements Core repository
types and SQLite adapter/tests. Do not create migration-only commits, public DTOs,
general workflow infrastructure or completion without produced comparison results.

D1 verification (2026-09-30): Sol/medium implemented the Core repository and
SQLite v4 adapter/tests; main reviewed input boundaries, exact account/profile
binding, read/write transactions, attempt guards and migration fixtures. Review
refinements cover snapshot-consistent pagination/delete races, rollback, queued
and running recovery after clock regression, fixed retention, JSON-escaped byte
limits, attempt overflow and preservation of profiles/account/auth rows. Scoped
Core 55 unit + 2 doctests, Storage 26 tests, IMDb 6 focused tests and Clippy passed.
Main's full `pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 141 Rust unit tests,
2 compile-fail doctests, 25 TS/UI tests, fmt/Clippy, generated contracts, dependency
boundaries, lint/typecheck, builds and actual CLI/local HTTP/desktop-mobile browser
smoke. Product YAML/evidence references and `git diff --check` passed. No new
dependency, public DTO/route, worker, result completion or real-account/keychain
operation. D2/E remain pending and this child remains active.

D2a ownership: Sol/medium owns `collector-core/src/preview_records.rs`, its tests,
module registration and the minimal reusable expected-binding method in `preview.rs`.
Main owns design/contracts, review, full verification and checkpoint commit.
Use fake ports/clock/control; cover matched/manual comparison and preserved
evidence, unknown/empty/conflict, unsupported-only search, unresolved outcomes with
zero secret/mark reads, validation before I/O, stale/disconnected tuple across
stages, port failures and cancellation/hung work/outer deadline in every I/O phase.
Use existing library mechanisms and ports, not custom production polling/racing.
No Storage/Server/client or public-schema changes belong to D2a.

D2a verification (2026-10-01): Sol/medium implemented the Core producer and 11
fake-port tests; main reviewed the full code/tests and aligned the owning contracts.
Review additions cover hung opt-in search under cancellation/expiry and endpoint
rotation/disconnection before publication of unresolved results. Scoped Core 66
unit + 2 compile-fail doctests and fmt passed. Main's full `pnpm verify` with
`CARGO_BUILD_JOBS=1` passed: 152 Rust unit tests, 2 doctests, 25 TS/UI tests,
fmt/Clippy, generated contracts, dependency boundaries, lint/typecheck, builds and
actual CLI/local HTTP/desktop-mobile browser smoke. Product YAML/34 evidence IDs
and references plus `git diff --check` passed. Tests used fake ports or loopback
mocks, without live NeoDB or OS credential access. No dependency, migration,
public DTO/route, worker or persistent result completion was added. D2a is complete;
D2/D2b/E remain pending, so keep the preview child active.

D2b1 ownership: Sol/medium owns Core/Storage implementation, migration and tests.
Main owns the persisted design/specs, independent review, full gate and commit.
Read the D2b1 design and existing preview-jobs/records/comparison, mapping-storage,
state-and-pairing and repository-layout contracts. No Server/client/API or new
library version; Storage may directly reuse workspace serde derives for strict
private persistence DTOs. Core's dependency allowlist remains unchanged.
Verify the migration with a populated v4 database and every older fixture;
prove that decoded historical evidence cannot be used as a fresh producer record.

D2b1 verification (2026-10-01): Sol/medium implemented the Core contracts,
Storage adapter, compatible v5 migration and fake-port tests; main reviewed the
full diff and aligned the owning specs. Review refinements cover exact producer
inputs, current endpoint generation, atomic timestamp updates, structural extent
audits, shared comparison-policy reuse, consistent sink states, late terminal
writes before deletion, sparse pages, rollback and JSON-escaped byte bounds.
Scoped Core 68 unit + 2 compile-fail doctests and Storage 36 unit tests passed.
Main reran full `pnpm verify` after the final test additions with
`CARGO_BUILD_JOBS=1`: 164 Rust unit tests, 2 doctests, 25 TS/UI tests, fmt/Clippy,
generated contracts, dependency boundaries, lint/typecheck, WebUI/CLI builds and
actual CLI/local HTTP/desktop-mobile browser smoke passed. Product YAML/34
evidence IDs and references plus `git diff --check` passed. Populated exact-v4
and prior-v1-v3 fixtures upgrade without losing operational state; result data
contains no input proof, raw payload or token. Storage directly reuses existing
locked workspace serde without new versions/features or a Core dependency.
Tests use fake ports or loopback mocks, not live NeoDB or OS credential access.
D2b1 is complete; worker/startup/retention composition (D2b2), D2/D2b and E remain
pending. No public capability/API or actual scheduled preview was enabled; keep
the preview child active.

D2b2a ownership: Sol/medium implements `collector-core/src/preview_runner.rs`,
its tests and module registration only. Main owns design/specs, review, full gate
and checkpoint commit. Use existing pure adapter callback, ResolutionControl,
job/result/mapping/access/clock ports and PreviewRecordService; no new trait solely
to wrap a pure function, no serializer/runtime/dependency or source matcher in Core.
Read the D2b2a design, preview-jobs/records/results, mapping-storage/evaluator,
repository-layout and connector/IMDb contracts. Test against fake ports and the
real producer without live accounts/keychain. Do not modify Storage/Server/CLI,
public contracts or mark D2b2 complete. Report scoped tests/fmt/Clippy, then stop
editing so main can independently review and run full `pnpm verify`.

D2b2a verification (2026-10-01): Sol/medium implemented the internal runner and
13 fake-port tests. Main independently reviewed production/tests, requested fixed
claim-time tuple binding, a shared setup deadline and truthful cleanup races,
then reviewed the supplemental pending-phase tests after the sub-agent hit its
usage limit. Main reran scoped Core tests (81 unit + 2 compile-fail doctests),
fmt and Core Clippy, followed by full `pnpm verify` with `CARGO_BUILD_JOBS=1`:
177 Rust unit tests, 2 doctests, 25 TS/UI tests, fmt/workspace Clippy, generated
contract drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual
CLI/local HTTP/desktop-mobile browser smoke passed. Product YAML/34 evidence IDs
and references plus `git diff --check` passed. Tests cover empty/205-record batches,
real snapshot comparison, partial failure, exact profile revision, fixed binding,
expiry after endpoint reads, a second runner denied ownership, cancel/timeout/drop
and cleanup races including resume/delete. No live NeoDB or OS credential access,
new dependency, migration, public DTO/route, automatic scheduler or API capability.
Only D2b2a is complete; Server admission/cancellation (D2b2b), startup/retention
(D2b2c), D2/D2b/D2b2 and E remain pending. Keep the preview child active.

D2b2b ownership: Sol/medium owns Server `preview_runtime.rs`, its tests and minimal
module registration/resolution-control comment alignment. Main owns persisted
design/contracts, independent review, full gate and commit. Load D2b2b design,
preview-runner/jobs/results, foundation-runtime, repository-layout and
state-and-pairing contracts. Use existing Tokio semaphore/cancellation and
tokio-util TaskTracker; fixed bounded admission and real runner, not a fake
processor port or general scheduler. Do not modify CLI/HTTP/Storage/Core or public
schemas without first reporting a concrete missing reusable primitive.
Library review: locked Tokio 1.53.1/tokio-util 0.7.19 already provide the needed
primitives; tokio-util local manifest declares MIT and Rust 1.71, and workspace
`rt` enables task/sync support. TaskTracker `close` does not reject future spawns,
so the runtime must explicitly serialize closed-state checks with registration.
Its wait tracks task destruction; neither observer drop nor tracker drop alone
cancels work. Retain owned CancellationToken/drop guards and scoped deregistration.
No version/features/allowlist change; local checks are not three-OS evidence.

D2b2b verification (2026-10-01): Sol/medium implemented Server runtime ownership
and 10 tests using the real Core runner/producer, temporary SQLite and fake
transport/secret ports. Main reviewed all production/tests and requested a missing
Tokio-context guard, deterministic observer-drop verification, serial completion
across distinct jobs, real resumed-attempt cancellation isolation and exact-deadline
management tests. Management reuses fresh independent TokioResolutionControl with
strict 10-second checks; signalling a record token does not cancel management I/O.
Scoped Server 27 tests, fmt and Server Clippy passed. Main strengthened different-
generation duplicate admission, nonqueued rejection and fully released shutdown
assertions, then ran full `pnpm verify` with `CARGO_BUILD_JOBS=1`: 187 Rust unit
tests, 2 compile-fail doctests, 25 TS/UI tests, fmt/workspace Clippy, generated
contract drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual
CLI/local HTTP/desktop-mobile browser smoke passed. Product YAML/34 evidence IDs
and references plus `git diff --check` passed. No live NeoDB/account/keychain,
new dependency, migration, HTTP/client schema or CLI startup composition. One
shared internal owner admits at most 9 attempts with one active permit; shutdown
does not fabricate persisted user cancellation. D2b2b is complete; startup/recovery/
retention (D2b2c), D2/D2b/D2b2 and public delivery E remain pending. Keep this
preview child active; do not archive it at this checkpoint.

- [ ] Implement repositories, ordered migration, atomic bounded submission,
  version/account binding and conditional worker transitions.
- [ ] Add bounded runtime worker, startup interruption, explicit fresh resume,
  cancellation/deletion race handling and configurable retention cleanup.
- [ ] Test migration/reopen from prior schemas, crash/restart, duplicate workers,
  failed transactions, late completion, expiry and preservation of credentials/profiles.
- [ ] Audit persisted columns/values against the no-raw-payload rule; verify and commit.

### E. Shared API and client delivery

- [ ] Define domain DTOs and executable API contract; regenerate schemas/TS validators.
- [ ] Add guarded import/status/results/cancel/resume/delete routes, body and
  pagination caps; test session CSRF, Bearer scopes, invalid input and redaction.
- [ ] Add thin shared client, CLI commands and WebUI progress/diff/review controls.
- [ ] Test generated-response validation, loading/error/cancellation flows and
  end-to-end CSV -> preview -> restart -> resume -> delete with mock NeoDB.
- [ ] Enable truthful capability reporting only for delivered operations.
- [ ] Commit coherent API/client checkpoints after their full validation.

## Verification gates

For each slice run scoped Rust tests and formatting while iterating, then before
handoff/commit run `pnpm verify` (workspace Rust tests, Clippy, generated contract
drift, dependency boundaries, lint/typecheck, frontend tests/build and smoke) plus
`git diff --check`. Inspect actual command outcomes; do not infer all-platform
success from local Windows checks. Tests use fake clocks/credentials and loopback
transport; the integration result is not proof of real-account operation.

For the final child pass, load `trellis-check`, check every affected layer rather
than only the last client change, then `trellis-update-spec`. Reconcile all PRD
acceptance checkboxes with test evidence. Archive this child only after all five
checkpoints and its acceptance criteria pass. Keep it active between checkpoints.
Parent integration and archival remain separate from completion of this child.

## Review / rollback points

- Review catalog path and source URL validation before adding HTTP entrypoints.
- Review account replacement, retention and deletion races before persistence.
- Ship migrations with their compatible implementation, never as an isolated
  schema-only checkpoint. Roll back through follow-up commits; never reset user work.
- New product direction, ambiguous ownership or failed/missing checks pauses the
  automatic commit path. Ordinary verified code/spec-alignment commits need no
  additional confirmation. Do not push or create a PR automatically.
