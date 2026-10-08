# Catalog search evidence — 2026-09-21

Reviewed official source at immutable commit
`496e68811f730a333f1166440d2dbff24134b176`:

- [catalog/apis.py, search_item and SearchResult](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/catalog/apis.py#L204-L248):
  public GET `/api/catalog/search`, query/category/page inputs, response data/pages/
  count. Searches only local index; unlike web search, no external results or URL
  fetching. Pagination totals are estimated.
- [catalog/models/item.py, BaseSchema and ItemInSchema](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/catalog/models/item.py#L324-L352):
  normalized catalog identity fields and title; display_title is deprecated.

Implementation policy (not an upstream guarantee): one first page, 20 candidates,
512-byte query, 1024-byte title hint, 64 KiB response, ten-second request, no redirect
following or retries, same 120-second Core budget as native resolution. Restrict
adapter categories to implemented movie/TV detail evidence. Reuse existing reqwest,
serde, URL validation and runtime cancellation; no new generic mechanism/library.
Search candidates remain review-only; fresh explicit manual validation is separate.
No live NeoDB account/catalog operation was needed for this evidence review.
