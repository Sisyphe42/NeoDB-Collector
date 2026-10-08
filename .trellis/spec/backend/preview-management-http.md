# Preview cancellation and deletion HTTP

## 1. Scope / Trigger

E2a under `09-16-preview-jobs` exposes explicit local cancellation/deletion through
the existing single CLI-owned runtime. Owners are Domain `src/preview.rs`, Server
`src/preview_http.rs`, `src/preview_runtime.rs` and `src/preview_management_tests.rs`.
[Submission/resume](./preview-submission-http.md) and [preview screens](../frontend/preview-panel.md)
are delivered separately and synthetically accepted; connector advertising remains
separately gated (see [foundation evidence](./foundation-runtime.md#accepted-synthetic-preview-scope-2026-10-09)). This changes operational
state only, never source or NeoDB personal state.

## 2. Signatures

- POST `/api/v1/previews/{id}/cancel`: strict `PreviewCancelRequest`
  `{ expectedAttempt: u32 }`, required positive generation.
- DELETE `/api/v1/previews/{id}`: no request body.
- Both return strict `PreviewMutationResult { applied: boolean }`, HTTP 200.
- Domain owns generated OpenAPI/JSON Schema/TS request/result types and validators.
- `AccessState.preview_management: Option<Arc<dyn PreviewManagement>>` is Server's
  narrow runtime view, with `cancel(id, expected_attempt)` / `delete(id)` returning
  the existing `PortFuture<bool>`. CLI uses its owned runtime Arc; None is unavailable.
- Shared client `cancelPreview(id, expectedAttempt, signal?)` and
  `deletePreview(id, signal?)` use generated paths, mutation CSRF and result checks.

## 3. Contracts

- WebUI session mutations require exact same-origin Origin and session CSRF;
  paired Bearer requires current fixed `imports.submit`. Presence of Authorization
  selects Bearer only, even invalid/duplicate/revoked alongside a valid cookie.
  Shared Host/Origin, no-store/security headers and no CORS remain unchanged.
- Reuse bounded ASCII job IDs and the 4 KiB JSON cap. Cancel envelope rejects
  unknown/duplicate fields, missing/zero/negative/fractional/overflow attempts.
  Clients never supply endpoint, profile, connector or credentials to management.
  Apply the cap on the preview router itself: a layer on an earlier router does
  not cover routes merged afterwards. OpenAPI operation IDs must be globally
  unique, including the separate existing OAuth cancellation operation.
- `applied` is the repository's truthful boolean, not an invented job status.
  Cancelled-attempt repeats may return true idempotently; stale/missing/terminal
  refusal returns false. Repeated missing deletion returns false. False is not
  an exception, and neither result proves a cancelled/deleted remote catalog entry.
- A narrow object-safe Server management view delegates to the exact same runtime
  Arc retained by CLI shutdown. No repository bypass, second owner, scheduler,
  Core port or detached mutation task. Cancel signals only the matching local
  generation before repository cancellation; delete signals any local generation
  before atomic repository deletion. Late stores cannot recreate deleted jobs.
- Runtime and handler reuse existing strict ten-second management control.
  Timeout/error returns static diagnostics. Dropping/aborting a request cannot undo
  SQL already committed: callers reconcile with historical GET and never retry
  mutations automatically. Local interruption alone does not prove persistence.
- Preserve partial history on cancel and unrelated profiles/endpoint/auth/OS
  credentials on delete. Management does not enqueue/resume or access network or
  credentials. Existing startup/maintenance/shutdown/drain/serve-lease ownership
  remains unchanged. Historical evidence stays non-executable.
- Capability routes `previewCancel` and `previewDelete` describe only these
  delivered methods. `connectorIds=[]` and personal-state writes remain disabled.

## 4. Validation & Error Matrix

| Condition | Required response |
|---|---|
| Missing/invalid/revoked/duplicate auth | 401 authentication_required; no cookie fallback |
| Missing/foreign Origin or invalid session CSRF | Existing 403 boundary error |
| Invalid ID/envelope/attempt | Static 400, no input echo |
| Cancel JSON exceeds 4 KiB | 413 request_invalid |
| Management port absent | 503 preview_unavailable |
| Cancel error/hung/exact-deadline reply | Static 503 preview_cancel_failed |
| Delete error/hung/exact-deadline reply | Static 503 preview_delete_failed |
| Repository transition refused or delete absent | 200 applied=false, status remains authoritative |
| Repository successful/idempotent transition | 200 applied=true, not write-readiness |
| Unimplemented HTTP methods | Structured 405 method_not_allowed |

## 5. Good / Base / Bad Cases

Good: cancelling attempt 1 leaves its partial review evidence and cannot stop a
new attempt 2. Base: deleting an absent job returns applied=false. Bad: fabricate
cancelled status when signalling succeeded but the repository failed, or report
that cancelling local work undoes NeoDB catalog creation.

## 6. Tests Required

Server management router tests use real temporary SQLite/runtime/runner and fake
catalog/credential ports: active/queued cancel, wrong-attempt isolation, retained
history, delete/late publication, preservation of configuration and auth; session
Origin/CSRF and Bearer fallback/revocation/duplicates, body/ID/attempt limits,
optional ports, static error redaction and strict hung/late ten-second budgets.
TS `preview-management.test.ts` checks generated request/result wiring, no retries,
mutation-only CSRF, malformed responses, static errors and abort/timeout behavior.
`tests/smoke.mjs` proves actual host composition, capabilities and auth denials
without actual accounts/keychains. Full `pnpm verify`, product YAML/evidence and
`git diff --check` before checkpoint commit; local Windows checks are not hosted
three-OS evidence.

## 7. Wrong vs Correct

Wrong: HTTP calls `jobs.cancel()` directly or creates another runtime; the active
owned worker remains uncancelled. Correct: dispatch through the CLI-owned runtime
management view and return the actual repository result.

Wrong: automatically retry a timed-out mutation. Correct: inspect job/status to
reconcile a possibly committed operation and let the user explicitly act again.
