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

E5h checkpoint (2026-10-07): Sol/medium Rust agent owns only new
`preview_integration_tests/browser/polling.rs`, browser.rs registration and narrow
test-only visibility/constructor/snapshot/event-waiter/prepared-notification reuse
in resolution.rs for its existing PollState/timed handler.
Sol/medium Node agent owns only existing native browser driver, preserving all
E5f/g paths/EOF/assertions and smoke registration. Main owns task/spec/evidence,
review, independent browser repeats/visual QA/full gate and commits. Read E5h/E5c
design, frontend preview/WebUI, backend foundation/repository/state/OAuth/history/
submission/management/runtime/catalog/resolver/mark/comparison contracts. Reuse
actual composition and existing Node helpers/schema validators; no production,
dependency/API/schema/migration/capability/write, live accounts/keychain, recursive
delegation or agent commits. Rust agent alone owns serial Cargo until handoff;
Node static checks/build only, main owns actual browser/full gate after handoff.
Cancellation keeps Core alive through first response preparation +16s, observing
the next-request event under that deadline before owner drain; shutdown must not
substitute for proof that explicit cancellation suppressed the next native poll.

- [x] E5h: Native browser 202 polling and explicit cancellation/retained history.

E5h verification (2026-10-07): two Sol/medium agents implemented disjoint Rust
host/Node driver files; main reviewed the integrated lifecycle, identity/profile/
fingerprint, authentication/I/O and cleanup assertions. Two independent actual
native-driver repeats passed, preserving original happy/six-fault/EOF cases and
adding fresh one-row poll_complete/poll_cancel plus a new natural-exit EOF probe.
Completion uses unchanged >=15s native polling; cancellation keeps Core alive
through first response preparation +16s with an unexpected-request event waiter,
not immediate shutdown or arbitrary settlement sleep. Exact traces are four
GETs/one NeoDB-secret read versus one public GET/zero reads, zero NeoDB-secret
writes and no search/refetch/retry/source or personal write. Full terminal job/pages/normalized
observations/Native input survive same-origin reopen without reseeding; explicit
known-ID inspection/deletion preserves original endpoint/profiles/auth/secrets.
Closed retained/post-delete DB/WAL and public DTOs exclude E5a/E5c markers/tokens.
Main visually checked pending/cancelled desktop/mobile screenshots; overflow,
storage/page-error and actual Origin/CSRF/no Authorization checks passed. Promise
handlers attach immediately while screenshots and normal UI polling run together.
Scoped CLI 48 tests/three ignored hosts, format/scoped Clippy and Node static
checks passed. Final pnpm verify passed 307 Rust tests, three ignored browser
hosts explicitly driven by smoke, two compile-fail doctests, 85 TS/UI tests,
fmt/workspace Clippy, generated drift/boundaries, lint/typecheck/build and prior
mock/actual CLI/auth/native browser smoke. Product YAML/evidence/decisions/
disabled capabilities and task context validation passed; dependency/product
files remain unchanged and full diff checks passed. No production/API/schema/
migration/capability change. Local Windows synthetic terminal-history evidence
is not partial-row cancellation, Search/Manual, Running restart/migration,
shipped-process, native-keychain, live-account or hosted three-OS CI acceptance.
Keep child/parent active and connector IDs empty; next native browser slice is
explicit Search/Manual resolution, followed by Running restart/migration.

E5g checkpoint (2026-10-07): Sol/medium Rust agent owns only
`preview_integration_tests/browser/failure.rs` and narrow parent browser.rs
registration/helper reuse; Sol/medium Node agent owns only the existing native
browser driver, preserving all E5f checks and its smoke registration. Main owns
task/spec changes, review, independent repetitions/visual QA/full gate and commits.
Read E5g design and referenced E5f frontend preview/WebUI, backend foundation/
repository/state/OAuth/submission/inspection/management/runtime/catalog/mark/
comparison contracts. Reuse six E5b faults and current fixture, no copied Core
composer, production/dependency/API/schema/migration/capability/write changes,
live accounts/keychain, recursive delegation or agent commits. Rust agent alone
owns Cargo until handoff; Node static checks/build only, then main runs real smoke.

- [x] E5g: Native browser fault outcomes, retained evidence and job-only deletion.

E5g verification (2026-10-07): two Sol/medium agents implemented disjoint Rust
host and Node driver files; main reviewed/integrated the exact lifecycle, ordinal
pages, fingerprints, retained-state baselines, authentication/I/O boundaries and
all-owner cleanup. Six fresh synthetic cases passed through actual built WebUI,
session/CSRF/Core/runtime/SQLite/NeoDbClient with no intercepted Core responses.
Fetch 422/429 retain Unsupported/RateLimited outcomes; redirect/mark 401/Item 404/
real ten-second timeout retain only the first comparison with Failed lifecycle.
MarkHang uses ordinary UI polling, drains the released handler and cannot revive
history. Upstream 401 does not invalidate the local session. Full job/pages and
normalized observations stay equal across terminal close/scan/same-origin reopen;
confirmed job-only deletion retains exact auth/config/secrets. Traces are four
GETs/one secret read for fetch faults and six GETs/two reads for mark faults, no
trap hit/Cookie/CLI Bearer/search/retry/write or additional I/O on inspection/
reopen/delete/shutdown. Both E5a/E5b markers and tokens are absent from closed
retained/post-delete DB/WAL and public evidence.

The first actual run caught a locator using a child text rather than the renderer's
combined outcome paragraph. Corrected only the scoped test locator, preserving
the semantic assertion. Running may precede result publication; reuse bounded
real UI-poll convergence instead of an immediate-row assumption. All listeners
bind before spawning; serving errors and both joins are retained even if another
check fails. Existing E5f assertions and its independent EOF probe are preserved;
the failure host adds a separate natural-exit EOF/closed-port probe, no force-kill
success. Main repeated the full native driver twice after fixes and inspected
terminal/failed desktop/mobile screenshots; overflow/storage/page-error checks
passed. Final pnpm verify passed: 307 Rust tests, two explicitly ignored hosts
run separately in smoke, two compile-fail doctests, 85 TS/UI tests, fmt/workspace
Clippy, contracts/boundaries, lint/typecheck/build and mocked/actual CLI/auth/native
browser smoke. Product YAML/34 evidence IDs/11 refs/8 paths/unique-disjoint
decisions/disabled capabilities, task context validation and diff checks passed.

No production/dependency/feature/API/schema/migration/capability/write change,
real account/keychain or hosted three-OS CI proof. Six native faults and terminal
history are not exhaustive faults, pending-catalog poll/cancel, explicit Search/
Manual or Running restart/migration. Keep child/parent active and unarchived;
next bounded slice is native browser pending-catalog polling/cancellation.

E5f checkpoint (2026-10-07): Sol/medium owns the named test-only browser host,
parent registration/narrow concrete fixture composition, named Node browser driver
and smoke registration only. Read E5f design and frontend preview/WebUI, foundation,
repository layout, state/pairing/OAuth HTTP, submission/inspection/management,
comparison/runtime and NeoDB catalog/mark contracts. Main owns task/specs, review,
independent browser repetitions/visual QA/full gate and commits. No production/
dependency/API/schema/capability/write/live account/keychain/recursive delegation
or agent commit. Ignore host test in ordinary Rust suite; resolve/run it explicitly
via Cargo artifacts after assets build in smoke. Retain every E5a-e assertion and
default fixture behavior. Bound protocol/process/handler lifecycle, redact failure
diagnostics and drain all owners on EOF/error. Cargo is serial; stop it at handoff.

E5f review adjustment: actual 390px browser evidence exposed long generated job
IDs overflowing the heading. Main additionally owns only the `style.css` wrap/
flex fix and its browser regression check. Keep the full ID/status visible, with
no hidden overflow, Core/API/client/timer change or weakened mobile assertion.

- [x] E5f: Actual native browser-to-Core/NeoDB-adapter history and deletion.

E5f verification (2026-10-07): two Sol/medium agents implemented separate Rust
host/fixture and Node driver files. Main reviewed baseline preservation, full
history equality, deletion cascades, exact upstream/secret traces, and cleanup.
The ignored test host serves actual built WebUI/OAuth status; the Node driver uses
real bootstrap/session/CSRF and generated schemas, with no fulfilled Core requests
or substituted catalog/mark ports. Cargo artifact discovery avoids executable
path guessing. Standard-thread/channel stdin avoids uncancellable Tokio blocking
reads; an independent ready-then-EOF probe requires natural nonzero exit and a
closed Core port, rejecting forced termination as evidence.

Real BOM/Unicode CSV uses original disabled v1 despite v2 max=5; counts are 3
observations, 1 skipped row/column and 1 duplicate. Browser observes Running with
2 committed rows before releasing the final identity read, then Completed with
equal/conflict/MissingShelf Unknown results and real 2+1 ordinal pages. Host drains
and scans retained SQLite/optional WAL before same-origin reopen without reseeding;
full public job/pages and normalized observations remain equal. Browser reload
requires explicit known-ID opening, has no File/draft and cannot resume/cancel the
Completed task. Confirmed deletion removes job/observations/results only; endpoint,
both profiles, original paired-client hash/metadata/authentication and secret values
remain exact. Upstream is exactly ten GETs with public catalog/no auth, personal
NeoDB Bearer only, no Cookie/search/retry/write, three NeoDB-secret reads and no
new upstream/secret work on history reopen/delete/shutdown.

The first actual run exposed long-ID mobile overflow; main fixed only wrapping/
flex CSS, preserving full ID/status and the regression assertion. Full Playwright
allHeaders reads replaced its security-filtered headers view without weakening
Origin/CSRF checks. Main independently repeated native browser/EOF twice after
the fixes and inspected desktop/mobile screenshots. Final pnpm verify passed:
307 Rust tests, one explicitly ignored host (run separately in smoke), two
compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries,
lint/typecheck, builds, prior mock UI smoke and actual local CLI/auth/native browser
smoke. Product YAML/34 evidence IDs/11 evidence refs/8 paths/unique-disjoint
decisions/disabled capabilities and diff checks passed separately.

No dependency, feature, API/schema/migration or Core/Server/Storage/client behavior
change, shipped test backdoor, live account/native-keychain access or capability/
write activation. Local Windows completed-history evidence does not cover browser
failure/poll/cancel/search/manual or Running restart/migration. Keep child/parent
active and unarchived; the next bounded slice is native browser failure acceptance.

E5e checkpoint (2026-10-04): Sol/medium owns a named migration-test child,
parent registration and narrow E5d helper parameterization, CLI Cargo.toml's
SQLx dev-dependency plus its lockfile edge only. Read E5e/E5d design, CLI client/
preview, foundation, jobs/results/runtime, submission/inspection/management,
comparison, state/pairing, mapping storage and repository-layout contracts.
Reuse production Storage::open/real HTTP/client/NeoDbClient; test raw SQL touches
only closed temporary fixtures to construct/assert genuine populated v5/v6.
Preserve all E5a-d assertions and default E5d behavior. Main owns task/specs,
dependency review, independent repetitions, full verification and commit. No
production/API/new migration/dependency versions/features/capabilities/writes,
live account/keychain, recursive delegation or agent commit. Cargo tests/fmt/
Clippy are serial; stop Cargo before handing back. Report blockers, never expand.

- [x] E5e: Populated v5/v6 upgrade through real CLI history/resume/delete chain.

E5e verification (2026-10-04): Sol/medium added two Native-only migration cases
reusing the complete E5d chain, preserving default None behavior and every prior
assertion. Main reviewed canonical committed-schema SQL/column/index/CHECK/FK/
trigger comparison, exact retained ledger history/checksums/success and operational
rows, v7 restored NULL inputs/triggers/ledger, owner shutdown and production open
before recovery. CLI SQLx dev-dependency reuses the existing workspace settings;
lock diff is one edge only and main locked/offline metadata confirms unchanged
SQLx packages/features. No production/API/new migration/capability/write change.

Scoped E5e 2/2, CLI 48/48, fmt/Clippy passed. Main independently repeated all ten
integration tests twice (20.55s/20.58s), then pnpm verify passed: 307 Rust unit
tests, two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contract
drift, dependency boundaries, lint/typecheck, builds and actual local CLI/HTTP/
auth/browser smoke. Product YAML evidence/path/decision/disabled-capability and
diff checks passed. Native-only CLI migration evidence does not cover v6 Search/
manual history, shipped process, browser-to-upstream, native keychain or live
accounts. Child/parent stay active; connector IDs empty, writes/scheduling disabled.

E5d checkpoint (2026-10-04): Sol/medium owns a named restart integration-test
child and registration plus narrow shared CoreFixture composition changes. Read
E5d design, CLI client/preview, runtime/runner/record/jobs/results, submission/
inspection/management HTTP, account comparison, NeoDB catalog/marks, mapping
storage, state/pairing and repository-layout contracts. Main owns task/specs,
review, independent repetitions, full gate and checkpoint commit. Preserve E5a-c
assertions; reuse original origin/client/secrets on reopen without reseeding or
re-pairing. Bound handler lifecycle and fresh-attempt gates. No production,
dependency/API/schema/migration/capability/write/live-account/keychain change,
recursive delegation or agent commit. Run scoped tests/fmt/Clippy serially, stop
Cargo before main verification, and report blockers rather than broaden scope.

- [x] E5d: Real-adapter CLI restart, explicit fresh resume and job-only deletion.

E5d verification (2026-10-04): Sol/medium added the named restart child and
narrow concrete fixture composition/reopen. Seeding/pairing remain first-start
only; the same actual Core port/origin, PairReport, client token/hash and fake
secret store survive reopen. Main reviewed all code and required deterministic
shutdown before held-response release, actual handler lifecycle acknowledgment,
pre-recovery Running versus startup Interrupted, full partial-page equality,
fresh source/sink/identity/fingerprint/time assertions and complete preservation.
Closed DB/optional WAL scans run while partial evidence exists and after deletion;
opaque credential references remain permitted DB metadata but absent publicly.

Explicit real CLI resume clears attempt-1 rows before gated attempt-2 fetch,
rejects the obsolete page and freshly compares both original ordinals with later
snapshots, changed marks and original disabled v1/fixed expiry despite v2. Exact
ten GET requests/no-cookie/auth boundaries, zero automatic startup work, full
endpoint/profile/secret and original client hash/metadata preservation, repeat
delete false and not-found inspection are asserted through owned shutdown.
Scoped E5d 1/1, CLI 46/46, fmt/Clippy and two main independent integration-module
repetitions (eight tests each) passed. Main `pnpm verify` passed: 305 Rust unit
tests, two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contracts/
boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke.
Product YAML/34 evidence IDs/11 evidence refs/8 path refs/unique-disjoint decisions/
disabled capabilities and diff checks passed separately. A test-only detail URL
expectation was corrected from relative to the actual normalized absolute form;
no production workaround or timer change. No production/dependency/API/schema/
migration/capability/write/live account/keychain change. Restarted synthetic
Rust-client composition is not shipped-process restart, browser-to-upstream,
migration or complete product acceptance. Keep both tasks active; next cover
integrated migration and remaining browser acceptance without enabling writes.

E5c checkpoint (2026-10-04): Sol/medium owns a named resolution integration-test
child module and its registration in the existing CLI integration module. Reuse
the concrete CoreFixture and only narrowly extend test helpers if required.
Read E5c design plus CLI client/preview, submission/history, runtime/runner/record,
catalog resolver/search, comparison, NeoDB catalog/mark and repository-layout
contracts. Main owns task/specs, independent review/repetitions/full gate/commit.
Use real 15-second polling and actual HTTP responses; no production edits,
dependency/schema/API/migration, timer override, live account/keychain, capability/
write, recursive delegation or agent commit. Scoped Cargo tests/fmt/Clippy are
serial; stop Cargo before main validation. Report blockers rather than expand scope.

- [x] E5c: Native polling/cancel and explicit search-to-fresh-manual integration.

E5c verification (2026-10-04): Sol/medium added a named test-only child and parent
registration, reusing the concrete CoreFixture. Main reviewed all assertions and
required complete comparisons/fresh identity evidence, unchanged original status/
page, normalized input modes, preserved endpoint/profiles/secrets, public redaction
and mandatory closed DB/optional WAL scans for all scenarios. Real pending polling
keeps >=15s response-to-request spacing; cancellation observes arrival and owned
drain, never asserts receipt/internal timer entry or remote rollback. Main caught
the reused 15s terminal helper under a longer outer timeout; a local bounded 22s
reconciliation inside the 25s scenario preserves the actual production interval.
Structured imports exercise real temp files, Unsupported-only exact-ID singleton
review, native-match/429 search exclusion and fresh missing/conflicting Manual
detail. Original candidate history remains unchanged and comparisons use v1.

Scoped E5c 3/3, CLI 45/45, fmt/Clippy and two main independent full integration-
module repetitions (seven tests each) passed. Main `pnpm verify` passed: 304 Rust
unit tests, two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy,
contracts/boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser
smoke. Product YAML/34 evidence IDs/11 evidence refs/8 path refs/decisions/disabled
capabilities and diff checks passed separately. The implementing agent reported
one rustc/LLVM OOM; unchanged retry compiled and passed. An initial usage-limit
interruption resumed on the same Sol/medium model without substitution. No
production/dependency/API/schema/migration/capability/write change. Synthetic
Rust-client evidence is not shipped-process success, live account/keychain,
browser-to-upstream or integrated restart/migration acceptance. Keep both tasks
active and connector IDs empty; next cover real-adapter restart/resume/delete.

E5b checkpoint (2026-10-04): Sol/medium owns test-only changes to the existing
CLI preview integration module and an optional named child failure-test module.
Read E5b design plus CLI client/preview, submission/history, runtime/runner,
catalog resolver, comparison, NeoDB catalog/mark and repository-layout contracts.
Reuse a concrete shared fixture without weakening E5a; main owns task/specs,
review/full gate/commit. Fault only the simulated HTTP upstream. No production
change, new dependency/schema/API/migration, timeout override, live account,
credential-store access, capability/write, recursive delegation or agent commit.
Run scoped CLI tests/fmt/Clippy serially, then stop Cargo for main validation.

- [x] E5b: Integrated native terminal outcomes and catalog/mark failure boundaries.

E5b verification (2026-10-04): Sol/medium implemented three test-only scenarios
covering ten faults and concrete shared fixture extraction. Main reviewed E5a
preservation, exact requests/auth, first comparison metadata, full endpoint and
profile/credential preservation, public and closed DB/WAL redaction. Main required
valid held responses plus an actual handler responded-or-dropped RAII signal;
an independent release watcher is not handler completion or delivery evidence.
Scoped CLI tests (42), fmt/Clippy and two main full integration-module repetitions
(four tests each, including production ten-second timeouts) passed.

Main final `pnpm verify` passed: 301 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, workspace fmt/Clippy, contract drift, dependency boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. The first
full gate stopped at eleven Vitest worker startup exits (3221226505), with no
tests executed. Standalone Vitest and the unchanged full gate then passed; cause
is unconfirmed, and no code/configuration workaround was applied. Product YAML/
34 evidence IDs/11 evidence refs/8 path refs/unique-disjoint decisions/disabled
capabilities and diff checks passed separately. No production/dependency/API/
schema/migration/capability/write change. Synthetic Rust-client/native-adapter
evidence is not shipped-process success, browser-to-upstream, live account/native
keychain or restart/migration acceptance. Keep child/parent active and connector
IDs empty; next cover integrated polling/search/manual and remaining acceptance.

E5a checkpoint (2026-10-04): Sol/medium owns a CLI test-only integration module
and its cfg(test) registration. Compose the real CLI client, authenticated
loopback HTTP router, temp SQLite, preview runtime/runner/producer and production
NeoDbClient against a simulated loopback NeoDB server. Main owns task/specs,
independent review, full verification and commit. Read E5a design plus CLI,
submission/history, runtime/runner/record, mapping storage and NeoDB contracts.
No fake preview replies or catalog/mark/identity ports, production behavior,
dependency/API/schema change, capability, writes, live account/keychain, recursive
delegation or agent commit. Scoped Cargo builds are serial before main full gate.

- [x] E5a: Real CLI/client-to-adapter native comparison acceptance.

E5a verification (2026-10-04): Sol/medium implemented only the CLI test module
and cfg(test) registration. Main reviewed real HTTP pairing/client/runtime/SQLite/
adapter composition and required exact Chinese-title retention, public credential
redaction, unchanged credential entries and closed DB/WAL marker/token checks.
The synthetic upstream gate distinguishes Running from persisted Completed;
exact ten-request GET order proves native fetch/detail/personal-read boundaries.
Equal/conflict/missing-shelf, selected v1 despite changed v2, stable paginated
history and normalized reopened storage are asserted. Scoped CLI tests (39),
fmt/Clippy and five main exact-test repetitions passed.

Main final `pnpm verify` passed: 298 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, workspace fmt/Clippy, contract drift, dependency boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/
34 evidence IDs/decision/reference/disabled-capability checks and diff checks
passed separately. No production/dependency/API/schema/migration/capability/write
change. This proves the Rust CLI client's native happy path against synthetic
NeoDB HTTP, not real accounts/keychain, shipped-process successful preview,
browser-to-upstream, integrated negatives or restart/migration acceptance. Keep
child/parent active and capability IDs empty; next cover integrated failures.

E4e checkpoint (2026-10-03): Sol/medium owns CLI parsing, bounded structured
input/preview HTTP and Rust tests only. Read E4e design, cli-preview/cli-client,
preview-submission-http and repository-layout. Main owns task/specs, README,
existing actual-process smoke, independent review, full gate and commit. Reuse
Domain wire shapes, standard Read/UTF-8/serde and existing transport/receipt guards.
No dependency/API/schema/Core/Server/Storage/TS/WebUI change, CSV parser, capability,
write, recursive delegation, live upstream/keychain or agent commit. Cargo is
serial; stop scoped builds before main verification.

- [x] E4e: Optional file-based explicit CLI choices submit a bounded NEW preview.

E4e verification (2026-10-04): Sol/medium implemented file-only choice inputs,
bounded strict Domain deserialization, exact UTF-8/BOM and encoded JSON guards,
local preparation before health/token I/O, shared raw/structured admission
transport, static semantic diagnostics and focused CLI tests. Main reviewed all
production/test changes and required actual temp-file success, exact Bearer/body/
revision, no-I/O failure tables, count/encoded exact/+1 and strict numeric/duplicate
field checks. Scoped CLI tests (38), fmt and Clippy passed; no new dependency,
schema/API/Core/Server/Storage behavior, CSV parser or capability was introduced.

Main extended existing actual-process smoke with the shipped flag/help, file-only
stdin refusal, unsafe origins, strict nested shapes/UTF-8/missing input and static
redaction before native keychain access. The first full gate exposed the WebUI
test settlement race recorded below; Luna/max repaired only its observable busy
state wait. All 35 panel tests and five repeated target runs passed without
weakening no-staging-I/O assertions, changing production UI or padding timeouts.

Main final `pnpm verify` after all code/test changes passed: 297 Rust unit tests,
two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contract drift,
dependency boundaries, lint/typecheck, WebUI/CLI builds and actual local CLI/HTTP/
auth/browser smoke. Product YAML/34 evidence IDs/decision/reference/disabled
capability and diff checks passed separately. Fixture ports/upstream/accounts/
credentials are fake; local Windows checks are not hosted three-platform or live
NeoDB evidence. Broader real-client/simulated-NeoDB-adapter negative, migration
and end-to-end acceptance remains pending; keep child/parent active, connectorIds
empty and personal/source writes disabled.

E4e full-gate repair (2026-10-04): Rust/Clippy/contracts/boundaries/lint/typecheck
passed; WebUI's existing new-job selection test counted a post-page action status
reconciliation as staging I/O. Luna/max owns only PreviewPanel.test.tsx's fixture
settlement/assertion repair. Preserve the no-staging-I/O assertion, wait for real
action/reconciliation settlement rather than timeouts, and do not change production
UI or polling. Main owns the corresponding spec rule and full rerun. Run focused
panel tests and repeat the target case five times before handing back control.

E4d checkpoint (2026-10-03): Sol/medium owns PreviewPanel, focused tests and
preview-only CSS (a small named panel-specific component/helper is allowed).
Main owns narrow generated validator re-exports from core-client-ts, task/specs,
existing schema-validated Chromium smoke, screenshots,
independent review, full gate and commit. Follow E4d design, frontend preview-panel/
webui-boundaries and backend preview HTTP/submission/repository-layout. Use existing
generated types/validators/client/action guards, no new parser/dependency/API,
CLI/Core/Server/Storage change, capability, write, recursive delegation or commit.
No live upstream/account/keychain. Scoped lint/typecheck/tests/build first;
no Cargo from agent while main owns smoke/full gate.

- [x] E4d: Interactive WebUI search/manual sparse choices and explicit new-job
  resubmission of this mounted panel's confirmed File/profile association.

E4d verification (2026-10-03): Sol/medium implemented mounted receipt/File/profile
ownership, generated source-bound search/manual staging, sparse replacement/reset/
pagination, exact frame guards and explicit new-job resubmission. Main reviewed
the complete client/UI/test diff, required Unicode Cc reuse, stale rejected-path
guards, off-page/Compared reset, exact encoded JSON bounds, delayed File/preflight/
submission evidence and identity-scoped inspection suppression. Main's explicit
refresh clears suppression. The shared client only re-exports existing generated
validators; no new dependency, parser, API, schema, CLI/Core/Storage/Server behavior
or capability is added. Raw upload remains compatible and unknown receipts detach
old resolution authority without retries or rollback claims.

Focused panel tests passed (35). Main extended the existing schema-validated
Chromium fixture with raw -> explicit Search -> new candidate job -> Manual ->
another new job, exact BOM/CSV/profile/source ID/ordinal payloads, preserved old
jobs, read-only reopen, keyboard/desktop/mobile/overflow and empty storage evidence.
Main final `pnpm verify` passed after all code/test edits: 291 Rust unit tests,
two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries,
lint/typecheck, WebUI/CLI builds and actual local host/auth/browser smoke. Product
YAML/34 evidence IDs, decision/reference/disabled-capability and diff checks passed
separately. Screenshots were visually inspected; upstream/account/keychain ports
are fake, not live catalog-fetch or hosted three-platform acceptance. CLI choice
inputs and broader real-client/simulated-upstream acceptance remain pending; keep
the child/parent active with connectorIds empty and personal/source writes disabled.

E4c checkpoint (2026-10-03): Sol/medium owns Domain request DTOs, Server additive
JSON CSV/resolution submission and tests, focused OpenAPI registration/closure,
contract generation and shared TS client/tests. Main owns task/spec edits,
independent review, full gate and commit. Read E4c design; backend preview
submission/inspection/jobs/runtime, catalog-search and repository-layout plus
frontend webui-boundaries. Reuse existing validators/parser/runtime/auth/client
machinery. No Core/Storage behavior change, migration, dependency, UI/CLI command,
standalone catalog route, recursive delegation, live upstream/keychain or agent
commit. Cargo serial; stop scoped builds before main verification.

- [x] E4c: Add compatible structured submission with explicit source-bound
  resolution inputs and a generated-contract thin client; no historical mutation.

E4c verification (2026-10-03): Sol/medium implemented strict Domain request DTOs,
additive JSON on the existing CSV route, focused tagged OpenAPI closure, generated
contracts and thin TS client. Main independently reviewed the complete production/
schema/client/test diff, required Core-owned count constant reuse, preserved the
default fake-port no-I/O guards, and strengthened count/UTF-8/numeric/nested-field
rejection plus exact fresh-read/old-history/fixed-binding resume assertions and
structured caller-abort coverage. Scoped Server submission tests (16), fmt/Clippy,
contract generation/check, lint/typecheck and focused TS tests (14) passed.
Main full `pnpm verify` passed again after all code/test edits: 291 Rust unit tests,
two compile-fail doctests, 65 TS/UI tests, workspace fmt/Clippy, contract drift,
dependency boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser
smoke. Product YAML/evidence/decision/reference/capability checks and diff check
passed. No dependency/migration/Core/Storage production change, interactive UI/CLI
selection, standalone catalog route, capability or personal/source write.
Interactive clients and broader client/upstream acceptance remain pending; keep
this child and parent active. Native catalog/account/credential tests use fakes.

E4b checkpoint (2026-10-03): Sol/medium owns Core/Storage validated compact manual
references, immutable plans, unified resolution-input repository read, runner and
private proof integration, compatible v7 migration and tests. Server edits are
minimal test fixture port updates only. Main owns task/spec edits, independent
review, full gate and checkpoint commit. Follow E4b design and the same backend
contracts listed for E4a. No new dependency, public API/client change, recursive
delegation, live upstream/keychain access or agent commit. Cargo remains serial.

- [x] E4b: Persist explicit manual choices and require fresh detail plus exact
  stored resolution-input proof on new results; retain compatible old history.

E4b verification (2026-10-03): Sol/medium implemented compact validated references,
mutually exclusive immutable plans, unified input reads, private exact request/mode
proof, fresh runner detail and compatible v7 storage. Main independently reviewed
all changed implementation/test paths and strengthened UUID/path/count boundaries,
mixed-mode/provenance/attempt assertions, no-detail/secret access after rebind,
Native/Search/other-Manual proof refusal, corruption preservation, mixed-input
rollback and populated v6 Native/Search/old-manual-history migration evidence.
Scoped Core (89 + two compile-fail doctests), Storage (43), Server (87), fmt and
Core/Storage Clippy passed. Main full `pnpm verify` passed: 288 Rust unit tests,
two compile-fail doctests, 63 TS/UI tests, workspace Clippy/fmt, generated contracts,
dependency boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser
smoke. Product YAML/evidence/decision/reference checks and `git diff --check` also
passed. No public API/Domain/client/dependency/capability or personal/source write
change. Public search/manual actions and broader acceptance remain pending; retain
this task and its parent as active rather than treating this checkpoint as PRD completion.

E4a checkpoint (2026-10-03): Sol/medium owns Core/Storage search-input types,
repository/runner/proof integration, compatible v6 migration, tests and minimal
trait-implementation updates in Server test fixtures only. Main owns task/spec
edits, independent review, full gate and commit. Read E4a design and backend
preview-jobs/records/results/runner, catalog-search, mapping-storage,
state-and-pairing and repository-layout contracts. Reuse existing validation and
strict serde/SQLx; no new dependencies, public API/Domain/client changes, manual
selection persistence, recursive delegation or agent commits. Cargo is serial.
Stop after scoped Core/Storage tests/fmt/Clippy for main review and full gate.

- [x] E4a: Immutable explicit per-record search queries survive reopen/resume;
  the existing runner and result proof consume the exact stored query.

E3c checkpoint (2026-10-03): Sol/medium owns a new test-only Server integration
module and its minimal `#[cfg(test)]` registration. Exercise authenticated raw CSV
import -> real comparison -> drained shutdown/SQLite reopen -> production startup
interruption -> explicit fresh resume -> deletion, with fake upstream/secret/time
ports only. Read E3c design and backend preview submission/HTTP/management/runtime,
jobs/results/runner/comparison, state-and-pairing, mapping-storage, imdb-csv and
repository-layout contracts. No production behavior/API/dependency/migration/
capability changes, recursive delegation or agent commits. Cargo commands remain
serial. Main owns task/spec edits, independent review, full gate and commit.

- [x] E3c: Real backend mock-port restart/resume/delete integration acceptance.
- [x] Audit broader PRD evidence; do not equate internal search/manual support
  or browser-only mocks with delivered public behavior.
  Evidence and remaining public/client acceptance: [acceptance.md](./acceptance.md).

E3c gate repair: the first full gate passed all Rust tests and contracts, then
found a PreviewPanel mock sequencing race. Page-read actions deliberately trigger
an inspection after settlement; a two-response mock may be exhausted before the
explicit status click. Luna/max owns only this focused test fixture, modelling
the current remote job explicitly instead of timing-dependent once responses.
Do not change production UI behavior or increase wait timeouts to hide the issue.
Main reviews the fix and reruns the full gate before committing E3c.

E3b checkpoint (2026-10-03): Sol/medium owns WebUI PreviewPanel and focused tests,
minimal AccessPanel integration and preview-only CSS. Main owns design/specs,
mock Chromium smoke, independent review, full verification and commit. Read E3b
design, frontend webui-boundaries/mapping-editor, backend preview-http/submission/
management and repository-layout. No new API/dependencies, source/personal writes,
recursive delegation or agent commits. Keep connectorIds empty and final mock
restart/resume integration acceptance pending. Scoped lint/typecheck/test first.

E3a checkpoint (started 2026-10-02, verified 2026-10-03): Sol/medium owns CLI command parsing,
client preview HTTP methods/output and Rust tests only. Main owns persisted
design/specs, independent review, real-process smoke, full verification and commit.
Read the E3a design and cli-client, foundation-runtime, preview HTTP/submission/
management and repository-layout contracts. Reuse existing dependencies and
Domain DTOs, no Core/Server/Storage/API/TS/UI changes, recursive delegation or
sub-agent commits. Cargo commands remain serial. Report scoped CLI tests/fmt/
Clippy, then stop edits/builds before main's gate. WebUI and end-to-end PRD
completion remain pending, personal writes disabled and connectorIds empty.

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
- [x] D2: Persist validated identity/snapshot/comparison results with conditional
  completion, connect the bounded runtime worker, startup recovery and retention
  configuration/scheduling. Run fresh comparison after explicit resume.
  - [x] D2a: Internal single-record producer joining resolution/snapshot/comparison,
    explicit provenance, whole-record cancellation/deadline and binding checks.
  - [x] D2b: Compatible result persistence and conditional completion, followed by
    runtime worker/startup/configuration composition; do not mark D2 complete at D2a.
    - [x] D2b1: Consume produced records through an atomic bounded result repository,
      current-attempt pagination and conditional completion; compatible v5 migration.
    - [x] D2b2: Compose bounded worker, startup recovery and retention configuration.
      - [x] D2b2a: Internal sequential single-attempt runner using existing ports
        and real record production; no automatic runtime/startup/public surface.
      - [x] D2b2b: Server-owned single-job admission/cancellation and lifecycle.
      - [x] D2b2c: Startup recovery and retention configuration/periodic cleanup.

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

D2b2c ownership: Sol/medium implements Core's pure PreviewRetention in the existing
preview-jobs owner/tests, Server runtime start/maintenance and tests, CLI configuration
and real serve composition/tests. Main owns the persisted design, owning-spec
updates, independent review, full gate and checkpoint commit. No recursive
delegation or sub-agent commit. Read D2b2c design, preview-runtime/jobs/results/
runner/comparison, foundation-runtime, internal-secrets, repository-layout and
connector/IMDb contracts. Reuse existing primitives; no HTTP/schema, migration,
new dependency/version, automatic preview resume or real account/keychain call.
Scoped Core/Server/CLI tests, fmt/Clippy first; stop edits/builds before main's gate.
Separate Sol/medium lease implementation owns only CLI `serve_lease.rs` and its
tests; startup agent owns module registration/acquisition in main.rs. Lease agent
must not run Cargo while startup agent is building. Main reviews both and owns
the real-process smoke assertions where appropriate.

D2b2c verification (2026-10-01): Sol/medium implemented Core retention policy,
Server startup/maintenance, CLI composition/configuration and the separate
serve-only standard-library lease. Main independently reviewed code/tests, aligned
product/engineering/README/example contracts and added real-process same-database
exclusion/doctor smoke checks. Review corrections include default-policy reuse,
pre-I/O executor rejection, root-token-confirmed shutdown classification, prompt
drain on signal and maintenance-error precedence. Main regression tests prove
the shared recovery/cleanup deadline, late/hung initial cleanup, fake repository
cancellation versus real shutdown, maintenance observer/owner drop and retained
hashed client authentication. Scoped Server runtime 20 tests passed after review.
Final main `pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 207 Rust unit tests,
2 compile-fail doctests, 25 TS/UI tests, fmt/workspace Clippy, generated contract
drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual CLI/local
HTTP/session/pairing/revocation/desktop-mobile Chromium smoke. Product YAML/34
evidence IDs and references plus `git diff --check` passed. No migration,
dependency/version or public DTO/route changed; no live NeoDB/native fetch,
real account or OS credential access. Retention is default 30 days, configured by
CLI/env/YAML, fixed for saved jobs; recovery never automatically resumes work.
One owned 60-second Skip maintenance task stops the host truthfully on failure.
Same-data-directory hosts on any port are excluded before DB mutation. Runtime
drain precedes pool close and lease release; doctor remains nonrecovering.
D2b2c/D2b2/D2b/D2 are complete internally; public delivery E and all broader
client/end-to-end PRD acceptance remain pending. Keep this child active, not archived.

- [x] Implement repositories, ordered migration, atomic bounded submission,
  version/account binding and conditional worker transitions.
- [x] Add bounded runtime worker, startup interruption, explicit fresh resume,
  cancellation/deletion race handling and configurable retention cleanup.
- [x] Test migration/reopen from prior schemas, crash/restart, duplicate workers,
  failed transactions, late completion, expiry and preservation of credentials/profiles.
- [x] Audit persisted columns/values against the no-raw-payload rule; verify and commit.

### E. Shared API and client delivery

- [x] E1: Authenticated historical job/status/result inspection DTOs/routes and
  generated validators; optional read ports composed with Storage in CLI serve.
  Preserve identity/field evidence, strict attempt/ordinal pagination, binding
  state, retention and read races under a bounded management budget.
- [x] E1: Thin shared TS GET client and wire/auth/redaction/timeout regression tests.
- [x] E2: Bounded CSV submission and explicit cancel/resume/delete through the
  single owned runtime, without source or personal-state writes.
  - [x] E2a: Guarded cancel/delete API and generated thin-client management methods.
  - [x] E2b: Bounded CSV submission/resume with truthful runtime admission.
    - [x] E2b1: Owned admission-before-persistence submission/resume internals.
    - [x] E2b2: CSV upload/resume HTTP contracts and thin generated clients.
- [ ] E3: CLI/WebUI import/progress/diff controls and mock end-to-end acceptance.

E2b1 ownership: Sol/medium implements only Server `preview_runtime.rs` and its
test module(s), using the real runner and existing repository ports. Main owns
design/specs, independent review, full verification and commit. No new port,
dependency, Core/Storage/CLI/HTTP/schema change, recursive delegation or agent
commit. Preserve existing enqueue/management/maintenance behavior; include
pending-mutation ownership and admission rejection before I/O in focused tests.

E1 ownership: Sol/medium owns Domain `preview.rs`, Server inspection routes and
tests, module/OpenAPI/AccessState registration, and the minimal CLI read-port
composition. Main owns persisted contracts, independent review and full gate/
commit; generated artifacts are regenerated after Rust settles. Luna/max owns
only thin shared TS client/test additions after DTO names are finalized. No
recursive delegation or sub-agent commits. Read E1 design plus foundation-runtime,
preview-jobs/results/runtime, state-and-pairing, mapping-http/storage,
repository-layout and guides. Keep broader E/PRD acceptance unchecked.

E2a ownership: Sol/medium implements Domain cancel/result DTOs, Server object-safe
runtime management view, guarded cancel/delete HTTP and separate router tests,
AccessState/OpenAPI registration and minimal same-Arc CLI wiring. Luna/max owns
thin TS cancel/delete methods and tests only. Main owns specs/generated artifacts,
WebUI client-test mock, actual-process smoke, independent review and full gate/
commit. Reuse existing runtime/control/access boundaries; no recursive delegation,
agent commits, new dependencies/migration or CSV/resume work in this slice.

E1 verification (2026-10-01): Sol/medium implemented Domain inspection DTOs,
Server read routes and temporary-SQLite/fake-port tests plus minimal CLI port
composition; Luna/max implemented only the generated-contract shared TS GET
client and tests. Main independently reviewed source/tests, rejected duplicated
schema mirror models, aligned product/backend/frontend/README contracts and
extended actual-process auth/capability smoke. A focused typed OpenAPI adjustment
closes tagged enum variants missing from locked utoipa's serde handling; generated
fixed historical/non-executable flags and variant closure were independently
checked. No dependencies, migrations or API version bump were needed.

Scoped inspection tests passed 14/14. A real PreviewRecordService-produced
Unsupported record is persisted and inspected after credential rotation without
using credentials or remote transport. Existing real-SQLite malformed JSON
pagination regression passed; HTTP errors remain static/redacted without partial
pages. Notify-synchronized paused-time tests prove the shared ten-second budget,
exact-deadline rejection and hung-result timeout. Other cases cover sparse
ordinals, malformed rows/query, optional ports, retention/read races, binding
rotation/disconnection and Bearer precedence/revocation. Shared client scoped
tests passed 18/18; its new GET methods preserve caller abort and response checks.

Final main `pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 222 Rust unit tests,
2 compile-fail doctests, 31 TS/UI tests, workspace fmt/Clippy, generated contract
drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual CLI/local
HTTP/session/pairing/revocation/desktop-mobile Chromium smoke. Product YAML/34
evidence IDs and references plus `git diff --check` passed. No live NeoDB/native
fetch, real account or OS credential operation. E1 is complete; E2 mutations and
E3 client screens/end-to-end acceptance remain pending, connectorIds stays empty,
and historical evidence is never executable. Keep this child active, not archived.

E2a verification (2026-10-01): Sol/medium implemented Domain cancel/result DTOs,
Server management type-erasure and guarded HTTP, same-Arc CLI wiring and router
tests reusing a test-only real runtime fixture. Luna/max implemented thin TS
cancel/delete methods and five tests. Main independently reviewed code/tests,
corrected route-local body-limit composition and duplicate OAuth/preview OpenAPI
operation names, aligned specs/README/product and extended actual-process smoke.
Main retained native incoming Requests in cancellation mocks instead of cloning
away the original abort bridge. No UI controls, enqueue/resume, migration,
dependencies or public API version change were introduced.

Management regressions passed 7/7: active and queued cancel, wrong-attempt
isolation, retained committed partial history, delete preventing late publication,
preserved profile/account/auth, Origin/CSRF including cross-session mismatch,
exclusive/revoked/duplicate Bearer, strict duplicate/negative/fractional/overflow
request fields, exact 4 KiB versus +1 bodies, absent management and truthful
applied=false. Notify-synchronized hung/exact-ten-second tests cover both methods,
with static redacted errors rather than a fabricated applied result. Existing
runtime regressions retain actual resumed-generation cancellation isolation.

Final main `pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 229 Rust unit tests,
2 compile-fail doctests, 36 TS/UI tests, workspace fmt/Clippy, generated contract
drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual CLI/local
HTTP/session/pairing/revocation/desktop-mobile Chromium smoke. Generated management
bounds/strict shapes/route operation IDs and product YAML/34 evidence IDs and
references were separately verified; `git diff --check` passed. Tests used fake
ports/temporary SQLite/loopback only, not live NeoDB, native fetch, real accounts
or OS credentials. E2a is complete; E2b submission/resume and E3 screens/end-to-end
acceptance remain pending. Keep this child active and connectorIds empty.

E2b1 verification (2026-10-01): Sol/medium implemented internal owned submission/
resume and 14 new runtime tests; main reviewed production/tests, refined supervised
panic handling and pre/post-I/O clock guards, and aligned product/runtime contracts.
Synchronous reservation precedes mutation, and one tracked operation keeps the
same admission through preparation and execution. Discarded observers do not
abandon accepted work; durable acceptance is separate from completion/status.
Tests cover full/closed/duplicate admission before mutation, concurrent resume,
empty submission, fresh ordinal-zero resume using v1 despite v2, changed binding
refusal preserving partial history, fixed expiry, observer drop, error/panic/hung
preparation, retention-limited deadlines, clock regression, post-commit submit/
resume cancellation ambiguity, shutdown drain, overflow and missing executor.
Uncertain commits are inspected truthfully, never erased or reported rolled back.

Scoped runtime 34 tests and Server 73 tests/fmt/Clippy passed. Final main
`pnpm verify` with `CARGO_BUILD_JOBS=1` passed: 243 Rust unit tests,
2 compile-fail doctests, 36 TS/UI tests, workspace fmt/Clippy, generated contract
drift, dependency boundaries, lint/typecheck, WebUI/CLI builds and actual CLI/local
HTTP/session/pairing/revocation/desktop-mobile Chromium smoke. Product YAML/34
evidence IDs and references and `git diff --check` passed. Temporary SQLite/fake
ports/loopback only; no live NeoDB/native fetch, real account or OS credential
operation. No dependency, migration, Core/Storage/CLI change or public API/schema/
capability was added. E2b1 is complete; E2b2 public CSV upload/resume and thin
clients, E3 screens and end-to-end acceptance remain pending. Keep this child
active; do not archive it or mark broader E complete.

E2b2 verification (2026-10-02): Sol/medium implemented Domain admission DTOs,
Server raw text/csv upload and explicit resume with a narrow same-Arc runtime view,
minimal CLI wiring and 13 focused HTTP tests. Luna/max implemented thin TS methods,
typed sanitized reconciliation errors and nine tests. Main reviewed code/tests,
required shared runtime error/ID validators and truthful post-reservation IDs,
corrected paused-time SQL fixture ordering and BOM byte assertions, strengthened
missing-revision refusal, regenerated contracts and aligned product/backend/frontend/
README, consumer mocks and actual-process smoke. No Core/Storage or migration
change; no source/personal-state writes, real account/keychain or live catalog fetch.

HTTP tests prove authentication before body polling, streamed exact/+1 byte limits,
strict query/Content-Type/JSON, UTF-8/BOM/CSV failures, bounded skip counts and only
normalized storage, saved v1 selection despite v2, full binding, queue-full before
mutation, fresh resume with fixed revision/expiry and wrong-attempt/ineligible refusal.
Real post-commit held submit/resume tests return the known ID/next generation under
timeout and inspect actual persisted state without asserting rollback. Ambiguous
failures are sanitized envelopes; pre-admission failures never invent a job receipt.
Standard typed http-body-util error classification distinguishes 413 from an ordinary
broken stream; only already locked library versions were promoted to direct deps.

Final main `pnpm verify` with `CARGO_BUILD_JOBS=1` passed after all code/test changes:
256 Rust unit tests, two compile-fail doctests, 45 TS/UI tests, workspace fmt/Clippy,
non-mutating contract drift, dependency boundaries, lint/typecheck, WebUI/CLI builds
and actual CLI/HTTP/session/pairing/revocation/desktop-mobile Chromium smoke.
Generated strict DTO bounds/fixed false/202 routes/error unions/unique operation IDs,
product YAML/34 evidence IDs and references, and `git diff --check` passed separately.
The shared client compares raw BOM bytes, defaults admission to 30 seconds without
changing five-second GET defaults, validates actual receipts/failures and never retries.
E2 is complete; E3 CLI commands/screens and mock end-to-end acceptance remain pending.
Keep this child active, personal writes disabled and connectorIds empty until E3.

- [x] Define domain DTOs and executable API contract; regenerate schemas/TS validators.
- [x] Add guarded import/status/results/cancel/resume/delete routes, body and
  pagination caps; test session CSRF, Bearer scopes, invalid input and redaction.
- [x] Add thin shared client, CLI commands and WebUI progress/diff/review controls.
  - [x] E3a: Thin CLI import/status/one-page results/cancel/resume/delete commands.
  - [x] E3b: WebUI upload/progress/field-diff/explicit management controls.
- [ ] Test generated-response validation, loading/error/cancellation flows and
  end-to-end CSV -> preview -> restart -> resume -> delete with mock NeoDB.
- [ ] Enable truthful capability reporting only for delivered operations.
- [ ] Commit coherent API/client checkpoints after their full validation.

E3a verification (2026-10-03): Sol/medium implemented the six thin CLI commands,
raw bounded file/stdin import, shared origin/health/OS-token preflight, exact
Domain DTOs, generation-aware mutations, bounded response checks and focused
loopback/fake-credential tests. Scoped CLI 28 tests, fmt and Clippy passed.
Main reviewed all production/tests, strengthened independent invariant cases,
otherwise-valid exact/+1 response caps, sparse paging and output preservation,
and added path-normalization refusal, strict operation status codes, preflight
ordering, unknown-error redaction and control/bidi output roundtrip tests.
Final CLI count is 32 tests (17 new); no raw CSV persistence or client-side parser.

Main's final `pnpm verify` after the last rendering change passed with
`CARGO_BUILD_JOBS=1`: 273 Rust unit tests, two compile-fail doctests, 45 TS/UI
tests, workspace fmt/Clippy, non-mutating generated contract drift, dependency
boundaries, lint/typecheck, WebUI/CLI builds and actual CLI/HTTP/session/pairing/
revocation/desktop-mobile Chromium smoke. Product YAML/34 evidence IDs and
references and `git diff --check` passed. Real-process checks exercise shipped
command help and syntax/unsafe-origin refusal before native keychain access;
successful preview commands use loopback fixtures and fake credentials.
No live NeoDB/native-fetch, real account or OS credential operation, dependency,
migration, Core/Storage/Server/API/generated/TS/UI change. Specs and README now
describe delivered CLI behavior; connectorIds remains empty and personal writes
disabled. E3b WebUI, mock restart/resume end-to-end and broader child/parent
acceptance remain pending. Keep this child active, not archived.

## Verification gates

E4a verification (2026-10-03): Sol/medium implemented immutable sparse query
inputs, compatible v6 migration, repository/runner/record proof integration and
tests; main independently reviewed production and test diffs. Main review required
shared stored-query decoding across observation/history/store/completion reads,
bounded plan length before allocation, exact error and UTF-8 byte assertions,
matching trimmed-request proof insertion, explicit nonempty exact-ID candidates
remaining review-only, fresh query forwarding, and query-phase cancel/timeout/
rebind/delete guards. Native defaults and existing internal Manual/NULL behavior
remain unchanged; no public search/manual action or advertised capability exists.

Scoped Core 86 unit + 2 compile-fail doctests and Storage 40 unit tests passed;
Server fixtures compiled, scoped fmt/Clippy and diff checks passed. Main's full
`pnpm verify` after all code/test changes passed with `CARGO_BUILD_JOBS=1`:
282 Rust unit tests, 2 compile-fail doctests, 63 TS/UI tests, workspace fmt/Clippy,
non-mutating generated-contract drift, dependency boundaries, lint/typecheck,
WebUI/CLI builds and actual CLI/local HTTP/session/pairing/revocation/desktop-mobile
Chromium smoke. Product YAML/34 evidence IDs, unique/disjoint decision IDs and
references plus final `git diff --check` passed separately. All fixtures are fake
ports/temporary SQLite/loopback, not live NeoDB/native fetch, accounts or OS
credentials. Existing v1-v4 fixtures and populated exact-v5 upgrade preserve
operational state; raw payloads/tokens are not added. No dependency, public API,
Domain/client or production Server change. Keep this child active, connectorIds
empty and personal writes disabled; public search/manual and broader acceptance
remain pending rather than marking E/PRD complete.

E3c verification (2026-10-03): Sol/medium implemented the new test-only routed
restart scenario; main reviewed it and strengthened production identity-helper
reuse, valid catalog UUID/path evidence, full endpoint/token guards, upstream and
credential counters, shutdown-vs-cancel distinction, historical item roundtrip,
immutable v1 semantics despite changed v2, fresh timestamps/known sink conflicts,
obsolete-attempt refusal, provenance/identity and post-delete session/Bearer auth.
The scoped test passed after final assertions. No production behavior, API,
dependency, migration, personal/source write or capability change was introduced.

The first full gate passed Rust/contracts/boundaries/lint/typecheck but exposed an
existing PreviewPanel once-response mock race. Luna/max replaced only that test's
fixture with explicit remote state and settlement checks; main reviewed the fix.
All 15 panel tests and five repeated target runs passed, without timeout padding
or production UI changes. The owning frontend contract captures the fixture rule.

Main's final `pnpm verify` passed after all code/test edits, with
`CARGO_BUILD_JOBS=1`: 274 Rust unit tests, two compile-fail doctests, 63 TS/UI tests,
workspace fmt/Clippy, non-mutating contract drift, dependency boundaries,
lint/typecheck, WebUI/CLI builds and actual CLI/local HTTP/session/pairing/
revocation/desktop-mobile Chromium smoke. Product YAML/34 evidence IDs, unique
and disjoint decision IDs/references and `git diff --check` passed separately.
Fixtures use temporary SQLite/fake ports/loopback only, not live NeoDB/native
fetch, real accounts or OS credentials. E3c is backend mock-port integration,
not full CLI/browser-to-real-NeoDB-adapter end-to-end evidence. Broader PRD gaps
are recorded in `acceptance.md`; public search/manual selection, integrated
negative paths and parent client acceptance remain pending. Keep the child
active, connectorIds empty and personal writes disabled; do not archive it.

E3b verification (2026-10-03): Sol/medium implemented the session-only preview
panel, focused tests, AccessPanel integration and responsive styles. Main owns
the exact six-route capability gate, immutable selection/raw-BOM Chromium mock,
all-outcome evidence, cancel/resume/delete and missing-route/reload checks, plus
spec/README alignment. A separate read-only Sol/medium review identified stale
rejection/finalization and observation-vs-action races. Main added synchronous
inspection abort, matching generation/abort guards on every completion path and
a regression proving abort precedes dispatch even when the old port returns
attempt 99. Reviewer spot-check found no remaining actionable issue.

Main's full `pnpm verify` after the final code/test changes passed with
`CARGO_BUILD_JOBS=1`: 273 Rust unit tests, two compile-fail doctests, 63 TS/UI
tests (18 new), workspace fmt/Clippy, non-mutating generated contract drift,
dependency boundaries, lint/typecheck, WebUI/CLI builds and actual local CLI/
HTTP/session/pairing/revocation/browser smoke. Product YAML/34 evidence IDs and
references and `git diff --check` passed. Desktop/mobile status and preview
screenshots were visually inspected; no horizontal overflow or browser storage.
No real account/keychain/native fetch, dependency, migration or API change.
Preview browser flows use schema-validated mocks and the actual shared client;
they do not prove backend restart/resume integration. No mutation retries,
fabricated percentages, manual-confirmation endpoint or personal/source writes.
E3a/E3b client delivery is complete; final mock CSV -> restart -> fresh resume ->
delete and broader child/parent acceptance remain pending. Keep this child active
and connectorIds empty rather than archiving or marking all PRD criteria complete.

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
