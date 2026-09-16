# Implementation and gates

1. Create workspace manifests, configuration resolver, DTOs and Core health composition.
2. Implement SQLx migration baseline and preflight, Axum API/static serving, CLI serve/doctor and lifecycle.
3. Generate OpenAPI/JSON Schema/TS contracts and runtime validators; implement typed client and status WebUI.
4. Add config, migration, route, client, UI and real-process smoke tests. Add three-OS CI and root verify command.
5. Update `.trellis/spec/backend/foundation-runtime.md`, relevant indexes and product decisions to actual source paths; preserve later product contracts as explicitly deferred.
6. Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, pnpm lint/typecheck/test/build, contracts:check, boundary checks and real server/browser smoke via `pnpm verify`; inspect diffs and `git diff --check`.
7. Commit scoped implementation, archive task and record session per workflow. Preserve all user-owned changes.

Applicable specs: product/collector-spec.yaml; backend/repository-layout.md; backend/state-and-pairing.md; backend/runtime-distribution-and-connectors.md; frontend/webui-boundaries.md; guides/cross-layer-thinking-guide.md; guides/code-reuse-thinking-guide.md (all under `.trellis/spec/`).

Rollback: remove only changes from this task by a reviewed follow-up commit; never reset the workspace. No source/NeoDB data is touched by this milestone.

## Verification record — 2026-09-16

- Steps 1–6 complete. `pnpm verify` passed on Windows: Rust format/clippy, 7 Rust tests, generated-contract comparison, dependency boundaries, ESLint, TypeScript, 11 client/UI tests, production build, actual binary/API/Chromium desktop/mobile smoke, and `git diff --check`.
- Actual browser screenshots: ignored `test-results/status-desktop.png` and `status-mobile.png`, visually reviewed. No fake API fixtures used in browser smoke; unit tests use explicit isolated fixtures.
- Negative contract-drift probe: introduced a temporary output comment; `contracts:check` returned 1 and before/after SHA256 stayed equal. Restored probe; full gate passed afterward. Export uses `cargo --locked` so checking cannot update the lockfile either.
- Source/spec review confirmed empty connectors, no platform credentials or writes, no business tables, version rejection, same-port serving and explicit future-feature boundaries.
- CLI/YAML/env data-directory overrides still work when OS default discovery is unavailable; regression exercised in config tests.
- Three-OS CI is configured, not run remotely (no configured remote). Local toolchain: Rust 1.98.0, Node 26.7.0, pnpm 10.33.0. Used isolated Cargo cache and process-local Rust bin PATH as documented in research.
- User confirmed the proposed work commit with “继续”: `feat: add runnable core CLI and status WebUI`. Finish-work archives this task and records the journal after the work commit; no remote push is authorized.
