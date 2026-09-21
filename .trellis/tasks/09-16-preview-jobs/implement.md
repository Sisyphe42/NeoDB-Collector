# Preview implementation checkpoints

Status: approved and activated on 2026-09-18.
Dependencies: preview-auth and preview-mapping are completed and archived.

## Delegation for remaining work (user request, 2026-09-21)

The main agent owns planning, boundaries, acceptance and checkpoint commits.
Delegate bounded local/mechanical implementation to `gpt-5.6-luna` / `max`, and
cross-module or authentication work to `gpt-5.6-sol` / `medium`. Supply the active
task path and exact scope/specs explicitly, split file ownership and prohibit
recursive delegation. The current secret-value review/test completion uses Sol.
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
- [ ] Add bounded search fallback/manual resolution; title-only/contradictory
  candidates remain review-required, unavailable is not absent.
- [x] Test movie/show/season distinctions, pending exhaustion, cancellation,
  rate-limit behavior and identity mismatches; verify and commit.

Checkpoint B1 verification (2026-09-18): native resolver, IMDb expectation
validation and Tokio/CancellationToken adapter passed `pnpm verify`: 76 Rust
tests, 25 TS/UI tests, Clippy, contracts, boundaries, builds and browser smoke.
No live catalog fetch or personal-state write. Search/manual fallback is still
unchecked above; B is not complete and the child remains active.

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

### C. Personal-state reads and pure comparison

- [ ] Verify mark-read API and absence semantics with primary evidence; persist
  references in canonical specs before relying on platform behavior.
- [ ] Implement restrictive authenticated read port/adapter and normalized snapshots.
- [ ] Reuse mapping evaluator; test equal, empty, conflict, missing shelf, stale
  account and unreachable state. No writer or activation path.
- [ ] Verify and commit the complete internal comparison slice.

### D. Persistent resumable jobs

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
