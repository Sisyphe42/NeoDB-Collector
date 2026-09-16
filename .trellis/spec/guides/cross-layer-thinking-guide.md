# NeoDB Collector Cross-layer Checklist

Use this checklist before changing a connector, observation, mapping rule, API payload, storage record, or UI state.

## Trace the Full Flow

```text
source/API/page/archive
  -> built-in connector
  -> Observation validation
  -> identity resolution
  -> mapping profile
  -> NeoDB state comparison
  -> PlannedOperation and policy
  -> dry-run/review or NeoDB execution
  -> checkpoint and operation result
```

Check every affected boundary:

- Does `.trellis/spec/product/collector-spec.yaml` already define the capability, field, route, or policy?
- Is the source interaction still read-only?
- Does the connector emit an `Observation` instead of calling NeoDB directly?
- Is a new source field supported by NeoDB? If not, does the result explicitly say `skipped`?
- Is identity based on a canonical URL, verified external ID, or manual mapping? Fuzzy results must become `needs_review`.
- Do WebUI, extension, Playnite plugin, CLI, and Docker use the same versioned API shape?
- Is the dry-run produced by the same planner as execution?
- Does a configuration or mapping change invalidate prior activation?
- Does SQLite retain only normalized operational state, never raw API/page payloads or plaintext secrets?

## Security Boundaries

- Browser login state remains inside the browser; no `cookies` permission or raw-cookie transport.
- Pairing URI secrets appear only in a fragment and are short-lived/single-use.
- Long-lived client tokens use `Authorization: Bearer`, are hashed at rest, and are redacted from logs.
- WebUI secrets never enter `localStorage`, query strings, analytics, or error reports.
- Loopback is the default bind; LAN/remote exposure requires explicit authentication and TLS termination.

## Required Spec Updates

When changing a cross-layer contract, update together:

1. `.trellis/spec/product/collector-spec.yaml`.
2. The matching file under `.trellis/spec/backend/` or `.trellis/spec/frontend/`.
3. `AGENTS.md` only when a repository-wide invariant changes.
4. Contract fixtures and tests once source code exists. Files under `temp/` are never dependencies.

Never add a second private payload definition in a UI or connector. The schema owner must be shared or generated.
