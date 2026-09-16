# NeoDB Collector Implementation Specs

This repository is currently contract-first: application source has not been scaffolded yet. These specs therefore record approved executable boundaries from [`product/collector-spec.yaml`](./product/collector-spec.yaml); they do not claim framework conventions that have not been proven by code.

The product YAML is the machine-readable source of truth. Backend/frontend Markdown files expand it into executable implementation contracts. When implementation establishes repeatable code patterns, update the relevant contracts with real source and test paths.

## Product

- [Product Spec Index](./product/index.md) — canonical YAML ownership and maintenance workflow.
- [Collector Product Spec](./product/collector-spec.yaml) — decisions, platform matrix, fields, deployment, authentication, and evidence.

## Backend

- [Connector Contract](./backend/connector-contract.md) — observations, capabilities, policy, matching, and idempotency.
- [Mapping and Conflict Contract](./backend/mapping-and-conflict-contract.md) — declarative profiles, dry-run, review, and activation.
- [Runtime Distribution and Connectors](./backend/runtime-distribution-and-connectors.md) — cross-platform hosts, singleton NeoDB endpoint, and built-in registry.
- [Operational State and Pairing](./backend/state-and-pairing.md) — embedded SQLite, secrets boundary, pairing URI, and Bearer tokens.
- [Repository Layout and Dependency Boundaries](./backend/repository-layout.md) — Monorepo ownership, workspace boundaries, dependency direction, and independent artifacts.

## Frontend

- [Shared WebUI Boundaries](./frontend/webui-boundaries.md) — one WebUI/API contract across hosts and secret handling.
- [Browser Extension Onboarding](./frontend/browser-extension-onboarding.md) — optional per-site permissions and fallbacks.

## Guides

- [Cross-layer Checklist](./guides/cross-layer-thinking-guide.md)
- [Reuse Checklist](./guides/code-reuse-thinking-guide.md)
