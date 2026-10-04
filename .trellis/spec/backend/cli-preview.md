# CLI import and historical preview

## 1. Scope / Trigger

E3a delivers thin commands in `collector-cli/src/main.rs` and
`src/client/preview.rs`; tests live in `src/client/preview_tests.rs` and main's
command parsing tests. Reuse pairing's origin/health/credential helpers and Domain
wire types. E4e adds explicit file-based structured resolution inputs to the same
command. [WebUI screens](../frontend/preview-panel.md) are delivered separately;
broader real-client/simulated-NeoDB-adapter acceptance remains pending.
No source/personal-state writes, direct DB access, alternate parser/planner,
API change or dependency is added.

## 2. Signatures

```text
collector import imdb-ratings FILE --profile-id ID --profile-version N [--resolution-inputs PATH]
collector preview status ID
collector preview results ID --attempt N [--offset 0] [--limit 50]
collector preview cancel ID --expected-attempt N
collector preview resume ID --expected-attempt N
collector preview delete ID
```

Every leaf supports `--core-url http://127.0.0.1:19423` and
`--format human|json` (default human). Client commands do not load serve/doctor
YAML/environment configuration. FILE is a path or `-` for piped stdin.
Profile revision and generation are explicit positive u32; limit is 1..100
and offset is an original observation ordinal (u32), not a SQL row offset.

JSON success is the existing Domain `PreviewImportAccepted`, `PreviewJobView`,
`PreviewResultPage`, `PreviewMutationResult` or `PreviewAccepted`, not a second
CLI-shaped wire model. Failure is sanitized `ApiError` or validated
`PreviewAdmissionFailure` with known job ID/attempt. Success exits 0 (including
applied=false), failure exits 1; command-line syntax errors exit 2.

## 3. Contracts

- Validate literal loopback HTTP origin before network or credential access;
  no DNS localhost, proxy, redirects, remote host or secrets in parameters.
  Check health API/schema compatibility before loading the origin's OS-stored
  CLI Bearer token; no cookie, token flag/environment or plaintext fallback.
  Require composed URL path to equal the intended API path: `.`/`..` satisfy the
  shared ASCII ID grammar but URL normalization must not target a different route.
  Query profile IDs are unaffected. Fail closed before any preview request.
- Collect FILE/stdin using standard bounded Read, at most 10 MiB plus a sentinel.
  Refuse interactive stdin and sanitize I/O failures without paths/data.
  Send original bytes as text/csv; Server owns UTF-8/BOM/CSV validation,
  normalization and skip counts. No client parsing, raw-data persistence or
  implicit latest/default profile selection.
- With `--resolution-inputs PATH`, read a file-only UTF-8 JSON array of existing
  Domain `PreviewResolutionChoice` values; refuse `-` for choices. CSV may still
  use piped stdin. Strict serde closes nested shapes; no second schema/parser.
  Bound each input file and the actual serialized `PreviewImdbResolutionRequest`
  to 10 MiB and choices to 10,000. Preserve CSV BOM and exact UTF-8 characters.
  Local structured-input failures occur before health/credential/POST access,
  with static diagnostics only. Core owns normalized ordinal/source binding,
  duplicates, query semantics and supported media/model references; CLI does not
  parse CSV or duplicate identity policy. Send `application/json` to the same
  import route using the exact selected revision, creating a NEW job rather than
  editing history. CLI never persists raw CSV or choice-file JSON; Core may retain
  the validated normalized choices under the existing immutable job contract.
- Preview requests are bounded at 30 seconds, responses at 64 KiB except
  result pages at the existing 8 MiB cap. Require 200 for inspection/management
  and 202 for admission; no automatic polling/retry/redirect following.
  The shared CLI builder disables reqwest's default protocol-nack retry with
  `reqwest::retry::never()`, not a hand-written retry wrapper.
- Deserialize strict Domain serde shapes, then check positive attempts, caps,
  historical=true/executable=false, echoed ID/attempt/page parameters and ordered
  bounded rows. No response creates fresh-read authority or enables writes.
  Core/Server/Storage remain owners of identity and comparison semantics.
- Fetch exactly one requested page. Empty while running/queued does not mean done.
  Status reports lifecycle/account binding/retention/counts, not a fabricated
  processed count, percentage or fresh snapshot.
- Admission means durable acceptance, not completion. Cancel/delete report the
  actual applied boolean, not invented terminal state. Lost responses may follow
  committed work: inspect status and let the user decide, never retry or declare
  rollback. Preserve validated known jobId/attempt from admission failures;
  never invent an ID or forward arbitrary upstream diagnostic text.
  Only the contract's 400/409/413/503 admission error statuses can carry a receipt;
  unknown codes become core_unavailable while retaining the uncertain-outcome
  warning and validated ID/generation. Allowlist safe diagnostic field names.
  Preserve known structured query/search/manual/resolution/reference error codes
  with static messages; never collapse those semantic refusals into an invented
  network failure or forward their upstream diagnostic text.
- Human output states historical/non-executable evidence and that accepted
  previews may create public catalog entries without changing personal marks or
  source data. JSON-escape payload strings to prevent terminal control injection;
  preserve all field diffs, Known/Empty/Unknown, times and external-ID claims.
  Secrets never enter output.
  After serde_json serialization, escape non-ASCII control and bidi-formatting
  characters as JSON Unicode escapes; keep ordinary Chinese/Unicode and layout.
  Decoding the rendered JSON must recover the exact DTO values.

## 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| Missing revision/attempt, zero/overflow, limit outside 1..100 | Clap refusal before credential or HTTP operation |
| Unsafe origin | core_url_invalid before network/credential access |
| Interactive stdin, unreadable or over-cap source | Static input error; no path/data echo or submission |
| Choice PATH is `-` | Clap refusal; choices never consume stdin |
| Unreadable/invalid UTF-8/JSON/closed choice shape | preview_resolution_inputs_invalid before health/token access |
| Choice file over 10 MiB / array over 10,000 | preview_resolution_inputs_too_large / preview_resolution_inputs_too_many |
| Structured CSV is not UTF-8 / encoded JSON over 10 MiB | preview_input_encoding_invalid / preview_request_too_large before health/token access |
| Unsupported API/schema | core_incompatible before credential lookup/mutation |
| Invalid/revoked token / HTTP 401 | authentication_required; no fallback |
| Redirect, malformed/over-cap/wrong-status reply, flags/echo mismatch | Static core_unavailable; no retry |
| Recognized ordinary API failure | Static sanitized ApiError |
| Valid admission failure envelope | Validated ID/attempt, static nested error; exit 1 |
| 202 receipt | Non-executable acceptance, no completion claim |
| 200 applied=false | Actual refusal/absence; exit 0 |
| Timeout or lost response | Ambiguous; no rollback claim/retry |

## 5. Good / Base / Bad Cases

Good: import with saved v1, inspect status, fetch attempt 1 diffs at ordinal 50,
then explicitly cancel attempt 1 if needed. Base: a running job's empty page is
historical, not done. Bad: reimplement parsing/conflicts in CLI or silently retry
a timed-out upload or select a newer profile.

## 6. Tests Required

Fake credentials/loopback tests assert raw UTF-8/BOM bytes, exact profile query,
methods/Bearer/envelopes for all operations, one-page queries, strict receipt/
status/flags/ID/generation checks, sparse order, evidence and field distinctions,
sanitized errors/admission metadata, no retries, compatibility/revocation/redirect/
timeout and input/response limits. Parsing tests cover missing/zero/overflow
parameters. Human output must escape control characters; JSON retains Domain
shape/non-executability. Structured tests assert file-only choice paths, strict
closed Domain shapes, exact UTF-8/BOM preservation and selected revision,
mixed sparse choices, file/count/encoded bounds and no health/secret access on
local preparation failure. Core still owns source/ordinal/query/reference semantics.
`tests/smoke.mjs` checks the shipped command tree and
unsafe/invalid commands before native keychain access. Full `pnpm verify` and
`git diff --check`; no real-account/keychain tests.

`crates/collector-cli/src/client/preview_integration_tests.rs` composes the real
Rust CLI client with authenticated loopback Core HTTP, temporary SQLite, the
owned preview runtime and production NeoDbClient against a second synthetic
loopback HTTP server. Fake only clock/secrets; do not replace catalog/mark/identity
ports or preview API responses when claiming this integrated evidence. Assert
native fetch/detail identity, account-bound equal/conflict/missing-shelf comparison,
exact disabled profile revision despite a changed latest version, skipped/duplicate
counts, stable result pages and historical/non-executable metadata. Check exact
upstream request order, GET-only behavior, no Cookie/refetch, no catalog credential
and only the NeoDB credential on personal reads. Inspect normalized repository
data and closed fixture database bytes for discarded raw markers/plaintext tokens.
Drain runtime and close HTTP/storage owners under bounded waits. This is not
shipped-process/native-keychain, browser, restart/migration or live-account proof;
those acceptance paths require their own explicit evidence.

The same integration test owner also covers synthetic upstream failure paths.
For a batch with a committed first comparison, native fetch 422/429 retains
Unsupported/RateLimited as historical outcomes and can complete without a personal
read for that row. Fetch timeout/unsafe 302, detail 429 and personal-mark
401/302/non-missing 404/timeout must instead preserve Failed and the first result,
with no fabricated comparison for the failed row. Assert no retry/search/identity
fallback, exact GET/auth trace, zero foreign redirect-trap requests, unchanged
endpoint/profiles/credentials and no raw diagnostic/token persistence. Hang gates
exercise the production ten-second adapter timeout; do not override it or use
sleeps as completion evidence. Release held handlers and await their actual
responded-or-dropped lifecycle with a test-only RAII guard before asserting the
full historical page unchanged. A separate spawned release watcher is not handler
completion evidence; a cancelled handler is not a delivered late response.
Existing status DTOs expose lifecycle, not a new last-error code.
Shared fixture extraction must preserve the native happy-path
regression assertions and drain runtime/HTTP/storage owners under bounded waits.
Concrete tests in that owner are
`cli_native_terminal_fetch_outcomes_complete_with_historical_rows`,
`cli_native_redirect_detail_and_mark_failures_preserve_the_first_result` and
`cli_native_real_timeouts_fail_without_late_result_revival`.

The named child `preview_integration_tests/resolution.rs` owns polling and
explicit search/manual acceptance over the same concrete fixture. Exercise actual
production >=15-second pending spacing with response/request timestamps and
bounded event waits, not shortened timers or sleep-based settlement. Inspect
Running/unresolved evidence before resolving; cancel through the real CLI client
and drain the owner before asserting no later request/result. Local cancellation
cannot prove remote catalog rollback.

Structured acceptance must use the real CSV/choice file preparation and client
submission. Unsupported-only page-1 search leaves even exact-ID candidates
review-only without details or personal reads. A new explicit Manual job reads
fresh detail: compatible missing-ID evidence may compare with Manual provenance;
contradictory fresh identity remains NeedsReview without credentials. Assert the
original candidate job/page unchanged, normalized immutable input modes, selected
disabled profile/full binding, exact GET/auth/no-cookie trace and public/closed
DB-WAL redaction. Search opt-in must not override native match or rate limit.
This is synthetic-upstream Rust-client evidence, not shipped-process success,
browser/restart/migration or complete capability acceptance.
Concrete tests are `cli_native_pending_polling_uses_real_interval_before_comparison`,
`cli_cancel_during_pending_drains_without_follow_up_or_result` and
`cli_structured_search_and_fresh_manual_jobs_preserve_resolution_boundaries`.
The first-fetch notification establishes request arrival, not client receipt or
entry into an internal timer wait. A real polling test's terminal reconciliation
budget must exceed the 15-second interval plus subsequent I/O; wrapping a shorter
inner wait in a longer outer timeout cannot extend that inner budget.

## 7. Wrong vs Correct

Wrong: read collector.db, parse CSV, recalculate conflicts, silently retry.
Correct: bounded raw upload via versioned API, inspect actual Domain receipts
and historical pages, explicitly manage the selected generation.

Wrong: print a raw title/upstream error into the terminal.
Correct: static errors and JSON-escaped normalized evidence preserve content
without executing control sequences.
