# Shared WebUI Boundaries

## 1. Scope / Trigger Conditions

This contract applies to the WebUI embedded in desktop, served by `collector serve`, and bundled in Docker. All hosts serve the same built assets from Core and use the same `/api/v1` contract. Browser-extension UI remains a separate surface but shares generated API and domain types.

Implementation status: CLI-hosted status plus local session/client management exists. See [Runnable Foundation](../backend/foundation-runtime.md) and [Local Access](../backend/state-and-pairing.md). OAuth follows backend/oauth-http.md; mapping configuration follows [Mapping Editor](./mapping-editor.md). Preview, SSE and other host bundles remain future requirements. Public status endpoints require no session; client metadata and management require a session and mutation CSRF checks.

## 2. Signatures and Data Shapes

```ts
interface ApiError {
  code: string;
  message: string;
  field?: string;
  retryable: boolean;
}

interface CapabilityDocument {
  schemaVersion: number;
  connectorIds: string[];
  mediaTypes: string[];
  routes: Record<string, string>;
}

interface DryRunViewModel {
  importId: string;
  fields: Array<{
    field: string;
    sourceValue: unknown;
    neodbValue: unknown;
    status: "accepted" | "skipped" | "needs_review" | "rejected";
    reason: string;
  }>;
}
```

The WebUI discovers features from `GET /api/v1/capabilities`, uses same-origin `/api/v1` requests, and receives long-running progress through `/api/v1/events` SSE by default.

## 3. Behavioral Contracts

- Desktop, CLI WebUI, and Docker render the same application and domain behavior.
- API/domain types come from one schema-generated or shared package. Components do not redefine connector, observation, plan, or error payloads.
- Secrets, NeoDB tokens, client tokens, and pairing keys never enter `localStorage`, query strings, analytics, or error reports.
- WebUI sessions use HttpOnly SameSite=Strict cookies plus CSRF protection. A desktop bootstrap token may arrive in a URL fragment, is exchanged once, and is then removed.
- `apps/webui/src/session.ts` removes the fragment synchronously before any request, waits for Core compatibility, then starts one shared bootstrap promise outside React. StrictMode must not replay the key. Failed/expired auth directs the user to a fresh startup link; keys are not cached for retries.
- `AccessPanel` retains only in-memory CSRF and temporary pairing links. Hide links on expiry/logout; list/revoke through the generated client. Do not manually duplicate wire DTOs in components. `NeoDbPanel` shows authenticated account/pending status and supports instance entry, authorize/replace, refresh and cancellation; see [OAuth HTTP](../backend/oauth-http.md).
- UI styling extends the existing blue-grey status console with an inline access form; desktop/mobile layouts retain visible focus and no horizontal overflow. Tests capture screenshots only after hiding pairing keys.
- Mapping forms and YAML import/export round-trip through the same canonical profile schema and validator.
- Dry-run views show field-level source value, current NeoDB value, outcome, and reason before activation.
- UI never labels browser-online/session-dependent connectors as unattended scheduled automation.

## 4. Validation and Error Matrix

| Condition | Result | UI behavior |
|---|---|---|
| Capability schema version is unsupported | Stop feature rendering | Show upgrade-required state |
| API returns structured field error | Preserve form state | Attach error to field |
| SSE disconnects | Back off and reconnect | Show stale/reconnecting status |
| CSRF/session expires | Stop mutation | Re-bootstrap session without retaining secrets |
| Mapping YAML is invalid | Do not save/enable | Show normalized validator errors |
| Dry-run contains `needs_review` | Keep operation non-executable | Open review surface |
| Connector requires browser session but extension is offline | Do not claim scheduled success | Show browser-required status |

## 5. Good, Base, and Bad Cases

**Good case:** A user edits a mapping in the form, exports it as YAML, reimports it, and sees the same normalized rules and dry-run results on desktop and Docker WebUI.

**Base case:** The user opens the same-origin WebUI, receives an HttpOnly session, and reads connector status without any secret in JavaScript storage.

**Bad case:** Desktop UI calls Tauri-only business commands while Docker UI calls REST, or a component keeps the NeoDB token in `localStorage`. Both split the product contract.

## 6. Tests Required

- Pending fetch mocks must retain the incoming native Request until settlement,
  not only its signal. Node's Request owns a controller through a strong reference;
  its input-signal bridge uses a WeakRef. If the Request is collected, holding
  request.signal alone can lose cancellation and cause intermittent timeout tests.
  A local --expose-gc reproduction confirmed signal-only abort=false versus a
  retained Request abort=true. See core-client-ts/src/index.test.ts.

- Contract tests generated from or validated against the Core API schema.
- Round-trip tests for mapping form, canonical JSON, and YAML.
- Browser tests asserting no secret-bearing keys are written to `localStorage` or URLs.
- CSRF, expired-session, and one-time fragment-bootstrap tests.
- SSE reconnect and stale-state tests.
- Cross-host smoke tests for desktop, `collector serve`, and Docker using the same WebUI bundle.
- Accessibility tests for onboarding, review queue, errors, and keyboard-only mapping edits.

## 7. Wrong vs Correct Implementations

```ts
// Wrong: browser storage retains a credential.
localStorage.setItem("neodb_token", token);

// Correct: JavaScript relies on the HttpOnly same-origin session.
await api.getCapabilities({ credentials: "same-origin" });
```

```ts
// Wrong: UI invents a private API payload shape.
type LocalImportResult = { ok: boolean; msg: string };

// Correct: consume the shared generated contract.
import type { ImportResult } from "@neodb-collector/api-contract";
```
