# Frontend Contracts

The React status and local-session/client-pairing WebUI is implemented in `apps/webui`; it consumes `packages/core-client-ts` and generated `packages/contracts-ts`. NeoDB OAuth and mapping configuration UI are implemented; extension and sync UI remain planned. Current executable behavior is documented in [Runnable Foundation](../backend/foundation-runtime.md), [Local Access](../backend/state-and-pairing.md) and [Mapping Editor](./mapping-editor.md).

| Contract | Owns |
|---|---|
| [Shared WebUI Boundaries](./webui-boundaries.md) | Same-origin API, sessions, shared types, dry-run UI, and host parity |
| [Mapping Editor](./mapping-editor.md) | Disabled configuration form, YAML/JSON interchange and version conflicts |
| [Browser Extension Onboarding](./browser-extension-onboarding.md) | Chrome/Edge permission onboarding, per-site grants, and `activeTab` fallback |

## Pre-Development / Quality Check

Read the applicable contract and active task artifacts. Run `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build`; use `pnpm verify` for final cross-layer verification. Assert loading/error/empty states use real API data, unsupported versions stop feature rendering, generated validators guard responses, mobile layout does not overflow and no secrets enter browser storage. Check actual Chromium screenshots from `tests/smoke.mjs` for UI changes.
