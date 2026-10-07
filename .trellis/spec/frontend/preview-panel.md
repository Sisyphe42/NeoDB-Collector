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
submitImdbRatings, submitImdbRatingsWithResolution, previewJob, previewResults, cancelPreview, resumePreview and
deletePreview. API ownership remains backend/preview-http.md,
preview-submission-http.md and preview-management-http.md. The component does
not hand-maintain wire DTOs. `App.access` may be a render callback receiving the
validated Snapshot; `previewAvailable(snapshot)` checks every required route.
Choice/reference validators are narrowly re-exported by core-client-ts from the
generated contracts package; WebUI does not add a dependency or duplicate schemas.

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
  `apps/webui/src/style.css` must wrap long generated task IDs/history text without
  hiding or truncating evidence. Shrink the task heading's flex child while keeping
  status visible; test real-length IDs at a narrow viewport, not only short mocks.
- E4d review can stage explicit search/manual choices for a NEW preview only.
  A completed/current-binding job must have a confirmed receipt associated with
  this mounted panel's retained File and exact profile revision. Match job/page/
  row attempt, ordinal/source ID, profile and public base/account metadata before
  staging; reopened jobs stay read-only. File/profile edits, explicit job opening,
  attempt/binding changes, deletion and unmount invalidate the association/draft.
  No browser CSV parser, recovered raw file, history mutation, automatic singleton
  selection, saved cross-job mapping, activation or synchronization control.
- Unsupported/SearchCandidates rows allow explicit query staging: original at
  most 512 UTF-8 bytes, nonblank and control-free; Core still owns Unsupported-only
  fallback. Reuse TextEncoder for bytes and standard Unicode `\p{Cc}` for controls
  (including C1) rather than an ASCII-only control list. Returned candidate/
  NeedsReview items can be explicitly staged using
  only generated `{uuid, mediaType, model}` choice/reference validation. Malformed
  historical references cannot become selectable. Show identity/granularity and
  warn that manual selection may still need review; it is not IMDb equivalence.
  Sparse choices are per ordinal/source ID, replaceable/resettable, bounded by
  10,000 and retained across pages of the SAME job/attempt. Staging/clearing does
  not read File or send requests. The draft can clear any retained choice even
  when its row is off-page or now Compared. Auth expiry/in-flight actions disable
  staging and clearing as well as submission. No arbitrary UUID/URL entry in this slice.
- Explicit new-preview submission snapshots the retained File, exact revision
  and sorted sparse choices under the existing action lock. Reuse fatal UTF-8/BOM
  and actual byte guards; additionally TextEncoder-check the encoded JSON envelope
  at 10 MiB before transport. Escaping/choice overhead consumes space, not a larger
  allowance. Re-read source job just before upload to check original ID/attempt,
  completed/current binding, profile/base/account. This client preflight is only
  point-in-time; Core remains current singleton binding/identity authority.
  A confirmed receipt opens a NEW job and clears stale displayed job/page, keeping
  only the explicitly submitted sparse plan for this mounted lineage. Unknown or
  ambiguous receipts cannot establish a confirmed association, never imply rollback
  and never auto-retry. Starting either raw or structured transport detaches old
  resolution authority; a known ambiguous receipt clears the previous job/page
  before inspecting the known ID. Preserve useful form drafts and historical/error context.
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
  queued replies can settle before React effect cleanup. Inspection rejection/
  finally use the same selection/abort guards as fulfillment. Action errors use
  the same File/profile/selection/resolution frame guard as their fulfilled paths;
  finalization releases only its own action ID/mount epoch, even when its success
  legitimately changed the frame. An old request cannot expire a newer frame's
  authorization or clear a newer owner's loading indicator.
  A local structured-body refusal may skip the original completed job's automatic
  post-action inspection, but that suppression must be scoped to job/selection
  generation, consumed once and never block a new job or explicit refresh. Do not
  suppress existing raw-upload reconciliation or running-job polling to satisfy mocks.
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
| Reopened job, running/incomplete job or missing confirmed File association | Inspection only; no resolution staging/submission |
| File/profile/job/attempt/frame changes | Invalidate associated sparse plan; do not transfer choices |
| Invalid query/reference or encoded JSON above 10 MiB | No new-job request; explain without echoing CSV |
| Cancel/delete applied=false | Reconcile real status; no fabricated success |
| Ambiguous admission with known ID | Preserve ID; query state, do not retry |
| Network loss without receipt | State unknown outcome; do not invent ID/rollback |
| Auth/CSRF expiry | Stop mutations, preserve transient inspection/draft for user |
| Logout/unmount/selection change | Stop polling and ignore stale responses |

## 5. Good / Base / Bad Cases

Good: upload against saved v1 even when v2 exists, inspect historical Unknown
rating distinctly from Empty, cancel attempt 1, then explicitly resume freshly.
E4d good: stage an explicit query on a completed local upload, generate a new
candidate preview, then explicitly select a candidate and generate another preview
with fresh manual detail. Neither prior job nor historical evidence is edited.
Base: reopen an interrupted job by ID and inspect its partial evidence. Bad:
automatically retry a timed-out upload or label an accepted rating as ready to write.

## 6. Tests Required

PreviewPanel.test.tsx covers explicit raw CSV/revision, file limits/UTF-8/BOM,
all outcomes/evidence/diffs, sparse and empty pages, loading/error/auth states,
polling/abort/stale response and StrictMode safety, attempt-aware management,
refusal and ambiguous admission reconciliation without retries. App tests prove
all route declarations are required and incompatible status cannot mount access.
E4d tests cover query -> new candidate job -> manual -> NEW job, no staging I/O,
sparse replacement/reset/pagination, ineligible/reopened/changed frames, generated
unsafe-reference/query validation, encoded JSON cap, preflight refusal, unknown/
known ambiguous receipts, auth expiry and stale File/status/submission promises.
Check same-tick double actions and StrictMode/unmount cannot submit implicitly.
For attempt-change tests, model mutable remote job state rather than a finite
`mockResolvedValueOnce` queue: a page-read action restarts inspection after
settlement, so both reconciliation and an explicit refresh may read status.
Keep the old attempt until the test deliberately changes the remote state;
assert the new attempt clears old evidence without an error or timeout padding.
For no-staging-I/O assertions, settle the preceding page-read action and its
automatic status reconciliation before recording call-count baselines. The page
can render before action finalization restarts inspection; finding a row alone
does not prove those reads are drained. Use async React `act`/observable completion,
not arbitrary sleeps, larger timeouts or a weakened no-request assertion.
`tests/smoke.mjs` exercises actual local host/session/mapping and browser UI.
`tests/preview-ui-smoke.mjs` owns the clearly isolated mocked preview responses;
its evidence is UI/client-only, not backend CSV -> restart -> resume acceptance.
The separate native browser helper below uses real adapters/synthetic upstream,
never real NeoDB accounts or OS credentials. Capture preview desktop/mobile
screenshots and check overflow, keyboard-safe controls and empty browser storage.
Run full pnpm verify.
The Node browser helper compiles the committed generated JSON schemas with the
existing AJV/formats; browser standalone validators are bundled by Vite. Do not
invent Node-specific payload definitions or alter the generator just for fixtures.

`tests/preview-native-browser-smoke.mjs` separately owns native browser-to-adapter
acceptance. Drive the actual built WebUI/shared client, authenticated Core HTTP,
SQLite/runtime and production NeoDbClient against synthetic loopback upstream;
do not fulfill/intercept Core responses or substitute catalog/mark ports. Resolve
the explicitly ignored Rust host `preview_integration_tests/browser.rs` from
standard Cargo JSON artifacts and run it only in smoke after building assets.
Ordinary Rust compilation/ignored status is not successful browser evidence.
The host's private bounded stdio phase exchange is test-only, never a shipped
command, environment backdoor or public fixture HTTP endpoint. Redact buffered
host output and browser failure URLs; the bootstrap fragment is short-lived.

Assert real BOM/Unicode upload counts and immutable v1 despite changed disabled
v2, gated Running partial evidence, Completed equal/conflict/MissingShelf Unknown
comparisons, generated-schema validation and real ordinal pages. Completed-history
drain/closed DB scan/same-origin reopen followed by browser reload must preserve
the original session, require explicit job opening and perform no new upstream/
NeoDB-secret work or upload. This is not interrupted/running browser recovery.
Confirm job-only deletion, preserved unrelated state, exact request boundaries,
public/closed DB-WAL redaction, desktop/mobile screenshots, no overflow/page errors
or browser storage. Verify host EOF exit/cleanup separately.
Collect full security headers with Playwright `request.allHeaders()` and await
them before asserting Origin/CSRF/auth boundaries; `request.headers()` is filtered.
Keep narrow-screen assertions against real generated task IDs rather than masking
overflow or shortening fixture IDs to fit the layout.
Native happy-path history/deletion alone does not prove browser faults or
poll/cancel/search/manual/restart/migration, shipped collector behavior, live
accounts or native secrets.

The same Node helper separately drives
`preview_integration_tests/browser/failure.rs::browser_native_failure_host`.
Keep the six E5g cases bounded and independent: fetch 422/429 complete with
Unsupported/RateLimited historical rows; unsafe foreign redirect, mark 401,
Item-not-found 404 and held mark timeout fail with only the first committed
comparison. Do not invent a public upstream error code absent from PreviewJobView
or convert an empty failed-row page into empty/Unknown personal-state evidence.
Upstream NeoDB 401 must not expire the independent local WebUI session; Failed
resume stays explicit and no resume/retry/search occurs in these cases.

Only the held mark case observes Running/partial history before awaiting the
unchanged real ten-second transport timeout. Release/drain its handler after
terminal evidence and assert no late revival. Compare full job/pages after
terminal-history close/scan/same-origin reopen, then confirm job-only deletion
while retaining auth/config/secret state. Schema/trace/redaction, desktop/mobile,
empty browser storage and independent natural-exit EOF checks remain required.
Six synthetic browser faults are not exhaustive fault, pending-catalog polling/
cancellation, explicit Search/Manual or interrupted/restart/migration evidence.

## 7. Wrong vs Correct

Wrong: infer completed from `items.length === 0`, auto-resume on remount, or
reinterpret `{kind:'unknown'}` as a missing rating.

Wrong: selecting a historical candidate updates that old row or silently reuses
its choice with a different File/account/attempt. Correct: stage explicit generated
inputs in the matching transient frame, recheck on user action and create a new
Core preview; manual identity/detail and personal comparison are read freshly.

Correct: render the reported lifecycle, preserve historical evidence/sink kinds,
stop stale polling, and dispatch only explicit user mutations through AccessClient.
