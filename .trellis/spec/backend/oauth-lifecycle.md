# NeoDB OAuth lifecycle

## 1. Scope / Trigger

Implemented in `crates/collector-core/src/oauth.rs`; transport lives in
`collector-connectors/src/neodb.rs`. This is a runtime-independent service,
composed by CLI startup and the [OAuth HTTP contract](./oauth-http.md). Compose exactly one shared service per Core
audience. Pending authorization is memory-only and never restored.

## 2. Signatures

- `start(&AccessRecord, requested_base: String, callback: String) -> OAuthStartResult`
- `complete(raw_state: &str, code: String) -> OAuthCompletion`
- `cancel(&AccessRecord) -> ()`
- `status() -> OAuthStatus`

All methods are async and return `Result<_, ApiError>`. Start/cancel callers
must pass a freshly authenticated WebUI session from AccessService; a record
is an internal trusted input, not an HTTP payload. Complete requires no cookie.

## 3. Contracts

- Start validates session kind, audience and expiry. Callback must exactly equal
  `{audience}/api/v1/oauth/neodb/callback`. The transport canonicalizes NeoDB's
  origin and constructs a read-only authorization URL.
- Reserve before registration; atomically match and consume state before exchange.
  States are idle, starting, pending and consumed. Consumed remains exclusive
  until token persistence and cleanup finish. No mutex is held across an await.
- State is random, expires in 600 seconds, and only its digest is retained alongside
  initiating session ID, canonical instance and callback. Wrong state leaves
  a live pending flow untouched.
- Temporary app secret reference is `neodb-oauth-app:{digest(audience)}`.
  Start and idle status remove crash leftovers. Long-lived references are
  `neodb-token:{random}`; SQLite receives only the opaque reference and account.
- Exchange accepts only nonempty read scope (whitespace/duplicate read allowed),
  removes the app secret before identity lookup, then stores the token before
  atomic singleton endpoint replacement. Repository errors must leave the old
  endpoint intact. Old-token deletion happens only after replacement succeeds.
- Cleanup references are queued in memory before credential writes. Reconciliation
  checks the current endpoint before deleting, including after interrupted futures.
  A drop guard releases in-flight reservations; status/start retry cleanup.
  Only the fixed temporary reference is recoverable across process crashes:
  this does not promise cross-store transactions or durable orphan-token tracking.
- Status returns connected/base_url/account/pending without secrets or references.
  Failed old-token cleanup does not hide the new connection. Cancellation is
  owner-only and idle-idempotent; in-flight starting/completing operations return
  busy and must finish before cancellation can be retried.
- Runtime integration must drain credential/database operations during shutdown;
  dropping a future does not necessarily stop a native adapter's blocking work.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Non-session, wrong audience, expired session | `session_required` |
| Foreign callback | `oauth_callback_invalid` |
| Another flow or cleanup operation owns reservation | `oauth_in_progress` |
| Wrong state / expired state / replay | `oauth_state_invalid` / `oauth_state_expired` / `oauth_not_pending` |
| Another session cancels | `oauth_cancel_forbidden` |
| Unsupported token scope | `oauth_scope_invalid` |
| Credential deletion fails | `oauth_credential_cleanup_failed`; retained reference for retry |
| Endpoint replacement fails | Preserve old endpoint, remove new token; sanitized port error |
| Old-token cleanup fails after commit | Explicit cleanup error; status reports new connection |

## 5. Good / Base / Bad Cases

Good: replace an account, then delete the previous credential.
Base: restarting discards pending state; status removes its fixed app credential.
Bad: releasing the reservation immediately after consuming state allows a new
start to delete the app secret still needed by exchange.

## 6. Tests Required

`crates/collector-core/src/oauth_tests.rs` uses fake time/randomness, memory ports
and blocking mock transport. Assert success/replacement/read scope, wrong state,
expiry/replay/owner cancellation, concurrent starting and consumed operations,
interrupted futures, crash leftovers, failures at transport/credential/database
stages and truthful status after cleanup failure. No network or native keychain.
Run Core tests and the workspace `pnpm verify` gate.

## 7. Wrong vs Correct

Wrong: read pending under a lock, unlock, then reset idle before exchange.

Correct: validate and transition pending to consumed in one critical section;
retain an operation reservation through persistence and cleanup. Disarm the
start reservation while publishing pending under the same lock.
