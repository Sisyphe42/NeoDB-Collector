# Operational State and Client Pairing

## 1. Scope / Trigger Conditions

This contract applies whenever Core persists state across restarts or authorizes a local client. The local-access slice is implemented: WebUI bootstrap/session/CSRF, pairing, client listing/revocation and the OS credential adapter. The [OAuth lifecycle](./oauth-lifecycle.md) and restrictive transport are composed through the [OAuth HTTP/WebUI](./oauth-http.md) boundary by CLI startup. [CLI pairing and authenticated status](./cli-client.md) are implemented; imports and preview jobs remain unfinished. No media-platform writes are enabled. SQLite exists only for operational metadata, not a media-history warehouse.

Owners: `crates/collector-core/src/access.rs` (ports/policy), `collector-domain/src/access.rs` (DTOs), `collector-storage/src/access.rs` and `credentials.rs` (adapters), `collector-server/src/access.rs` (HTTP boundary).

## 2. Signatures and Data Shapes

The default database is `{data_dir}/collector.db`, where `data_dir` resolves in this order: CLI `--data-dir`, `NEODB_COLLECTOR_DATA_DIR`, YAML `storage.data_dir`, then the operating-system application-data directory. See [Runnable Foundation](./foundation-runtime.md) for path bases and implemented migration/startup behavior.

Migration `0002_access.sql` implements:

```sql
access_credentials(id, token_hash, kind, audience, name, client_kind, expires_at, revoked)
neodb_endpoint(singleton, base_url, account, credential_ref)
```

Kinds are bootstrap/session/pairing/client; hashes are unique. `singleton=1` constrains endpoint metadata exposed through authenticated GET /api/v1/settings/neodb.
Migration `0003_mapping_profiles.sql` now stores immutable validated disabled
configuration revisions: `mapping_profiles(id, version, profile_json)`; see
[Mapping Storage](./mapping-storage.md). Later schema ownership (not implemented):

```sql
sync_checkpoints(connector_instance_id, stream, cursor_json, updated_at)
item_mappings(connector_id, external_item_id, canonical_url, neodb_item_uuid, method, resolved_at)
planned_operations(id, canonical_item_id, field, operation, value_json, status, reason)
operation_results(id, planned_operation_id, idempotency_key, status, error_code, executed_at)
```

Implemented routes:

```text
POST   /api/v1/session/bootstrap -> SessionView + Set-Cookie
GET    /api/v1/session           -> SessionView
DELETE /api/v1/session           -> ActionResult + expired cookie
POST   /api/v1/pairings          -> PairingLink
POST   /api/v1/pairings/exchange -> PairingToken
GET    /api/v1/clients           -> ClientList
DELETE /api/v1/clients/{id}      -> ActionResult
```

Domain DTOs generate OpenAPI, JSON Schema and TS validators. Fields are camelCase except the established snake_case exchange/token bodies. Pairing exchange:

```http
POST /api/v1/pairings/exchange
Content-Type: application/json

{
  "one_time_key": "base64url-256-bit-value",
  "client_name": "Edge extension",
  "client_kind": "browser_extension"
}
```

```json
{
  "client_id": "client_...",
  "token": "base64url-256-bit-value",
  "token_type": "Bearer",
  "scopes": ["settings.read", "imports.read", "imports.submit", "mapping.read", "mapping.write"]
}
```

## 3. Behavioral Contracts

- Initial scopes are fixed non-admin privileges, not client-requested authority; reserved scopes do not enable absent routes. Future heterogeneous scopes require per-client persistence/enforcement, not expanding this constant to grant existing tokens new privileges.
- Startup delivers one 256-bit bootstrap key in the local terminal's `/#key=...` URI. It expires in 600 seconds and is consumed transactionally once. This explicit delivery is not a request log. Exchange requires exact Origin.
- Sessions are opaque random credentials, hash-only at rest, HttpOnly, SameSite=Strict, Path=/, absolute lifetime 8 hours. Secure is deliberately omitted on loopback HTTP. Stateful cookie requests require exact Origin and session-bound `X-CSRF-Token`; Authorization never falls back to cookies on admin routes.
- Pairing names are nonempty after trimming, at most 80 UTF-8 bytes, no control characters; kinds are cli/browser_extension/local_plugin. Challenges bind both values and the actual Core origin including port. Changing port requires new pairing; restart preserves live credentials and revocations on the same origin.
- After session and Origin validation, Server CSRF verification rejects unequal
  lengths, then uses `subtle::ConstantTimeEq`
  on equal-length byte slices, not a handwritten XOR loop. Missing, empty, short,
  long or mismatched tokens return the same `403 csrf_invalid`; failures do not
  consume the session. This does not claim constant-time behavior for the whole HTTP path.
- `AccessRepository::exchange(ChallengeMatch, AccessRecord)` consumes a live matching challenge and inserts its replacement in one transaction; insertion failure rolls back consumption. Revocation is idempotent and only targets client-kind records; logout revokes only the current session.
- Bootstrap, pairing creation and exchange share a process-wide 60-attempt/minute fixed window and 4096-byte JSON body cap. Every response, including denials, has no-store/nosniff/no-referrer and restrictive CSP. Expiry is checked on every lookup; expired non-client records are cleaned at startup.
- `OsSecretStore` uses keyring 3.6.3: Windows Credential Store, macOS Keychain, Linux Secret Service with encrypted DBus transport. Blocking workers serialize native calls within the process; failures are sanitized and have no plaintext fallback. Linux builds need libdbus headers/pkg-config. CLI OAuth composition uses this adapter; tests inject fake credentials and never access real keychains.
- A pairing URI has the shape `http://127.0.0.1:{port}/pair#key={one_time_key}`. The client reads the fragment locally and sends the key only in the exchange request body.
- One-time keys are cryptographically random, expire after 10 minutes, are consumed once, and are stored only as hashes.
- Client tokens are shown once, stored only as hashes in SQLite, independently scoped, and independently revocable.
- Authenticated client calls use `Authorization: Bearer <token>`. Long-lived keys or tokens never appear in URI paths, queries, logs, or event payloads.
- Pairing-key consumption and paired-client creation occur in one SQLite transaction.
- SQLite stores checkpoints, manual mappings, normalized review plans, mapping profiles, idempotency metadata, operation results, credential references, and authentication hashes.
- SQLite never stores raw source API payloads, page HTML, cookies, passwords, plaintext client tokens, or NeoDB access tokens.
- Source and NeoDB secrets live in an OS credential store or explicit container secret; SQLite stores only an opaque reference.
- All schema changes use ordered migrations. Do not create tables opportunistically from request handlers.

## 4. Validation and Error Matrix

| Condition | Result | Code |
|---|---|---|
| Unknown, expired, reused or wrong-client key | 401, no distinguishing secret oracle | `challenge_invalid` |
| Missing/invalid/expired/revoked session | 401 | `authentication_required` |
| Authorization header on session-admin route | 403, never fall back to cookie | `session_required` |
| Missing/foreign Origin on cookie mutation | 403 | `origin_not_allowed` |
| Missing/incorrect CSRF | 403 | `csrf_invalid` |
| Malformed JSON/unknown fields/wrong content type | 400, body not echoed | `request_invalid` |
| JSON body exceeds limit | 413 | `request_invalid` |
| Access attempt limit exceeded | 429 | `auth_rate_limited` |
| Invalid revoke ID | 400 | `client_id_invalid` |
| Client kind or requested scopes are unsupported | Reject before token creation | `pairing_scope_invalid` |
| SQLite transaction fails | Create neither client nor consumed challenge, sanitized 503 | `database_unavailable` |
| Bearer token hash is unknown or revoked | Reject GET /api/v1/client in Core | `authentication_required` |
| Raw payload persistence is attempted | Reject the write path | `raw_payload_storage_forbidden` |
| Migration version is newer than Core | Refuse startup without changing data | `database_version_unsupported` |

## 5. Good, Base, and Bad Cases

**Good case:** The desktop WebUI creates a pairing URI. The extension extracts its fragment, exchanges the key once, receives a scoped token, and sends observations with a Bearer header. SQLite contains only hashes and operational metadata.

**Base case:** Open the bare local URL without a session; health remains available but client metadata is denied.

**Bad case:** Every API request uses `?key=...`, or the Core stores source HTML/API responses for later convenience. Both violate the security and storage boundary.

## 6. Tests Required

- Implemented tests: `crates/collector-storage/src/access.rs` covers hash-only rows, expiry, replay, client/origin binding, concurrent exchange, rollback and revoked-token restart. `src/lib.rs` covers empty/v1 upgrade/reopen and byte-preserving newer-schema rejection.
- `crates/collector-server/src/access_tests.rs` covers real router lifecycle, Origin/CSRF, Bearer non-escalation, caps/redaction/rate limit and revoke. `packages/core-client-ts/src/access.test.ts` covers validators, mutation-only CSRF and sanitized failures.
- `csrf_rejects_missing_length_and_equal_length_mismatches` tests missing/empty,
  short/long, first/last-byte mismatch and another session's CSRF, then confirms
  the original valid token still succeeds. These are behavior tests, not timing proofs.
- `apps/webui/src/AccessPanel.test.tsx` proves pre-request fragment removal, StrictMode single exchange, incompatible-Core refusal and logout. `tests/smoke.mjs` exercises real CLI/Core/Chromium, HttpOnly cookie, clean address bar, pairing/revoke/reload/logout, empty browser storage and screenshots without keys. No remote-account/keychain evidence is implied.
- Smoke startup parsers wait for the terminating newline: stderr lines can arrive in multiple chunks. Never include buffered bootstrap output in failure diagnostics. Cancellation mocks should observe the incoming Request's signal rather than cloning it unnecessarily.
- Migration test from an empty database and every committed prior schema version.
- Transaction test proving a failed exchange creates neither a consumed challenge nor a paired client.
- Time-bound tests for valid, expired, replayed, and revoked credentials.
- Assertion that request logs redact `Authorization`, pairing bodies, and credential references.
- Schema/fixture audit proving no table or JSON column retains raw page/API payloads.
- Integration test that restart preserves checkpoints, mappings, review plans, and token revocation.
- Cross-platform data-directory tests for Windows, macOS, Linux, CLI override, and environment override.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: secret is repeated in request URLs and logs.
fetch(`/api/v1/imports?key=${clientToken}`);

// Correct: a one-time fragment bootstraps a revocable Bearer token.
fetch("/api/v1/imports", {
  headers: { Authorization: `Bearer ${clientToken}` },
});
```

```rust
// Wrong: retain source response for unspecified future use.
sqlx::query!("INSERT INTO source_payloads(raw_json) VALUES (?)", raw).execute(db).await?;

// Correct: persist only the checkpoint and normalized operation outcome.
checkpoint_store.commit(cursor, operation_result).await?;
```
