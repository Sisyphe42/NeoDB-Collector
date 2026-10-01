# Backend Contracts

The runnable foundation, local session/pairing, NeoDB OAuth HTTP/UI and CLI pairing/status are implemented. Sync, source connectors and distribution clients remain planned. Product authority is `../product/collector-spec.yaml`; individual contracts distinguish implemented behavior from later requirements.

| Contract | Owns |
|---|---|
| [Runnable Foundation](./foundation-runtime.md) | Implemented CLI/config, local HTTP, baseline SQLite, generated contracts and verification |
| [Connector Observation and Capability](./connector-contract.md) | Manifests, observations, policy, matching, and idempotency |
| [IMDb CSV Adapter](./imdb-csv.md) | Internal bounded ratings parser and normalized observations |
| [NeoDB Catalog Transport](./neodb-catalog.md) | Internal native fetch and restrictive catalog-detail evidence reads |
| [Catalog Identity Resolvers](./catalog-resolver.md) | Bounded native polling, explicit manual selection and identity/granularity checks |
| [Bounded Catalog Search](./catalog-search.md) | Unsupported-only first-page fallback and review-only candidates |
| [NeoDB Personal Mark Reads](./neodb-mark-read.md) | Restrictive authenticated single-item reads and narrow missing-shelf semantics |
| [Preview Snapshots and Comparison](./preview-comparison.md) | Account-bound snapshots, unknown/empty distinction and pure non-executable field comparison |
| [Single-record Preview](./preview-records.md) | Internal resolution/snapshot/comparison producer with explicit provenance and whole-record cancellation/deadline; public delivery pending |
| [Persistent Preview Jobs](./preview-jobs.md) | Bounded normalized inputs, transactional lifecycle/attempt guards and retention policy; read API implemented, submission pending |
| [Persistent Preview Results](./preview-results.md) | Guarded normalized historical evidence, current-attempt read API pages and exhaustive conditional completion |
| [Single-attempt Preview Execution](./preview-runner.md) | Internal sequential claim/produce/persist/complete orchestration; public API pending |
| [Owned Preview Runtime](./preview-runtime.md) | Internal single-job admission, bounded waiters, explicit cancel/delete, startup recovery, retention and shutdown; API pending |
| [Historical Preview HTTP](./preview-http.md) | Authenticated job/result inspection, strict attempt/ordinal pagination and historical evidence; submission/management/clients pending |
| [Mapping and Conflict](./mapping-and-conflict-contract.md) | Mapping validation, dry-run, activation, and review |
| [Mapping Evaluator](./mapping-evaluator.md) | Implemented pure suggestions and bounded YAML/JSON configuration |
| [Mapping Storage](./mapping-storage.md) | Immutable disabled profile revisions and atomic version checks |
| [Mapping HTTP](./mapping-http.md) | Authenticated validation/read/save and generated clients |
| [Runtime Distribution and Connectors](./runtime-distribution-and-connectors.md) | Desktop targets, singleton NeoDB endpoint, built-in registry |
| [Operational State and Pairing](./state-and-pairing.md) | SQLite boundary, migrations, secret references, and client authentication |
| [NeoDB OAuth Lifecycle](./oauth-lifecycle.md) | Runtime-independent authorization, state consumption, replacement and cleanup |
| [Internal Secret Values](./internal-secrets.md) | SecretString ports, explicit exposure and unchanged wire/credential boundaries |
| [OAuth HTTP and WebUI](./oauth-http.md) | Session/CSRF routes, cookie-free callback and generated client |
| [CLI Client](./cli-client.md) | Stdin pairing, OS-stored client token and authenticated status |
| [Repository Layout and Dependency Boundaries](./repository-layout.md) | Monorepo directories, workspace ownership, dependency direction, and release isolation |

## Pre-Development / Quality Check

Read the owning contract above and active task artifacts. For changes to implemented crates, run scoped Rust tests while iterating and `pnpm verify` before delivery. Verify schema changes regenerate contracts, local API returns structured errors, and no source/NeoDB writes or secret persistence were introduced. Hosted three-OS CI results must be reported separately from local verification.
