# Historical preview inspection API

## 1. Scope / Trigger

E1 under `09-16-preview-jobs` exposes authenticated inspection only. Owners are
Domain `src/preview.rs`, Server `src/preview_http.rs` and its tests, and the thin
shared TS client. CLI serve composes the existing Storage job/result ports.
Cancel/delete are owned by [Preview Management HTTP](./preview-management-http.md).
Submission/resume, client screens and runnable IMDb capability
remain separate checkpoints. No credential/network read or personal-state write
is performed by inspection, and no migration or new dependency is required.

## 2. Signatures

- GET `/api/v1/previews/{id}`: `PreviewJobView` historical job metadata.
- GET `/api/v1/previews/{id}/results?attempt=N&offset=0&limit=50`: historical
  ordered `PreviewResultPage`; positive attempt is required, offset defaults 0, limit 50.
- Domain owns wire DTOs and generated OpenAPI/JSON Schema/TS validators. Core
  private jobs/results are converted through getters, never serialized directly.
- Shared `createAccessClient.previewJob(id, signal?)` and
  `previewResults(id, {attempt, offset?, limit?}, signal?)` use the generated
  response validators and paths; caller signals combine with bounded timeout.

## 3. Contracts

- Session reads need no CSRF; paired Bearer reads require `imports.read` under
  current fixed scopes. Authorization presence selects Bearer exclusively, even
  when invalid/revoked/duplicate and a valid cookie is present. Host/Origin and
  no-store/security headers remain the existing shared boundary.
- IDs use the existing bounded ASCII job grammar. Strict query rejects unknown,
  duplicate, negative, fractional, zero attempt/limit and out-of-range u32 values.
  Limit is 1..100. Offset is the original observation ordinal lower bound, not
  SQL row offset. No arbitrary order, connector or account query is accepted.
- Job metadata carries ID/status/attempt/createdAt/updatedAt/expiresAt,
  observationCount, immutable profile ID/version, baseUrl/account and a binding
  state `current|changed|disconnected`. `current` checks the private full endpoint
  tuple at this instant only; it does not make an old snapshot fresh or executable.
- Every result page is explicitly historical. Preserve all normalized outcomes,
  comparison fields, snapshot/commit times, automatic/manual provenance and
  evidence. External IDs remain a list of scheme/value claims, not a dictionary
  that could collapse contradictions. Reuse Domain ScalarValue. Keep Known,
  Empty and Unknown sink values distinct; do not reinterpret accepted suggestions
  as write readiness or enable profiles. Credentials/references never appear.
  Job/page/row fields `historical=true` and `executable=false` are fixed generated
  schema constraints, not merely UI hints. Outcome and sink use `kind` tags with
  snake_case variants; other metadata keys are camelCase.
  Locked utoipa does not carry `deny_unknown_fields` into tagged enum variants.
  Server's `openapi()` applies a focused typed adjustment to only the two generated
  preview enum components, with a regression asserting every variant is closed.
  Do not duplicate enum field definitions or add a general generator postprocessor.
- No invented total, processed count, percentage or hasMore. A page contains
  jobId/attempt/offset/limit and ordered items; empty while queued/running is not
  completion or absence. Poll status and reread pages after terminal transitions.
- Read job before and after results, refusing deletion/expiry/attempt change.
  Revalidate each returned row's order, bounds, attempt and public job metadata;
  never return partial rows on corruption. Unrelated newer profile revisions do
  not replace the selected immutable revision. Stored results remain inspectable
  after account replacement, with changed/disconnected binding explicitly shown.
- Repository/endpoint reads share one strict ten-second ResolutionControl budget.
  Static sanitized errors replace port diagnostics. Response JSON has an 8 MiB
  encoded cap; reject overflow, never silently truncate. The normal 100-row page
  is bounded independently by the repository's 64 KiB per-result cap.
- Optional inspection ports absent => 503, not a fake empty job/page. Production
  CLI wires Storage. Capabilities advertise only the two delivered GET routes;
  connectorIds remains empty until the end-to-end submission path is delivered.
  Explicit cancel/delete have a separate mutation contract; no inspection enables work.

## 4. Validation & Error Matrix

| Condition | Required response |
|---|---|
| Missing/invalid/revoked/duplicate Bearer or session | 401 authentication_required; no cookie fallback |
| Bad Host/foreign Origin | Existing 403 boundary error |
| Invalid ID/query/attempt/page size | Static 400; no input echo |
| Job absent/deleted/expired | 404 preview_job_not_found |
| Requested attempt obsolete, including during page read | 409 preview_attempt_changed |
| Optional ports absent, corrupt/error/hung/late reads | Static 503 preview_unavailable |
| Encoded response over cap | Static 503; no partial page |
| POST/PUT on job, or mutations on results | Structured 405 method_not_allowed |

## 5. Good / Base / Bad Cases

Good: inspect interrupted attempt 1's partial historical field diffs; account
rotation displays changed binding without deleting evidence. Base: a queued job
returns an empty page, not completed. Bad: quietly returning attempt 2 rows to an
attempt 1 caller, collapsing contradictory IDs, or claiming an old rating is live.

## 6. Tests Required

`collector-server/src/preview_http_tests.rs` router tests use temporary SQLite and
fake ports/time; `src/preview_http.rs` projection/boundary tests cover all outcomes/statuses,
evidence and Known/Empty/Unknown diffs, sparse ordinal pages, immutable revision,
binding rotation/disconnection, expiry, attempt/deletion races, malformed queries,
auth fallback/revocation, static corruption/timeouts, encoded cap and redaction.
The persisted-result router case uses a real PreviewRecordService-produced record
and checks token rotation without losing historical evidence. Storage's existing
`preview_results::tests::sparse_pagination_is_ordered_and_never_returns_a_partial_corrupt_page`
mutates real SQLite JSON and proves all-or-error decoding; the HTTP failing-port
case independently proves that repository diagnostics become a static 503.
`packages/core-client-ts/src/preview.test.ts` uses generated response validators and paths, caller abort,
timeout, malformed wire and sanitized failures. Full `pnpm verify`, product YAML
validation and `git diff --check`; no live NeoDB/OS credentials or native fetch.

## 7. Wrong vs Correct

Wrong: serialize private PreviewJob, thereby leaking its credential generation,
or cast a JSON result page to a handwritten client type.

Correct: convert explicit safe getters to Domain DTOs, generate all wire schemas,
and validate actual responses in the shared client. Historical evidence never
recreates a service-produced record or enables a personal-state write.
