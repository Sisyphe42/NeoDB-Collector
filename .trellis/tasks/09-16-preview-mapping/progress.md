# Progress — 2026-09-17

Activated after completing preview-auth. Added missing design/implementation
artifacts under the approved parent direction.

First checkpoint: normalized domain observations plus internal IMDb CSV adapter.
Five parser tests cover structure, normalization, duplicates, diagnostics and limits.
CSV library line positions were insufficient for physical-line errors in mixed
CRLF/multiline input; incremental byte-position scanning fixes this without
building a large line-offset table. Unsupported source cells are not retained.

Remaining: Core mapping schema/evaluator, YAML/JSON normalization, disabled-version
persistence and authenticated API, shared WebUI editing/import/export.
No importer is yet advertised through capabilities or exposed to end users.
Task remains in progress; do not archive.

Validation: full pnpm verify passed on Windows, 36 Rust and 19 TS/UI tests,
Clippy/formatting, contracts, dependency boundaries, lint/typecheck, build and
actual local CLI/Chromium smoke. No live IMDb/NeoDB account was accessed.
An existing cancellation mock GC-lifetime bug was reproduced and fixed separately
in cba3819; see research/cancellation-mock.md. Subsequent full gate passed.
