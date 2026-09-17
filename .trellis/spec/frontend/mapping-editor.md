# Mapping Configuration Editor

## 1. Scope / Trigger

`apps/webui/src/MappingPanel.tsx` is mounted within AccessPanel only after local
session authorization. It edits disabled profiles, not source records, previews,
activation or schedules. The existing blue-grey console style remains shared by
all hosts, with responsive source-to-target rule rows and explicit save status.

## 2. Signatures

`MappingPanel({client: AccessClient})` uses the shared generated profile/format
types re-exported by core-client-ts. Calls: defaultMapping, mappingProfile,
validateMapping, saveMapping, convertMapping. HTTP contracts live in
`backend/mapping-http.md`; no hand-maintained wire models or browser YAML parser.

## 3. Contracts

- Defaults come from Core, not a frontend copy. Rating-only is default; completion
  requires explicitly checking the completion rule. No watched-date inference.
- Form edits profile ID/source connector and per-rule enabled/target fields.
  Full allowlisted operation arguments and rule lists are editable through the
  JSON/YAML text surface. Import validates through Core before replacing the form.
- Form and text are intentionally separate drafts. Buttons explicitly identify
  direction: validate/import text, export form, read/replace form, save disabled
  version. A failed import/save preserves the existing form and source text.
- Save uses the last loaded/saved version for the same ID as expectedVersion,
  otherwise 0. Proposed version is expected+1; an imported version number cannot
  claim a newer baseline. On conflict do not auto-refresh/retry/overwrite.
- Export calls Core conversion; clean saved profiles retain their version, dirty
  forms export their proposed version. Conversion/import does not persist.
- Files are UTF-8 (fatal decoder), at most 64 KiB before reading. Selecting a file
  populates text only; the user then validates/imports. Download uses a transient
  Blob URL and revokes it; draft configuration never enters browser storage.
- In-flight actions disable editing and serialize buttons; aria-busy and live
  statuses identify pending/success. Errors display sanitized codes/field paths,
  never upstream raw messages. Failed saves are not claimed as confirmed failures
  to persist: users can reload after exporting their draft.
- Logout unmounts the editor. Session-expiry errors leave the current draft visible
  for copying; no media write authority is implied by a loaded profile.

## 4. Validation & Error Matrix

| Condition | UI behavior |
|---|---|
| No selected profile | Read saved ID, use defaults or import text |
| Missing saved profile | Explain absence; no implicit creation |
| Invalid imported rules/arguments | Show field path when available; preserve both drafts |
| enabled=true | Reject; explain disabled-only configuration |
| Stale version | Keep draft; suggest export, explicit reload and reconciliation |
| Oversize file/document | Reject with size explanation |
| Auth/CSRF expired | Preserve draft and request reauthorization |
| Network/save uncertainty | Do not claim success; advise reading saved state |

## 5. Good / Base / Bad Cases

Base: fetch Core defaults, enable completion explicitly, save disabled v1.
Good: export YAML, import it, save v2, reload and see the same rules. Bad: taking
version=99 from imported YAML as permission to overwrite another editor's work.

## 6. Tests Required

MappingPanel.test.tsx tests default/opt-in saves, imported-version distrust,
conflict preservation, invalid import field diagnostics and conversion without
saving. Backend route tests verify actual codec round trips and no persistence.
`tests/smoke.mjs` uses actual local CLI/Core/SQLite and Chromium to save, export,
download, upload, import, revise and reload, plus desktop/mobile screenshots,
overflow checks, logout and empty browser storage. OAuth status alone is mocked
there to avoid touching the native keychain. Run full pnpm verify.

## 7. Wrong vs Correct

Wrong: overwrite an existing configuration after incrementing to whatever version
the server currently reports. Correct: preserve the original expectedVersion and
let the atomic repository reject stale saves; users reconcile edits explicitly.
