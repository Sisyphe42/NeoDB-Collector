# NeoDB Collector Reuse Checklist

Search before adding a helper, schema, platform branch, or connector-specific mapping.

## Single Owners

- `.trellis/spec/product/collector-spec.yaml` owns product decisions and platform capability declarations.
- Connector manifests own connector IDs, media types, authentication, permissions, and capabilities.
- Shared API schemas own request, response, error, `Observation`, and `PlannedOperation` shapes.
- The mapping evaluator owns declarative rule execution for both dry-run and live planning.
- The policy layer owns allowed flow directions; connectors must not reproduce policy checks.
- The credential abstraction owns OS keychain/container-secret access.
- The state layer owns SQLite migrations and transactions.

## Before Adding Code

- Search for the field, route, status, capability, or error code first.
- Extend an open namespaced ID or manifest; do not add a closed platform enum.
- Add a connector adapter, not a platform branch inside Core.
- Add a mapping operation to the allowlisted evaluator only when it is reusable and safe; never embed executable scripts.
- Reuse the production planner for previews; do not implement a second dry-run approximation.
- Reuse the same WebUI bundle and API client across desktop, CLI WebUI, and Docker.
- Keep OS differences behind desktop adapters instead of branching product behavior.

## Duplication Smells

- Two consumers locally cast the same JSON fields.
- Extension and WebUI define separate import result types.
- Each connector implements its own rating scale or conflict logic.
- Platform-specific code writes checkpoints or tokens directly.
- A connector keeps raw payloads because another layer lacks a normalized field.

When any smell appears twice, establish or extend the shared owner before adding a third copy.
