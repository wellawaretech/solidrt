---
title: Reactivity halt containment
description: One unclaimed error used to halt the whole app for good; decided and built as a root error boundary around the app's window (error window with reset) plus per-node containment in the renderer's effect/insert exports, with the verified mechanics and the measured cost.
created: 2026-07-17
---

# Reactivity halt containment

App-port postmortem (2026-07-16) item 1.1, severity critical: one uncaught
error reaching the scheduler bricks every app permanently. Both observed
triggers were fixed (fontStyle no longer throws; image errors surface to the
nearest `<Errored>`), but any app could still be killed by a single unclaimed
error. Decided 2026-08-23 and built the same day; the reset path awaits a
client build to verify (see Status).

## How the halt worked

Verified against @solidjs/signals 2.0.0-beta.17 and again on 2.0.0-rc
(unchanged):

- The scheduler has a module-global `halted` flag. `haltReactivity()` sets it
  and logs `[REACTIVITY_HALTED]`; from then on `schedule()` refuses every
  flush. Nothing reactive runs again. The JS thread itself is fine (timers,
  raw callbacks and the dev server's exec closures still run).
- The halt fires when an effect gets STATUS_ERROR and no queue up the owner
  chain claims it. `createErrorBoundary` (what `<Errored>` uses) installs a
  queue that claims it; without a boundary above, notify bubbles to the
  global queue and fails.
- EFFECT_RENDER (every prop-applying effect the renderer creates via
  @solidjs/universal): unclaimed STATUS_ERROR -> halt. EFFECT_USER
  (createEffect): an error arriving from something it reads runs the `error`
  handler or console.errors, no halt; the effect function itself throwing
  with no boundary -> halt. Boundaries only collect STATUS_ERROR; suspension
  (NotReadyError) is routed separately via Loading.
- A boundary keeps the failed subtree alive and marks only the failed
  computations; `reset` recomputes those in place. The fallback receives the
  error as an accessor (`err()`), not the error.

## Decision

Two layers, composed. Directions B (per-node) and A (root boundary) from the
option space; C (upstream policy) and D (halt-then-recover) dropped, C for
the fork cost, D for the unverified post-halt state.

1. **Root boundary around the app, window included** (packages/core/src/
   renderer.ts render()). `createErrorBoundary` wraps `code()`; the fallback
   is an error window (blue, message, stack, Reset), the in-app sibling of
   the startup BSOD. Around the root rather than inside the window so the
   window's own prop expressions are covered too, and so reset is the
   boundary's own in-place retry. Creating the error window makes it the
   native root as a side effect (create_root overwrites); the way back is a
   new `setRoot(id)` (alloy RenderTree::set_root, marshalled in
   flux/src/alloy_plugins/tree.rs) because the app's window returns as the
   same node. The app's window is kept, un-rooted, behind the error window;
   an error window replaced by anything is destroyed. window.ts keeps the
   root id movable (`setWindowRoot`) for key routing and the pointer interest
   root; focus is cleared on a swap.

2. **Per-node containment in `effect` and `insert`** (same file). The
   compiled JSX writes into the tree through exactly these two core exports:
   one grouped effect per element for its dynamic props (or a single-value
   effect for one prop) and one insert per child expression. Each is wrapped
   by a guard: a throw keeps the last good value (a SKIP sentinel on a first
   run, so the apply side does nothing), the effect stays subscribed so the
   node recovers when the throwing read changes, NotReadyError passes
   through, and it logs once per site (`Contained error: ...` with the node
   path; `Recovered: ...` on recovery). A child expression resolving to a
   function (component children) is guarded recursively; prop values are
   not, so event handlers keep their own reporting. Not covered by design: a
   user createEffect body that throws, and universal's internal spread
   effects; both fall through to layer 1.

Loudness: a contained error is one error-level log line with the node path
and the .tsx line in the stack; an uncaught one is the `Uncaught error` line
plus the error window itself, which /tree and /snapshot show.

## Findings

- Cost of the guard (probes/signal-bench.tsx, N=1000 elements all updating
  every frame, release client, medians of ~10 batches): flush 6.9 ms with
  the guard vs 6.45 ms bypassed, i.e. ~0.45 us per effect run on ~6.5 us.
  About 7% in this worst case, nothing at real update rates, zero for
  elements that do not update. If it ever matters the `try` can move into
  the babel preset's emitted effect body, removing the extra call frame.
- The root boundary caught its first real bug during the work: the probe
  used the one-argument `createEffect`, which Solid 2 rejects at mount, and
  the error window showed the message with the remapped .tsx stack instead
  of a halted app.
- Universal's type declarations trail its runtime (effect takes an options
  argument, insert takes initial and options); the wrappers carry the
  runtime signatures.
- The dev server watches the entry's sourceDir only; an edit under
  packages/core needs `POST /__control__/reload`.

## Status

- Verified on the current client: contained error and recovery (the text
  keeps "ok", the counter next to it keeps counting), the root boundary on a
  user effect throw and on a mount-time error, /snapshot of the error window.
- Pending a client build (setRoot is native): Reset swapping the app's
  window back in, and a second error after a reset.

## Non-goals

- A native diagnostic flag on a contained node so /tree and the inspector
  show it, and a dev-only outline: ideas.md, only if the log line proves
  insufficient.
- Keeping hard-halt behind a dev flag: with layer 1 there is nothing a halt
  preserves; the dev/prod split (okf/backlog/dev-prod-validation-policy.md)
  is unaffected.
