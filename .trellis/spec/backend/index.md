# Backend Contracts

The application has not been scaffolded yet. These files define approved implementation contracts from `../product/collector-spec.yaml`; add framework-specific conventions only after source and tests establish them.

| Contract | Owns |
|---|---|
| [Connector Observation and Capability](./connector-contract.md) | Manifests, observations, policy, matching, and idempotency |
| [Mapping and Conflict](./mapping-and-conflict-contract.md) | Mapping validation, dry-run, activation, and review |
| [Runtime Distribution and Connectors](./runtime-distribution-and-connectors.md) | Desktop targets, singleton NeoDB endpoint, built-in registry |
| [Operational State and Pairing](./state-and-pairing.md) | SQLite boundary, migrations, secret references, and client authentication |
| [Repository Layout and Dependency Boundaries](./repository-layout.md) | Monorepo directories, workspace ownership, dependency direction, and release isolation |
