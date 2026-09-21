# Repository Layout and Dependency Boundaries

## 1. Scope / Trigger Conditions

This contract applies when scaffolding, moving, publishing, or adding a first-party component. The repository is a Monorepo, but each executable or externally installed integration remains independently buildable, testable, versioned, and publishable. A shared repository does not authorize internal imports across runtime boundaries.

Do not create empty top-level directories before the owning component is scaffolded. When the first member of a planned directory is implemented, create that directory and add it to the applicable workspace configuration in the same task.

## 2. Planned Layout and Workspace Signatures

```text
/
├─ apps/                         # User-facing applications
│  ├─ webui/                     # Shared React/Vite WebUI
│  ├─ desktop/                   # Thin Tauri shell; src-tauri is a Cargo member
│  └─ browser-extension/         # First-party WebExtension client and site probes
├─ crates/                       # Rust libraries and binaries
│  ├─ collector-domain/          # Pure domain types and invariants; no I/O
│  ├─ collector-core/            # Use cases, mapping, planning, policy, and ports
│  ├─ collector-connectors/      # Built-in API/archive connector adapters
│  ├─ collector-storage/         # SQLite and credential-store adapters
│  ├─ collector-server/          # Axum API, scheduler, events, and WebUI serving
│  └─ collector-cli/             # serve, sync, import, and doctor binary
├─ packages/                     # Reusable JavaScript/TypeScript libraries only
│  ├─ contracts-ts/              # Generated cross-process types and validators
│  └─ core-client-ts/            # Generated or thin typed Core API client
├─ integrations/                 # Packages installed into another host product
│  ├─ n8n-node/                  # Independently published npm community node
│  └─ playnite-plugin/           # Independently released .NET plugin
├─ contracts/                    # Language-neutral generated contracts and fixtures
│  ├─ openapi/
│  ├─ json-schema/
│  ├─ connector-manifests/
│  └─ fixtures/
├─ deploy/                       # Docker and later deployment assets
├─ tests/                        # Cross-component contract and end-to-end tests
└─ tools/                        # Repository maintenance and code-generation tools
```

The root Cargo workspace owns `crates/*` and `apps/desktop/src-tauri`. The root pnpm workspace owns `apps/webui`, `apps/browser-extension`, `packages/*`, and `integrations/n8n-node`. `integrations/playnite-plugin` owns its .NET solution. Root commands orchestrate these workspaces but do not merge their dependency graphs.

## 3. Ownership and Dependency Contracts

- `collector-domain` has no database, network, desktop, web framework, or connector dependency.
- `collector-core` depends on domain contracts and declared ports. It does not depend on Axum, SQLx, Tauri, n8n, browser APIs, or Playnite.
- Rust adapters implement Core ports. Platform-specific source access stays in `collector-connectors`, `collector-storage`, or the client that actually owns the session.
- `collector-server` composes Core and Rust adapters and is the only shared HTTP/WebSocket service implementation.
- `collector-cli` and the Tauri shell invoke the same server/use-case surface; neither owns alternate business logic.
- WebUI, browser extension, n8n Node, and Playnite plugin communicate through the versioned Core API and generated contracts. They never import Rust internals or duplicate planning and policy.
- `packages/` contains reusable libraries, not deployable applications or host-specific integrations.
- `integrations/` contains independently distributed adapters for external host products. An n8n Node belongs here even though npm publishes it as a package.
- Cross-process shapes have one owner. Generated files are regenerated and checked for drift; consumers do not hand-maintain parallel `Observation`, `PlannedOperation`, error, or pairing types.
- Built-in Core connectors begin as modules in `collector-connectors`; split one into its own crate only when dependency weight, platform constraints, or independent testing justifies it.
- Browser-session connectors stay under `apps/browser-extension/src/connectors/`; their manifests and emitted observations still conform to `contracts/`.
- Avoid vague ownership buckets such as root-level `common/`, `shared/`, or `utils/`. Put code with the layer that owns the behavior or extract a specifically named package after a second real consumer exists.

The allowed high-level dependency direction is:

```text
contracts -> generated clients -> apps/integrations
domain -> core -> adapters/server -> CLI/desktop host
apps/integrations --versioned HTTP/WS--> collector-server
```

Independent release versions must declare a compatible Core API range. Sharing the Monorepo does not imply lockstep release versions.

### Standard-library and dependency reuse

- Prefer the standard library, then maintained, widely used, focused libraries for
  generic mechanisms such as parsing, cryptography and asynchronous control.
  Core owns identity, mapping, conflict and authorization lifecycle policy; it
  should not reimplement those generic mechanisms merely to avoid a dependency.
- Runtime independence is not a zero-dependency goal. Reviewed pure libraries may
  be used in Core; network/runtime/database/OS capabilities remain behind adapters.
  Do not introduce a port solely to hide a pure library function.
- Before adding a library, review its purpose, maintenance, license, supported
  Rust version/platforms, enabled features/transitive capabilities and net
  complexity. Record the evidence and tradeoff in the owning task. Preserve
  limits, cancellation, redaction and other existing contracts over library defaults.
- `tools/boundaries.mjs` is an exact, reviewed dependency allowlist, not a permanent
  ban on new Core dependencies. Update the affected allowlist, owning contract,
  Cargo manifests/lockfile and tests together when introducing a Core dependency;
  never relax the check to allow arbitrary crates.
- Do not replace simple `clamp`, collection operations or small domain state
  transitions with frameworks. Evaluate one bounded migration at a time.
- Current examples: Server uses `subtle::ConstantTimeEq` for CSRF verification;
  Core's allowlist includes `secrecy` for [internal secret values](./internal-secrets.md).
  Neither binds Core to a runtime. OAuth protocol migration remains a separate
  evaluation; this convention does not approve replacements en masse.

## 4. Validation and Error Matrix

| Condition | Required result |
|---|---|
| An app or integration imports Core implementation internals | Reject in dependency-boundary checks |
| A Core library is added without explicit allowlist review | Fail `pnpm boundaries:check`; review capabilities before allowing it |
| A cross-process type is handwritten in more than one consumer | Replace with the generated contract owner |
| A host-specific package is placed in `packages/` | Move it to `apps/` or `integrations/` before merge |
| A new top-level `common`, `shared`, or `utils` bucket is proposed | Reject until ownership and at least two real consumers are demonstrated |
| A published client omits its supported Core API range | Fail package validation/release |
| Generated contracts differ from the checked-in contract artifacts | Fail contract drift check |
| One component changes but an unrelated toolchain is unavailable | Run its scoped checks; full release verification still requires every affected artifact |
| A package needs independent ownership, release governance, or incompatible licensing | Review a Polyrepo split without changing the Core API boundary |

## 5. Good, Base, and Bad Cases

**Good case:** A Core API field changes in one commit with its schema, Rust handler, generated TypeScript client, WebUI use, extension use, n8n Node compatibility declaration, fixtures, and contract tests. Each artifact can still be released separately.

**Base case:** A browser-only parser changes inside `apps/browser-extension`. It emits the existing `Observation` schema, so Core crates and the n8n integration do not change.

**Bad case:** The n8n Node imports a workspace-only Core module or reimplements NeoDB conflict resolution because all sources are in one repository. The published package then works only from the repository checkout and diverges from desktop behavior.

## 6. Tests Required

- A dependency-boundary check proving `collector-domain` and `collector-core` do not acquire forbidden framework/runtime dependencies.
- For a library replacement, regression tests prove existing error, limit and lifecycle behavior; inspect resolved features/transitive dependencies as well as direct imports.
- Contract generation and clean-tree drift checks for OpenAPI, JSON Schema, TypeScript validators, and shared fixtures.
- Consumer contract tests for WebUI, extension, n8n Node, and Playnite payloads against the same schema version.
- A package test proving the n8n Node can build and pack without workspace-only runtime files.
- A browser-extension production build that contains no Core, NeoDB, or unrelated connector secret.
- A release-matrix check that records each artifact version and supported Core API range.
- Scoped CI path filtering plus a full integration job that catches invalid assumptions between workspaces.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: a first-party client reaches across the process boundary.
import { planOperations } from "../../crates/collector-core";

// Correct: every external client uses the published API contract.
await coreClient.createDryRun({ flowId, observations });
```

```text
# Wrong: unrelated code accumulates without an owner.
shared/utils/normalize.ts

# Correct: the owning layer is explicit; extract only after proven reuse.
apps/browser-extension/src/connectors/douban/normalize.ts
packages/contracts-ts/src/observation.ts  # generated shared shape only
```
