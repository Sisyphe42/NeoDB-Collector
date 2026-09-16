# Resumable read-only preview

## Goal

Compare mapped records with NeoDB and retain resumable review results.

## Requirements and acceptance criteria

- [ ] Depends on preview-auth and preview-mapping.
- [ ] Search and detail reads only; verify IMDb IDs, ambiguous candidates need explicit manual confirmation; catalog/fetch forbidden.
- [ ] Read current marks; equal skipped, conflict needs_review preserving sink; network failure is not absence; missing shelf blocks readiness.
- [ ] Background jobs with polling, cancel, interrupted-on-restart and explicit resume; fresh comparison on resume.
- [ ] Persist normalized fields, identity evidence, snapshot time/account/profile version; never raw files/responses or unsupported field values.
- [ ] Default preview retention 30 days configurable, periodic/startup cleanup and manual deletion; keep profiles and credentials.
- [ ] WebUI/CLI pagination, progress and field diffs; no executable operations. Timeout/429/auth/redirect and restart/migration end-to-end tests.

## Approval

User approved the complete plan with “实施计划” on 2026-09-16. Work remains read-only toward media platforms. No live account access or remote publish is authorized by this task.
