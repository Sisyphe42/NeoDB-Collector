# IMDb CSV and declarative mapping

## Goal

Parse real IMDb ratings format and produce safe deterministic mapped observations.

## Requirements and acceptance criteria

- [ ] Depends on preview-auth before exposing any private HTTP data.
- [ ] UTF-8/BOM CSV, column-name parsing, quotes/newlines, structural validation and line errors; 10 MiB/10,000 rows.
- [ ] movie/tvMovie/short and tvSeries/tvMiniSeries map to movie/tv; episodes and other types skipped; conflicting duplicate records rejected.
- [ ] Built-in connector emits observations only. Core owns validated declarative mapping: direct, scale, clamp, enum, condition, skip.
- [ ] YAML and WebUI round-trip one schema, persisted disabled versions; personal rating default, completion opt-in, Date Rated not watched date.
- [ ] Golden tests, invalid-field/code rejection, version tests; no NeoDB/source writes.

## Approval

User approved the complete plan with “实施计划” on 2026-09-16. Work remains read-only toward media platforms. No live account access or remote publish is authorized by this task.
