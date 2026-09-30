# Account-bound preview snapshots and comparison

## 1. Scope / Trigger

Internal checkpoint C2 under `09-16-preview-jobs`. Core composes existing endpoint,
credential, clock, OAuth identity and personal-mark ports, then compares suggestions
using the existing mapping evaluator. No database migration, API DTO, job, writer,
activation or new dependency. Transport evidence remains in `neodb-mark-read.md`.

## 2. Signatures

`crates/collector-core/src/preview.rs` owns:

- `PreviewSnapshotService::new(repository, secrets, clock, identity, marks)` using
  `Arc<dyn AccessRepository/SecretStore/Clock/NeoDbOAuthTransport/NeoDbMarkTransport>`.
- `async read(expected: Endpoint, resolved_item: CatalogItem)` returns
  `Result<AccountMarkSnapshot, ApiError>` and takes ownership of resolved evidence.
- `async ensure_current_binding(&AccountMarkSnapshot) -> Result<(), ApiError>`.
- `async ensure_expected_binding(&Endpoint) -> Result<(), ApiError>` exposes the
  same point-in-time guard for [single-record composition](./preview-records.md),
  without a secret lookup or endpoint mutation.
- Snapshot read-only accessors: `base_url()`, `account()`, `observed_at()` (Unix
  seconds), `item()`. State and credential binding have no public constructor.
- `compare(&MappingProfile, &Observation, &AccountMarkSnapshot)` returns
  `Result<Comparison, ApiError>`; output has `profile_id`, `profile_version`, `fields`.
- `ComparedField { rule_id, field, suggestion: Option<ScalarValue>, sink: SinkValue,
  status: ComparisonStatus, reason }` retains mapped suggestions, not raw source data.
- `SinkValue::{Known(ScalarValue), Empty, Unknown}` and
  `ComparisonStatus::{Accepted, Skipped, Rejected, NeedsReview}`.

Endpoint includes the opaque credential reference and is trusted internal input,
never a client request body. These types are internal, not wire DTOs; no public
API capability is advertised. Comparison has no execution/activation operation.

## 3. Contracts

- Compare base/account/credential-reference equality before secret lookup, after
  lookup before remote I/O, and after remote I/O before accepting a snapshot.
  Same-account reauthorization still invalidates the old binding: OAuth produces
  a fresh random token reference. No implicit rebind or retry on a replacement.
- Read only through existing ports. Borrow SecretString for mark/identity reads;
  never call secret set/delete or repository mutation. Mark errors, credential
  errors and repository errors return no partial snapshot or guessed absence.
- Present marks must match the expected owner. MissingShelf requires the same
  token's `/api/me` identity to match before acceptance because its response has no
  owner. Reuse that OAuth read, not another HTTP implementation.
- The injected built-in mark adapter owns same-instance item and field-schema
  validation under C1. Core does not duplicate its URL parser or accept unvalidated
  wire replies through this port. A mocked implementation must honor that contract.
- Snapshot includes observation time, base/account and the original resolved item
  evidence. Do not replace richer resolution evidence with reduced embedded mark
  metadata. Credential references are private and absent from diagnostics and any
  future client DTO. Do not derive snapshot Serialize or leaking Debug.
- A later current-binding check is point-in-time only; pure comparison does not
  perform I/O or promise that a snapshot remains current. Future job result commits
  and execution need their own conditional freshness checks. No mutex/SQLite
  transaction is held over network requests.
- Dropping a service future prevents subsequent local calls. It does not undo an
  already-started OS credential-store blocking read. Runtime cancellation and job
  scheduling stay behind runtime adapters; do not build another future-racing loop.
- Reuse `mapping::evaluate` and preserve its validation/errors and rule ordering.
  Mapping accepted means a typed suggestion, not an executable operation. Preserve
  skipped/rejected reasons without inventing source values.
- Sink state has three cases: known value, confirmed empty and unknown. Equal
  mapped/sink values skip; unequal nonempty values need review with sink preserved;
  confirmed empty permits a suggestion; unknown always needs review.
- MissingShelf means confirmed empty shelf and unknown rating, never an empty
  rating. An explicit shelf suggestion cannot resolve that uncertainty. A rating
  without an existing or accepted suggested shelf is not ready. Determine shelf
  availability across all rules so rule order cannot change the decision.
- Keep numeric mapping behavior unchanged: no rounding, zero/null equivalence or
  write encoding is inferred by comparison. Bind output to profile ID/version.
  All results are non-executable. Calling code must have established observation-to-
  catalog identity through resolution; comparison is not another matching engine.

## 4. Validation & Error Matrix

| Condition | Required outcome |
|---|---|
| Disconnected tuple at any check | preview_account_disconnected |
| Changed tuple before/after I/O or later check | preview_binding_changed; no implicit rebind |
| Credential / repository read error | preview_credential_unavailable / preview_binding_unavailable |
| Mark / identity transport error | Existing sanitized transport error, no partial snapshot |
| Wrong mark owner or `/api/me` account | preview_account_changed, not absence |
| Equal known value | Skipped: already_equal |
| Unequal nonempty value | NeedsReview: sink_conflict_needs_review |
| Confirmed empty value | Accepted: sink_empty_accepts_suggestion |
| Rating without shelf | NeedsReview: rating_requires_shelf |
| Unknown with shelf suggestion | NeedsReview: sink_state_unknown |
| Invalid profile or observation | Existing evaluator error |
| Disabled/missing/rejected mapping rule | Preserve evaluator status/reason |

## 5. Good / Base / Bad Cases

Good: matching account has complete shelf and no rating; mapped 8 is a suggestion.
Base: matching rating 8 is skipped; a sink rating 9 versus mapped 8 needs review.
Bad: MissingShelf plus explicit complete mapping is used to assume no existing
rating and approve overwriting it. The rating remains unknown and needs review.

## 6. Tests Required

`crates/collector-core/src/preview/tests.rs` uses fake repository, secret store,
clock and transports: no remote account or OS store.
Assert original evidence, observation time, same-token identity checks only when
needed, every binding boundary including same-account token rotation, disconnected
and failed reads, later staleness, no mutation and dropped-future short-circuiting.
Assert all supported shelves, empty/equal/conflicting/unknown fields, explicit
shelf suggestions in either order, mapping validation and skipped/rejected reasons.
Run Core tests, full `pnpm verify`, product YAML validation and `git diff --check`.

## 7. Wrong vs Correct

Wrong: compare `Option<rating>` after converting every failed or missing read to None.
Correct: errors stop snapshot creation; MissingShelf retains Unknown; only a valid
present mark with explicit null establishes an empty rating.

Wrong: treat a successful current-binding check as indefinite execution authority.
Correct: label the snapshot historical, recheck at later consumption/commit boundaries
and keep this comparison layer unable to execute any write.
