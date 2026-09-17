# Mapping Profiles, Dry-run, and Current Conflict Policy

Implementation status: the pure evaluator, bounded YAML/JSON codec, immutable
version storage and authenticated validation/read/save routes are implemented;
see [Mapping Evaluator](./mapping-evaluator.md) and [Mapping HTTP](./mapping-http.md)
for executable schemas. Conflict planning, activation and WebUI editing remain
planned. The TypeScript shapes below describe future planning, not extra accepted
profile fields in the implemented strict schema.

## 1. Scope / Trigger Conditions

This contract applies whenever an observation is transformed into a proposed NeoDB field update. Mapping profiles are editable through WebUI forms and YAML import/export, but both surfaces use the same schema and validator. A profile must pass validation and dry-run preview before it can be enabled.

A newly configured connector starts with writes and scheduling disabled. Its first run is a manual dry-run; only an explicit user confirmation for the exact connector configuration and mapping profile versions may enable writes or scheduling.

The current product flow is source-read-only to NeoDB. When a mapped source value differs from an existing NeoDB value, the default is to keep the NeoDB value and emit `needs_review` without mutating that field.

## 2. Signatures and Data Shapes

```ts
type ConflictPolicy = "keep_neodb" | "replace" | "newest" | "needs_review";

interface MappingProfile {
  id: string;
  version: number;
  sourceConnectorId: string;
  sinkConnectorId: string;
  enabled: boolean;
  rules: MappingRule[];
  conflicts?: Partial<Record<string, ConflictPolicy>>;
}

interface FieldPlan {
  field: string;
  sourceValue: unknown;
  currentSinkValue: unknown;
  proposedValue?: unknown;
  status: "accepted" | "skipped" | "needs_review" | "rejected";
  reason: string;
  policy: ConflictPolicy;
}

interface MappingDryRunResult {
  profileId: string;
  profileVersion: number;
  executable: false;
  fields: FieldPlan[];
  summary: Record<FieldPlan["status"], number>;
}

interface ConnectorActivation {
  connectorInstanceId: string;
  connectorConfigVersion: number;
  mappingProfileId: string;
  mappingProfileVersion: number;
  successfulDryRunId: string;
  writesEnabled: boolean;
  scheduleEnabled: boolean;
  confirmedAt?: string;
}
```

HTTP contracts:

- `POST /api/v1/mapping-profiles/validate` accepts a JSON envelope `{format, document}` with YAML or JSON text, returning the normalized profile or sanitized errors without saving or enabling it.
- `PUT /api/v1/mapping-profiles/{profile_id}` saves a valid, disabled profile version by default.
- `POST /api/v1/mapping-profiles/{profile_id}/dry-run` evaluates selected observations against current NeoDB state and returns `MappingDryRunResult`; it performs no writes.
- Enabling a profile requires the exact profile version to have a successful dry-run record.
- Enabling connector writes or a schedule requires an explicit confirmation referencing a successful dry-run for the same connector configuration and mapping profile versions.

## 3. Behavioral Contracts

- WebUI form edits and YAML imports round-trip through one canonical mapping representation.
- Validation rejects unknown target fields, unsafe operations, arbitrary executable code, and values outside the NeoDB capability schema.
- Safety/schema constraints override connector defaults, user profiles, and per-item overrides.
- The default policy for an unequal, non-empty NeoDB field is effectively `needs_review`: retain the sink value, generate no executable write for that field, and preserve both values in the review plan.
- Explicit profile or per-item conflict policies may select another supported behavior, but never enable source-platform writes under the current flow policy.
- Dry-run and execution use the same planner and mapping evaluator; only the executor is disabled during dry-run.
- Editing connector configuration or the active mapping profile invalidates the previous activation and disables scheduled writes until another successful dry-run is confirmed.

## 4. Validation and Error Matrix

| Condition | Result | Code |
|---|---|---|
| YAML syntax is invalid | Reject profile | `mapping_yaml_invalid` |
| Target field is unsupported by sink | Reject rule | `mapping_target_unsupported` |
| Operation is outside the declarative allowlist | Reject rule | `mapping_operation_forbidden` |
| Value violates sink schema | Reject field plan | `mapped_value_invalid` |
| NeoDB value equals mapped value | Skip write as idempotent | `already_equal` |
| NeoDB value differs and no override exists | Keep NeoDB and queue review | `sink_conflict_needs_review` |
| Dry-run version differs from enabled version | Refuse enablement | `dry_run_stale` |
| A schedule is enabled before confirmed dry-run | Refuse enablement | `connector_not_activated` |
| NeoDB state cannot be read | Do not guess or write | `sink_state_unreachable` |

## 5. Good, Base, and Bad Cases

**Good case:** A YAML profile is imported, normalized into the same form model used by the WebUI, validated, and dry-run against sample observations. A rating conflict appears with both values and a field-level reason; NeoDB remains unchanged until the user resolves it.

**Base case:** A new source value targets an empty supported NeoDB field. The dry-run marks it `accepted`; later execution may write it under current policy.

**Bad case:** The importer silently overwrites a non-empty NeoDB rating because the source observation is newer, even though no `newest` policy was explicitly selected. This violates the default conflict contract.

**Bad activation case:** Creating a connector automatically starts its scheduler or performs writes before the user has reviewed its dry-run. This violates the activation contract.

## 6. Tests Required

- Golden tests proving YAML and WebUI JSON normalize to the same `MappingProfile`.
- Schema tests for every supported operation and every NeoDB target field.
- Property tests that dry-run never invokes an endpoint writer.
- Planner tests for empty, equal, conflicting, missing, and invalid field values.
- Contract test that the default conflict plan retains `currentSinkValue`, omits an executable write, and returns `needs_review`.
- Version test that editing a profile invalidates its prior dry-run approval.
- Activation test that new connectors have both `writesEnabled` and `scheduleEnabled` false, and that configuration changes revoke a prior activation.
- End-to-end test that a resolved per-item override affects only that item and remains within current source-read-only policy.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: dry-run has a separate approximation of mapping behavior.
const preview = summarizeRules(profile.rules);

// Correct: dry-run uses the production planner with a no-op executor.
const preview = planner.plan(observations, sinkState, profile, { execute: false });
```

```ts
// Wrong: an unspecified conflict overwrites the sink.
plan.write(field, sourceValue);

// Correct: preserve NeoDB and create a reviewable, non-executable field plan.
plan.needsReview({ field, sourceValue, currentSinkValue, reason: "sink_conflict_needs_review" });
```

```ts
// Wrong: connector creation implicitly enables scheduled writes.
await scheduler.enable(connector.id);

// Correct: only a confirmed, version-matched dry-run can activate it.
await activator.confirm({ connector, profile, successfulDryRunId });
```
