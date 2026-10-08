# IMDb import and read-only NeoDB preview

## Goal

OAuth → IMDb ratings CSV → declarative mapping → verified catalog matching → read-only comparison → resumable previews.

## Requirements and acceptance criteria

- [x] One configurable NeoDB instance/account; OAuth read scope; source and personal marks/ratings/reviews never mutated in preview. Native catalog/fetch may create catalog items from validated public source links.
- [x] CSV up to 10 MiB and 10,000 rows; first source IMDb ratings, supported movie/TV types only; unsupported values not persisted.
- [x] Versioned YAML/form mapping, no code; default personal rating only, no inferred completion or watched date.
- [x] Persist normalized preview rows and comparison provenance, not raw CSV/API payloads. Default configurable retention 30 days, manual deletion.
- [x] Refresh on resume; stale results never authorize writes. No scheduling, extra platforms, desktop, extension or n8n implementation.
- [x] Children in order: preview-auth, preview-mapping, preview-jobs. Parent acceptance is integrated WebUI + CLI preview against a simulated server, with live-account verification explicitly separate.

The 2026-10-09 final audit accepts these criteria in the approved synthetic scope;
see [parent acceptance](./acceptance.md). Archive after the full gate passes and
the preview-jobs child is completed. Connector advertising remains separately gated.

## Approval

User approved the complete plan with “实施计划” on 2026-09-16 and native NeoDB catalog fetching on 2026-09-17 after requesting equivalence with pasted-link search. Source and personal user-state writes remain forbidden in preview. Tests use mocks, not live accounts or remote publishing.
