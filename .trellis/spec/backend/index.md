# Backend Contracts

The runnable foundation and local session/pairing slice are implemented. NeoDB OAuth, CLI pairing, sync, connectors and distribution clients remain planned. Product authority is `../product/collector-spec.yaml`; foundation and state/pairing contracts distinguish implemented behavior from later requirements.

| Contract | Owns |
|---|---|
| [Runnable Foundation](./foundation-runtime.md) | Implemented CLI/config, local HTTP, baseline SQLite, generated contracts and verification |
| [Connector Observation and Capability](./connector-contract.md) | Manifests, observations, policy, matching, and idempotency |
| [Mapping and Conflict](./mapping-and-conflict-contract.md) | Mapping validation, dry-run, activation, and review |
| [Runtime Distribution and Connectors](./runtime-distribution-and-connectors.md) | Desktop targets, singleton NeoDB endpoint, built-in registry |
| [Operational State and Pairing](./state-and-pairing.md) | SQLite boundary, migrations, secret references, and client authentication |
| [Repository Layout and Dependency Boundaries](./repository-layout.md) | Monorepo directories, workspace ownership, dependency direction, and release isolation |

## Pre-Development / Quality Check

Read the owning contract above and active task artifacts. For changes to implemented crates, run scoped Rust tests while iterating and `pnpm verify` before delivery. Verify schema changes regenerate contracts, local API returns structured errors, and no source/NeoDB writes or secret persistence were introduced. Hosted three-OS CI results must be reported separately from local verification.
