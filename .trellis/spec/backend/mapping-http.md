# Mapping Configuration HTTP

## 1. Scope / Trigger

Implemented in `collector-server/src/mapping_http.rs`, composed with the SQLite
MappingRepository by CLI startup. Shares existing same-origin/Host security headers
and access credentials. These routes configure disabled profiles only; no importer,
dry-run, activation, catalog fetch or user-state writes are invoked.

## 2. Signatures

- POST `/api/v1/mapping-profiles/validate`: MappingDocument -> MappingProfile.
- GET `/api/v1/mapping-profiles/{id}`: latest saved MappingProfile, or 404.
- PUT `/api/v1/mapping-profiles/{id}`: SaveMappingProfile -> saved MappingProfile.
- GET `/api/v1/mapping-profiles/defaults/imdb`: unsaved Core default MappingProfile.
- POST `/api/v1/mapping-profiles/documents/convert`: ConvertMappingDocument -> MappingDocument.
- Domain owns DTOs, OpenAPI/JSON Schema/TS validators are generated.
- `createAccessClient`: `validateMapping(body)`, `mappingProfile(id)`,
  `saveMapping(id, body)`, `defaultMapping()` and `convertMapping(body)` use generated paths and validate responses.

## 3. Contracts

All request bodies are JSON envelopes. MappingDocument has `format: json|yaml`
and `document: string`; SaveMappingProfile adds `expectedVersion: uint32`.
The document is parsed by the same bounded strict codec for both actions; an
editable JSON object is serialized to text by the client. This deliberately avoids
letting generic JSON BTreeMap parsing silently swallow duplicate enum keys.
Unknown envelope fields are rejected. Body cap is 512 KiB to allow JSON string
escaping; decoded document cap remains 64 KiB. Existing access routes retain 4 KiB.

ConvertMappingDocument has `input: MappingDocument` and `targetFormat: json|yaml`.
Conversion uses the shared decode/validate/encode path, does not persist or enable,
and requires the same read authorization as validation. Defaults likewise do not
persist. There is no second set of default business rules embedded in WebUI.

GET and validation require a WebUI session or valid paired Bearer with mapping.read;
save requires mapping.write, or a session with same Origin and correct CSRF.
Validation is side-effect-free and does not require mutation CSRF. Current paired
tokens all have fixed PAIRING_SCOPES; no per-client/custom scope grant is implied.
Authorization presence selects Bearer only: malformed/duplicate/revoked Bearer
headers cannot fall back to a privileged cookie. Every route remains no-store.

Save checks path ID equality and expected+1, then the repository atomically checks
the latest revision. Success returns the normalized **disabled** saved profile.
Validation never persists; missing GET never creates a default. IDs follow the
Core ASCII grammar, including the valid ID `validate`: static POST action routing
must not shadow GET/PUT for that resource name.

AJV recognizes Rust's `double` as an annotation alongside uint32/int64; strict
number checks still reject Infinity/NaN. Generated validators check wire shape;
Core remains the authority for operation ranges, target capability and activation.
TS client keeps existing in-memory CSRF and sanitized error codes; conflicts are
not automatically retried with a new version.

## 4. Validation & Error Matrix

| Condition | HTTP / code |
|---|---|
| Missing/invalid/revoked authentication | 401 / authentication_required |
| Missing Origin/CSRF on session save | 403 / origin_not_allowed or csrf_invalid |
| Invalid envelope | 400 / request_invalid |
| Invalid profile | 400 / Core or codec validation code |
| Path/profile ID mismatch | 400 / mapping_identifier_mismatch |
| Latest version changed | 409 / mapping_version_conflict |
| Missing profile | 404 / mapping_profile_not_found |
| Body/document too large | 413 / request_invalid or mapping_document_too_large |
| Missing repository / failed database | 503 / mapping_unavailable or database_unavailable |

## 5. Good / Base / Bad Cases

Good: validate YAML without saving, then save v1 with expectedVersion=0. Base:
GET unknown profile returns 404. Bad: silently re-submit a stale edit at the newer
version, thereby defeating the expected-version protection.

## 6. Tests Required

- `collector-server/src/mapping_http_tests.rs`: router/real SQLite validation,
  save/read/history/conflict, action-name collision, session CSRF, Bearer fallback
  denial and revocation, invalid documents/envelopes, caps, no persistence on error.
- `packages/core-client-ts/src/mapping.test.ts`: generated request/response wiring,
  save-only CSRF, wire rejection/nonfinite doubles, sanitized conflict errors.
- Router tests additionally prove default/convert authorization, full YAML/JSON
  round trips and no persistence. MappingPanel tests consume the generated client.
- Full `pnpm verify`: contracts, boundaries, Rust/TS/UI and local smoke. No live
  NeoDB accounts or native credential-store test is implied.

## 7. Wrong vs Correct

Wrong: `Json<MappingProfile>` bypasses strict import parsing and allows duplicate
enum keys to collapse before validation. Correct: decode the document text with
the shared codec, then apply Core validation and atomic repository save.
