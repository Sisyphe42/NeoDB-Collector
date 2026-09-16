# OAuth HTTP and WebUI

## 1. Scope / Trigger

Implemented in server/oauth.rs, domain/access.rs, CLI startup,
core-client-ts/access.ts and webui/NeoDbPanel.tsx. CLI serves one shared OAuthService
with restrictive NeoDbClient and OsSecretStore. No media writes or import routes.

## 2. Signatures

- GET /api/v1/settings/neodb -> NeoDbStatus
- POST /api/v1/oauth/neodb {baseUrl} -> NeoDbAuthorization
- DELETE /api/v1/oauth/neodb -> ActionResult
- GET /api/v1/oauth/neodb/callback?state=...&code=... -> 303

Rust DTOs generate OpenAPI, JSON schemas, TS types and runtime validators.
No caller-selected callback URI. Server derives it from the Core audience.

## 3. Contracts

Settings requires a WebUI session. Start/cancel additionally require same Origin
and session-bound CSRF. Bearer headers never fall back to cookie administration.
Start shares the existing auth rate limiter and 4096-byte JSON cap.

Callback does not require cookies/CSRF; the Core consumes a correct state once.
Provider error or missing code consumes matching state without token exchange.
Malformed, duplicate, oversized (>4096 bytes) or invalid query/state is never
echoed. All callback outcomes redirect to /#oauth=complete or /#oauth=failed;
no arbitrary redirect target, code or state reaches the WebUI.
Local host/origin boundary and no-store/no-referrer/CSP remain in force.

WebUI removes the outcome fragment before session lookup, remembers only the fixed
failure flag and retrieves actual status. Cleanup errors can coexist with a valid
connection. It supports instance entry, authorization navigation, refresh and
owner cancellation; no browser token storage. Returned authorization destination
must match the requested instance origin and /oauth/authorize.

## 4. Validation & Error Matrix

Missing session -> 401; Bearer/Origin/CSRF/foreign cancellation -> 403;
invalid instance/body -> 400; oversized body -> 413; active flow -> 409;
rate limit -> 429; credential/network/repository failure -> sanitized 503.
Callback always uses a sanitized local 303 after reaching the handler;
global host/origin rejection remains 403.

## 5. Good / Base / Bad Cases

Good: callback without SameSite cookie completes, then the clean local page reads
the connected account using its existing session.
Base: user denies NeoDB consent; correct state is consumed, previous account stays.
Bad: render upstream errors, log callback queries, or show connection success solely
because a return fragment says complete.

## 6. Tests Required

server/oauth_tests.rs uses SQLite, fake credentials and fake transport for
session/CSRF/Bearer rejection, busy flow, wrong state, cookie-free callback,
replay, denial, cancellation and redaction. Core tests own expiry and failure
injection. NeoDbPanel.test.tsx covers start navigation and cancellation/refresh.
Smoke exercises real local access but intercepts OAuth status with an explicit
disconnected fixture to avoid the native keychain; it is not real OAuth evidence.

## 7. Wrong vs Correct

Wrong: callback forwards provider error_description or redirects to a query return URL.
Correct: fixed local outcome fragment, then authenticated status as connection truth.
