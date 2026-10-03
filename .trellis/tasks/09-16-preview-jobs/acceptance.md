# Preview acceptance evidence

Status: in progress. This is task-level evidence, not a new product contract.
Checkboxes in `prd.md` require delivered behavior, not merely internal primitives.
All fixtures are synthetic. No live NeoDB/native-fetch, real account or native
credential-store verification is implied. Local Windows checks are not three-OS CI.

## Evidence boundaries

- Catalog transport loopback tests prove restrictive request/response handling;
  Core resolver tests prove polling, identity/granularity and explicit selection.
  Internal search/manual primitives are not yet integrated into public job work.
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

- Integrate approved explicit search/manual selection into public preview work;
  never turn title hints or a singleton candidate into automatic identity proof.
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
