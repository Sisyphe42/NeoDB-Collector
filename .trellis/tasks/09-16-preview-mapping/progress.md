# Progress — 2026-09-17

Activated after completing preview-auth. Added missing design/implementation
artifacts under the approved parent direction.

First checkpoint: normalized domain observations plus internal IMDb CSV adapter.
Five parser tests cover structure, normalization, duplicates, diagnostics and limits.
CSV library line positions were insufficient for physical-line errors in mixed
CRLF/multiline input; incremental byte-position scanning fixes this without
building a large line-offset table. Unsupported source cells are not retained.

Second checkpoint: shared MappingProfile schema, pure Core evaluator (direct,
numeric_scale, clamp, enum_map, condition, skip), rating-only default with explicit
completion opt-in, and bounded YAML/JSON codec. Unknown/duplicate keys, unsupported
tags, anchors/aliases, invalid numeric ranges and activation attempts are rejected.
Outputs are suggestions only, not sink comparison or authorization to write.

Third checkpoint (storage): Core repository port, migration 0003 and immutable
normalized JSON revisions. Atomic expected-version saves prevent lost updates;
old versions remain readable. Disabled-only validation, migration preservation,
restart/history, races, write failures, corruption and size limits are tested.

Remaining: authenticated profile API/generated contracts and shared WebUI
editing/import/export. The storage port is internal and grants no HTTP authority.
No importer is yet advertised through capabilities or exposed to end users.
Task remains in progress; do not archive.

Validation: full pnpm verify passed on Windows, 49 Rust and 19 TS/UI tests,
Clippy/formatting, contracts, dependency boundaries, lint/typecheck, build and
actual local CLI/Chromium smoke. No live IMDb/NeoDB account was accessed.
An existing cancellation mock GC-lifetime bug was reproduced and fixed separately
in cba3819; see research/cancellation-mock.md. Subsequent full gate passed.
