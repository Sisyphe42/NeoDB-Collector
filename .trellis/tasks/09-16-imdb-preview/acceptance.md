# IMDb read-only preview milestone acceptance

Accepted in the approved synthetic scope on 2026-10-09. This is not live-account
or distribution acceptance and does not enable connector advertising or writes.

## Criterion mapping

| Parent requirement | Current source and evidence |
|---|---|
| One NeoDB account, read OAuth, no source/personal mutation | Core OAuth tests, singleton Storage and authenticated settings/OAuth routes; preview E5a-k request traces and fake-secret assertions. Catalog fetch is the sole approved preview side effect. |
| IMDb CSV bounds/type filtering; declarative versioned mapping | `collector-connectors/src/imdb/tests.rs`, Core mapping tests and disabled immutable Storage revisions; CLI/WebUI import tests exercise original BOM/Unicode CSV and saved v1 despite v2. |
| Normalized provenance only, configurable 30-day retention and manual deletion | Core/Storage preview tests; CLI configuration precedence; production startup and periodic retention tests; E5a-k stored/public equality, closed DB-WAL scans and retained auth/config/secrets after deletion. |
| Fresh explicit resume, historical results never authorize execution | CLI E5d/e and browser E5j/k recover Running as Interrupted without I/O, explicitly start attempt2, remove old results, reject stale reads and compare changed sink state under fixed expiry/revision. |
| All three children and integrated CLI/WebUI against simulated upstream | Auth and mapping completed earlier; preview-jobs E5a-e actual Rust client and E5f-k actual built WebUI compose production Core HTTP/runtime/SQLite/NeoDbClient against synthetic upstream. Default `tests/smoke.mjs` drives the browser chains and six independent EOF probes. |

Crate paths are relative to `crates/`; detailed current criterion mapping and the
final full-gate result live in the sibling preview-jobs `acceptance.md` (both tasks
archive together under the same month). Stable engineering ownership is
`.trellis/spec/backend/foundation-runtime.md` and its linked preview contracts.

The final audit reconciles the child PRDs without changing approved requirements.
Live-account, native credential-store, shipped-process preview/restart and hosted
three-OS execution remain unestablished separate validation scopes. Source and
personal writes, scheduling, desktop, extension, n8n and general imports remain
out of this milestone. `connectorIds=[]` and `writesEnabled=false` are unchanged.
