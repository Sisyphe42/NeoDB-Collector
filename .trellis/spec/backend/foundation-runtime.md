# Runnable Foundation

## 1. Scope / Trigger

Applies to configuration, startup/doctor, SQLite baseline, health/capabilities/schema, generated contracts and status WebUI. Implemented sessions/pairing and read-only OAuth are specified in [Operational State and Pairing](./state-and-pairing.md). Internal preview startup/retention is implemented, but public import/preview delivery, synchronization, scheduling, desktop and external integrations remain deferred.

Source owners: `crates/collector-domain`, `collector-core`, `collector-storage`, `collector-server`, `collector-cli`; `packages/contracts-ts`, `core-client-ts`; `apps/webui`.

## 2. Signatures

```text
collector serve [--config PATH] [--bind IP] [--port PORT] [--data-dir PATH] [--assets-dir PATH] [--preview-retention-days DAYS]
collector doctor --format human|json [same overrides]
GET /api/v1/health        -> HealthDocument (200 or degraded 503)
GET /api/v1/capabilities  -> CapabilityDocument (200)
GET /api/v1/schema        -> OpenAPI 3.1 (200)
GET /                    -> built WebUI or structured 503
GET /assets/*            -> static bundle assets
```

`collector_core::StorageHealth::is_ready` returns a boxed Send future without requiring a concrete async runtime in Core. SQLx implements it with a real `SELECT 1`; the server uses that result, not a constant success flag.

SQLite uses `{data_dir}/collector.db`; `0001_baseline.sql` establishes SQLx's migration ledger, `0002_access.sql` adds hashed local-access credentials and singleton endpoint metadata, `0003_mapping_profiles.sql` adds immutable disabled profile revisions, and `0004_preview_jobs.sql` adds bounded normalized preview inputs and lifecycle metadata. `0005_preview_results.sql` preserves v4 data while adding bounded historical result storage and guarded completed jobs. Internal preview runtime is composed at serve startup; public submission/routes remain pending. No raw payloads or plaintext secrets are stored. Existing databases are checked read-only for newer migration versions before writable migration startup.

## 3. Contracts

- Config owner: `crates/collector-cli/src/config.rs`. YAML requires `schema_version: 1`, optional `server.bind/port`, `storage.data_dir`, `webui.assets_dir`, `preview.retention_days`; unknown keys fail at every level.
- Precedence: CLI > environment > YAML > defaults. Environment keys: `NEODB_COLLECTOR_CONFIG`, `BIND`, `PORT`, `DATA_DIR`, `ASSETS_DIR`, `PREVIEW_RETENTION_DAYS` (all with the same prefix). Retention is a positive u32 day count, default 30, using Core PreviewRetention; selected zero/invalid CLI/environment values are config_invalid on preview.retention_days. YAML type/range/unknown-field failures use the existing config field diagnostic. Checked expiry rejects negative time/overflow; saved expiry is never rewritten.
- YAML paths resolve against its parent directory; CLI/environment paths against cwd. No implicit config-file search. Defaults: `127.0.0.1:19423`, `directories::ProjectDirs` local data directory, `apps/webui/dist` from cwd. Explicit data-directory settings must still work when OS directory discovery is unavailable. Port 0 requests an ephemeral port.
- Only literal loopback IPs are accepted. Host must equal the actual listener address or `localhost:{actual_port}`. If Origin exists it must match `http://{Host}`. No CORS grants. Loopback HTTP is deliberate; remote exposure is not implemented.
- `serve` binds before acquiring a serve-only data-directory lease and opening/
  migrating the DB. CLI `serve_lease.rs` opens `collector.serve.lock` read/write/
  create without truncation, then uses standard File::try_lock. Refuse another
  cooperating host on any port before DB mutation. Hold the handle through worker
  drain and pool close; never delete the lock file or use stale PID checks.
  Handle drop/process exit releases the lock. This does not protect against an
  external tool deleting/replacing the lock file.
- Compose one Storage/clock/secrets/NeoDbClient with OAuth and preview snapshot/
  producer/runner/runtime. Startup recovery/cleanup precedes published URL/bootstrap;
  it never auto-enqueues saved jobs or reads network/keychain. Ctrl-C/Unix SIGTERM
  and maintenance failure gracefully stop HTTP. Normal/error paths signal/drain
  runtime before pool close/lease release; see [Owned Preview Runtime](./preview-runtime.md).
- `doctor` creates the data directory and opens/migrates the DB; it is not a read-only operation. It never acquires the serve lease, interrupts preview jobs or launches maintenance. Checks are config, directory accessibility, database write/migration readiness, port availability and assets readability. Invalid config stops subsequent checks. JSON is a deterministic report `{ok, checks: [{name,status,code,message}]}`; success exits 0, failed checks exit 1. Running Core makes the same-port diagnostic fail by design.
- Rust DTOs in `collector-domain` own wire fields. `HealthDocument` is camelCase with `status`, `coreVersion`, `apiVersion`, `schemaVersion`, `storage`. Capability `connectorIds` is empty until connectors are implemented. Media types stay strings, not Core enums.
- `tools/contracts.mjs` generates OpenAPI/JSON Schema/TS types and AJV standalone validators. Check mode compares bytes in memory and never rewrites artifacts. Rust schema integers include explicit bounds; AJV treats `uint32` as an annotation while integer/minimum/maximum enforce it. Keep TS within the shared peer range of openapi-typescript and typescript-eslint.
- `packages/core-client-ts` consumes generated types and validates actual responses, checks API/schema compatibility, uses a bounded request timeout, and preserves health documents on HTTP 503. UI never reaches Rust internals.
- Browser status has loading, connected, unreachable, incompatible, invalid-response and degraded states. Refresh cancels obsolete requests. No secret or status is persisted in browser storage.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Non-loopback bind, malformed port, unknown YAML key/schema | `config_invalid`, exit 1 |
| Port unavailable | `port_unavailable`, no DB change from `serve` |
| Same data directory owned by another cooperating host on any port | core_data_directory_in_use; no migration/recovery/bootstrap |
| Cannot open/acquire directory lease | core_data_directory_lock_failed; no DB fallback |
| Preview recovery/startup cleanup failure | Static preview startup error; no published bootstrap |
| Periodic cleanup failure | preview_maintenance_failed; graceful stop, drain before pool close |
| Data directory inaccessible | diagnostic/startup error, no fallback location |
| Database version newer than binary | `database_version_unsupported`, database bytes unchanged |
| Invalid migration checksum/history | `database_migration_failed`, no automatic repair |
| Closed/unavailable DB during health request | 503 with `status=degraded`, `storage=unavailable` |
| Missing WebUI index | 503 `webui_assets_missing`; API still works |
| Unimplemented API route | JSON 404 `route_not_found`, never HTML SPA fallback |
| Disallowed method on API route | JSON 405 `method_not_allowed` |
| Bad Host or cross-origin request | 403 `host_not_allowed` / `origin_not_allowed` |
| Unsupported response version | client `incompatible`, no feature rendering |
| Malformed response | `invalid_response` or `unavailable` for unreadable JSON |

## 5. Good / Base / Bad Cases

- Good: `serve --port 0 --data-dir <temporary directory>` reports its actual address; the bundled UI shows the DB probe and empty capabilities from that server.
- Base: no assets are built; CLI and APIs remain usable and `/` tells the user to build or configure assets.
- Bad: showing “connected” from a hard-coded fixture, creating sync tables early, accepting an arbitrary LAN bind, or rewriting generated contracts during a check.

## 6. Tests Required

- `crates/collector-cli/src/config.rs`: precedence, YAML-relative paths, explicit overrides, invalid keys/schema/IP/port/retention; absent preview defaults 30.
- `crates/collector-cli/src/serve_lease.rs`: separate handles exclude each other,
  release/reacquire, stale-file/content preservation, independent directories and
  sanitized filesystem failure. CLI startup tests inject shutdown/fakes and prove
  doctor never recovers preview jobs.
- `crates/collector-cli/src/main.rs`: doctor checks report missing assets and an occupied port while testing real temporary storage.
- `crates/collector-storage/src/lib.rs`: empty migration, prior-v1-v3 upgrade, repeat startup, operational tables only, real readiness, byte-preserving newer-DB rejection. Prior-schema fixtures remove all later tables in foreign-key order and every later migration-ledger entry before reopening. `src/preview_results/tests.rs` upgrades populated v4 fixtures with the committed v4 CHECK/FK definitions rather than merely deleting the v5 ledger entry.
- `crates/collector-server/src/lib.rs`: real routes, degraded 503, unknown API 404, method 405, Host/Origin denial, no CORS, injected graceful shutdown releases the port.
- `packages/core-client-ts/src/index.test.ts`: runtime schema rejection, version mismatch, network errors and valid degraded responses.
- `apps/webui/src/App.test.tsx`: loading, refresh, empty capabilities and all error states.
- `tests/smoke.mjs`: real binary doctor/serve, ephemeral address, port conflict, live API and Chromium desktop/mobile UI, no page errors/storage, cleanup of its own temporary directory. Windows process cleanup is forceful; graceful server lifecycle is tested separately.
- `pnpm verify` is the root full gate: Rust format/clippy/tests, non-mutating contract drift check, import boundaries, ESLint, TypeScript, Vitest, WebUI build, binary build, process/browser smoke, `git diff --check`. `.github/workflows/verify.yml` runs the same gate on Windows/macOS/Linux; a CI definition alone is not hosted evidence.
- Toolchain/cache failures are environment issues, not permission to skip gates. Use a disposable Cargo cache when global cache contents are incomplete; never repair global user caches silently. `tools/verify.mjs` supports both native and JS pnpm launchers.

## 7. Wrong vs Correct

```ts
// Wrong: static type assertion trusts arbitrary network data.
const health = await response.json() as Health;

// Correct: generated validator first, supported version check second.
if (!validateHealth(value)) throw new CoreClientError('invalid_response', 'Invalid response');
if (value.apiVersion !== API_VERSION) throw new CoreClientError('incompatible', 'Upgrade required');
```

```text
Wrong: open writable SQLite -> migrate -> discover a newer schema.
Correct: read-only migration preflight -> reject future versions -> writable migration.
```
