# Frontend Contracts

The React status WebUI is implemented in `apps/webui`; it consumes `packages/core-client-ts` and generated `packages/contracts-ts`. Extension, mapping, pairing and sync UI remain planned. Current executable behavior is documented in [Runnable Foundation](../backend/foundation-runtime.md).

| Contract | Owns |
|---|---|
| [Shared WebUI Boundaries](./webui-boundaries.md) | Same-origin API, sessions, shared types, dry-run UI, and host parity |
| [Browser Extension Onboarding](./browser-extension-onboarding.md) | Chrome/Edge permission onboarding, per-site grants, and `activeTab` fallback |

## Pre-Development / Quality Check

Read the applicable contract and active task artifacts. Run `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build`; use `pnpm verify` for final cross-layer verification. Assert loading/error/empty states use real API data, unsupported versions stop feature rendering, generated validators guard responses, mobile layout does not overflow and no secrets enter browser storage. Check actual Chromium screenshots from `tests/smoke.mjs` for UI changes.
