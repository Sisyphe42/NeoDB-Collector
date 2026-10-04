# Preview acceptance evidence

Status: in progress. This is task-level evidence, not a new product contract.
Checkboxes in `prd.md` require delivered behavior, not merely internal primitives.
All fixtures are synthetic. No live NeoDB/native-fetch, real account or native
credential-store verification is implied. Local Windows checks are not three-OS CI.

## Evidence boundaries

- Catalog transport loopback tests prove restrictive request/response handling;
  Core resolver tests prove polling, identity/granularity and explicit selection.
  E4a/E4b integrate explicit inputs into the internal runner; E4c exposes structured
  submission, E4d adds explicit WebUI choices and E4e adds file-based CLI choices.
- Core comparison/record tests prove account binding, non-executable comparisons
  and Known/Empty/Unknown semantics. Storage tests prove normalized persistence,
  atomic guards, retention, reopen and migration independently of clients.
- Server router/runtime tests use real SQLite/runner with fake upstream ports.
  E3c's routed import/restart/fresh resume/delete chain passed its scoped test;
  final workspace verification is recorded below.
- CLI tests use loopback fixtures/fake credentials. `tests/preview-ui-smoke.mjs`
  validates preview responses and browser actions against schema-validated mocks;
  `tests/smoke.mjs` runs real host/auth/client boundaries. Neither alone proves
  the full CLI/WebUI-to-real-NeoDB-adapter preview path across restart.

## Criterion audit (2026-10-03, before E3c verification)

| PRD area | Concrete existing evidence | Still missing |
|---|---|---|
| Identity, polling, search/manual | `collector-connectors/src/neodb/catalog/tests.rs`; Core `catalog_resolver/tests.rs::pending_spacing_includes_request_duration_and_exact_identity_matches`, `search_is_only_used_once_after_unsupported_and_stays_review_only`, `manual_exact_or_missing_identity_is_distinct_and_preserves_full_evidence` | Before E4a the runner was native-only; E4a internal opt-in is recorded below, but public fallback/selection is not delivered |
| Personal marks and comparison | Core `preview/tests.rs::present_rating_comparison_distinguishes_equal_empty_and_conflict`, `missing_shelf_is_empty_only_for_shelf_and_rating_stays_unknown`, `repository_secret_and_mark_failures_are_sanitized_and_stop` | Compose real NeoDB adapter reads with public import, not just fake ports |
| Runtime lifecycle | Server `preview_runtime/tests.rs::submit_and_resume_accept_real_work_with_fixed_metadata`, `shutdown_signals_active_and_queued_without_persisted_cancel`, `production_start_interrupts_unfinished_and_cleans_expired_without_resuming` | Routed CSV -> close/reopen -> fresh explicit resume -> delete is E3c; real client/upstream composition remains separate |
| Normalized persistence/migration | Storage `preview_results/tests.rs::real_producer_results_round_trip_complete_and_reopen`, `populated_exact_v4_schema_upgrades_without_losing_operational_state`; `preview_jobs/tests.rs` | Integrated public migration path; component migration tests alone are not end-to-end |
| Retention and deletion | Server `preview_runtime/tests.rs::maintenance_waits_for_first_tick_then_signals_expired_active_work`, `skipped_interval_does_not_catch_up_after_a_large_time_jump`; Storage `preview_results/tests.rs::terminal_delete_and_expiry_transitions_never_accept_late_results` | Final integration acceptance and preservation checks, not new retention policy |
| HTTP/CLI/WebUI | Server `preview_submission_tests.rs`; CLI `client/preview_tests.rs`; WebUI `PreviewPanel.test.tsx`; `tests/preview-ui-smoke.mjs` | Parent integrated CLI/WebUI-to-simulated-upstream acceptance; the browser preview routes currently use mocks |

Paths prefixed with crate names above are relative to `crates/`; client paths are
under `apps/webui/src/` or `tests/` as named. Upstream timeout/429/auth/redirect
coverage exists at component boundaries. Do not label it a public import-to-real
NeoDB-adapter end-to-end suite. Audit was read-only and did not run tests.

### Next acceptance checkpoints

- E4e completes CLI resolution-choice inputs after E4c/E4d's structured API/shared
  client and interactive WebUI; never turn title hints or a singleton candidate
  into automatic identity proof. Verified checkpoint evidence is recorded below.
- Audit upstream timeout/429/auth/redirect and migration through the integrated
  path, distinguishing prior component evidence from end-to-end coverage.
- Finish parent CLI/WebUI integration review against a simulated upstream server.
- Enable connector capability advertising only when corresponding delivered
  behavior is verified; keep personal writes and scheduling disabled.

Keep this child active until its full criteria pass. Do not archive it based on
the backend restart scenario alone.

## E3c scoped evidence

`crates/collector-server/src/preview_restart_tests.rs::authenticated_import_restart_resume_and_delete_preserve_unrelated_state`
passed after main review strengthened full tuple/token guards, detail-call
counters, shutdown-vs-cancel distinction, preserved HTTP evidence, exact v1
semantics despite changed v2, source URL order, attempt conflicts, both fresh
comparisons, provenance/identity, no credential exposure and post-delete auth.
The clock moves 110 -> 120 -> 130; sink rating moves 8 -> 5. New comparisons
retain source 8/7, original expiry and v1 rather than v2's clamp-to-5 rule.
The fixture drops old owners and closes SQLite before reopening the same directory.
No production/API/dependency/migration/capability change is required.

Main's final `pnpm verify` passed after all edits: 274 Rust unit tests, two
compile-fail doctests, 63 TS/UI tests, fmt/Clippy, generated-contract drift,
dependency boundaries, lint/typecheck, WebUI/CLI builds and actual local host/
authentication/browser smoke. YAML/34 evidence IDs and decision/reference checks
and `git diff --check` passed separately. The first gate exposed an existing
WebUI mock sequencing race; its state-driven fixture repair passed all 15 panel
tests plus five repeated target runs before the final gate. No timeout increases
or production UI changes. These results do not close the remaining criteria above.

## E4a internal search-input evidence

Core `preview_jobs/tests.rs::explicit_search_plan_is_sparse_normalized_bounded_and_unique`
proves sparse unique ordinals and exact ASCII/UTF-8 512-byte validation. Runner
`mixed_native_and_explicit_search_inputs_run_in_ordinal_order` preserves Native
defaults, retains even an exact-ID candidate as review-only, reads no credentials/
marks, and re-forwards the explicit query on a fresh attempt without historical
result input. Native match never searches; pending query reads are cancelled or
timed out, and rebind/deletion after the read stops before catalog operations.

Storage query/reopen/resume tests retain original revision, full binding and
expiry despite v2. Atomic insertion failure removes the whole query-bearing job.
`result_write_requires_the_exact_stored_search_query_proof` refuses Native/Manual/
different-query records for an opted-in slot and accepts the matching normalized
request; query proof is private and absent from historical result JSON. Shared
query decoding rejects corrupt input in query/observation/history/store/completion
paths without advancing the job or overwriting evidence. Populated exact-v5 rows
upgrade to v6 with NULL/native queries and historical evidence retained; prior
schema fixtures and byte-preserving newer-schema refusal also pass.

Main's final `pnpm verify` passed: 282 Rust unit tests, two compile-fail doctests,
63 TS/UI tests, fmt/Clippy, contracts/boundaries, lint/typecheck, builds and actual
local CLI/HTTP/auth/browser smoke. YAML/evidence/decision/reference checks and
`git diff --check` passed separately. This checkpoint adds a usable internal job
input and compatible migration, not a public route or manual-selection mapping.
Public search/manual actions, integrated negative paths and parent client/upstream
acceptance still remain pending. No live upstream/account/keychain verification,
new dependency, source/personal write or connector capability was introduced.

## E4b internal manual-input evidence

Core `catalog.rs::tests::compact_reference_validates_item_and_derives_only_safe_relative_paths`
validates all four supported route/model pairs, UUID bounds and unsafe/mismatched
evidence without a URL parser or transport call. The sparse plan test rejects
duplicates/count/range and search/manual collisions in either builder order.
`invalid_inputs_fail_before_binding_or_catalog_io` includes oversized raw manual
requests. Runner `mixed_manual_search_and_native_inputs_use_fresh_manual_detail_on_every_attempt`
checks ordered mixed modes, fresh detail/mark reads and manual provenance on each
attempt, with no historical-result reads. The unified input-read guard test drops
hung reads on cancellation/deadline and stops before detail/credentials after rebind.

Storage `manual_inputs_survive_reopen_resume_and_corruption_never_becomes_native`
retains compact choices across reopen/resume; payloads exclude URLs and metadata.
Shared strict decoding rejects malformed/version/unknown-field/unsafe UUID inputs
and search/manual collisions. `manual_result_proof_uses_requested_reference_not_redirected_item_evidence`
accepts the requested selection while retaining redirected final evidence and
Manual provenance, rejects another request/Native/Search mode, and checks corrupt
input rejection in history/store/completion without status/time/count changes.
The insertion-failure test includes mixed Manual/Search rollback. Populated exact-v6
migration preserves Native/Search inputs and historical Manual evidence; new NULL
slots accept only Native production and cannot recreate old manual authority.

Scoped Core tests (89 + two compile-fail doctests), Storage (43), Server (87),
fmt and Core/Storage Clippy passed. Main full `pnpm verify` passed: 288 Rust unit
tests, two compile-fail doctests and 63 TS/UI tests, workspace fmt/Clippy,
contracts/boundaries, lint/typecheck, WebUI/CLI builds and actual local
CLI/HTTP/session/pairing/revocation/browser smoke. Product YAML with 34 unique
evidence IDs, decision/reference/capability checks and `git diff --check` passed.
No public search/selection route, UI/CLI action, dependency, capability or personal/
source write is delivered. Broader public and client/upstream acceptance remains
pending; this child stays active. All upstream/account/credential tests are fake.

## E4c structured public submission evidence

The existing authenticated CSV import now also accepts strict JSON containing CSV
and explicit per-record Search/Manual choices. Generated Domain/OpenAPI/JSON Schema
and TS request validators close both variants and nested references. Raw `text/csv`
and existing receipts/auth/deadlines/ambiguous acceptance remain compatible.
`submitImdbRatingsWithResolution` is a thin generated-contract client, not a new
identity policy or workflow owner. Structured resubmission creates a new job;
there is no retained-input/history mutation or automatic candidate matching.

Server `preview_submission_tests.rs::structured_submission_persists_source_bound_native_search_and_manual_inputs`
drives the real runtime/producer/SQLite: missing choices stay Native, explicit
search yields review-only candidates, and manual choice reads fresh detail and
personal state with Manual provenance. Exact counters show only manual comparison
reads a token/mark. `structured_submission_binds_ordinals_after_skip_and_dedup_and_rejects_bad_plans`
binds choices to normalized ordinals and exact source IDs, rejects count > actual
observations and >10,000, negative/fractional/overflow ordinals, duplicate/cross-kind
plans, mismatched IDs/models, unsafe references, blank/control/513-byte UTF-8 queries,
unknown fields/kinds and duplicate JSON fields. The submit spy and external-read
counters stay unchanged through rejection.

`routed_resume_retains_structured_manual_and_search_modes` commits one manual
result, holds later catalog work, cancels via HTTP and explicitly resumes attempt 2.
Exact counters prove a second detail/token/mark read, original choices survive,
attempt-1 evidence is cleared and expiry/profile/full endpoint binding stay fixed.
Shared runtime fakes require explicit opt-in for detail/search/identity/secret/mark
access, preserving prior native-only tests' no-I/O guards. Chunked JSON at exactly
10 MiB is valid and admitted; +1 is rejected. Authentication precedes body polling.
TS tests cover generated pre-fetch rejection, JSON/query/CSRF, invalid receipts,
sanitized uncertain metadata, timeout/caller abort and no retries, retaining native
Request objects in pending mocks.

Scoped Server submission tests (16), fmt/Clippy, generation/drift, lint/typecheck
and focused TS tests (14) passed. Main's final full `pnpm verify`, after review
strengthening, passed: 291 Rust unit tests, two compile-fail doctests, 65 TS/UI
tests, workspace fmt/Clippy, contracts/boundaries, lint/typecheck, builds and actual
local CLI/HTTP/session/pairing/revocation plus desktop/mobile browser smoke.
Product YAML/34 evidence IDs/decision/reference/capability checks and diff check
also passed. Windows-local evidence is not three-platform CI or live NeoDB proof.

No new dependency, migration, Core/Storage production logic, standalone search
route, UI/CLI interactive choice, capability advertisement or personal/source write.
Interactive selection and broader real-client/simulated-upstream negative-path
acceptance remain open; keep this task active. All upstream/account/keychain ports
are fake, with no live native catalog fetch.

## E4d interactive WebUI evidence

`apps/webui/src/PreviewPanel.tsx` stages source-bound Search/Manual inputs using
the existing generated validators/client. Only this mounted panel's confirmed
File/profile/job receipt and completed/current matching metadata establish local
choice authority. Explicit resubmission creates a new job; prior inputs/results
are never edited. Reopened jobs are read-only, and File/profile/attempt/binding/
row-frame changes revoke drafts. No CSV parser, arbitrary UUID/URL input, saved
cross-job mapping or personal/source writer is introduced.

Focused panel tests (35, including 20 new cases) prove Search -> new candidates ->
Manual -> new preview; unsafe legacy reference rejection; Unicode C1/query UTF-8
validation; sparse replace/reset/off-page submission and Compared/off-page clear;
ineligible frames; just-in-time attempt/binding checks; exact encoded JSON 10 MiB
and +1 with escaping; no preflight/POST after local refusal or stale File reads;
stale fulfilled/rejected preflight/submission guards; single-action double-click
and actual unmount abort; known/unknown/auth admission failures without retries
or confirmed File association. Raw known-ID ambiguity clears the previous job/page.
Inspection suppression is scoped to the original job/selection generation and
cannot swallow a newly opened job or explicit refresh.

`tests/preview-ui-smoke.mjs` extends the existing Chromium/local-host harness with
generated request/response schema checks, exact CSV/BOM and v1 despite saved v2,
ordinal/source ID and compact manual reference, three distinct IDs, unchanged old
jobs, no staging POST, keyboard actions and read-only historical reopen. Main
visually inspected desktop/mobile draft and candidate screenshots; overflow and
empty local/session storage assertions passed. Preview routes are mocks; real
host/session/pairing/mapping boundaries still run in `tests/smoke.mjs`. This is
browser/shared-client evidence, not the final real-NeoDB-adapter integration suite.

Main final `pnpm verify` passed: 291 Rust unit tests, two compile-fail doctests,
85 TS/UI tests, fmt/workspace Clippy, generated drift, dependency boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/
34 evidence IDs/decision/reference/disabled-capability and `git diff --check`
passed separately. No live upstream/account/keychain, dependency, migration,
schema/API/CLI/Core/Server/Storage production change, standalone search route or
capability advertisement. Local Windows evidence is not hosted three-platform CI.
CLI resolution-choice inputs and broader simulated-upstream negative/end-to-end
acceptance remain pending; keep this child/parent active and personal writes disabled.

## E4e CLI resolution-choice evidence (2026-10-04)

`collector import imdb-ratings ... --resolution-inputs PATH` accepts a file-only
UTF-8 array of existing Domain choices and submits the existing structured import
API as a new job. Missing flag preserves raw transport. The CLI owns no CSV
parser/identity policy and persists neither raw CSV nor choice-file JSON. Core
still owns normalized ordinal/source matching and validated immutable choices.

Six additional CLI tests (38 total) cover strict nested/duplicate fields and
integer shapes, choice-file 10 MiB and +1, 10,000 and +1 inputs, actual encoded
JSON exact/+1 with escaping, BOM/UTF-8, mixed Search/Manual with Native omission,
actual temp-file preparation and exact health/token/Bearer/body/revision, and
no health/secret/POST after missing/invalid/count/encoded input refusal. Structured
requests share existing admission validation and preserve known IDs without retry.
Known query/search/manual/resolution/reference refusals retain their codes with
static messages; unknown diagnostics and dynamic field paths remain sanitized.

`tests/smoke.mjs` now exercises the shipped flag/help, choice-stdin syntax refusal,
unsafe-origin structured requests and malformed/unknown nested fields, invalid
UTF-8 and missing choice files against an unavailable literal-loopback endpoint.
It asserts local static errors without private path/query/CSV leakage. Synthetic
loopback unit tests prove actual health/secret/POST counters remain zero; these
process refusals do not imply a real-account CLI-to-NeoDB success scenario.

The first full gate found an existing WebUI fixture count race: a page may render
before post-action status reconciliation finishes. Luna/max added only an
observable `aria-busy=false` wait before the baseline; no-staging-I/O assertions
remain intact. All 35 panel tests plus five repeated target runs passed. Main
reviewed that one-line fix and documented settlement in the frontend contract.

Main final `pnpm verify` passed after all code/test changes: 297 Rust unit tests,
two compile-fail doctests, 85 TS/UI tests, workspace fmt/Clippy, contracts/boundaries,
lint/typecheck, builds and actual local CLI/HTTP/auth/browser smoke. Product YAML/
34 evidence IDs/decision/reference/disabled-capability and diff checks passed
separately. No new dependency, schema/API/migration/Core/Server/Storage production
behavior, source/personal write or capability. All upstream/account/credential
fixtures are fake; this is local Windows, not hosted three-platform evidence.
CLI choice inputs are delivered; broader real-client/simulated-NeoDB-adapter
negative, migration and end-to-end acceptance remains pending. Keep this child
and parent active rather than treating this client checkpoint as PRD completion.
