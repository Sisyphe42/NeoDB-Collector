# Backend Contracts

The runnable foundation, local session/pairing, NeoDB OAuth HTTP/UI and CLI pairing/status are implemented. Sync, source connectors and distribution clients remain planned. Product authority is `../product/collector-spec.yaml`; individual contracts distinguish implemented behavior from later requirements.

| Contract | Owns |
|---|---|
| [Runnable Foundation](./foundation-runtime.md) | Implemented CLI/config, local HTTP, baseline SQLite, generated contracts and verification |
| [Connector Observation and Capability](./connector-contract.md) | Manifests, observations, policy, matching, and idempotency |
| [IMDb CSV Adapter](./imdb-csv.md) | Internal bounded ratings parser and normalized observations |
| [NeoDB Catalog Transport](./neodb-catalog.md) | Internal native fetch and restrictive catalog-detail evidence reads |
| [Mapping and Conflict](./mapping-and-conflict-contract.md) | Mapping validation, dry-run, activation, and review |
| [Mapping Evaluator](./mapping-evaluator.md) | Implemented pure suggestions and bounded YAML/JSON configuration |
| [Mapping Storage](./mapping-storage.md) | Immutable disabled profile revisions and atomic version checks |
| [Mapping HTTP](./mapping-http.md) | Authenticated validation/read/save and generated clients |
| [Runtime Distribution and Connectors](./runtime-distribution-and-connectors.md) | Desktop targets, singleton NeoDB endpoint, built-in registry |
| [Operational State and Pairing](./state-and-pairing.md) | SQLite boundary, migrations, secret references, and client authentication |
| [NeoDB OAuth Lifecycle](./oauth-lifecycle.md) | Runtime-independent authorization, state consumption, replacement and cleanup |
| [OAuth HTTP and WebUI](./oauth-http.md) | Session/CSRF routes, cookie-free callback and generated client |
| [CLI Client](./cli-client.md) | Stdin pairing, OS-stored client token and authenticated status |
| [Repository Layout and Dependency Boundaries](./repository-layout.md) | Monorepo directories, workspace ownership, dependency direction, and release isolation |

## Pre-Development / Quality Check

Read the owning contract above and active task artifacts. For changes to implemented crates, run scoped Rust tests while iterating and `pnpm verify` before delivery. Verify schema changes regenerate contracts, local API returns structured errors, and no source/NeoDB writes or secret persistence were introduced. Hosted three-OS CI results must be reported separately from local verification.
