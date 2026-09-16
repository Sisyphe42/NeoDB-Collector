# Bootstrap NeoDB Collector Implementation Specs

## Goal

Replace Trellis placeholder guidelines with project-specific executable contracts derived from the approved product specification before application source is scaffolded.

## Evidence Sources

- `AGENTS.md`
- `.trellis/spec/product/collector-spec.yaml`
- `AGENTS.md`

The repository is contract-first and has no application source yet. Specs must therefore describe approved boundaries and must not invent framework coding conventions or claim source-backed patterns that do not exist.

## Completed Scope

- [x] Remove unfilled backend and frontend template files.
- [x] Add connector, mapping/conflict, runtime/distribution, operational-state, and pairing contracts.
- [x] Add shared WebUI and browser-extension onboarding contracts.
- [x] Replace generic thinking guides with NeoDB Collector checklists.
- [x] Add root and layer indexes matching the final spec tree.
- [x] Include concrete signatures, validation matrices, cases, tests, and wrong/correct examples for cross-layer contracts.
- [x] Keep `.trellis/spec/product/collector-spec.yaml` as the machine-readable source of truth.

## Acceptance Criteria

- `.trellis/spec/` contains no `To fill`, placeholder instruction, or unrelated Trellis product example.
- Every indexed spec link resolves.
- Every backend/frontend contract documents only approved current decisions.
- Future framework-specific conventions are added only after real source and tests establish them.
