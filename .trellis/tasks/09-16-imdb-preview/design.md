# Approved design

Keep the existing Monorepo boundaries. Domain owns wire contracts; Core owns authentication/preview use cases through ports, mapping and planning. Storage owns SQLite and OS credential adapters. A new built-in connectors crate owns NeoDB network access and IMDb parsing; no source adapter invokes a sink. Server composes the use cases, HTTP guards and background jobs. CLI and WebUI consume the same versioned API and generated response validators.

Three child deliveries: secure local/OAuth access; CSV/mapping; resumable comparison. No media writer implementation. Only app registration and token exchange are allowed remote POST operations. Never call catalog/fetch even though it uses GET: it can enqueue catalog creation. Resolve existing items through search plus verified external IDs/details or explicit user mappings.

Persist operational previews, not raw payloads. Preview expiry defaults to 30 days, user-configurable; access is authenticated. Old account/profile snapshots are never silently re-bound or treated as current. Missing/malformed/forbidden remote state is not an empty mark.

Tests use synthetic CSV and a loopback mock NeoDB; label this as integration evidence, not real-account success. No plaintext fallback for secrets. Public release/license and container distribution remain deferred.
