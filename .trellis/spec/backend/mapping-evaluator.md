# Declarative Mapping Evaluator

## 1. Scope / Trigger

Implemented internal preview building block. Domain owns types, Core owns pure
validation/evaluation, Server owns configuration decoding. The separate
[Mapping Storage](./mapping-storage.md) adapter persists validated revisions.
No HTTP, WebUI, activation, matching, sink comparison or media writes yet.
The wider conflict/activation contract remains in mapping-and-conflict-contract.md.

## 2. Signatures

- `collector_core::mapping::imdb_default() -> MappingProfile`
- `validate(&MappingProfile) -> Result<(), ApiError>`
- `evaluate(&MappingProfile, &Observation) -> Result<Vec<MappedField>, ApiError>`
- `collector_server::mapping_profiles::decode(&str, Format) -> Result<MappingProfile, ApiError>`
- `encode(&MappingProfile, Format) -> Result<String, ApiError>`
- `Format::{Json, Yaml}`; no filesystem or network access.

## 3. Contracts

`crates/collector-domain/src/mapping.rs` defines the shared representation:
`schemaVersion: 1`, `id`, positive `version`, `sourceConnectorId`, `sinkConnectorId`,
`enabled: false`, `rules`. Each rule has `id`, `enabled`, `target`, `operation`.
Identifiers/source field keys are 1–128 ASCII letters/digits or `_.:-`.
Profile/rule structs use camelCase; operation kinds and their argument fields use
snake_case. Rules and enum entries are bounded to 128; strings to 4096 bytes.
Rule IDs are unique; enabled rules cannot share a target. Disabled rules are also
validated. Only sink `neodb` is implemented; identifiers remain open strings.

Implemented targets are the staged IMDb subset: finite `rating_grade` in 0–10 and
`shelf_type` in wishlist/progress/complete/dropped. Other NeoDB-supported fields
remain future capability additions, not silently accepted data.

| Operation kind | Arguments / semantics |
|---|---|
| direct | from; typed scalar copy |
| numeric_scale | from, input_min/input_max, output_min/output_max; finite increasing input range, source must be in range, output range within 0–10 |
| clamp | from, min/max; finite ordered bounds within 0–10 |
| enum_map | from, values; exact string keys to validated target scalars |
| condition | from, test, value; test kind exists, equals (scalar value), greater_than (finite numeric value) |
| skip | no arguments; no suggestion |

Evaluation verifies observation schema, connector match and bounded finite state.
Output follows rule order: ruleId/target/status/reason/optional value. Accepted
means a valid suggestion, **not** an approved write or a comparison result.
Rejected/skipped fields contain no value. Default profile clamps personal rating;
the completion rule is disabled until explicitly selected. No date inference.

Codec accepts at most 64 KiB, depth 16, one document, 20,000 events/10,000 nodes.
It rejects unknown fields/operations, duplicate keys including nested maps,
anchors, aliases, merge keys and unsupported tags. No include handlers or code
execution. JSON must also pass serde_json syntax/type parsing and agree with the
duplicate-aware parse. Export validates and verifies re-importability. Parser
diagnostics never echo document content; Core errors expose static/indexed paths.

## 4. Validation & Error Matrix

| Condition | Result/code |
|---|---|
| Bad syntax/schema/opcode/unknown field/duplicate/budget | mapping_json_invalid or mapping_yaml_invalid |
| Over 64 KiB | mapping_document_too_large |
| Bad schema/version/identifier | mapping_version_invalid / mapping_identifier_invalid |
| Enabled profile | connector_not_activated |
| Bad target / duplicate enabled target | mapping_target_unsupported / mapping_target_duplicate |
| Invalid operation bounds/values | mapping_arguments_invalid |
| Source schema/mismatch/invalid state | observation_schema_unsupported / mapping_source_mismatch / observation_state_invalid |
| Disabled/skip rule | skipped: rule_disabled / rule_skipped |
| Missing source/unmapped enum/false condition | skipped: source_missing / enum_unmapped / condition_false |
| Wrong type/out-of-range scale/output | rejected: source_type_invalid / source_range_invalid / mapped_value_invalid |

## 5. Good / Base / Bad Cases

Good: rating 8 maps to 8; enabling the completion rule adds complete while the
profile itself remains disabled. Base: missing rating is skipped. Bad: treating
an accepted suggestion as permission to overwrite an existing NeoDB rating.

## 6. Tests Required

- `crates/collector-core/src/mapping/tests.rs`: all operations, default/opt-in,
  missing/type/range failures, finite arithmetic, duplicates, versions/activation.
- `crates/collector-server/src/mapping_profiles.rs` tests: golden YAML equals
  built-in profile, YAML/JSON stable round trips, strict schema and duplicate
  keys including nested JSON enum maps, tags/aliases/documents/budgets.
- Run scoped tests and full `pnpm verify`; no live platform accounts required.

## 7. Wrong vs Correct

Wrong: decode JSON directly to a BTreeMap and silently keep a duplicate enum key.
Correct: apply duplicate-aware bounded parsing, JSON syntax checks when selected,
then the same Core validator used for programmatic profiles.

Wrong: `accepted` means execute. Correct: pass suggestions into the future
comparison/policy planner; this evaluator has no writer dependency.
