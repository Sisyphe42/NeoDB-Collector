# Preview implementation checkpoints

Status: approved and activated on 2026-09-18.
Dependencies: preview-auth and preview-mapping are completed and archived.

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

- [ ] Add runtime-independent polling/cancellation ports and fake-clock tests.
- [ ] Enforce >=15-second poll spacing and <=120-second overall budget, including
  slow requests; cancellation produces no subsequent poll.
- [ ] Normalize and compare exact IDs, canonical links and media granularity.
- [ ] Add bounded search fallback/manual resolution; title-only/contradictory
  candidates remain review-required, unavailable is not absent.
- [ ] Test movie/show/season distinctions, pending exhaustion, cancellation,
  rate-limit behavior and identity mismatches; verify and commit.

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
