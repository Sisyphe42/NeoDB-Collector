# Historical preview WebUI

## 1. Scope / Trigger

`apps/webui/src/PreviewPanel.tsx` is a thin session-only client of the existing
preview APIs. AccessPanel mounts it only when App/main have validated compatible
Core status and all six required preview route declarations. Extend the shared
blue-grey console, not a separate desktop or import application. No new API,
parser, persistence, business mapping, activation or personal-state writer.

## 2. Signatures

`PreviewPanel({client: AccessClient})` consumes generated PreviewJobView,
PreviewResultPage and admission receipts through core-client-ts. Calls are
submitImdbRatings, previewJob, previewResults, cancelPreview, resumePreview and
deletePreview. API ownership remains backend/preview-http.md,
preview-submission-http.md and preview-management-http.md. The component does
not hand-maintain wire DTOs. `App.access` may be a render callback receiving the
validated Snapshot; `previewAvailable(snapshot)` checks every required route.

## 3. Contracts

- Users explicitly choose a local CSV and a saved disabled profile ID/version.
  There is no implicit save, default or latest revision. Core checks the exact
  immutable selection; the latest-profile GET cannot prove an older revision.
- Reject files above 10 MiB before reading and recheck actual buffer length. File
  selection retains a File in memory; only an explicit valid submit reads it.
  Fatal UTF-8 decoding preserves BOM. Raw contents and drafts remain memory-only;
  no logging, browser storage, download copy or browser CSV parsing.
- A known job ID can reopen historical evidence after page/Core restart; there
  is no job-list API or persisted browser history. IDs/versions/pages are bounded
  inputs, but Core remains the authorization/domain authority.
  Profile/job IDs use the bounded ASCII grammar; job IDs `.` and `..` are refused
  before HTTP because URL path normalization can change the resource route.
- Poll queued/running jobs only, four seconds after each completed inspection,
  with bounded nonoverlapping cadence and cleanup
  on unmount, errors or terminal state. Abort and generation guards prevent stale
  requests publishing into a newer selection. Read exactly one attempt-bound
  ordinal page; never invent hasMore, total result count, processed percentage,
  or completion from an empty/sparse page. Refresh status to reconcile attempts.
- Render all outcome kinds, item UUID/URLs/model/media/granularity/external claims,
  automatic/manual provenance, timestamps and all field suggestions, sink states,
  decisions/reasons. Keep Known, Empty and Unknown distinct. Accepted is a
  historical comparison decision, not permission to write. Binding current is
  point-in-time only; changed/disconnected evidence stays historical.
  Canonical URLs are clickable only for HTTPS; malformed/unsafe URLs remain inert
  text. Schema-valid timestamps outside Date's range use a Unix-number fallback.
- Review means inspection only: no public manual-selection endpoint exists yet.
  No fabricated confirmation, profile activation or synchronization control.
- User actions alone submit/cancel/resume/delete. Serialize them and bind cancel/
  resume to the displayed attempt. Resume remains explicit, eligible retained
  terminal state only; original expiry/revision/binding are Core-controlled.
  Confirm deletion of operational history without touching profiles/credentials.
- `applied=false` is refusal/absence, not an exception or fabricated lifecycle.
  Read status after management. Errors may follow committed mutations; never
  auto-retry. PreviewAdmissionClientError retains its validated job ID/generation
  for inspection. Lost receipts without ID remain unknown, not rolled back.
- Preserve useful drafts/error context; a successful background read must not
  erase mutation uncertainty. A ref lock blocks same-tick double submissions.
  StrictMode setup/cleanup uses mount epochs, not a permanently false mounted ref.
  Abort the current inspection controller synchronously when an action starts;
  queued replies can settle before React effect cleanup. Guard rejection/finally
  with the same selection generation and abort checks as fulfillment, so an old
  request cannot lock a new selection's auth state or clear its loading indicator.
  Session expiry stops subsequent mutations until the panel is reauthorized/remounted and
  directs reauthorization. Logout unmounts/aborts observation but cannot cancel
  Core-owned accepted work. No secret enters links or local/session storage.
- Warn that native preview may create public catalog entries; cancellation cannot
  undo those entries. Source data and NeoDB personal marks are never changed.
  Use accessible labels, live statuses, visible focus and stacked responsive
  evidence/diff cards with wrapping; no mobile-wide unbounded table.

## 4. Validation & Error Matrix

| Condition | UI behavior |
|---|---|
| Missing required capability route or incompatible version | Hide preview controls |
| Oversize/invalid UTF-8 file or invalid selection | No upload; explain without echoing payload |
| Accepted receipt | Show actual counts/job ID; inspect, not claim completed |
| Queued/running empty page | Explain no committed results here; do not claim done |
| Attempt changes or page fails | Do not mix generations; explicitly refresh |
| Binding changed/disconnected | Label historical binding; do not claim fresh readiness |
| Cancel/delete applied=false | Reconcile real status; no fabricated success |
| Ambiguous admission with known ID | Preserve ID; query state, do not retry |
| Network loss without receipt | State unknown outcome; do not invent ID/rollback |
| Auth/CSRF expiry | Stop mutations, preserve transient inspection/draft for user |
| Logout/unmount/selection change | Stop polling and ignore stale responses |

## 5. Good / Base / Bad Cases

Good: upload against saved v1 even when v2 exists, inspect historical Unknown
rating distinctly from Empty, cancel attempt 1, then explicitly resume freshly.
Base: reopen an interrupted job by ID and inspect its partial evidence. Bad:
automatically retry a timed-out upload or label an accepted rating as ready to write.

## 6. Tests Required

PreviewPanel.test.tsx covers explicit raw CSV/revision, file limits/UTF-8/BOM,
all outcomes/evidence/diffs, sparse and empty pages, loading/error/auth states,
polling/abort/stale response and StrictMode safety, attempt-aware management,
refusal and ambiguous admission reconciliation without retries. App tests prove
all route declarations are required and incompatible status cannot mount access.
`tests/smoke.mjs` exercises actual local host/session/mapping and browser UI;
preview-specific browser responses use clearly isolated mocks, not real NeoDB or
OS credentials. Capture preview desktop/mobile screenshots and check overflow,
keyboard-safe controls and empty browser storage. This is UI/client evidence,
not final backend CSV -> restart -> resume acceptance. Run full pnpm verify.
The Node browser helper compiles the committed generated JSON schemas with the
existing AJV/formats; browser standalone validators are bundled by Vite. Do not
invent Node-specific payload definitions or alter the generator just for fixtures.

## 7. Wrong vs Correct

Wrong: infer completed from `items.length === 0`, auto-resume on remount, or
reinterpret `{kind:'unknown'}` as a missing rating.

Correct: render the reported lifecycle, preserve historical evidence/sink kinds,
  stop stale polling, and dispatch only explicit user mutations through AccessClient.
