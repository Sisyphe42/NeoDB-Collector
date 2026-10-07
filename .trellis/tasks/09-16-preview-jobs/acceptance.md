# Preview acceptance evidence

Status: in progress. This is task-level evidence, not a new product contract.
Checkboxes in `prd.md` require delivered behavior, not merely internal primitives.
All fixtures are synthetic. No live NeoDB/native-fetch, real account or native
credential-store verification is implied. Local Windows checks are not three-OS CI.

## Evidence boundaries

- Catalog transport loopback tests prove restrictive request/response handling;
  Core resolver tests prove polling, identity/granularity and explicit selection.
  E4a/E4b integrate explicit inputs into the internal runner; E4c exposes structured
  submission, E4d adds explicit WebUI choices and E4e adds file-based CLI choices.
- Core comparison/record tests prove account binding, non-executable comparisons
  and Known/Empty/Unknown semantics. Storage tests prove normalized persistence,
  atomic guards, retention, reopen and migration independently of clients.
- Server router/runtime tests use real SQLite/runner with fake upstream ports.
  E3c's routed import/restart/fresh resume/delete chain passed its scoped test;
  final workspace verification is recorded below.
- CLI tests use loopback fixtures/fake credentials. `tests/preview-ui-smoke.mjs`
  validates preview responses and browser actions against schema-validated mocks;
  its preview evidence stays client-only. `tests/smoke.mjs` runs real host/auth
  boundaries and separately registers E5f's native browser-to-adapter helper.
  E5a-g below distinguish actual adapter, client, terminal-history reopen and
  interrupted/resume evidence; one client's acceptance is not another's proof.

## E5g native browser failure evidence (2026-10-07)

The existing native browser driver now separately runs
`preview_integration_tests/browser/failure.rs::browser_native_failure_host`.
Six ordered cases each use a fresh temporary DB, Core origin and synthetic
upstream/foreign-redirect trap. The actual built WebUI/shared generated client,
bootstrap/session/CSRF, Core HTTP/runtime/SQLite and production NeoDbClient remain
in the chain; only fake secrets, clock and loopback upstream are substituted.
No route fulfillment, mocked Core responses or production control route is added.

Fetch 422/429 complete with the first equal comparison plus an ordinal-one
Unsupported/RateLimited row. Foreign redirect, mark 401, Item-not-found 404 and
held mark timeout fail with only the committed first comparison; ordinal-one
inspection is an exact empty historical page, not evidence of absent personal
state or readiness. Upstream 401 leaves the independent local WebUI session
authorized. Failed resume is offered explicitly but never requested. The public
job DTO exposes lifecycle only: no invented upstream error code is asserted.

MarkHang observes Running/one committed row, then the real unchanged ten-second
transport timeout through normal UI polling. Initial result publication may lag
Running; the driver reuses the existing bounded UI-poll convergence helper rather
than assuming immediate publication. After Failed evidence, the host releases
and awaits the actual held handler's response-or-drop and verifies no late revival.
Both ordinal pages, immutable v1/profile/account/time/identity/rating evidence and
source fingerprints are validated; normalized observation fingerprints match
stored/public result ordinals. Closed retained DB/optional WAL are scanned before
same-origin reopen without reseeding. Full public job/pages and normalized
observations remain equal; browser reload requires explicit known-ID opening and
has no retained File/profile/draft.

Confirmed deletion removes job/observations/results only. Endpoint, both disabled
mapping versions, original paired-client hash/all metadata/authentication and
secret values remain exact. Fetch faults issue four GETs/one NeoDB-secret read;
mark faults issue six GETs/two reads. Catalog has no auth; personal reads have
only NeoDB Bearer; no Cookie, CLI Bearer, search/refetch/retry/write occurs. The
foreign trap sees zero requests. Inspection/reopen/delete/shutdown add no upstream
or NeoDB-secret work, and secret writes stay zero. Retained and post-delete closed
DB/WAL scans exclude both E5a/E5b raw source/upstream markers and plaintext tokens.

Only bootstrap/import/delete browser mutations occur, with real Origin/CSRF and
no Authorization headers. Generated schemas validate actual DTOs; allHeaders is
awaited for security assertions. Terminal/failed desktop/mobile screenshots show
retained first-row evidence; viewport overflow, storage/redaction and page-error
checks pass. The first actual run caught an exact-text locator mismatch against
the renderer's combined `结果：不支持` paragraph. The test was corrected to target
the second result card's full canonical text; no UI behavior changed.

The original E5f happy path and separate EOF probe remain intact. A second EOF
probe targets the failure host's first ready case and requires natural nonzero
integer exit, no signal/error/forced termination and an unreachable Core port.
All listeners bind before spawn; serving errors are retained and both server
joins are awaited even after another check fails. Post-scan errors cannot bypass
owner cleanup. Ignored-host compilation alone is not browser acceptance.

Scoped CLI validation passed 48 tests with two hosts explicitly ignored, plus
format/scoped Clippy. Main independently repeated the full native driver twice
after fixes and visually inspected all three representative screenshots. Final
pnpm verify passed 307 Rust tests, two ignored hosts explicitly run in smoke, two
compile-fail doctests, 85 TS/UI tests, fmt/workspace Clippy, generated drift,
dependency boundaries, lint/typecheck/build and prior mock UI/actual CLI/auth/
native browser smoke. Product YAML/34 evidence IDs/11 refs/8 paths/unique-disjoint
decisions/disabled capabilities, task context validation and diff checks passed.

No dependency/feature/API/schema/migration/Core/Server/Storage/client behavior
change, source/personal write or capability activation. This is six local Windows
synthetic browser faults and terminal-history reopen, not exhaustive faults,
pending-catalog polling/cancel, explicit Search/Manual, Running restart/migration,
shipped-process, native-keychain, live-account or hosted three-OS CI evidence.
Keep child/parent active and connector IDs empty.

## E5f native browser-to-adapter evidence (2026-10-07)

`tests/preview-native-browser-smoke.mjs` drives an explicitly ignored test-only
host in `crates/collector-cli/src/client/preview_integration_tests/browser.rs`.
It serves the actual built WebUI and production Core HTTP/Access/OAuth status,
runtime/runner/snapshot/Storage/NeoDbClient composition. Only synthetic loopback
upstream, fake secrets and a fixed clock are substituted. No Playwright route
fulfillment or mocked Core/catalog/mark response participates in this chain.
Original E5a-e fixture defaults and all assertions are preserved.

The host executable comes from standard Cargo JSON artifacts. Its bounded private
stdio phases coordinate Running evidence, release, retained history and deletion;
the ephemeral bootstrap fragment is delivered to the driver, never echoed in
diagnostics. Ordinary Rust tests compile but explicitly ignore the host; smoke
runs it after the assets build. It is not a shipped CLI command, environment
backdoor or public fixture control endpoint. A separate ready-then-EOF probe must
exit naturally with a nonzero integer code, no signal/error/forced termination,
and an unreachable Core port. Runtime/HTTP/storage/upstream owners are drained.

The browser bootstraps a real HttpOnly/SameSite Strict session, removes the URL
fragment, and uploads BOM/UTF-8 CSV including a Chinese title. Actual generated
schemas validate receipt, job, result pages, mutation and mapping responses.
Counts are three observations, one skipped row, one skipped column and one
duplicate. Explicit profile v1 remains immutable despite saved disabled v2's
rating clamp max=5. The held final identity request allows browser and host to
observe Running with two committed rows before release. Completed comparisons
then retain suggestions 8/7/6: equal 8 is skipped, known 5 conflicts with 7, and
MissingShelf's rating remains Unknown/needs_review rather than absence or readiness.
Assertions cover identity claims/URLs/model/granularity, source fingerprints,
automatic provenance, times, endpoint/account/revision and real 2+1 ordinal pages.

Before deletion the host drains/closes the Completed fixture and scans the DB/
optional WAL while normalized history still exists. Production Storage::open
reopens the same Core origin with original session/client/secrets and no reseed.
Full public job/pages and normalized observations are unchanged. Browser reload
has no retained File/profile/draft, requires explicit known-ID opening, and keeps
Completed cancel/resume disabled. Reopening/reading never uploads, resumes or
performs new upstream/NeoDB-secret work. This is Completed-history persistence,
not interrupted/running browser recovery or shipped-process restart evidence.

Confirmed browser deletion removes that job's observations/results; endpoint,
both mapping versions, original paired-client hash/all metadata/authentication
and secret values remain exact. The trace is exactly ten GETs: three public fetch/
detail pairs, three authenticated mark reads and one identity read. Catalog has
no auth, personal requests have only NeoDB Bearer, and no request has Cookie,
CLI Bearer, search/refetch/retry/write. NeoDB-secret reads remain three and writes
zero through reopen/delete/shutdown. Public DTOs/UI exclude credential references,
tokens and raw markers; closed DB/WAL scans exclude discarded source/upstream
markers and plaintext NeoDB/paired-CLI tokens before and after deletion.

Only bootstrap/import/delete mutations occur in the browser; import/delete have
actual same-origin/CSRF headers, and Core browser calls never carry Authorization.
Playwright allHeaders is required because headers() filters security headers.
The first native run exposed a long generated task ID overflowing the 390px
heading. Main fixed only wrap/flex CSS rather than shortening IDs, hiding evidence
or weakening overflow checks. Desktop/mobile screenshots were visually inspected;
both viewport overflow checks, empty local/session storage and zero page errors pass.

Scoped CLI validation passed 48 tests with the host explicitly ignored. Main
independently repeated the actual browser/native chain and EOF probe twice after
review fixes. Final pnpm verify passed 307 Rust tests, one ignored host explicitly
run by smoke, two compile-fail doctests, 85 TS/UI tests, fmt/workspace Clippy,
generated drift, dependency boundaries, lint/typecheck, builds, previous mock UI
smoke and actual CLI/HTTP/session/pairing/revocation/native browser smoke. Product
YAML/34 evidence IDs/11 evidence refs/8 paths/unique-disjoint decisions/disabled
capabilities and diff checks passed separately. This is local Windows evidence,
not hosted three-OS CI, real-account or native-keychain verification.

No dependency/feature/API/schema/migration/Core/Server/Storage/client behavior
change or source/personal write/capability activation. Native browser failures,
poll/cancel, explicit Search/Manual and Running restart/migration remain bounded
future acceptance work. Keep child/parent active and connector IDs empty.

## E5e populated v5/v6 CLI migration evidence (2026-10-04)

`crates/collector-cli/src/client/preview_integration_tests/migration.rs` adds
`cli_populated_v5_upgrade_preserves_restart_resume_delete_chain` and
`cli_populated_v6_upgrade_preserves_restart_resume_delete_chain`. Both reuse the
entire E5d restart scenario via an optional prior-schema input; the original test
passes None and retains its original assertions. There is no copied lifecycle
fixture, new production migration, public API or raw-SQL production escape hatch.

After original runtime/HTTP/storage owners drain and the held upstream handler
acknowledges responded-or-dropped, a test-only SQLx connection transforms only the
closed temporary DB. It removes v7 input triggers/manual column/ledger; v5 also
removes v6 search column/ledger. A reference schema executes committed migrations
in memory rather than duplicating definitions. Full preview schema SQL, column
type/nullability/default/PK, indexes/index columns, CHECK/FK definitions and
triggers match genuine v5/v6. Retained migration versions, description, success,
checksum, installed time and execution time remain exact. Account/client hashes,
endpoint, both profiles, Running job, two observations and one partial result
are unchanged; removed input columns were explicitly NULL and foreign keys valid.
No current repository is called against the deliberately old schema.

Production Storage::open upgrades before recovery/publication. Tests assert the
v7 schema/triggers/ledger identities, NULL restored input columns, unchanged
retained ledger history/operational rows and valid foreign keys. The same actual
Core origin and original paired token then read Interrupted partial history;
startup/history inspection performs no upstream or NeoDB-secret read. Explicit
resume obtains fresh original-v1 comparisons with later snapshots and changed
marks, and job-only deletion preserves configuration, original authentication and
credential values. All E5d request/auth/redaction/expiry/attempt assertions remain.

SQLx is added only to CLI dev-dependencies through the existing workspace setting.
Cargo.lock changes only the CLI dependency edge; no package/version changes.
Main's locked/offline metadata check confirms all seven resolved SQLx package
feature sets match the pre-change baseline. Both new cases use Native input only;
populated v6 Search/manual-history compatibility remains separate Storage-test
evidence, not an integrated CLI claim. This is synthetic-upstream Rust-client
composition, not shipped-process/browser/live-account/native-keychain acceptance.
Keep child/parent active, connector IDs empty and writes disabled.

Scoped E5e 2/2, all CLI 48/48, format and scoped Clippy passed. Main independently
repeated all ten integration cases twice (20.55s/20.58s), then pnpm verify passed:
307 Rust unit tests, two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy,
contract drift, dependency boundaries, lint/typecheck, builds and actual local
CLI/HTTP/auth/browser smoke. Product YAML/34 evidence IDs/11 evidence refs/8 path
refs/unique-disjoint decisions/disabled capabilities and diff checks passed
separately. These are local Windows checks, not hosted three-OS CI evidence.

## E5d real-adapter restart/resume/delete evidence (2026-10-04)

`crates/collector-cli/src/client/preview_integration_tests/restart.rs::cli_restart_resume_and_delete_preserve_auth_and_unrelated_state`
uses actual Rust pair/status/PreviewClient, authenticated Core HTTP, temporary
SQLite, production runtime/runner/snapshot/producer and NeoDbClient against
synthetic upstream HTTP. A narrow concrete fixture refactor separates initial
seeding/pairing from composition/reopen, preserving every E5a-c assertion.
The actual original Core origin/port and original client token/identity/hash,
PairReport, secret entries, endpoint tuple and both disabled revisions are reused;
no reseeding or new pairing substitutes for credential restart evidence.

First native comparison commits source 8/sink 8/Skipped/already_equal; the second
fetch is held. The runtime/maintenance/Core HTTP drain and SQLite closes BEFORE
releasing that reply. A RAII guard acknowledges the actual handler responded or
dropped, not delivery or remote rollback. Reopened storage before recovery still
has Running attempt 1 and one row. Production startup at NOW+60 changes it to
Interrupted, preserving the exact full public partial page and normalized original
observations/native modes, v1 and fixed expiry. Startup plus authenticated history
inspection leave the request count at four and NeoDB-secret reads at one.
Original public client identity and complete stored access hash/metadata survive.

Explicit CLI resume accepts attempt 2. A fresh first-fetch gate proves both old
and new repository result pages empty and obsolete attempt-1 HTTP reads rejected.
Every original ordinal gets fresh fetch/detail/mark; source 8/7 versus changed
sink 5/4 produces NeedsReview/sink_conflict_needs_review at NOW+60. Assertions
retain original fingerprint, item identity, automatic provenance, account,
historical/non-executable metadata, fixed expiry and exact v1 rather than v2's
clamp-to-5 rule. Exact ten-request traces contain GET only, public catalog no auth,
personal marks only NeoDB Bearer and no Cookie/refetch/search/retry.

Real CLI deletion removes job/observations/results only; repeated deletion returns
false and status is not-found. Full endpoint, both profile contents, original
secret values and original client's hash/metadata/authenticated status remain.
Request vectors, secret values and write/read counters are rechecked after owned
shutdown. Public DTOs omit internal references/credential values/discarded markers;
mandatory closed collector.db and optional WAL scans run before reopen while
partial history exists and after deletion. DB metadata may retain opaque credential
references; plaintext tokens/raw payloads are forbidden. No timing sleeps or
production timeout/interval overrides are used.

Scoped E5d 1/1 and CLI 46/46/fmt/Clippy passed. Main independently repeated all
eight integration tests twice, then `pnpm verify` passed: 305 Rust unit tests,
two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contract drift,
dependency boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser
smoke. Product YAML/34 evidence IDs/11 evidence refs/8 path refs/decisions/disabled
capabilities and diff checks passed separately. A relative test-only detail URL
expectation was corrected to normalized absolute URL before final verification.

No production/dependency/API/schema/migration/capability/write change or live
account/keychain access. This proves restarted synthetic Rust-client composition,
not shipped CLI-process restart, browser-to-upstream, integrated migration,
live account or complete product acceptance. Keep child/parent active, connector
IDs empty and writes disabled; integrated migration/browser acceptance remains.

## E5c polling and explicit resolution integration evidence (2026-10-04)

`crates/collector-cli/src/client/preview_integration_tests/resolution.rs` adds
three scenarios through the existing real Rust pair/client, protected Core HTTP,
SQLite/runtime/runner and production NeoDbClient. Only synthetic upstream replies
and the existing fake clock/secret store are substituted; prior E5a/E5b assertions
are unchanged. Structured imports use actual CSV/choice temp files through CLI's
`connect_with_resolution_request` and existing JSON submission method.

| Scenario | Observed result | Personal credential reads |
|---|---|---|
| Native 202 -> 302 -> detail | Running/empty page before Completed automatic comparison; next fetch >=15s after first response preparation | 0 while unresolved, then 1 |
| Cancel after first fetch arrives | Actual applied=true, persisted Cancelled, retained job/empty results; one fetch only after runtime drain | 0 |
| Explicit Search followed by new Manual jobs | Unsupported-only exact-ID singleton remains candidates; missing-ID fresh detail compares Manual; contradictory fresh ID remains NeedsReview | 1 for separate native match, 1 for valid Manual; none for unresolved rows |

Search/native-match/rate-limit inputs are mixed in one three-row structured job.
Exactly one trimmed query/category/page=1 search is sent; native match and 429
do not search. New manual jobs read fresh detail only, never fetch/search or cached
candidate evidence. The selected item's fresh empty identity evidence differs
from its exact-ID search candidate. A conflicting fresh ID is retained with
`item_identity_conflict`, not converted into a comparison. Original search status
and full page remain equal after both new jobs.

Assertions cover normalized immutable Native/Search/compact Manual inputs,
exact selected v1 with source 8/sink 8/Skipped/already_equal, automatic/manual
provenance, full endpoint tuple, retained v1 and disabled v2 clamp 5, unchanged
two credential entries and no credential writes. Exact request vectors are GET
only, no Cookie/refetch/retry, public catalog no Bearer and personal reads only
the NeoDB Bearer. Public DTOs/observations and mandatory closed collector.db plus
optional WAL exclude credential values and discarded raw/upstream markers.

The polling test uses real production timers, no sleeps or shortened interval.
Request-arrival notification does not prove receipt of 202 or entry into an
internal timer wait; cancellation proves local interruption, not remote rollback.
Terminal reconciliation is bounded at 22s inside a 25s scenario; outer timeout
alone cannot extend a reused 15s inner helper. Runtime/maintenance/HTTP/storage
owners drain within bounded scenarios. Scoped CLI 45 tests/fmt/Clippy and two
main independent full integration-module repetitions (seven tests each) passed.
Main `pnpm verify` passed: 304 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries, lint/typecheck,
builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/34 evidence IDs/
11 evidence refs/8 path refs/unique-disjoint decisions/disabled capabilities and
diff checks passed separately.

One scoped rustc/LLVM out-of-memory failure was reported by the implementing
agent; unchanged retry compiled and focused/full CLI suites passed. Initial Sol
work was interrupted by a usage limit and resumed with the same Sol/medium model,
without substitution or production workaround.

These are Rust-client synthetic-upstream assertions, not shipped-process preview
success, live accounts/keychain, browser-to-upstream or integrated restart/migration
acceptance. Keep child/parent active, connector IDs empty and writes disabled.

## E5b native failure-path integration evidence (2026-10-04)

`crates/collector-cli/src/client/preview_integration_tests.rs` adds three tests
covering ten fault modes through the real Rust pair/client, authenticated Core
HTTP, SQLite/runtime/runner/producer/snapshot and production NeoDbClient. The
concrete shared CoreFixture reuses setup/shutdown without weakening E5a assertions.
Only synthetic upstream HTTP replies/gates and fake clock/secrets are injected.

| Second observation's upstream behavior | Persisted status/results | NeoDB secret reads |
|---|---|---|
| Fetch 422 or 429 | Completed; first Compared plus Unsupported or RateLimited | 1 |
| Fetch timeout, foreign/same-origin noncatalog 302, detail 429 | Failed; first Compared only | 1 |
| Mark 401, 302, Item-not-found 404 or timeout | Failed; first Compared only | 2 |

Each case preserves the first ordinal's source/profile/account/snapshot/automatic
provenance and exact rating 8 / sink 8 / Skipped / already_equal comparison. Public
attempt/time/expiry/binding/non-executability and normalized repository row parity
are checked. Both disabled profile revisions (v1 max 10, v2 max 5), full original
endpoint tuple and two fake credential entries survive unchanged. No failed row
becomes an Empty/Unknown comparison or causes /api/me, search or retry.

Exact request vectors assert GET-only, correct source/detail/mark URLs, public
catalog no auth, personal reads only the NeoDB Bearer, no Cookie/refetch. A real
foreign-origin loopback trap receives zero requests, rechecked after all servers
drain. Public DTOs, normalized observations and mandatory closed database/optional
WAL byte checks exclude internal references, raw/diagnostic markers and plaintext
tokens as appropriate to each boundary.

Timeout cases retain the actual production ten-second timeout and assert at least
nine elapsed seconds before Failed. Valid held fetch/mark replies are released;
a RAII lifecycle guard confirms the handler either constructed the response or
was cancelled/dropped before full historical page equality is asserted. There is
no detached release watcher, fabricated delivery claim or sleep-based settlement.
Runtime, maintenance, HTTP/trap and SQLite shutdown remain explicitly bounded.
Scoped CLI tests (42), fmt/Clippy and two main independent integration-module
repetitions (four tests each) passed. Main final `pnpm verify` passed: 301 Rust
unit tests, two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy,
contracts/boundaries, lint/typecheck, builds and actual local CLI/HTTP/auth/browser
smoke. Product YAML/34 evidence IDs/11 evidence refs/8 path refs/decisions/disabled
capabilities and diff checks passed separately.

The first full gate stopped before any Vitest test ran: eleven workers exited
during startup with 3221226505. Standalone Vitest and the unchanged full gate
subsequently passed. Cause remains unconfirmed; no code/configuration workaround
or weakened verification was applied.

These are Rust client/real-adapter synthetic upstream scenarios, not shipped-process
preview success, native keychain, browser, restart/migration, live account or all
possible upstream failure coverage. Pending polling/search/manual, restart,
migration and browser-to-real-adapter acceptance remain; keep both tasks active,
connector IDs empty and writes disabled.

## E5a native CLI client integration evidence (2026-10-04)

`crates/collector-cli/src/client/preview_integration_tests.rs::cli_import_uses_real_native_neodb_adapters_and_persists_bounded_history`
uses real Rust pair()/PreviewClient over authenticated Core HTTP, temporary SQLite,
PreviewRuntime::start/runner/record/snapshot and production NeoDbClient. A second
literal-loopback Axum server is the synthetic NeoDB upstream; no catalog/mark/
identity port or Core preview response is substituted. Only clock/secrets are fake.

Raw BOM/Chinese-title CSV yields three observations, one skipped row/column and
one duplicate. Disabled v2 clamps to 5 but explicit v1 still suggests 8/7/6.
The public and stored results preserve equal/skipped, conflict/needs_review with
sink 5, and exact missing-shelf 404 followed by /api/me account confirmation:
unknown rating, `rating_requires_shelf`, never confirmed empty. A final identity
gate proves Running before release; bounded CLI status reconciliation then proves
durable Completed attempt 1. Two result pages preserve stable ordinals, source
IDs/fingerprints, automatic provenance, snapshot/account/profile/time/expiry and
historical=true/executable=false. Normalized observations retain the Chinese title;
closed/reopened storage retains completed history without granting fresh authority.

The exact ten-request upstream trace contains only GET: public native fetch 302
and detail have no authorization; mark/me use only the NeoDB Bearer token. No
Cookie, refetch, CLI Bearer, search, personal mutation or duplicate remote row
request occurs. Public status/pages omit internal credential references, tokens
and discarded raw markers. Preview leaves both fake credential entries unchanged;
closed database and any remaining WAL bytes exclude raw markers/plaintext tokens.
Runtime, maintenance, HTTP servers and SQLite owners are drained/closed within
the scenario's outer bound. Scoped CLI tests (39), fmt/Clippy and main's five
independent exact-test repetitions passed; full-gate evidence follows below.

Main final `pnpm verify` passed: 298 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries, lint/typecheck, builds
and actual local CLI/HTTP/auth/browser smoke. Product YAML/34 evidence IDs/
decisions/references/disabled capabilities and `git diff --check` passed separately.
No production behavior, dependencies, API/schema/migration or capability changed.

This is the Rust CLI client's real-adapter native happy path, not shipped-process/
native-keychain, browser-to-upstream, integrated negative, restart/migration or
live-account evidence. Keep both tasks active, connector IDs empty and writes off.

## Criterion audit (2026-10-03, before E3c verification)

| PRD area | Concrete existing evidence | Still missing |
|---|---|---|
| Identity, polling, search/manual | `collector-connectors/src/neodb/catalog/tests.rs`; Core `catalog_resolver/tests.rs::pending_spacing_includes_request_duration_and_exact_identity_matches`, `search_is_only_used_once_after_unsupported_and_stays_review_only`, `manual_exact_or_missing_identity_is_distinct_and_preserves_full_evidence` | Before E4a the runner was native-only; E4a internal opt-in is recorded below, but public fallback/selection is not delivered |
| Personal marks and comparison | Core `preview/tests.rs::present_rating_comparison_distinguishes_equal_empty_and_conflict`, `missing_shelf_is_empty_only_for_shelf_and_rating_stays_unknown`, `repository_secret_and_mark_failures_are_sanitized_and_stop` | Compose real NeoDB adapter reads with public import, not just fake ports |
| Runtime lifecycle | Server `preview_runtime/tests.rs::submit_and_resume_accept_real_work_with_fixed_metadata`, `shutdown_signals_active_and_queued_without_persisted_cancel`, `production_start_interrupts_unfinished_and_cleans_expired_without_resuming` | Routed CSV -> close/reopen -> fresh explicit resume -> delete is E3c; real client/upstream composition remains separate |
| Normalized persistence/migration | Storage `preview_results/tests.rs::real_producer_results_round_trip_complete_and_reopen`, `populated_exact_v4_schema_upgrades_without_losing_operational_state`; `preview_jobs/tests.rs` | Integrated public migration path; component migration tests alone are not end-to-end |
| Retention and deletion | Server `preview_runtime/tests.rs::maintenance_waits_for_first_tick_then_signals_expired_active_work`, `skipped_interval_does_not_catch_up_after_a_large_time_jump`; Storage `preview_results/tests.rs::terminal_delete_and_expiry_transitions_never_accept_late_results` | Final integration acceptance and preservation checks, not new retention policy |
| HTTP/CLI/WebUI | Server `preview_submission_tests.rs`; CLI `client/preview_tests.rs`; WebUI `PreviewPanel.test.tsx`; `tests/preview-ui-smoke.mjs` | Parent integrated CLI/WebUI-to-simulated-upstream acceptance; the browser preview routes currently use mocks |

Paths prefixed with crate names above are relative to `crates/`; client paths are
under `apps/webui/src/` or `tests/` as named. Upstream timeout/429/auth/redirect
coverage exists at component boundaries. Do not label it a public import-to-real
NeoDB-adapter end-to-end suite. Audit was read-only and did not run tests.

### Next acceptance checkpoints

- E4e completes CLI resolution-choice inputs after E4c/E4d's structured API/shared
  client and interactive WebUI; never turn title hints or a singleton candidate
  into automatic identity proof. Verified checkpoint evidence is recorded below.
- Audit upstream timeout/429/auth/redirect and migration through the integrated
  path, distinguishing prior component evidence from end-to-end coverage.
- Finish parent CLI/WebUI integration review against a simulated upstream server.
- Enable connector capability advertising only when corresponding delivered
  behavior is verified; keep personal writes and scheduling disabled.

Keep this child active until its full criteria pass. Do not archive it based on
the backend restart scenario alone.

## E3c scoped evidence

`crates/collector-server/src/preview_restart_tests.rs::authenticated_import_restart_resume_and_delete_preserve_unrelated_state`
passed after main review strengthened full tuple/token guards, detail-call
counters, shutdown-vs-cancel distinction, preserved HTTP evidence, exact v1
semantics despite changed v2, source URL order, attempt conflicts, both fresh
comparisons, provenance/identity, no credential exposure and post-delete auth.
The clock moves 110 -> 120 -> 130; sink rating moves 8 -> 5. New comparisons
retain source 8/7, original expiry and v1 rather than v2's clamp-to-5 rule.
The fixture drops old owners and closes SQLite before reopening the same directory.
No production/API/dependency/migration/capability change is required.

Main's final `pnpm verify` passed after all edits: 274 Rust unit tests, two
compile-fail doctests, 63 TS/UI tests, fmt/Clippy, generated-contract drift,
dependency boundaries, lint/typecheck, WebUI/CLI builds and actual local host/
authentication/browser smoke. YAML/34 evidence IDs and decision/reference checks
and `git diff --check` passed separately. The first gate exposed an existing
WebUI mock sequencing race; its state-driven fixture repair passed all 15 panel
tests plus five repeated target runs before the final gate. No timeout increases
or production UI changes. These results do not close the remaining criteria above.

## E4a internal search-input evidence

Core `preview_jobs/tests.rs::explicit_search_plan_is_sparse_normalized_bounded_and_unique`
proves sparse unique ordinals and exact ASCII/UTF-8 512-byte validation. Runner
`mixed_native_and_explicit_search_inputs_run_in_ordinal_order` preserves Native
defaults, retains even an exact-ID candidate as review-only, reads no credentials/
marks, and re-forwards the explicit query on a fresh attempt without historical
result input. Native match never searches; pending query reads are cancelled or
timed out, and rebind/deletion after the read stops before catalog operations.

Storage query/reopen/resume tests retain original revision, full binding and
expiry despite v2. Atomic insertion failure removes the whole query-bearing job.
`result_write_requires_the_exact_stored_search_query_proof` refuses Native/Manual/
different-query records for an opted-in slot and accepts the matching normalized
request; query proof is private and absent from historical result JSON. Shared
query decoding rejects corrupt input in query/observation/history/store/completion
paths without advancing the job or overwriting evidence. Populated exact-v5 rows
upgrade to v6 with NULL/native queries and historical evidence retained; prior
schema fixtures and byte-preserving newer-schema refusal also pass.

Main's final `pnpm verify` passed: 282 Rust unit tests, two compile-fail doctests,
63 TS/UI tests, fmt/Clippy, contracts/boundaries, lint/typecheck, builds and actual
local CLI/HTTP/auth/browser smoke. YAML/evidence/decision/reference checks and
`git diff --check` passed separately. This checkpoint adds a usable internal job
input and compatible migration, not a public route or manual-selection mapping.
Public search/manual actions, integrated negative paths and parent client/upstream
acceptance still remain pending. No live upstream/account/keychain verification,
new dependency, source/personal write or connector capability was introduced.

## E4b internal manual-input evidence

Core `catalog.rs::tests::compact_reference_validates_item_and_derives_only_safe_relative_paths`
validates all four supported route/model pairs, UUID bounds and unsafe/mismatched
evidence without a URL parser or transport call. The sparse plan test rejects
duplicates/count/range and search/manual collisions in either builder order.
`invalid_inputs_fail_before_binding_or_catalog_io` includes oversized raw manual
requests. Runner `mixed_manual_search_and_native_inputs_use_fresh_manual_detail_on_every_attempt`
checks ordered mixed modes, fresh detail/mark reads and manual provenance on each
attempt, with no historical-result reads. The unified input-read guard test drops
hung reads on cancellation/deadline and stops before detail/credentials after rebind.

Storage `manual_inputs_survive_reopen_resume_and_corruption_never_becomes_native`
retains compact choices across reopen/resume; payloads exclude URLs and metadata.
Shared strict decoding rejects malformed/version/unknown-field/unsafe UUID inputs
and search/manual collisions. `manual_result_proof_uses_requested_reference_not_redirected_item_evidence`
accepts the requested selection while retaining redirected final evidence and
Manual provenance, rejects another request/Native/Search mode, and checks corrupt
input rejection in history/store/completion without status/time/count changes.
The insertion-failure test includes mixed Manual/Search rollback. Populated exact-v6
migration preserves Native/Search inputs and historical Manual evidence; new NULL
slots accept only Native production and cannot recreate old manual authority.

Scoped Core tests (89 + two compile-fail doctests), Storage (43), Server (87),
fmt and Core/Storage Clippy passed. Main full `pnpm verify` passed: 288 Rust unit
tests, two compile-fail doctests and 63 TS/UI tests, workspace fmt/Clippy,
contracts/boundaries, lint/typecheck, WebUI/CLI builds and actual local
CLI/HTTP/session/pairing/revocation/browser smoke. Product YAML with 34 unique
evidence IDs, decision/reference/capability checks and `git diff --check` passed.
No public search/selection route, UI/CLI action, dependency, capability or personal/
source write is delivered. Broader public and client/upstream acceptance remains
pending; this child stays active. All upstream/account/credential tests are fake.

## E4c structured public submission evidence

The existing authenticated CSV import now also accepts strict JSON containing CSV
and explicit per-record Search/Manual choices. Generated Domain/OpenAPI/JSON Schema
and TS request validators close both variants and nested references. Raw `text/csv`
and existing receipts/auth/deadlines/ambiguous acceptance remain compatible.
`submitImdbRatingsWithResolution` is a thin generated-contract client, not a new
identity policy or workflow owner. Structured resubmission creates a new job;
there is no retained-input/history mutation or automatic candidate matching.

Server `preview_submission_tests.rs::structured_submission_persists_source_bound_native_search_and_manual_inputs`
drives the real runtime/producer/SQLite: missing choices stay Native, explicit
search yields review-only candidates, and manual choice reads fresh detail and
personal state with Manual provenance. Exact counters show only manual comparison
reads a token/mark. `structured_submission_binds_ordinals_after_skip_and_dedup_and_rejects_bad_plans`
binds choices to normalized ordinals and exact source IDs, rejects count > actual
observations and >10,000, negative/fractional/overflow ordinals, duplicate/cross-kind
plans, mismatched IDs/models, unsafe references, blank/control/513-byte UTF-8 queries,
unknown fields/kinds and duplicate JSON fields. The submit spy and external-read
counters stay unchanged through rejection.

`routed_resume_retains_structured_manual_and_search_modes` commits one manual
result, holds later catalog work, cancels via HTTP and explicitly resumes attempt 2.
Exact counters prove a second detail/token/mark read, original choices survive,
attempt-1 evidence is cleared and expiry/profile/full endpoint binding stay fixed.
Shared runtime fakes require explicit opt-in for detail/search/identity/secret/mark
access, preserving prior native-only tests' no-I/O guards. Chunked JSON at exactly
10 MiB is valid and admitted; +1 is rejected. Authentication precedes body polling.
TS tests cover generated pre-fetch rejection, JSON/query/CSRF, invalid receipts,
sanitized uncertain metadata, timeout/caller abort and no retries, retaining native
Request objects in pending mocks.

Scoped Server submission tests (16), fmt/Clippy, generation/drift, lint/typecheck
and focused TS tests (14) passed. Main's final full `pnpm verify`, after review
strengthening, passed: 291 Rust unit tests, two compile-fail doctests, 65 TS/UI
tests, workspace fmt/Clippy, contracts/boundaries, lint/typecheck, builds and actual
local CLI/HTTP/session/pairing/revocation plus desktop/mobile browser smoke.
Product YAML/34 evidence IDs/decision/reference/capability checks and diff check
also passed. Windows-local evidence is not three-platform CI or live NeoDB proof.

No new dependency, migration, Core/Storage production logic, standalone search
route, UI/CLI interactive choice, capability advertisement or personal/source write.
Interactive selection and broader real-client/simulated-upstream negative-path
acceptance remain open; keep this task active. All upstream/account/keychain ports
are fake, with no live native catalog fetch.

## E4d interactive WebUI evidence

`apps/webui/src/PreviewPanel.tsx` stages source-bound Search/Manual inputs using
the existing generated validators/client. Only this mounted panel's confirmed
File/profile/job receipt and completed/current matching metadata establish local
choice authority. Explicit resubmission creates a new job; prior inputs/results
are never edited. Reopened jobs are read-only, and File/profile/attempt/binding/
row-frame changes revoke drafts. No CSV parser, arbitrary UUID/URL input, saved
cross-job mapping or personal/source writer is introduced.

Focused panel tests (35, including 20 new cases) prove Search -> new candidates ->
Manual -> new preview; unsafe legacy reference rejection; Unicode C1/query UTF-8
validation; sparse replace/reset/off-page submission and Compared/off-page clear;
ineligible frames; just-in-time attempt/binding checks; exact encoded JSON 10 MiB
and +1 with escaping; no preflight/POST after local refusal or stale File reads;
stale fulfilled/rejected preflight/submission guards; single-action double-click
and actual unmount abort; known/unknown/auth admission failures without retries
or confirmed File association. Raw known-ID ambiguity clears the previous job/page.
Inspection suppression is scoped to the original job/selection generation and
cannot swallow a newly opened job or explicit refresh.

`tests/preview-ui-smoke.mjs` extends the existing Chromium/local-host harness with
generated request/response schema checks, exact CSV/BOM and v1 despite saved v2,
ordinal/source ID and compact manual reference, three distinct IDs, unchanged old
jobs, no staging POST, keyboard actions and read-only historical reopen. Main
visually inspected desktop/mobile draft and candidate screenshots; overflow and
empty local/session storage assertions passed. Preview routes are mocks; real
host/session/pairing/mapping boundaries still run in `tests/smoke.mjs`. This is
browser/shared-client evidence, not the final real-NeoDB-adapter integration suite.

Main final `pnpm verify` passed: 291 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, fmt/workspace Clippy, generated drift, dependency boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/
34 evidence IDs/decision/reference/disabled-capability and `git diff --check`
passed separately. No live upstream/account/keychain, dependency, migration,
schema/API/CLI/Core/Server/Storage production change, standalone search route or
capability advertisement. Local Windows evidence is not hosted three-platform CI.
CLI resolution-choice inputs and broader simulated-upstream negative/end-to-end
acceptance remain pending; keep this child/parent active and personal writes disabled.

## E4e CLI resolution-choice evidence (2026-10-04)

`collector import imdb-ratings ... --resolution-inputs PATH` accepts a file-only
UTF-8 array of existing Domain choices and submits the existing structured import
API as a new job. Missing flag preserves raw transport. The CLI owns no CSV
parser/identity policy and persists neither raw CSV nor choice-file JSON. Core
still owns normalized ordinal/source matching and validated immutable choices.

Six additional CLI tests (38 total) cover strict nested/duplicate fields and
integer shapes, choice-file 10 MiB and +1, 10,000 and +1 inputs, actual encoded
JSON exact/+1 with escaping, BOM/UTF-8, mixed Search/Manual with Native omission,
actual temp-file preparation and exact health/token/Bearer/body/revision, and
no health/secret/POST after missing/invalid/count/encoded input refusal. Structured
requests share existing admission validation and preserve known IDs without retry.
Known query/search/manual/resolution/reference refusals retain their codes with
static messages; unknown diagnostics and dynamic field paths remain sanitized.

`tests/smoke.mjs` now exercises the shipped flag/help, choice-stdin syntax refusal,
unsafe-origin structured requests and malformed/unknown nested fields, invalid
UTF-8 and missing choice files against an unavailable literal-loopback endpoint.
It asserts local static errors without private path/query/CSV leakage. Synthetic
loopback unit tests prove actual health/secret/POST counters remain zero; these
process refusals do not imply a real-account CLI-to-NeoDB success scenario.

The first full gate found an existing WebUI fixture count race: a page may render
before post-action status reconciliation finishes. Luna/max added only an
observable `aria-busy=false` wait before the baseline; no-staging-I/O assertions
remain intact. All 35 panel tests plus five repeated target runs passed. Main
reviewed that one-line fix and documented settlement in the frontend contract.

Main final `pnpm verify` passed after all code/test changes: 297 Rust unit tests,
two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/
34 evidence IDs/decision/reference/disabled-capability and diff checks passed
separately. No new dependency, schema/API/migration/Core/Server/Storage production
behavior, source/personal write or capability. All upstream/account/credential
fixtures are fake; this is local Windows, not hosted three-platform evidence.
CLI choice inputs are delivered; broader real-client/simulated-NeoDB-adapter
negative, migration and end-to-end acceptance remains pending. Keep this child
and parent active rather than treating this client checkpoint as PRD completion.
