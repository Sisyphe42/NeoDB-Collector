# Runnable foundation

## Goal

Deliver the user-approved runnable foundation: local Core HTTP service, CLI diagnostics, migration-only SQLite, generated contracts, and a Chinese status WebUI.

## Requirements

- Follow the approved Monorepo boundaries and Rust/React stack.
- Configuration is readable YAML; precedence is CLI > environment > YAML > defaults.
- Only loopback, read-only service endpoints are available in this milestone.
- Rust owns wire types; OpenAPI, JSON Schema and TS contracts are reproducible outputs.
- No platform connections, credentials, sync writes, business tables, scheduling, desktop or integration scaffolds.

## Acceptance Criteria

- [x] `collector serve` hosts API and built WebUI on one configurable port (default 19423).
- [x] `collector doctor --format json` reports config, data directory, database migration, port and assets with a meaningful exit code.
- [x] Health, capabilities and schema endpoints expose only implemented features and no private paths.
- [x] WebUI covers loading, connected, unavailable, incompatible and degraded storage states.
- [x] Invalid config and newer database migrations are rejected safely.
- [x] Contract drift check does not modify tracked files.
- [x] Rust, TS, UI and actual-server smoke checks pass locally; CI defines equivalent checks on all three OS targets.

## Notes

- User approved the complete plan with “实施计划”. Later milestones retain all existing product safety gates.
