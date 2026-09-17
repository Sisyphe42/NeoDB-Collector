# Progress — 2026-09-18

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

Fourth checkpoint: authenticated validation/GET latest/PUT versioned save routes,
generated contracts and shared TS client methods. Session saves require CSRF;
Bearer authentication never falls back to cookies. JSON-string envelopes preserve
strict duplicate-aware YAML/JSON parsing. Missing/invalid/oversize/stale saves
never create or enable profiles.

Final checkpoint: shared WebUI editor, Core defaults and document-conversion API.
Form controls edit identity/source/rule targets and opt-ins; advanced YAML/JSON
edits full allowlisted operations. Import validates before replacing the form;
export/download/upload round trips through Core. Version conflicts preserve the
draft, and imported versions never replace the loaded expected-version baseline.
Explicit textarea label association fixes Chromium's value-dependent label lookup.
Actual desktop/mobile screenshots were inspected; no horizontal overflow.
No importer is yet advertised through capabilities or exposed to end users.
This child is complete. Import submission and native catalog resolution/comparison
belong to preview-jobs; the parent milestone is not complete.

Validation: full pnpm verify passed on Windows, 53 Rust and 25 TS/UI tests,
Clippy/formatting, contracts, dependency boundaries, lint/typecheck, build and
actual local CLI/Chromium smoke. No live IMDb/NeoDB account was accessed.
After the final advanced-editor collapse/screenshot refinement, frontend lint,
typecheck, tests, build, real smoke and diff checks passed again. Smoke verifies
save v1, YAML export/download/upload/import, save v2 and reload from SQLite.
An existing cancellation mock GC-lifetime bug was reproduced and fixed separately
in cba3819; see research/cancellation-mock.md. Subsequent full gate passed.
