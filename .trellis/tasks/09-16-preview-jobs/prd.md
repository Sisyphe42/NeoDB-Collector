# Resumable read-only preview

## Goal

Compare mapped records with NeoDB and retain resumable review results, so users
can inspect identity matches and field conflicts before any personal-state write.

## Confirmed context

Preview-auth and preview-mapping are completed. The existing IMDb CSV adapter
emits normalized observations and saved mapping profiles have immutable disabled
revisions. This child connects those capabilities into a resumable preview.

## Requirements and acceptance criteria

- [x] Depends on preview-auth and preview-mapping (completed).
- [ ] Resolve validated public links with native catalog/fetch, then detail/identity checks; search is fallback. Verify IMDb IDs and show/season granularity; ambiguous candidates need manual confirmation. Handle 202 with bounded polling, 302 with same-instance detail validation, 422/429/timeouts explicitly; cancellation cannot undo remote catalog creation.
- [ ] Read current marks; equal skipped, conflict needs_review preserving sink; network failure is not absence; missing shelf blocks readiness.
- [ ] Background jobs with polling, cancel, interrupted-on-restart and explicit resume; fresh comparison on resume.
- [ ] Persist normalized fields, identity evidence, snapshot time/account/profile version; never raw files/responses or unsupported field values.
- [ ] Default preview retention 30 days configurable, periodic/startup cleanup and manual deletion; keep profiles and credentials.
- [ ] WebUI/CLI pagination, progress and field diffs; no executable operations. Timeout/429/auth/redirect and restart/migration end-to-end tests.

## Approval

User approved the complete plan on 2026-09-16 and native NeoDB catalog fetching on 2026-09-17. Preview may create catalog entries, but never changes personal marks/ratings/reviews or source data. Tests use mocks, not live accounts.

## Out of scope

Personal-state execution, write/schedule activation, new source platforms,
third-party connector loading and a general workflow engine are not part of this
child. User reviewed `design.md` and `implement.md` and requested continuation
on 2026-09-18; the child is now active.
