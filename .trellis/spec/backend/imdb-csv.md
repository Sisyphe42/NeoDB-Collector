# IMDb ratings CSV adapter

## 1. Scope / Trigger

Implemented in collector-connectors/src/imdb.rs with csv 1.4.0 and SHA-256.
Domain Observation/ObservedItem/ScalarValue live in collector-domain/src/observation.rs.
The pure adapter is reused by [Preview Submission HTTP](./preview-submission-http.md)
for normalized operational job storage. [CLI import](./cli-preview.md) and
[client screens](../frontend/preview-panel.md) are delivered and synthetically
accepted; see [foundation evidence](./foundation-runtime.md#accepted-synthetic-preview-scope-2026-10-09).
An enabled registry entry and public general observation intake remain deferred.

## 2. Signatures

`parse_ratings(input: &[u8], observed_at: &str) -> Result<ImportBatch, ImportError>`

ImportBatch contains observations, skipped_rows (line/reason), skipped_columns
(header names only) and duplicate_rows count. ImportError contains only stable
code, physical line and optional known field name. No raw values in diagnostics.

## 3. Contracts

- At most 10 MiB UTF-8 (optional leading BOM) and 10,000 data records, counting
  duplicates and skipped types. Blank lines are ignored by the CSV reader.
- Exact unique header names; Const, Your Rating and Title Type required.
  Title is optional. Header order is arbitrary; unsupported columns are reported,
  and their cell values are discarded. Date Rated is not watched date.
- csv parses escaped quotes/commas/multiline fields; a strict quote preflight rejects
  otherwise tolerated malformed quoting. Column count must match the header.
- movie/tvMovie/short -> movie; tvSeries/tvMiniSeries -> tv. All other types are
  skipped before field validation. Supported IDs are tt followed by 7–16 digits;
  ratings are canonical integer strings 1–10, never IMDb aggregate ratings.
- Canonical item URL is derived from Const. Source URL is never followed or trusted.
  Title hints trim surrounding whitespace, omit empties and cap at 4096 UTF-8 bytes.
- sourceConnectorId is imdb:ratings_csv; external ID scheme is imdb:title.
  The only state field is rating. No inferred completion/date/review fields.
- Same ID/media type/rating collapses to the first row (first title hint wins).
  Different media type or rating for a supported duplicate rejects the whole batch.
  Errors expose no partial accepted observations.
- Idempotency fingerprint hashes length-prefixed connector ID, title ID, normalized
  type and rating. It deliberately excludes display hints and ingestion time:
  semantic replay equality compares identity/state, not observation metadata.
- observed_at is supplied by a trusted ingestion caller; it is not sourced from CSV.
  Future external observation intake must separately validate timestamps/version.

## 4. Validation & Error Matrix

| Condition | Code |
|---|---|
| Bytes exceed limit / data rows exceed limit | csv_too_large / csv_too_many_rows |
| Invalid UTF-8 | csv_encoding_invalid |
| Quotes/column count invalid | csv_structure_invalid |
| Empty/duplicate header | csv_header_invalid |
| Required header missing | csv_column_missing |
| Supported row ID/rating/title invalid | imdb_id_invalid / imdb_rating_invalid / imdb_title_too_long |
| Same supported ID has conflicting normalized values | csv_duplicate_conflict |
| Unsupported media type | skipped media_type_unsupported |

## 5. Good / Base / Bad Cases

Good: BOM CSV with reordered columns and a quoted multi-line title produces one
rating observation, canonical identity and separate skipped-column diagnostics.
Base: valid header-only export produces an empty batch.
Bad: import IMDb Rating instead of Your Rating, infer completion, retain raw source
rows or trust the URL column as an arbitrary network destination.

## 6. Tests Required

imdb/tests.rs covers BOM/reordering/quotes/Unicode/multiline, supported type/rating
boundaries, semantic duplicate collapse/conflicts/fingerprints, malformed CSV,
UTF-8/ID/header errors, redaction, exact row limits and byte overrun.
Assert diagnostic lines for LF/CRLF/CR and embedded newlines.
All data is synthetic, not a real account export.

## 7. Wrong vs Correct

Wrong: rely on csv::Position::line for physical file diagnostics with mixed
CRLF and embedded newlines. This can point at the preceding line.
Correct: scan physical newlines incrementally up to the record byte position,
skipping leftover record terminators. This is linear and constant extra memory.
