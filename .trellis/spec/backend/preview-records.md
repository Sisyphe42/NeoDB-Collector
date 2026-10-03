# Single-record preview orchestration

## 1. Scope / Trigger

D2a under `09-16-preview-jobs` composes the existing catalog resolvers,
account-bound snapshot service and pure field comparison in Core. This is the
internal record producer consumed by the [result repository](./preview-results.md),
not a running worker or completed job. D2a itself has no migration, HTTP/client
shape, activation or new dependency.
Source owners: `crates/collector-core/src/preview_records.rs` and its `tests.rs`.
[Single-attempt execution](./preview-runner.md) invokes this producer sequentially
for claimed jobs; [owned runtime](./preview-runtime.md) supplies admission, startup
recovery and retention cleanup. Public import/history/management APIs, thin
clients, CLI commands and preview screens are implemented. The batch runner is
native by default with immutable per-record explicit search or manual inputs: public
search/manual selection and broader integrated acceptance remain pending;
internal resolution modes do not imply client delivery.

## 2. Signatures

- `PreviewRecordService::new(Arc<PreviewSnapshotService>)`.
- `async preview(transport, control, PreviewRecordRequest) -> Result<PreviewRecord, ApiError>`;
  transport implements the existing catalog and search ports, control implements
  `ResolutionControl`. No runtime/HTTP/database types enter Core.
- `PreviewRecordRequest<'a>` borrows `expected: &Endpoint`, `profile: &MappingProfile`,
  `observation: &Observation`, `identity: &CatalogIdentity` and owns `mode`.
  These are the expected internal endpoint, disabled profile, validated observation
  and trusted adapter identity, not unvalidated HTTP input.
- `PreviewResolutionMode<'a>` is `Native`, `Search { query: &str }` (native first),
  or `Manual { reference: &str }` (fresh selected detail only).
- `PreviewSnapshotService::ensure_expected_binding(&Endpoint)` reuses its existing
  full tuple guard without reading a credential or changing endpoint metadata.
- Record getters are `source_connector_id`, `source_record_id`, `source_fingerprint`
  (the observation idempotency key), `profile_id`, `profile_version`, `base_url`,
  `account`, `is_bound_to(&Endpoint) -> bool` and `outcome() -> &PreviewRecordOutcome`.
  Construction/binding are private and no Debug/serde is implemented.
- `PreviewRecordOutcome::Compared(Box<ComparedPreviewRecord>)` owns the original
  `snapshot: AccountMarkSnapshot`, `comparison: Comparison` and
  `provenance: PreviewRecordProvenance::{Automatic, Manual}`. The box avoids a large
  enum variant; callers get an immutable borrow through the record accessor.
  Other outcomes retain NeedsReview evidence/reason, review-only candidates,
  Unsupported, RateLimited or Unavailable.

These are internal Rust types, not additional public API DTOs. The output carries
no execution operation or credential-reference accessor.

## 3. Contracts

- Reuse `preview_jobs::validate_imdb_observation` and `mapping::evaluate` before
  I/O. Expectation must agree with observation canonical URL, media type and its
  external scheme/ID. Model is trusted built-in adapter output from
  `collector-connectors/src/imdb.rs::catalog_identity`, not a wire matching override.
  Keep platform/media identifiers open strings; do not add platform enums.
- Reject invalid explicit search queries and blank or over-2048-byte manual
  references before I/O and before allocating the private manual-request proof.
  Same-instance URL/reference validation remains the restrictive transport's
  responsibility; do not duplicate its URL parser in Core.
- Resolve through existing native/manual/search entrypoints. Search is explicit
  and Unsupported-only; never select even an exact-ID singleton automatically.
  A title hint is not an implicit search query or identity proof. Manual mode is
  invoked by a trusted later caller only after explicit user selection.
- One outer checked 120-second deadline uses `RESOLUTION_BUDGET` and the existing
  `ResolutionControl::run`. It covers binding, catalog, credentials, mark/identity
  reads and publication. Inner catalog budgets/poll spacing remain unchanged and
  cannot extend this deadline. No custom production future race, retry or scheduler.
- Cancellation/expiry wins over ready work and drops pending futures. No later
  calls occur after failure/interruption. This cannot undo queued public catalog
  creation or an OS credential read already running in a blocking adapter.
- Require exact base/account/credential reference before catalog, after resolution
  before account reads, and before publication of any outcome. Disconnection or
  same-account token rotation stops output. Hold no transaction/mutex over I/O.
- Only Matched/ManuallySelected proceeds to fresh account snapshot and comparison.
  Every unresolved outcome has zero credential/mark/identity calls and no sink
  field values. Errors return no partial record or invented absence.
- Reuse `preview::compare`: preserve source suggestions, sink Known/Empty/Unknown,
  field statuses/reasons, original identity evidence and snapshot time. MissingShelf
  still means unknown rating, not confirmed emptiness. Manual provenance is not
  automatic external-ID equivalence.
- Records are immutable historical evidence for one source fingerprint/profile
  revision/account generation. No raw payload, token, completed-job state or write
  authority is produced. D2b1 adds a private in-memory proof of the exact normalized
  observation/full profile for result insertion; E4a also records the normalized
  explicit search query; E4b also retains the exact requested manual reference.
  Native, Search and Manual are distinct input modes even if their outcomes agree.
  Compare the manual request, not a possibly redirected final item URL. Never
  persist that raw request as job input: jobs retain a validated compact catalog
  reference and derive a safe relative detail path. That proof is never serialized.
  The result write exposes it only to trusted storage adapters. Result transactions must recheck live job attempt,
  account/profile and expiry; this service cannot make those future checks atomic.

## 4. Validation & Error Matrix

| Condition | Outcome |
|---|---|
| Unsupported normalized observation | `preview_observation_invalid`; no I/O |
| Invalid/enabled/mismatched mapping | Existing evaluator error; no I/O |
| Expectation differs from source observation | `catalog_identity_invalid`; no I/O |
| Invalid query / blank or over-2048-byte manual reference | Existing resolver validation error; no I/O |
| Stale/disconnected/unavailable account binding | Existing snapshot binding error; no record |
| Native exact identity / explicit valid manual selection | Fresh snapshot + pure comparison, distinct provenance |
| NeedsReview / search candidates / terminal outcome | Preserve outcome, no personal-state read |
| Credential/mark/identity error | Existing sanitized error, no partial/empty result |
| Cancellation / outer deadline / overflow | Existing resolution cancellation/timeout error; no subsequent calls |

## 5. Good / Base / Bad Cases

Good: an explicit manual selection is freshly read and compared, preserving manual
provenance and the full catalog evidence. Base: Unsupported without opt-in search
stays Unsupported without accessing a token. Bad: a search candidate becomes a
matched record, or cancelling catalog work still allows a later mark read.

## 6. Tests Required

`crates/collector-core/src/preview_records/tests.rs` uses fake ports/clock/control,
not real network/OS credentials. Assert automatic/manual provenance, retained
evidence/metadata, unknown/empty/conflict, Unsupported-only search and zero credential/
mark/identity calls for all unresolved variants. Validate inputs before I/O and replace/disconnect
the full tuple at every boundary. Cover port failures, dropped futures, cancellation
and slow/hung work during binding/catalog/credentials/marks/identity/publication.
`invalid_inputs_fail_before_binding_or_catalog_io` includes the E4b 2049-byte
manual-reference rejection before binding/catalog/credentials are touched.
Run scoped Core tests/fmt/Clippy, full `pnpm verify`, YAML and `git diff --check`.

## 7. Wrong vs Correct

Wrong: independently race each phase with a new deadline and return a candidate
as a compared item. Correct: one outer guard bounds the composition, the existing
resolver owns identity classification, and only confirmed/manual resolution reads
personal state.

Wrong: publish a row and treat it as a completed persistent job. Correct: keep
publication historical and non-executable; job/result commits use the separate
conditional repository boundary with its compatible migration and tests.
