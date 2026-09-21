# Bounded catalog search fallback

## 1. Scope / Trigger

Internal movie/TV fallback for preview identity resolution. Core owns the fallback
decision in `crates/collector-core/src/catalog_resolver.rs`; the NeoDB adapter owns
the catalog-search request and normalized evidence. No public HTTP route, saved
manual mapping, background job, personal-state read/write or new dependency.

## 2. Signatures

The separate port is defined in `crates/collector-core/src/catalog.rs`:

- `NeoDbCatalogSearchTransport::search(base: String, query: String, category: String)
  -> PortFuture<CatalogSearchPage>`
- `CatalogSearchCandidate { title: String, item: CatalogItem }`
- `CatalogSearchPage { candidates: Vec<CatalogSearchCandidate>, more_available: bool }`
- `validate_catalog_search_query(&str) -> Result<&str, ApiError>` returns trimmed input.
- `resolve_with_search<T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport>(
  &T, &impl ResolutionControl, base: &str, expected: &CatalogIdentity, query: &str)
  -> Result<Resolution, ApiError>`

`Resolution::SearchCandidates(CatalogSearchPage)` is review-only, never a
search-derived Matched or ManuallySelected. Native and explicit manual entrypoints
are unchanged. Candidate evidence is not a verified identity binding.

## 3. Contracts

- Validate query before native I/O: nonblank after trim, original at most 512 UTF-8 bytes,
  no control characters in the original input. Do not truncate or launder controls.
- Only native `Unsupported` permits one search. Preserve Matched, NeedsReview,
  Unavailable, RateLimited and every transport error without searching or retrying.
- A single checked 120-second deadline covers native polls, waits, detail and
  fallback. Reuse the runtime control port; cancellation/expiry wins over a ready
  reply and drops hung work. Never restart the budget at the fallback boundary.
- Adapter requests `GET /api/catalog/search` with `query`, expected `category`
  (`movie` or `tv`) and `page=1`. No credentials, automatic redirects, fetch,
  detail expansion, pagination loop or retry. Unsupported categories fail before I/O.
- Ten-second transport timeout and 64 KiB declared/streamed response cap apply.
  Require a valid `data` array of at most 20 entries and nonnegative integer
  `pages` / `count`; these totals are estimates, not completeness guarantees.
- Validate each result with the existing detail identity/reference rules. Retain
  conflicting external-ID claims and model/parent granularity for later review.
  Titles are bounded display hints (nonblank, control-free, at most 1024 bytes).
  Discard descriptions, ratings, unrelated metadata and raw response bytes.
- Reject unsafe, inconsistent or oversized pages atomically rather than returning
  partial candidates. Duplicate UUIDs and candidates outside the requested category
  invalidate the page. A response URL is never fetched by search.
- Even an exact-ID singleton is review-only in this conservative fallback. An empty
  candidate list is not proof of catalog or personal-mark absence. Manual choice
  must call `resolve_manual` for fresh detail and retain manual provenance.

Primary evidence: product `neodb_catalog_search_api` pins the search endpoint,
local-index-only behavior and estimated pagination; `neodb_catalog_item_schema`
pins the title and catalog identity fields. Search does not invoke native fetch.

## 4. Validation & Error Matrix

| Condition | Result |
|---|---|
| Blank, oversized or control-containing query | catalog_search_query_invalid, no I/O |
| Unsupported adapter category | neodb_catalog_category_invalid, no search I/O |
| Native Unsupported | Exactly one search under the existing deadline |
| Other native outcome/error | Preserve unchanged, zero search calls |
| Valid page, including zero candidates | Review-only candidates |
| Search 429 | neodb_catalog_rate_limited, no retry |
| Search redirect, other failure status or malformed/oversized body | neodb_catalog_unavailable, no follow-up request |
| Unsafe candidate reference | neodb_catalog_reference_invalid, no partial result |
| Timeout/cancellation | Existing resolution timeout/cancellation error |

## 5. Good / Base / Bad Cases

Good: unsupported native source link yields one bounded TV search page; user
selects a show, and manual resolution re-reads detail before accepting it.
Base: zero local-index hits leaves identity unresolved, without inventing absence.
Bad: use a similarly titled season as a show, auto-bind the first result, search
after 429, or follow every page until a preferred title appears.

## 6. Tests Required

`crates/collector-core/src/catalog_resolver/tests.rs` fake-clock tests cover
unsupported-only gating, preserved terminal outcomes,
query rejection before native I/O, empty/singleton review-only results, one shared
deadline, hung/late search and sticky cancellation. Connector loopback tests in
`crates/collector-connectors/src/neodb/catalog/tests.rs` cover
exact query/category/page, no auth/cookies, no redirects/retry, normalized evidence,
unsafe references, duplicate/malformed data, byte/candidate/title limits, estimated
pagination, sanitized errors and unchanged detail behavior. Run `pnpm verify`.

## 7. Wrong vs Correct

Wrong: `search(title).first()` becomes Matched, or an empty array means no mark.
Correct: return review candidates; explicit selection obtains fresh detail before
separate account-bound personal-state comparison. Catalog search says nothing
about the account's shelf, rating or review.
