# Operational State and Client Pairing

## 1. Scope / Trigger Conditions

This contract applies whenever Core persists state across restarts or authorizes a browser extension or local plugin. Core still transforms observations and writes supported fields directly to NeoDB. SQLite exists only for durable operational metadata; it must not become a source-media history warehouse.

## 2. Signatures and Data Shapes

The default database is `{data_dir}/collector.db`, where `data_dir` resolves in this order: CLI `--data-dir`, `NEODB_COLLECTOR_DATA_DIR`, YAML `storage.data_dir`, then the operating-system application-data directory. See [Runnable Foundation](./foundation-runtime.md) for path bases and implemented migration/startup behavior.

Future minimum schema ownership (not implemented by the migration-only foundation):

```sql
pairing_challenges(id, key_hash, client_kind, expires_at, consumed_at)
paired_clients(id, name, kind, token_hash, scopes_json, created_at, revoked_at)
sync_checkpoints(connector_instance_id, stream, cursor_json, updated_at)
item_mappings(connector_id, external_item_id, canonical_url, neodb_item_uuid, method, resolved_at)
mapping_profiles(id, source_connector_id, version, rules_yaml, enabled, updated_at)
planned_operations(id, canonical_item_id, field, operation, value_json, status, reason)
operation_results(id, planned_operation_id, idempotency_key, status, error_code, executed_at)
```

Pairing exchange:

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
  "scopes": ["observations.submit", "imports.read"]
}
```

## 3. Behavioral Contracts

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
| Pairing fragment is absent | Do not call exchange | `pairing_key_missing` |
| One-time key hash is unknown | Reject | `pairing_key_invalid` |
| One-time key is expired | Reject and delete/expire it | `pairing_key_expired` |
| One-time key was already consumed | Reject | `pairing_key_consumed` |
| Client kind or requested scopes are unsupported | Reject before token creation | `pairing_scope_invalid` |
| SQLite transaction fails | Create neither client nor token | `pairing_persistence_failed` |
| Bearer token hash is unknown or revoked | Return 401 | `client_token_invalid` |
| Raw payload persistence is attempted | Reject the write path | `raw_payload_storage_forbidden` |
| Migration version is newer than Core | Refuse startup without changing data | `database_version_unsupported` |

## 5. Good, Base, and Bad Cases

**Good case:** The desktop WebUI creates a pairing URI. The extension extracts its fragment, exchanges the key once, receives a scoped token, and sends observations with a Bearer header. SQLite contains only hashes and operational metadata.

**Base case:** A manual one-shot import writes directly to NeoDB and records only its idempotency result; the raw input is not retained.

**Bad case:** Every API request uses `?key=...`, or the Core stores source HTML/API responses for later convenience. Both violate the security and storage boundary.

## 6. Tests Required

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
