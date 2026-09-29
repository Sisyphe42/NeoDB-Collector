# Mark-read evidence — 2026-09-22

Official source pinned to `496e68811f730a333f1166440d2dbff24134b176`:

- [journal/apis/shelf.py](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/journal/apis/shelf.py#L215-L239):
  authenticated single-item GET; an existing item without a shelf member returns
  404 with `Mark not found`. MarkSchema has item, owner, shelf_type and nullable
  rating_grade (1..10). Batch results may omit deleted, merged or unmarked items.
- [common/api.py](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/common/api.py#L156-L177):
  missing/deleted item returns `Item not found`; merged item redirects with 302.
  OAuth authentication maps GET to read scope (lines 18-60).
- [journal/models/mark.py](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/journal/models/mark.py#L173-L179):
  ratings are looked up independently of the shelf member. MissingShelf must not
  become a completely empty personal-state snapshot.
- [users/apis.py](https://github.com/neodb-social/neodb/blob/496e68811f730a333f1166440d2dbff24134b176/neodb/users/apis.py#L28-L42):
  embedded owner username is the local handle for local accounts; `/api/me` uses
  the authenticated local username. C1 validates owner against caller expectation.

Implementation refinement: fail closed on unknown/localized 404 messages; never
infer absence from status alone. Stop at every redirect and require identity
re-resolution instead of forwarding credentials to its destination. Only the
implemented shelf/rating fields are normalized. Core snapshot account-generation
binding and field comparison remain C2; transport alone does not establish freshness.
