# Runtime Distribution and Connector Availability

## 1. Scope / Trigger Conditions

This contract applies to desktop packaging, NeoDB endpoint configuration, connector registration, and first-run activation. Windows, macOS, and Linux are equal first-class desktop targets. The MVP supports exactly one configured NeoDB endpoint and loads only connectors shipped by this repository.

The connector boundary remains versioned so a later product decision can add reviewed third-party extensions without changing the core observation and planning interfaces. That future boundary does not authorize an executable plugin loader now.

Implementation status: `collector-connectors::neodb::NeoDbClient` implements the typed OAuth transport (composed into server routes) and a separate internal [catalog transport](./neodb-catalog.md). Catalog fetch/detail is not yet composed into jobs or public routes. No journal writer exists.

## 2. Signatures and Data Shapes

```ts
type DesktopPlatform = "windows" | "macos" | "linux";

interface NeoDbSettings {
  baseUrl: string;
  credentialRef: string;
}

interface ConnectorManifest {
  id: string;
  version: string;
  minimumCoreVersion: string;
  capabilities: string[];
  mediaTypes: string[];
}

interface BuiltInConnectorRegistration {
  manifest: ConnectorManifest;
  factoryId: string;
  distribution: "built_in";
}
```

`GET /api/v1/settings/neodb` returns the singleton `NeoDbSettings` without credential material. `PUT /api/v1/settings/neodb` creates or replaces that singleton after validating the base URL and credential reference.

The runtime registry accepts `BuiltInConnectorRegistration` values generated at build time. It has no filesystem package scanning, arbitrary module import, or remote connector installation API in MVP.

## 3. Behavioral Contracts

- The same product features and data contracts are supported on Windows, macOS, and Linux. Platform adapters may differ for installers, signing, OS credential stores, tray behavior, and startup registration.
- CI must compile and test Core, WebUI integration, and desktop adapter boundaries on all three desktop operating systems.
- There is exactly one NeoDB settings record. The base URL is configurable for hosted or self-hosted NeoDB, but account/profile arrays and routing selectors do not exist.
- Built-in connectors use versioned manifests and the same registry boundary intended for future extensions.
- Unknown or external connector packages are reported as unsupported; they are never dynamically executed.
- New connectors remain write-disabled and schedule-disabled until the activation contract in `mapping-and-conflict-contract.md` succeeds.
- NeoDB base origins accept HTTPS, or HTTP only for a literal loopback IPv4/IPv6 address. Reject DNS names on HTTP, userinfo, query, fragment and non-root paths. Match `url::Host::Ipv4/Ipv6` directly; bracketed IPv6 `host_str()` is not a generic `IpAddr` input.
- The OAuth adapter follows no redirect, has a 10-second total request timeout and caps decoded response bytes at 64 KiB even for chunked responses. It requests only `scope=read`, rejects any returned scope other than exactly read, and sanitizes all upstream bodies/errors.
- The OAuth adapter exposes no generic request method and no catalog fetch, mark, webhook or other media/state write operation. The reviewed endpoint set is `/api/v1/apps`, `/oauth/authorize`, `/oauth/token`, `/api/me`.

## 4. Validation and Error Matrix

| Condition | Result | Code |
|---|---|---|
| NeoDB base URL is not HTTP(S) or is malformed | Reject settings | `neodb_base_url_invalid` |
| A second profile/account is submitted | Reject the shape | `multiple_neodb_profiles_unsupported` |
| Built-in manifest ID is duplicated | Fail registry initialization | `connector_id_duplicate` |
| Manifest requires a newer Core | Keep connector unavailable | `connector_core_incompatible` |
| User supplies an external package or module path | Do not load or execute | `third_party_connectors_disabled` |
| Platform adapter is missing in a release build | Fail that build | `desktop_adapter_missing` |
| New connector requests schedule enablement before activation | Reject | `connector_not_activated` |

## 5. Good, Base, and Bad Cases

**Good case:** The same source tree produces Windows, macOS, and Linux desktop builds. Each uses its OS credential adapter, points the singleton NeoDB settings at the user's chosen base URL, and exposes the same built-in connector manifests.

**Base case:** A user changes the singleton NeoDB base URL. The Core updates that endpoint after validation and does not create a second profile or routing branch.

**Bad case:** The MVP scans a plugin directory and executes an unreviewed third-party connector because its manifest parses successfully. A manifest is metadata, not execution authorization.

## 6. Tests Required

- CI compile and unit-test matrix for Windows, macOS, and Linux.
- Contract-test `GET` and `PUT /api/v1/settings/neodb`, including credential redaction and rejection of profile arrays.
- Adapter tests must use a local mock server to assert form fields, `read` scope, Bearer identity lookup, redirect denial, response cap, sanitized errors and rejection of unsafe base origins. They must never contact a live account.
- Registry-test unique IDs, minimum Core version checks, and deterministic built-in enumeration.
- Security test that paths, URLs, archives, and modules supplied as third-party connectors are rejected without loading code.
- End-to-end test that each supported desktop adapter can start the shared Core and open the shared WebUI.
- Activation test that every newly registered connector begins with writes and scheduling disabled.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: platform priority leaks into product behavior.
if (platform !== "windows") disableScheduler();

// Correct: only OS integration is adapted; product capability is shared.
const osIntegration = desktopAdapters.for(platform);
await osIntegration.startSharedCore();
```

```ts
// Wrong: a valid manifest is treated as authority to execute external code.
await import(userSuppliedManifest.entrypoint);

// Correct: MVP registry is populated only from build-time registrations.
registry.registerBuiltIn(generatedRegistration);
```
