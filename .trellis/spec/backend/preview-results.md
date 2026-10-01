# Persistent preview review results

## 1. Scope / Trigger

D2b1 of `09-16-preview-jobs` extends the internal job repository with bounded
normalized review results and conditional completion. The migration and compatible
repository ship together. Runtime worker/startup/retention composition and public
API/client delivery remain D2b2/E; a completed preview is not a write approval.
Owners: `crates/collector-core/src/preview_results.rs`,
`crates/collector-storage/src/preview_results.rs` and their `tests.rs`.
[Single-attempt execution](./preview-runner.md) consumes these operations internally;
scheduled runtime ownership and public delivery remain pending.

## 2. Signatures

Internal contracts in `collector-core/src/preview_results.rs`:

- `PreviewResultWrite::new(job_id, ordinal, attempt, committed_at, PreviewRecord)`
  constructs a validated write containing a private service record/input proof.
- `PreviewResultRepository::store(PreviewResultWrite) -> PortFuture<bool>`.
- `results(job_id, attempt, offset, limit) -> PortFuture<Vec<StoredPreviewResult>>`.
- `complete(job_id, attempt, now) -> PortFuture<bool>`.
- `StoredPreviewResult` / `StoredPreviewOutcome` are historical review types only.
  Compared retains item evidence, snapshot observation time, Comparison and
  automatic/manual provenance; other outcomes preserve their normalized evidence.

No HTTP/SQLx/serde/runtime type enters the Core port. Encoders/decoders and SQLite
implementation belong to `collector-storage/src/preview_results.rs`.

`0005_preview_results.sql` upgrades the v4 jobs/observations tables to allow
`completed` and adds `(job_id, ordinal, attempt, committed_at, result_json)`.
The job/ordinal pair uniquely references an original observation. Result JSON is
normalized operational evidence, not an upstream reply or executable operation.

Private Storage JSON schema version 1 uses camelCase metadata: `schemaVersion`,
`sourceConnectorId`, `sourceRecordId`, `sourceFingerprint`, `profileId`,
`profileVersion`, `baseUrl`, `account` and `outcome`. Ordinal, attempt and commit
time remain relational columns. `outcome.kind` is one of `compared`,
`needs_review`, `search_candidates`, `unsupported`, `rate_limited`, `unavailable`:

- Compared stores normalized `item`, `observedAt`, `comparison` and
  `provenance` (`automatic` / `manual`), not a deserializable fresh snapshot.
- NeedsReview stores `item` and a supported identity/granularity `reason`.
- SearchCandidates stores normalized title/item candidates and `moreAvailable`.
- Item stores UUID, canonical/detail URLs, media type/model, optional parent UUID
  and sorted external ID pairs. Comparison stores profile ID/version and ordered
  rule ID/field/suggestion/sink/status/reason entries. Sink kind is `known`
  (with scalar `value`), `empty` or `unknown`; statuses use snake_case.

Serde DTOs reject unknown fields and unsupported variants. Optional parent UUID
and suggestion may deserialize as absent/null; emitted JSON includes the keys.
This is an internal persistence format, not a generated/public API contract.

## 3. Contracts

- A write consumes an immutable service-produced record. Carry a private bounded
  full-input equality proof in memory: the exact normalized Observation and disabled
  MappingProfile, not only their IDs/version/fingerprint. Never serialize that
  proof, duplicate profile/source payloads in result JSON, or expose credential
  references in historical results, diagnostics or public DTOs. Internal write
  proof getters are trusted adapter inputs, like the existing Endpoint port.
- Decoded historical review types must remain distinct from AccountMarkSnapshot
  and PreviewRecord. A load cannot construct a fresh-read record or execute writes.
- Mutations use SQLx-managed `BEGIN IMMEDIATE`. In the transaction verify a running
  job, exact attempt/ordinal, non-regressing time before expiry, current full
  endpoint/account/credential reference, exact valid profile and observation proof.
  Old workers, deleted jobs and duplicate ordinal inserts cannot overwrite/create
  evidence. Hold no transaction over network/OS credential reads.
  Successful insertion advances `updated_at` atomically. Duplicate writes do not
  advance time or replace evidence. Audit existing result extent before adding a
  new row: all attempts/times/ordinals/counts/byte totals must agree with the job.
- Reuse catalog identity and comparison types/normalization. Preserve identity
  claims/granularity, automatic/manual provenance, snapshot time and field-level
  suggestions, Known/Empty/Unknown sink states, statuses and reasons. Unresolved
  results carry their original outcome/evidence only, without invented sink state.
- Encode in Storage with existing serde/serde_json and private strict DTO derives,
  not custom generic JSON casting. Direct Storage serde reuse does not change
  versions/features/transitive capabilities or Core's allowlist. Strictly validate
  fields, variants, finite supported scalar values, bounded evidence and snapshot
  metadata on read.
  Reject unknown or malformed data with static diagnostics; never return part of
  a corrupt page or label historical reads current after account replacement.
  Replay the existing mapping evaluator and shared `preview::compare_mapped_field`
  helper to validate rule order and comparison semantics, not a second conflict
  policy implementation. Reject repeated rule IDs, inconsistent same-target sinks
  and impossible shelf/rating combinations. Suggestions allow finite ratings
  0..10; known remote ratings retain C1's integer 1..10 rule.
- Cap each encoded result at 64 KiB and aggregate result JSON at 32 MiB per job,
  separately from observation byte budgets. Validate encoded UTF-8 byte lengths,
  including JSON escaping, and reject excess without truncating evidence.
- Completion rechecks the same live attempt/time/account/profile guard and requires
  a valid result for every original ordinal. A zero-observation job may complete
  only after claim. Completion means processing finished; review-required and
  unresolved outcomes do not become matches, accepted writes or activation.
- Cancel/fail/interruption retain partial historical rows. A successful explicit
  resume deletes prior results in the same transaction as attempt increment and
  requeue. Resume neither extends expiry nor reuses an old comparison.
- Delete and expiry cleanup atomically remove results, observations and job only;
  retain profiles, account metadata, authentication rows and credentials. Late
  insertion/completion cannot resurrect deleted data.
- Reads use one transaction for job/profile/observation/result agreement and stable
  zero-based ordinal pagination, with page size 1..100. Missing jobs and obsolete
  requested attempts return no rows. Do not substitute latest profile revisions.
- Migration preserves populated v4 operational data and supports every committed
  prior version. Rebuild the constrained tables transactionally; do not modify v4,
  disable foreign keys or leave child references pointing at a dropped old table.
  Newer-schema preflight still refuses without modifying database bytes.

## 4. Validation & Error Matrix

| Condition | Required outcome |
|---|---|
| Invalid write metadata | `preview_result_invalid` before repository I/O |
| Invalid attempt/time or page limit | Existing `preview_transition_invalid` / `preview_page_invalid` |
| Stale status/attempt, cancelled/recovered/deleted/expired job | `false`; no result or completion accepted |
| Changed live endpoint/profile or mismatched full producer inputs | `false`; results/status unchanged |
| Duplicate ordinal result | `false`; no overwrite; concurrent writers have one winner |
| Missing ordinal result at completion | `false`; no completed transition |
| Corrupt result extent or decoded JSON/binding/comparison | `database_unavailable`; no affected mutation/read/completion |
| Encoded result/aggregate bytes over budget | `preview_result_too_large` / `preview_results_too_large`; no insertion |
| Invalid/corrupt historical JSON or metadata | `database_unavailable`; no partial page |
| Result insertion/resume/delete fails within transaction | Whole mutation rolls back |
| Empty claimed job | Conditional completion succeeds without fabricated rows |
| Loaded historical review evidence | Inspection only, never fresh snapshot/write authority |

Store encodes and checks the single-result size before opening a transaction;
oversize encoding can therefore report its bounded error before a stale-job guard.
Missing/corrupt selected profiles or observations are database errors, not silently
treated as a changed valid profile/input. Extent auditing is structural; JSON and
semantic corruption fail when that row is read and during exhaustive completion.

## 5. Good / Base / Bad Cases

Good: attempt 1 has partial results, explicit resume atomically removes them and
starts attempt 2; a late attempt-1 record is refused. Base: Unsupported remains
historical Unsupported even when the batch completes. Bad: reusing a serialized
comparison as a fresh record, or accepting a changed profile with the same ID/version.

## 6. Tests Required

Use the real single-record producer with fake catalog, mark, identity, secret and
clock ports; no live NeoDB/account/keychain. Cover all outcomes, provenance and
unknown/empty semantics; exact profile/observation proof, reopen, ordinal pages,
duplicate writers, cancel/fail/recovery/resume/delete/expiry/account replacement,
clock regression, exhaustive/empty completion and immutable partial evidence.
Inject transaction failures for insertion/resume/delete, result timestamp updates
and completion; reject malformed/oversized stored JSON. Test 64 KiB exact/+1 and
JSON escaping, 32 MiB aggregate boundaries, sparse pages and damaged later rows.
Upgrade populated v4 plus v1-v3 fixtures; check foreign keys, ledger,
existing operational rows, newer-schema refusal and absence of raw/token columns.
Run scoped Core/Storage tests, fmt/Clippy, `pnpm verify`, product YAML/evidence
validation and `git diff --check`. Report local Windows evidence separately from
hosted three-platform CI.

## 7. Wrong vs Correct

Wrong: read job status, insert output, then mark completed in separate writes.
Correct: guarded transactional result writes and a separately guarded exhaustive
completion check; no late result can bypass cancellation or attempt replacement.

Wrong: deserialize historical JSON into the private service-produced record.
Correct: a distinct validated historical representation with no fresh-read or
execution authority; resume always obtains new remote evidence.
