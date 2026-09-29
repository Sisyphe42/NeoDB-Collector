# NeoDB single-item personal mark reads

## 1. Scope / Trigger

Internal C1 transport checkpoint: Core owns the port in
`crates/collector-core/src/marks.rs`; NeoDB implements it in
`crates/collector-connectors/src/neodb/marks.rs`. This is not a public route or
comparison service. Credential lookup, endpoint-generation binding, timestamped
snapshots, pure field comparison and jobs remain C2 or later. No write capability.

## 2. Signatures

`NeoDbMarkTransport::read_mark<'a>(&'a self, base: String,
token: &'a SecretString, expected_account: String, expected_item: &'a CatalogItem)
-> PortFuture<'a, MarkRead>` borrows both secret and previously resolved evidence.

`MarkRead` is `Present(Box<MarkSnapshot>)` or `MissingShelf`. The boxed payload
avoids inflating the empty variant; no lint suppression is used. `MarkSnapshot` contains
`item: CatalogItem`, `owner: String`, `shelf_type: String`,
`rating_grade: Option<u8>`. Neither result is an execution plan or an activation approval.

Only shelf and rating are implemented comparison fields. Do not retain upstream
comments, tags, dates, visibility, owner profile, aggregate ratings or raw JSON in
this result. Supporting those fields later requires their own mapping/read contract.

## 3. Contracts

- One authenticated `GET /api/me/shelf/item/{uuid}`, no query, cookies, retries,
  implicit detail lookup, catalog fetch or mark mutations. Borrow the protected
  token; expose only at the HTTP header boundary and never in diagnostics.
- Before I/O validate the configured origin and expected movie/TV catalog identity
  with existing reference rules. UUID, canonical URL, detail route, media/model and
  parent must be consistent. Account must be nonblank, control-free, <=256 bytes;
  token must be nonempty and usable in the Authorization header.
- A 200 response must normalize to the expected catalog identity and have exactly
  the expected owner username. Preserve returned normalized identity claims; the
  embedded generic item schema need not contain all detail-specific IMDb metadata.
- Shelf is one of wishlist/progress/complete/dropped. Rating is a **required** field:
  JSON null means no rating in this successful mark response; otherwise integer
  1..10. Missing fields, zero, fractions, malformed values and schema drift fail
  closed. Do not confuse an item's aggregate rating with the owner's rating_grade.
- 404 is parsed under the same response bounds. Only exact `Mark not found` means
  MissingShelf. `Item not found` means item unavailable; all other 404 bodies fail.
  MissingShelf does not confirm an empty rating/comment/tag state: upstream stores
  those separately. Later comparison must preserve unknown values.
- Stop at 302 and require identity re-resolution; do not follow Location even when
  same-instance. Other redirects also fail. No credential forwarding to result URLs.
- Use the existing redirect-disabled client with ten-second request/body timeout,
  64 KiB declared and streamed response caps, and sanitized static diagnostics.
  Reuse catalog normalization and bounded JSON parsing rather than copying them.
- This low-level port does not read the credential store or prove that an endpoint
  remained current during I/O. C2 must bind/check endpoint, account and credential
  reference before accepting snapshots. A transport result alone is not freshness.
  A missing-shelf reply has no owner field to corroborate the supplied account;
  C2 must verify the token's `/api/me` identity before treating it as an account-bound
  observation. C1 checks owner equality only when a successful mark supplies it.

Evidence: product `neodb_mark_read_api`, `neodb_item_read_resolution`,
`neodb_independent_rating` and `neodb_identity_schema`; task research `mark-read.md`.

## 4. Validation & Error Matrix

| Condition | Required result |
|---|---|
| Invalid origin | neodb_base_url_invalid before I/O |
| Invalid expectation / account / token | neodb_mark_expectation_invalid / neodb_mark_account_invalid / neodb_mark_token_invalid before I/O |
| Valid 200 with matching item and owner | Present normalized mark |
| Wrong owner | neodb_mark_account_changed; never reuse under old binding |
| Changed parent / merged item | neodb_mark_identity_changed; require fresh identity resolution |
| Malformed or inconsistent embedded item | neodb_mark_unavailable; never accept a different item |
| 404 + exact Mark not found | MissingShelf only |
| 404 + Item not found | neodb_mark_item_unavailable, not missing mark |
| Unknown/malformed/oversized 404 | neodb_mark_unavailable, not absence |
| 302 | neodb_mark_identity_changed; re-resolve outside this transport |
| Other redirects / failures / malformed or oversized 200 | neodb_mark_unavailable; no retry |
| 401 / 403 / 429 | neodb_mark_unauthorized / neodb_mark_forbidden / neodb_mark_rate_limited |
| Timeout or dropped future | No successful snapshot or subsequent request |

## 5. Good / Base / Bad Cases

Good: a resolved show and matching owner return complete with personal rating 8;
later comparison can compare 8 without touching the stored mark.
Base: exact missing-mark response reports MissingShelf and leaves rating unknown.
Bad: convert a proxy's 404 page to an empty rating, treat missing rating_grade as
null, or send the credential to a merged item's redirect destination automatically.

## 6. Tests Required

Loopback mocks in `crates/collector-connectors/src/neodb/marks/tests.rs` cover exact
GET/Bearer/no-cookie, pre-I/O validation, present and nullable rating, missing-field
rejection, field bounds, owner/item binding, all absence/status cases, redirect
non-following, response limits including chunked bodies, timeout and redaction.
Existing OAuth and catalog tests must remain green after shared-parser extraction.
Run full `pnpm verify`, YAML/evidence validation and `git diff --check`.

## 7. Wrong vs Correct

Wrong: `status == 404` returns a snapshot with every value set to None.
Correct: recognize the specific missing-shelf response; other failures stay errors,
and unobserved independent fields stay unknown until separately established.
