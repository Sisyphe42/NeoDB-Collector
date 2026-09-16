# Browser Extension Onboarding and Site Permissions

## 1. Scope / Trigger Conditions

This contract applies to the first-party browser extension shipped for Chrome and Edge. The implementation must remain WebExtensions-compatible so Firefox can be added later through a thin browser adapter.

Show onboarding when no completed or skipped state exists for the current onboarding schema version. Onboarding explains site access before any browser permission prompt. A permission request may run only as the direct result of the user pressing the grant button for one connector.

This feature never grants the Collector access to raw cookies, source credentials, browser profiles, or unrestricted browsing data.

## 2. Signatures and Data Shapes

```ts
type FirstClassBrowser = "chrome" | "edge";
type SitePermissionStatus =
  | "not_requested"
  | "granted"
  | "denied"
  | "revoked";

interface ConnectorSitePermission {
  connectorId: string;
  origins: string[];
  status: SitePermissionStatus;
  checkedAt: string; // RFC 3339
}

interface ExtensionOnboardingStateV1 {
  schemaVersion: 1;
  completed: boolean;
  skipped: boolean;
  sitePermissions: Record<string, ConnectorSitePermission>;
}

function loadOnboardingState(): Promise<ExtensionOnboardingStateV1>;
function requestConnectorSitePermission(
  connectorId: string,
  origins: string[],
): Promise<"granted" | "denied">;
function refreshConnectorSitePermission(
  connectorId: string,
): Promise<SitePermissionStatus>;
```

State is stored in extension-local storage, not page `localStorage`. Origin patterns come from a built-in, versioned connector manifest; UI or page content cannot supply arbitrary origins. Parsed records continue to enter Collector through `POST /api/v1/imports` as `Observation` values.

## 3. Behavioral Contracts

- The first onboarding screen explains persistent-per-site access, `activeTab` fallback, and revocation before showing grant buttons.
- Each grant button requests origins for exactly one connector. Never request all supported origins in one prompt.
- `browser.permissions.request` or its Chromium equivalent must execute within the grant-button user gesture.
- Skipping marks onboarding as skipped and closes it without requesting permissions.
- Skipped, denied, or revoked sites retain public-URL and `activeTab` features where the connector supports them. Background or batch features for that site remain disabled.
- The settings screen always reflects `browser.permissions.contains`, not only cached state, and offers grant/revoke controls.
- A denial produces an inline status and never starts an automatic prompt loop.
- Chrome and Edge use the same feature code. Browser-specific API differences stay behind a typed adapter.

## 4. Validation and Error Matrix

| Condition | Result | User-visible behavior |
|---|---|---|
| Connector is unknown | Reject before browser API call | `unsupported_connector` |
| Origin is absent from connector manifest | Reject before browser API call | `invalid_origin_scope` |
| Request lacks a live user gesture | Do not retry automatically | Keep grant button enabled with guidance |
| User denies the browser prompt | Save `denied` | Explain that `activeTab` remains available |
| Permission was revoked externally | Refresh to `revoked` | Disable background/batch mode for that site |
| Browser API is unavailable | Return adapter error | Offer current-page/manual import only |
| Onboarding state is corrupt | Reset to safe empty V1 state | Show onboarding without requesting anything |

## 5. Good, Base, and Bad Cases

**Good case:** The user reads the explanation, presses “Authorize Douban,” grants only Douban origins, skips the other sites, and later enables Xiaoyuzhou from settings. Only granted connectors expose background/batch controls.

**Base case:** The user skips onboarding. The extension can still parse a user-opened supported page through `activeTab` and send an observation to Collector.

**Bad case:** Extension installation immediately opens a combined Chrome permission prompt for every supported platform or repeatedly reopens the prompt after denial. This violates the contract.

## 6. Tests Required

- Unit-test onboarding migration, corrupt-state recovery, and completed/skipped triggers.
- Unit-test connector-origin allowlisting and one-connector-per-request behavior.
- Adapter-test Chrome and Edge permission request, contains, and revoke flows.
- UI-test that permission requests occur only after the matching grant-button click.
- UI-test skip, denial, external revocation, and later grant from settings.
- Integration-test that a site without persistent permission still supports the declared `activeTab` path.
- Assert that the manifest and code never request the `cookies` permission.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: broad install-time prompt, no user choice per connector.
await browser.permissions.request({ origins: allConnectorOrigins });

// Correct: called from one connector's click handler after explanation.
await requestConnectorSitePermission(connector.id, connector.allowedOrigins);
```

```ts
// Wrong: cached grant is treated as permanent truth.
return saved.status === "granted";

// Correct: settings and task execution re-check browser state.
return browser.permissions.contains({ origins: connector.allowedOrigins });
```
