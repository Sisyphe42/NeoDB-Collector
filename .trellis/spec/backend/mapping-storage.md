# Mapping Profile Revision Storage

## 1. Scope / Trigger

Implemented operational configuration persistence. Core declares the repository
port; Storage implements it with SQLite. [Mapping HTTP](./mapping-http.md) owns
authorization and exposes configuration routes. No UI or platform writes are
implemented by this storage layer.

## 2. Signatures

`collector_core::mapping::MappingRepository`:

- `load(id: String, version: Option<u32>) -> PortFuture<Option<MappingProfile>>`
- `save(profile: MappingProfile, expected_version: u32) -> PortFuture<()>`
- Shared policy: `validate_revision(&MappingProfile, expected_version) -> Result<(), ApiError>`.

`0003_mapping_profiles.sql` adds `mapping_profiles(id TEXT, version INTEGER,
profile_json TEXT, PRIMARY KEY(id, version))`. Version is 1–u32::MAX; normalized
JSON has a 65,536-byte SQL constraint matching Domain's `MAX_PROFILE_BYTES`.

## 3. Contracts

- `expected_version=0` creates version 1; otherwise the proposed version must be
  exactly expected+1. Overflow is rejected. Caller chooses no implicit overwrite.
- Storage revalidates profiles before serialization: disabled only, declarative
  supported rules, bounded fields. It stores normalized JSON, not imported YAML,
  comments, observations, source payloads or tokens.
- One `INSERT ... SELECT ... WHERE COALESCE(MAX(version),0)=expected` statement
  atomically checks and appends. Competing writers cannot both succeed. Historical
  versions are never updated/deleted by this port.
- `load(id, None)` returns latest; `Some(version)` returns that exact historical
  revision. Missing records return None. Loaded profiles are revalidated and their
  embedded ID/version must match the selected row. Corruption fails closed.
- New revisions remain disabled; no activation or scheduling tables exist yet.
  Future preview/approval records must reference an exact immutable version.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Invalid or enabled profile | Existing Core validation code; no write |
| Proposed version not expected+1 / overflow | mapping_version_invalid |
| Latest version differs from expected | mapping_version_conflict; reload required |
| Serialized profile exceeds shared byte cap | mapping_document_too_large |
| SQLite failure or corrupt stored profile | database_unavailable; no raw diagnostics |
| Requested revision absent | None, not an implicit default |

## 5. Good / Base / Bad Cases

Base: save v1 with expected=0, then load latest. Good: two editors save v2 from v1;
one succeeds and one receives a conflict, while v1 remains available for previews.
Bad: overwriting v1 would change the meaning of an existing preview or approval.

## 6. Tests Required

`crates/collector-storage/src/mapping.rs` covers reopen/history, exact/missing
revision reads, concurrent creation/update, disabled-only saves, stale/invalid/
overflow versions, injected write failure preserving old data, normalized byte
limits, v2 migration preserving account metadata and corrupt-profile rejection.
`src/lib.rs` covers empty/v1/v2/v3 migration, reopen tables and newer-schema refusal.
Prior-schema fixtures remove all later preview tables and migration-ledger entries,
not just `mapping_profiles`. Preview jobs retain exact immutable revisions rather
than selecting latest; see [Persistent Preview Jobs](./preview-jobs.md).
Run `pnpm verify`; SQLite fixtures do not use live accounts or OS credentials.

## 7. Wrong vs Correct

Wrong: SELECT latest, compare in application code, then unconditional INSERT/UPDATE.
Correct: compare-and-append in one SQLite statement; the primary key preserves
immutable history and a stale client gets an explicit conflict.
