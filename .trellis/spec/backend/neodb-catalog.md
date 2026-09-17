# NeoDB catalog transport

## 1. Scope / Trigger

Internal single-attempt transport is implemented in
`crates/collector-connectors/src/neodb/catalog.rs`, implementing the Core port in
`crates/collector-core/src/catalog.rs`. No API route, polling scheduler, automatic
identity binding or personal-state operation is exposed by this checkpoint.
Native fetch can create public catalog entries, even during preview.

## 2. Signatures

`NeoDbCatalogTransport: Send + Sync`:

- `fetch(base: String, source_url: String) -> PortFuture<CatalogFetch>`
- `detail(base: String, reference: String) -> PortFuture<CatalogItem>`

`CatalogFetch`: Pending, Detail(validated absolute URL), Unsupported, RateLimited,
Unavailable. These are transport outcomes, not matching decisions.
`CatalogItem`: uuid, canonical_url, detail_url, media_type, model, optional
parent_uuid, external_ids as a vector of `(scheme, id)` evidence claims.
No public wire schema or generated client change is needed for these internal ports.

## 3. Contracts

- Reuse the OAuth adapter's origin validation and redirect-disabled HTTP client.
  Catalog requests have no Bearer token, cookies or force-refetch parameter.
- Fetch accepts only exact `https://www.imdb.com/title/tt<7..16 digits>/` at this
  stage. CSV and catalog share `imdb::valid_title_id`. Reject before network I/O;
  neither a claimed connector ID nor a parseable arbitrary URL grants authority.
- GET `/api/catalog/fetch` once; 202 returns Pending without retry. The future
  resolver must enforce >=15s pending spacing and a 120s overall deadline before
  this port is connected to jobs. No unattended polling exists yet.
- 302 requires one Location header, root-relative or absolute same origin.
  Validate before following. Supported detail paths: `/api/movie/{uuid}`,
  `/api/tv/{uuid}`, `/api/tv/season/{uuid}`, `/api/tv/episode/{uuid}`.
  This is a sink-adapter route allowlist, not a closed Core media enum.
- Reference cap 2048 bytes; UUID 1..22 ASCII alphanumeric bytes. Reject userinfo,
  foreign origin/port/scheme, query/fragment, whitespace/control bytes, percent
  escapes, backslashes, dot segments and non-detail/subresource paths. Validate
  raw strings before URL normalization. Direct detail input is revalidated too.
- Detail permits at most three validated 302 redirects (four requests), detects
  cycles and shares a ten-second I/O budget across hops/body. Fetch has a
  ten-second request timeout. Bodies are parsed only for 200 detail replies and
  capped at 64 KiB using both declared length and streamed-byte checks.
- Detail path, UUID, category, model, canonical ID and api_url must agree.
  Movie/TVShow/TVSeason/TVEpisode are kept distinct. An optional parent UUID must
  have the same lexical bounds. Missing identity yields empty evidence, not a match.
- Retain valid `imdb` metadata and exact canonical IMDb external-resource IDs,
  deduplicating equal claims while preserving conflicting IDs. Other external
  resources, titles, descriptions and raw replies are discarded and never fetched.
  Additional external-link normalization belongs in a separately tested extension.
- Transport errors use static sanitized diagnostics, never upstream bodies/URLs.
  Dropping the future cancels local I/O, not catalog creation already queued remotely.

Evidence: product entries `neodb_native_fetch_api`, `neodb_native_fetch_web` and
`neodb_catalog_item_schema`, `neodb_catalog_movie_schema`, `neodb_catalog_tv_schema`
pin the reviewed upstream behavior and fields.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Unsupported/noncanonical source input | catalog_source_url_invalid, no I/O |
| Invalid instance origin | neodb_base_url_invalid, no I/O |
| Unsafe/missing/duplicate Location or invalid detail reference | neodb_catalog_reference_invalid |
| Fetch 202 / 302 / 422 / 429 / 404 | Pending / Detail / Unsupported / RateLimited / Unavailable |
| Detail 429 | neodb_catalog_rate_limited |
| Other status, timeout, connection failure, malformed/oversized/inconsistent detail | neodb_catalog_unavailable |

No failure or 404 establishes absence of a personal mark. Generic redirects such
as 301/307/308 are errors, not silently followed.

## 5. Good / Base / Bad Cases

Good: exact IMDb URL -> 302 -> validated movie detail -> identity evidence for Core.
Base: 202 returns immediately; the caller schedules a future attempt, not a loop.
Bad: follow Location into `/api/me`, forward a token, infer show equivalence from
a season's IMDb metadata, or treat the first ID as authoritative despite conflict.

## 6. Tests Required

`collector-connectors/src/neodb/catalog/tests.rs` uses loopback mocks to assert
exact GET/query/no-auth, pre-I/O source rejection, redirect origin/path/header
checks, bounded hops/cycles, model distinctions, contradictory IDs, inconsistent
payloads, error redaction, timeout/connection failure, and size caps including
chunked bodies with no Content-Length. Existing OAuth and IMDb tests must stay green.
Run scoped connector tests and `pnpm verify` before delivery.

## 7. Wrong vs Correct

Wrong: enable reqwest automatic redirects and deserialize the resulting page.
Correct: keep redirects disabled, validate each catalog-detail destination and
verify response identity against the final requested path.

Wrong: collapse evidence into a map that overwrites one IMDb claim with another.
Correct: preserve distinct claims; the later resolver must detect disagreement.
