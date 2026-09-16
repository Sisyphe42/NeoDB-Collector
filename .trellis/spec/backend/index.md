# Backend Contracts

The runnable foundation, local session/pairing, NeoDB OAuth HTTP/UI and CLI pairing/status are implemented. Sync, source connectors and distribution clients remain planned. Product authority is `../product/collector-spec.yaml`; individual contracts distinguish implemented behavior from later requirements.

| Contract | Owns |
|---|---|
| [Runnable Foundation](./foundation-runtime.md) | Implemented CLI/config, local HTTP, baseline SQLite, generated contracts and verification |
| [Connector Observation and Capability](./connector-contract.md) | Manifests, observations, policy, matching, and idempotency |
| [Mapping and Conflict](./mapping-and-conflict-contract.md) | Mapping validation, dry-run, activation, and review |
| [Runtime Distribution and Connectors](./runtime-distribution-and-connectors.md) | Desktop targets, singleton NeoDB endpoint, built-in registry |
| [Operational State and Pairing](./state-and-pairing.md) | SQLite boundary, migrations, secret references, and client authentication |
| [NeoDB OAuth Lifecycle](./oauth-lifecycle.md) | Runtime-independent authorization, state consumption, replacement and cleanup |
| [OAuth HTTP and WebUI](./oauth-http.md) | Session/CSRF routes, cookie-free callback and generated client |
| [CLI Client](./cli-client.md) | Stdin pairing, OS-stored client token and authenticated status |
| [Repository Layout and Dependency Boundaries](./repository-layout.md) | Monorepo directories, workspace ownership, dependency direction, and release isolation |

## Pre-Development / Quality Check

Read the owning contract above and active task artifacts. For changes to implemented crates, run scoped Rust tests while iterating and `pnpm verify` before delivery. Verify schema changes regenerate contracts, local API returns structured errors, and no source/NeoDB writes or secret persistence were introduced. Hosted three-OS CI results must be reported separately from local verification.
