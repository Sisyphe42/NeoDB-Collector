# Approved design

Keep the existing Monorepo boundaries. Domain owns wire contracts; Core owns authentication/preview use cases through ports, mapping and planning. Storage owns SQLite and OS credential adapters. A new built-in connectors crate owns NeoDB network access and IMDb parsing; no source adapter invokes a sink. Server composes the use cases, HTTP guards and background jobs. CLI and WebUI consume the same versioned API and generated response validators.

Three child deliveries: secure local/OAuth access; CSV/mapping; resumable comparison. No personal-state writer implementation. Only app registration and token exchange are allowed remote POST operations. User approved native catalog/fetch on 2026-09-17: it may enqueue catalog creation despite being GET, but must not mutate marks/ratings/reviews or source records. Use validated public canonical links, bounded pending retries, manually validated same-instance detail redirects, and verified external IDs/type granularity. Search or explicit manual mapping remains fallback. Cancelling local preview cannot undo a queued remote catalog fetch.

Persist operational previews, not raw payloads. Preview expiry defaults to 30 days, user-configurable; access is authenticated. Old account/profile snapshots are never silently re-bound or treated as current. Missing/malformed/forbidden remote state is not an empty mark.

Tests use synthetic CSV and a loopback mock NeoDB; label this as integration evidence, not real-account success. No plaintext fallback for secrets. Public release/license and container distribution remain deferred.
