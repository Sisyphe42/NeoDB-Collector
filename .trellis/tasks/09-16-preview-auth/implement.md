# Implementation and gates

1. Add DTOs/ports, access-policy tests, SQL migration/auth repository and credential adapter.
2. Add restrictive NeoDB OAuth adapter and tests using local mocked HTTP; verify no source/media-write interface.
3. Compose session/CSRF/pairing/OAuth routes and startup bootstrap; add CLI pairing + authenticated status.
4. Generate wire contracts and add WebUI authorization/session/client management, with unit and live local-browser tests.
5. Update executable backend/frontend specs and parent progress. Run Rust fmt/clippy/tests, contract and boundary checks, TS lint/typecheck/tests/build and real-process smoke (pnpm verify).
6. Commit only after workflow approval; no live remote authorization, source mutation, push or task-complete claims without passing gates.

Specs: backend/state-and-pairing.md, runtime-distribution-and-connectors.md, repository-layout.md, foundation-runtime.md; frontend/webui-boundaries.md; product/collector-spec.yaml; shared guides. Parent artifacts define the overall boundary.
