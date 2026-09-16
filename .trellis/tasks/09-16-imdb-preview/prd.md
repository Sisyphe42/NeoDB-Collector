# IMDb import and read-only NeoDB preview

## Goal

OAuth → IMDb ratings CSV → declarative mapping → verified catalog matching → read-only comparison → resumable previews.

## Requirements and acceptance criteria

- [ ] One configurable NeoDB instance/account; OAuth read scope; source and media records never mutated; catalog/fetch forbidden.
- [ ] CSV up to 10 MiB and 10,000 rows; first source IMDb ratings, supported movie/TV types only; unsupported values not persisted.
- [ ] Versioned YAML/form mapping, no code; default personal rating only, no inferred completion or watched date.
- [ ] Persist normalized preview rows and comparison provenance, not raw CSV/API payloads. Default configurable retention 30 days, manual deletion.
- [ ] Refresh on resume; stale results never authorize writes. No scheduling, extra platforms, desktop, extension or n8n implementation.
- [ ] Children in order: preview-auth, preview-mapping, preview-jobs. Parent acceptance is integrated WebUI + CLI preview against a simulated server, with live-account verification explicitly separate.

## Approval

User approved the complete plan with “实施计划” on 2026-09-16. Work remains read-only toward media platforms. No live account access or remote publish is authorized by this task.
