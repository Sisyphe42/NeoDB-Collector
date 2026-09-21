# CLI pairing and authenticated status

## 1. Scope / Trigger

Implemented in collector-cli/src/client.rs. It is a thin versioned HTTP client,
using domain wire DTOs and the OS credential adapter. No Core database access,
browser sessions or administration rights are involved.

## 2. Signatures

- `collector pair [--name "Collector CLI"] [--format human|json]`: reads a URI from stdin.
- `collector status [--core-url http://127.0.0.1:19423] [--format human|json]`.
- GET /api/v1/client + Authorization: Bearer -> PairedClient.

Pair JSON: {paired, coreUrl, clientId}. Status JSON: {authenticated, coreUrl,
client, health}. Errors are ApiError; success exits 0, failure exits 1.
Status confirms local client authentication, not NeoDB authorization.

## 3. Contracts

Pair accepts at most 4096 input bytes and exactly /pair#key=<43-char key>.
Only literal loopback HTTP origins are accepted, including IPv6; no userinfo,
query, remote host, DNS localhost or extra fragment fields. The pairing name
must match the WebUI request; kind is fixed to cli. Terminal input is rejected:
pipe the copied URI to avoid secret arguments/history. Do not log input.

Before exchange, fetch health and check API/schema versions; probe credential-store
write/delete support before consuming the link. Store the token under
collector-cli:<SHA256(canonical origin URL including trailing slash)>.
Each origin has one locally selected CLI token; re-pairing replaces that selection.
Previous server credentials remain independently revocable in WebUI.
No plaintext file fallback, token stdout, or token environment variable.

Status loads that token and verifies it through GET /api/v1/client, returning
only the authenticated client's public identity and health. Cookie-only requests
cannot use this endpoint. Unknown/revoked tokens are 401, with no cookie fallback.
This endpoint grants no additional privileges or access to other clients.

The stored token follows [Internal Secret Values](./internal-secrets.md): wrap
the decoded pairing token before validation/storage and expose a retrieved token
only for validation and Bearer encoding. Public reports and API DTOs are unchanged.

HTTP uses no proxies, no redirects, 10-second request timeout and 64-KiB decoded
response cap. Upstream bodies and transport errors are not echoed.
Pair/status use their explicit endpoint inputs; serve/doctor config is not loaded.

## 4. Validation & Error Matrix

- Bad origin/link -> core_url_invalid or authentication_required before network.
- Oversized/unreadable stdin -> pairing_input_invalid.
- Interactive stdin -> pairing_input_required.
- Incompatible API/schema -> core_incompatible before sending the pairing key.
- Invalid/revoked/expired pairing or token -> authentication_required.
- Redirect, oversized/invalid response, other HTTP/network errors -> core_unavailable.
- Credential-store preflight failure -> sanitized adapter error; link not consumed.
- Token save fails after exchange -> pairing_storage_failed; revoke new client in
  WebUI and create another pairing. No distributed rollback or auto-retry is claimed.

## 5. Good / Base / Bad Cases

Good: copy a WebUI CLI pairing link, pipe it to pair, run status --format json,
then revoke the client in WebUI and observe status fail.
Base: another Core origin needs a separate pairing; no credential is sent to it.
Bad: use --token or place the long-lived token in a query/config/log.

## 6. Tests Required

client/tests.rs uses real loopback HTTP and SQLite with fake credentials:
store preflight failure preserves link, name binding, pairing/replay, status,
revocation, post-exchange storage failure, output redaction, unsafe URLs,
oversized input/response, redirects and incompatible versions.
Server tests deny cookie/invalid Bearer; smoke verifies identity and revocation.
Tests never use the native keychain or real remote accounts.

## 7. Wrong vs Correct

Wrong: read collector.db to decide that a CLI token is still valid.
Correct: GET /api/v1/client with the OS-stored Bearer token.
