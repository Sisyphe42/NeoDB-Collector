# Foundation design

Use five Cargo members (domain, core, storage, server, cli), three pnpm members (webui, contracts-ts, core-client-ts). No empty future packages. Domain owns serde/schemars/utoipa DTOs; Core owns read-only status composition and a storage-health port. Storage implements the port with SQLx. Server owns Axum routes and static assets. CLI owns config resolution, startup and diagnostics.

YAML v1 has server.bind/port, storage.data_dir, webui.assets_dir. CLI overrides environment overrides YAML overrides defaults. YAML paths are relative to its parent; CLI/env paths are relative to cwd. Bind must be a loopback IP. Port 0 is allowed for isolated tests. Default data path uses OS app-data resolution; assets default to apps/webui/dist relative to cwd. Unknown YAML fields and unsupported schema versions fail. Doctor uses the same resolver and opens/migrates the operational database, documenting this local effect.

HTTP GET health, capabilities and schema only. Errors use ApiError. Health reports a real storage probe; SQLite failure returns 503. Host and Origin validation protect the local service, no CORS grants. API misses remain structured 404s rather than SPA fallback. Missing assets produce a useful 503 at `/`. No auth required until a mutation or private data endpoint is introduced.

Rust DTOs generate draft-2020 JSON Schemas; route annotations generate OpenAPI. A Node tool generates TS types through openapi-typescript and standalone AJV validators from these artifacts in memory. Check mode compares exact normalized output without rewriting files. The typed client uses openapi-fetch and generated response validators.

SQLite migration 1 is a no-business-table baseline. Read-only preflight checks for migration versions newer than this binary before writable open. No retention policy is frozen. Tests own temporary databases. CLI handles Ctrl-C and Unix SIGTERM; graceful shutdown can be injected in server tests.

WebUI is a responsive Chinese status page using real endpoints and no localStorage, fake connectors or controls for unimplemented behavior. It exposes refresh, health details and a clear empty connector state.

No new release license; packages remain private/unpublished. No remote exists, so CI configuration can be delivered but hosted results cannot be claimed.
