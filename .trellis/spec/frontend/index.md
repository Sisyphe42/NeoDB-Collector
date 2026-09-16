# Frontend Contracts

The frontend source has not been scaffolded yet. These files define cross-host and extension behavior already approved by the product specification; framework-specific component conventions must wait for real code.

| Contract | Owns |
|---|---|
| [Shared WebUI Boundaries](./webui-boundaries.md) | Same-origin API, sessions, shared types, dry-run UI, and host parity |
| [Browser Extension Onboarding](./browser-extension-onboarding.md) | Chrome/Edge permission onboarding, per-site grants, and `activeTab` fallback |
