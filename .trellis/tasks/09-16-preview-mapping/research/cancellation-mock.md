# Bug analysis: intermittent client cancellation test

## 1. Root cause category

E — implicit assumption: keeping Request.signal alive was assumed to keep its
AbortController alive. Local Node bundled Undici source instead uses a weak
reference from the upstream signal listener to the controller; Request owns the
strong reference. Finalization unregisters forwarding.

## 2. Why earlier fixes failed

Avoiding an extra Request clone reduced indirection but still discarded the
original Request in the pending mock. Passing reruns did not distinguish GC timing.

## 3. Prevention

Retain each pending Request in the test and assert both timeout and pre-aborted
requests settle with aborted signals. No production-client change is needed.

## 4. Systematic expansion

Inspected native Request bridging and the jsdom AbortSignal implementation.
Local node --expose-gc reproduction with identical controllers:
signal-only retention -> aborted=false; Request retention -> aborted=true.
This directly discriminates GC lifetime from a slow 10-ms timer hypothesis.
Confidence is high for this test failure, not a claim about every abort bug.

## 5. Knowledge capture

Updated frontend/webui-boundaries.md and the client test. There is no project-owned
src/templates/markdown/spec tree to synchronize; bundled Trellis templates are
not the owner of application test behavior.
