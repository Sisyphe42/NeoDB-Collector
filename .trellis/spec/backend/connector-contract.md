# Connector Observation and Capability Contract

## 1. Scope / Trigger Conditions

This contract applies to every built-in API, archive, webpage, local-app, or webhook connector. Current source connectors are read-only and NeoDB is the only write endpoint. Types remain direction-neutral so a later explicitly authorized sink does not require rewriting collectors.

## 2. Signatures and Data Shapes

```ts
type NamespacedId = string;
type ConnectorRole = "source" | "sink" | "both";
type ConnectorKind = "api" | "archive" | "webpage" | "local_app" | "webhook";

interface ConnectorManifest {
  id: NamespacedId;
  version: string;
  minimumCoreVersion: string;
  role: ConnectorRole;
  kind: ConnectorKind;
  mediaTypes: NamespacedId[];
  capabilities: string[];
  identifierSchemes: string[];
  auth: string[];
  permissions: string[];
}

interface Observation {
  schemaVersion: 1;
  sourceConnectorId: NamespacedId;
  sourceRecordId: string;
  observedAt: string;
  idempotencyKey: string;
  item: {
    mediaType: NamespacedId;
    canonicalUrl?: string;
    externalIds: Record<string, string>;
    titleHint?: string;
  };
  state: Record<string, unknown>;
}

interface PlannedOperation {
  endpointConnectorId: NamespacedId;
  canonicalItemId: string;
  field: string;
  operation: "set" | "skip" | "needs_review";
  value?: unknown;
  reason: string;
}
```

## 3. Behavioral Contracts

- Connector IDs and media types are open namespaced strings, not closed Core enums.
- Collectors emit schema-validated `Observation` values and never call NeoDB or another connector directly.
- Resolution, mapping, conflict comparison, planning, policy checks, and execution remain separate stages.
- Current flow policy permits source reads, NeoDB reads for comparison, and NeoDB writes. It denies every source write capability even if a future-facing interface exists.
- Automatic matching requires a supported canonical URL, verified external ID, or existing manual mapping. Fuzzy matches become `needs_review`.
- Native NeoDB catalog fetching is approved for preview resolution (2026-09-17),
  with internal single-attempt transport now implemented in [NeoDB Catalog Transport](./neodb-catalog.md);
  polling and identity resolution remain pending. For supported external links, API `fetch_item` and
  web `fetch` share SiteManager/get_item and enqueue_fetch(..., False, user).
  The web confirmation flow is not identical to the API endpoint. See pinned
  `neodb_native_fetch_api` / `neodb_native_fetch_web` product evidence.
- Fetch may create a public catalog item even during preview; never describe it
  as side-effect-free. It must not write personal marks/ratings/reviews, force
  refresh an existing item or mutate source data. Only connector-validated public
  canonical item links may be submitted, not arbitrary/private/local URLs.
- Handle 202 with polls at least 15 seconds apart and a 120-second deadline;
  handle 422/429/network failures as unavailable, not proof of absence. Validate
  302 destinations as same-instance catalog-detail paths before following them;
  do not enable generic redirect following or forward credentials to source sites.
  Verify returned external identity and item granularity before automatic binding.
  Cancelling local work stops polling but cannot undo remote creation.
- Built-in connector registration is compile-time/build-time only in MVP. A valid external manifest is not execution authority.
- Unsupported source fields are reported as skipped and are not preserved as a general history record.

## 4. Validation and Error Matrix

| Condition | Result | Code |
|---|---|---|
| Manifest ID or version is invalid | Connector unavailable | `connector_manifest_invalid` |
| Observation schema version is unsupported | Reject batch item | `observation_schema_unsupported` |
| Source tries to invoke a writer | Deny before network call | `source_write_forbidden` |
| No strong identifier or manual mapping exists | Queue review | `item_match_ambiguous` |
| Field is unsupported by NeoDB | Skip with reason | `sink_field_unsupported` |
| Duplicate idempotency key has same content | Return prior result | `already_processed` |
| Duplicate idempotency key has different content | Reject | `idempotency_conflict` |
| External connector package is supplied | Do not load | `third_party_connectors_disabled` |

## 5. Good, Base, and Bad Cases

**Good case:** A Steam collector emits a game observation with Steam app ID and playtime. Core resolves the item, applies an explicitly enabled mapping, compares NeoDB, and creates a planned operation.

**Base case:** A source provides a supported canonical URL and one supported shelf field. Core writes that field and reports unsupported source fields as skipped.

**Bad case:** A Douban connector directly invokes NeoDB after parsing a page, or updates Douban after a NeoDB conflict. Both bypass policy and planner boundaries.

## 6. Tests Required

- Manifest contract tests for every built-in connector fixture.
- Observation schema and forward-version rejection tests.
- Policy tests proving all source-write paths are denied before I/O.
- Resolver tests for canonical URL, verified ID, manual mapping, fuzzy match, and no match.
- Idempotency tests for replay and conflicting duplicate keys.
- Connector test showing unsupported fields are skipped without raw-payload persistence.
- Registry test proving external packages cannot be executed in MVP.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: platform-specific collector bypasses the shared pipeline.
await neodb.setShelf(parsed.url, "complete");

// Correct: collector only emits an observation.
yield observationSchema.parse(parsedObservation);
```

```ts
// Wrong: a closed enum requires Core edits for every new platform.
type Platform = "douban" | "steam" | "imdb";

// Correct: manifests own open namespaced connector IDs.
type ConnectorId = string;
```
