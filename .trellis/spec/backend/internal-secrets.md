# Internal secret values

## 1. Scope / Trigger

Use this contract when changing credential-store ports, OAuth transport/lifecycle,
or Rust clients consuming stored tokens. `collector-core/src/secrets.rs` reexports
`secrecy::{SecretString, ExposeSecret}`; it does not implement a new secret wrapper.
Wire DTOs remain owned by domain. This migration does not change API v1 or SQLite.

## 2. Signatures

```rust
SecretStore::set(reference: String, secret: SecretString) -> PortFuture<'_, ()>
SecretStore::get(reference: String) -> PortFuture<'_, SecretString>
OAuthApp { client_id: String, client_secret: SecretString }
OAuthToken { access_token: SecretString, scope: String }
OAuthService::complete(raw_state: &str, code: SecretString)
NeoDbOAuthTransport::exchange(base: String, app: OAuthApp, code: SecretString, callback: String)
NeoDbOAuthTransport::identity<'a>(&'a self, base: String, token: &'a SecretString)
    -> PortFuture<'a, OAuthIdentity>
```

Existing result/error types and credential deletion signatures are unchanged.

## 3. Contracts

- Core may depend on reviewed runtime-independent `secrecy` 0.10; the exact
  allowlist in `tools/boundaries.mjs` remains enforced. Its `serde` feature permits
  deserialization, **not** implicit serialization of `SecretString`.
- OAuth app/token replies deserialize secret fields into `SecretString` before
  semantic validation, including rejected scopes and empty credentials. Callback
  code is wrapped at the Server-to-Core boundary. CLI moves a decoded pairing
  token into `SecretString` before validation/storage; the one-time wire delivery
  remains an ordinary serializable DTO and is never displayed in CLI reports.
- Use `expose_secret()` only where plaintext is consumed: validation, request
  encoding/authorization headers and the OS credential call. Do not unwrap into
  a `String` merely to pass between internal layers. Identity lookup borrows the
  token, then credential storage takes ownership; no clone is needed for lookup.
- OS adapter moves the secret into its serialized blocking operation and wraps
  keyring's returned string there. Existing error sanitization and no-plaintext-
  fallback policy remain in force. No real keychain access in tests.
- Debug formatting redacts the secret. No `Display` or blanket secret serialization
  implementation is allowed. Do not add custom serialization to bypass this rule.
- This protects owned secret allocations on drop, not all memory in the process:
  HTTP/query/JSON buffers, request copies, OS libraries and public DTOs may still
  contain plaintext transiently. It is not memory locking or whole-process wiping.
- State, bootstrap/session/pairing delivery, hashes and credential references keep
  their existing types and lifecycle rules. This slice is not an all-secret rewrite.

## 4. Validation & Error Matrix

| Condition | Required result |
|---|---|
| Direct `serde_json::to_string(&SecretString)` or Display | Compile failure |
| Debug a secret, OAuthApp or OAuthToken | No plaintext secret |
| Empty/malformed upstream secret or invalid scope | Existing sanitized error; never echo reply |
| Credential/database failure or interrupted OAuth | Existing rollback/reconciliation semantics, old connection preserved as specified |
| Public pairing response | Existing one-time token remains usable; no generated contract drift |

## 5. Good / Base / Bad Cases

Good: borrow a protected token for `/api/me`, then move it into `SecretStore::set`.
Base: a native credential read becomes a secret value before leaving its worker.
Bad: convert secrets back to ordinary strings at every port or serialize them into logs.

## 6. Tests Required

- `collector-core/src/secrets.rs`: Deserialize/Debug assertions plus compile-fail
  doctests for implicit Serialize and Display.
- `collector-core/src/oauth_tests.rs`: existing success, replay, expiry, concurrency,
  interrupted futures, scope rejection, rollback and cleanup failure paths.
- `collector-connectors/src/neodb.rs`: mock OAuth request fields, read-only scope,
  rejected responses and sanitized errors; no real account calls.
- Server OAuth and CLI client integration tests preserve callback/pairing/status,
  revocation, storage failure and output redaction using fake credentials.
- Full `pnpm verify`, including contract drift and exact dependency boundary checks.

## 7. Wrong vs Correct

```rust
// Wrong: erase the internal secret boundary before passing between layers.
let token = token.expose_secret().to_owned();
// Correct: expose only to the HTTP encoder; never to logging/diagnostics.
request.bearer_auth(token.expose_secret());
```
