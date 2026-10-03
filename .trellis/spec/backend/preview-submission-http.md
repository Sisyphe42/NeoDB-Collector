# Preview CSV/resolution submission and explicit resume HTTP

## 1. Scope / Trigger

E2b2 owns authenticated IMDb CSV upload and explicit retained-job resume.
Domain `src/preview.rs` owns wire DTOs; Server `src/preview_submission_http.rs`
composes the existing pure CSV adapter and one runtime shared with CLI. Tests live
in `src/preview_submission_tests.rs`. The thin generated-contract TS client
exposes these operations; [CLI commands](./cli-preview.md) are delivered in E3a,
and [upload/resume screens](../frontend/preview-panel.md) in E3b. E3c verifies
routed CSV -> real runner/SQLite -> close/reopen -> explicit fresh resume ->
delete with fake upstream ports. Broader client/NeoDB-adapter end-to-end acceptance
remains pending. E4c adds structured search/manual inputs to the same submission
route and thin client; the [WebUI](../frontend/preview-panel.md) stages explicit
choices against its confirmed local File/job association and submits a new preview.
CLI choice inputs and broader integrated acceptance remain pending.
No source write, personal NeoDB write, migration or automatic resume is added.
Native catalog fetch during an accepted preview may create a public catalog item.

## 2. Signatures

- POST `/api/v1/imports/imdb/ratings?profileId=ID&profileVersion=N`:
  UTF-8 `text/csv` or `application/json` `PreviewImdbResolutionRequest` ->
  202 `PreviewImportAccepted`.
- POST `/api/v1/previews/{id}/resume`: `PreviewResumeRequest` ->
  202 `PreviewAccepted`.
- `PreviewImportSelection` selects a saved profile ID and positive u32 revision.
- `PreviewImdbResolutionRequest` is strict `{csv, resolutionInputs}`.
  `PreviewResolutionChoice` is tagged `search` with `ordinal`, `sourceRecordId`,
  `query`, or `manual` with `ordinal`, `sourceRecordId`, `item`.
  `PreviewCatalogReferenceRequest` is strict `{uuid, mediaType, model}`;
  media/model are open bounded strings, not global closed Core enums.
  Missing ordinals use Native; there is no explicit Native wire variant.
- `PreviewAccepted` carries `jobId`, positive `attempt`, fixed `executable=false`.
- `PreviewImportAccepted` carries `preview` plus `observationCount`,
  `skippedRowCount`, `skippedColumnCount` and `duplicateRowCount`.
- Post-reservation failures carry `PreviewAdmissionFailure`:
  `{jobId, attempt, error: ApiError}`. Pre-reservation errors use ordinary ApiError.
- `createAccessClient.submitImdbRatings(csv, selection, signal?)`,
  `submitImdbRatingsWithResolution(body, selection, signal?)` and
  `resumePreview(id, {expectedAttempt}, signal?)` validate actual responses.
- `AccessState.preview_submission: Option<Arc<dyn PreviewSubmissionRuntime>>`
  in Server type-erases synchronous `submit(PreviewSubmission)` and
  `resume(&PreviewJob)` -> `Result<PreviewAdmissionObserver, ApiError>`, plus
  `retention() -> PreviewRetention`. CLI assigns the existing runtime Arc.

## 3. Contracts

Authorize before consuming either upload body. Session mutations require exact
Origin and CSRF; paired clients require `imports.submit`. Authorization presence
selects Bearer exclusively. Reuse shared Host/Origin/security/no-store boundaries.
Content-Type is `text/csv`, optionally unquoted UTF-8 charset, or exact
`application/json`; unsupported/duplicate headers are rejected. Bounded body
collection handles chunked uploads as well as Content-Length: both raw CSV and
the encoded JSON envelope have a 10 MiB cap. Decoded CSV retains the parser's
10 MiB UTF-8 byte cap. Escaping and resolution inputs consume JSON envelope
space; a near-limit raw CSV need not fit in JSON. Do not enlarge the limit.
Resume JSON stays 4 KiB.
Classify typed `http_body_util::LengthLimitError` through standard error sources
as 413; an ordinary broken stream is sanitized 400, not an invented size failure.

Strict selection rejects duplicate/unknown fields and invalid IDs/revisions.
Load the current singleton endpoint and the exact immutable disabled profile;
there is no implicit default, latest revision, profile save or account rebind.
Reuse `collector_connectors::imdb::parse_ratings`, including UTF-8/BOM, strict CSV,
10,000 rows and canonical identity rules. Trusted observedAt is decimal ingestion
Unix seconds. Unsupported values/raw rows/header lists are never persisted or
returned; skip/duplicate counts keep the receipt bounded. Domain schemas close
envelopes and fix non-executability; Core and Storage remain validation authorities.

Structured choices bind to zero-based normalized observation ordinals AFTER
parser skips/deduplication and must match that observation's exact sourceRecordId.
Cap inputs at 10,000 and the actual observation count. Reject unknown fields/kinds,
invalid integers, duplicates (including cross-kind), out-of-range/source-mismatched
choices, invalid queries and catalog references before admission/catalog/secret I/O.
Reuse Core submission builders, query validation and `CatalogReference::from_stored`;
do not duplicate identity policy in Server or TS. Search is explicit and native
Unsupported-only, never automatic matching. Manual choice derives a safe detail
path and re-reads detail with manual provenance, never trusts supplied identity.
Each submission creates a new immutable job; no old history/input is edited.
Raw CSV and request JSON are never persisted. Generated JSON Schemas and OpenAPI
close envelopes and both tagged choice variants; the focused OpenAPI closure fix
does not become a generic schema postprocessor.

Use TokenSource-generated IDs and the same runtime's configured PreviewRetention.
Post-auth body collection, endpoint/profile/job reads and admission observation
share a strict ten-second control budget. Check before/after synchronous bounded
parsing. Synchronously reserve admission before repository mutation; one tracked
runtime task owns preparation and execution. There is no second owner, queue,
parser port, workflow engine or persist-then-enqueue gap. Disconnect/observer drop
does not cancel accepted work; explicit cancel/delete and runtime shutdown do.

Resume requires the expected current attempt and retained interrupted/failed/
cancelled state. The repository atomically rechecks full endpoint/profile/lifecycle,
increments the generation and removes old results only on successful resume.
Always recompute original observations freshly; expiry is fixed and profile revision
does not change. A 202 receipt proves durable acceptance, not running/completed.

SQL can commit before timeout/panic/cancellation is observed. After reservation,
return known job ID/generation with a sanitized nested error, never assert rollback
or delete a possibly committed job. Query existing status/results to reconcile;
temporary absence is not a retry instruction. Shared TS
`PreviewAdmissionClientError` retains validated IDs/attempt/code/field, not arbitrary
server diagnostic text; it inherits AccessClientError. No automatic retries.
Raw upload uses an explicit identity body serializer, not JSON string encoding.
Structured upload uses generated request validation and JSON serialization;
both share authentication, deadline, receipt and ambiguous-acceptance handling.
Caller abort combines with a 30-second default admission timeout (explicit timeoutMs
overrides; ordinary GET defaults remain five seconds), so the ten-second Server
budget can finish. Network loss/abort can still lose the whole receipt: no ID or
absence of an error envelope proves rollback. CSRF remains in memory only.

## 4. Validation & Error Matrix

| Condition | HTTP / outcome |
|---|---|
| Missing/invalid/revoked authentication | 401 authentication_required; upload body unconsumed |
| Session mutation lacks Origin/CSRF | 403 origin_not_allowed / csrf_invalid |
| Invalid query/envelope/attempt | 400, static field diagnostic |
| Unsupported/duplicate import Content-Type | 415, no submission |
| Raw CSV / encoded import JSON exceeds 10 MiB; resume JSON exceeds 4 KiB | 413, no submission |
| Invalid UTF-8/CSV/unsupported parser input | 400, static parser code; never echo data |
| Invalid structured envelope, choice/source binding, query or reference | 400, static diagnostic; no admission/catalog/secret I/O |
| Disconnected singleton endpoint | 409 neodb_not_connected |
| Selected profile absent | 404 mapping_profile_not_found |
| Missing/expired retained job | 404 preview_job_not_found |
| Expected attempt changed | 409 preview_attempt_changed |
| Ineligible resume/binding/profile conflict | 409; old history preserved on refused mutation |
| Admission full | 429 preview_runtime_busy; no repository mutation |
| Locally registered duplicate | 409 preview_runtime_duplicate |
| Runtime unavailable/closed or repository failure | 503, sanitized |
| Failure after reservation may follow commit | Admission failure envelope; inspect status, never auto-retry |
| Confirmed persistence | 202, no execution-status claim |

Malformed envelopes/kinds and source/ordinal/count binding failures use
`request_invalid`. Existing Core semantic errors remain
`catalog_search_query_invalid`, `preview_search_plan_invalid`,
`preview_manual_plan_invalid`, `preview_resolution_plan_invalid` and
`neodb_catalog_reference_invalid`; never echo the choice or CSV.

## 5. Good / Base / Bad Cases

Good: upload selects saved v1 even if v2 exists; resume recomputes the same v1 job.
Base: header-only CSV yields a valid zero-observation job without catalog requests.
Structured example (query still selects the saved profile):

```json
{
  "csv": "Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n",
  "resolutionInputs": [
    {"kind": "search", "ordinal": 0, "sourceRecordId": "tt1234567", "query": "Example"}
  ]
}
```

Manual inputs replace `query` with `item: {uuid, mediaType, model}` and
`kind: "manual"`; they are explicit choices, not external-ID equivalence claims.
Bad: return “rolled back” after a timed-out admission, or retry an unknown submission
with a new random ID, creating duplicate work.

## 6. Tests Required

Server router tests use real temporary SQLite/runner and fake catalog/secret ports.
Assert auth before body polling, exact/+1 limits including streaming, strict query/
Content-Type, UTF-8/BOM/CSV rejection and skip counts; inspect normalized storage
for absence of raw rows/unsupported values. Prove exact revision/full account binding,
truthful 202, queue-full before mutation, one owned execution, missing/wrong/expired/
ineligible resume and generation guards. After uncertain acceptance, reconcile the
returned ID without inferring rollback; dropping clients must not abandon work.
`packages/core-client-ts/src/preview_submission.test.ts` asserts raw body/query/CSRF,
generated validation/fixed false, sanitized typed failure metadata, caller abort,
timeout and one request only. Structured tests assert mixed Native/Search/Manual,
skip/dedup ordinal/source binding, strict rejection before admission/I/O,
encoded/streamed caps, fresh manual provenance and retained modes after resume.
Thin-client tests validate requests before fetch, JSON body/query, generated
response validation and the same abort/timeout/ambiguous acceptance contract.
Actual-process
smoke covers route advertisement/auth/revocation/disconnected import with no native
keychain or remote account. Full gate: `pnpm verify`, YAML/evidence, diff check.

Raw UTF-8/BOM transport tests compare arrayBuffer bytes with TextEncoder output,
not Request.text(), which removes the BOM during decoding. Compare number arrays
across the Node/jsdom realms rather than relying on typed-array prototype identity.

## 7. Wrong vs Correct

Wrong: `submit(input).await?; runtime.enqueue(job)?` can strand durable work.
Correct: reserve once, let the tracked runtime own persistence and execution,
and separately observe durable acceptance/completion.

Wrong: `JSON.stringify(csv)` silently changes the raw CSV bytes.
Correct: send `text/csv` with the generated path and explicit raw body serializer.
