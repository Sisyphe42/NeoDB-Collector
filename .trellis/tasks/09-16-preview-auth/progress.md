# Implementation checkpoint — 2026-09-16

Task remains in progress and is not ready to archive. Local access, OAuth transport and Core lifecycle are committed. OAuth HTTP/UI composition is implemented; CLI pairing/status remains pending.

## Completed local-access slice

- Domain DTOs, pure Core ports/policy, migration 2 and transactional hashed-credential repository.
- OS credential-store adapter (compiled; no real keychain test or account access).
- WebUI fragment bootstrap, HttpOnly/Strict session, CSRF, pairing creation/exchange, client listing/revocation/logout, API limits and headers.
- Generated contracts and validating TS client; WebUI extends the existing console rather than replacing its visual design.
- Updated product/executable contracts and README with current versus pending capabilities.

## Verified

`pnpm verify` passed locally on Windows: 14 Rust tests, 17 TypeScript/UI tests, formatting/clippy/lint/typecheck, contract drift, dependency boundaries, builds, real CLI/HTTP/Chromium smoke and `git diff --check`. Desktop/mobile captures inspected under ignored `test-results/`; no secrets are visible. Smoke uses isolated temporary DB and removes it. Hosted macOS/Linux CI has not run here.

Two test issues fixed: the cancellation mock now observes the incoming Request's signal instead of cloning it again; the stderr startup parser waits for a complete newline before parsing the bootstrap URL and does not include secret-bearing output in errors.

## Continue next

1. Restrictive typed NeoDB OAuth transport and pure Core orchestrator are implemented. Ten Core tests cover lifecycle, failures, concurrent reservations and recovery using mock ports; see backend/oauth-lifecycle.md. No `/api/catalog/fetch`, media writes or real account authorization.
2. OAuth HTTP routes, generated contracts, CLI production composition and WebUI status/authorize/cancel are implemented. Tests use fake upstream and credentials; no real-account evidence is claimed.
3. Add CLI pair/status via the same API and OS credential store; API token exchange exists but no CLI command yet.
4. Commit each verified coherent code slice automatically under the project workflow; request confirmation only if direction, ownership or verification is unclear.
5. Only after this child finishes, review/activate mapping and preview-jobs children. Parent is not complete.

## Core OAuth checkpoint validation

Full `pnpm verify` passed on Windows (Rust workspace, 17 TS/UI tests, Clippy,
formatting, contracts, dependency boundaries, lint/typecheck, builds and real
CLI/Chromium smoke). The final additional rollback-cleanup test and partial-write
credential mock also passed scoped Core tests and Clippy. Core now has 10 tests.
No native credential store or real NeoDB account was accessed.

The first full run hit the existing TS client cancellation test's 5-second
timeout. An isolated TS rerun and the subsequent full verify both passed without
code changes to that test; its intermittent cause remains unconfirmed.

## OAuth HTTP/UI checkpoint

Full Windows verify passed: 28 Rust tests, 19 TS/UI tests, Clippy, format,
contracts, boundaries, lint/typecheck, build and local CLI/Chromium smoke.
OAuth status in browser smoke is an explicit fixture to avoid native keychain access;
real router tests inject mock transport/credentials and exercise SQLite metadata.
Mobile capture inspected: controls fit 390px with no horizontal overflow.
Rust emitted incremental-cache access-denied notes but all compile/check commands
completed successfully; no cache deletion or system changes were needed.
