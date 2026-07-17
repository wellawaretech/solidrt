---
type: plan
title: Reactivity halt containment
status: open (critical; design session pending, mechanics verified)
timestamp: 2026-07-17T00:00:00Z
---

# Reactivity halt containment

App-port postmortem (2026-07-16) item 1.1, severity critical: one uncaught
error reaching the scheduler bricks every app permanently. Both observed triggers are fixed
(fontStyle no longer throws; image errors surface to the nearest
`<Errored>`), but any app can still be killed by a single unclaimed error,
so this must be solved, not just avoided. Stage 0 is a design session; this
note carries the verified mechanics, the option space, and the open
decisions for it.

## How the halt works today

Verified against @solidjs/signals 2.0.0-beta.17 (solid-js 2.0.0-beta.17),
prod.js; dev.js matches with extra logging.

- The scheduler has a module-global `halted` flag. `haltReactivity()` sets it
  and logs `[REACTIVITY_HALTED] An uncaught error halted the reactive
  system.`; from then on `schedule()` refuses every flush ("Update ignored").
  Nothing reactive runs again: no effects, no renders, no event-driven
  updates. The JS thread itself is fine (timers, raw callbacks still fire).
- The halt fires when an effect gets STATUS_ERROR and
  `queue.notify(node, STATUS_ERROR, ...)` returns false, i.e. no queue up the
  owner chain claims the error. `createErrorBoundary` (what `<Errored>` uses)
  installs a queue that claims it; without a boundary above, notify bubbles to
  the global queue and fails.
- Path differences by effect kind (from `runEffect` / `notifyEffectStatus`):
  - EFFECT_RENDER (createRenderEffect - every prop-applying effect our
    renderer creates via @solidjs/universal): unclaimed STATUS_ERROR -> halt.
    This is the postmortem case: async image memo rejects, the `<texture>` prop
    effect reading it errors, no boundary above, app dead.
  - EFFECT_USER (createEffect): an error arriving from something it *reads*
    runs the `error` handler if the two-arg form provided one, else just
    `console.error` - no halt. But if the effect function (or the error
    handler itself) *throws* and no boundary claims it -> halt.
  - Boundaries only collect STATUS_ERROR; suspension (NotReadyError) is
    routed separately via Loading (verified during stage 2).
- `resetErrorHalt()` is exported from @solidjs/signals (not re-exported by
  solid-js). It just clears the flag; whether the graph is in a consistent,
  resumable state after an arbitrary halt is NOT established - writes made
  while halted were dropped, and the erroring node keeps STATUS_ERROR.

Not part of the halt problem (already handled elsewhere): event handler
callbacks run outside the graph, and flux reports their exceptions; genuinely
unhandled promise rejections outside the graph are logged by the engine after
the job queue drains (flux/src/engine.rs flush_rejections). Neither halts the
scheduler. Async errors *inside* the graph (async memos) become STATUS_ERROR
and follow the rules above.

## Failure taxonomy to design against

1. Error in a computation/render effect **with** an `<Errored>` above it -
   works today: subtree shows fallback, reset available.
2. Error in a render effect **without** a boundary - halt. The common case:
   most apps wrap nothing, and our renderer installs no root boundary.
3. Error thrown by a user effect's compute or error handler, no boundary -
   halt.
4. Error arriving at a user effect without an `error` handler -
   console.error only, effect skipped. Already contained (arguably too
   quietly).
5. Halt-adjacent but out of scope: event handler throws, fire-and-forget
   rejections - engine-reported, no halt.

## The design tension

Over-containing is the failure mode on the other side: if errors just poison
a node or subtree silently, "app bricks" becomes "app quietly rots" - dead
thumbnails, frozen panels, nothing in the developer's face. Whatever
containment we pick must keep errors at least as loud as the halt is today
(terminal + get_logs + ideally a render-tree marker on the poisoned subtree),
while keeping the rest of the app alive.

The postmortem's suggestion: contain at the effect/computation that threw,
log with component/owner context, keep hard-halt available behind a dev flag.
That last part maps onto the decided dev/prod validation policy (backlog:
dev-prod-validation-policy.md - throw in dev, warn in prod; the runtime
signal for it is itself still unbuilt).

## Candidate directions (to weigh in the session)

A. **Implicit root boundary.** Renderer wraps the app root in
   createErrorBoundary at mount. Cheapest possible fix; app becomes
   recoverable (root fallback + reset) instead of halted. But containment is
   all-or-nothing: one dead thumbnail still blanks the whole app, just
   recoverably. Probably right as a floor, insufficient as the answer.

B. **Per-node containment in the renderer.** Give each created node (or each
   prop effect) boundary behavior so an erroring prop poisons that node only.
   The prop effects are created inside @solidjs/universal's createRenderer,
   not by us - this needs either an upstream hook, a fork of universal (it is
   small), or reimplementing its element wiring in packages/core.

C. **Upstream policy change in signals.** Make the unclaimed-error behavior
   configurable: mark the erroring computation STATUS_ERROR and keep the
   scheduler alive (effectively an implicit boundary at every computation),
   halt only when a flag asks for it. Cleanest semantics, biggest lift;
   needs an upstream issue/PR or a fork of @solidjs/signals (we track beta
   releases, so a fork is a real maintenance cost).

D. **Halt-then-recover.** Keep upstream as is; on halt, log loudly and call
   resetErrorHalt() (plus dispose/remount of the poisoned subtree?). Least
   invasive, but built on unestablished ground: post-halt graph consistency
   is unverified, and dropped writes during the halted window are lost
   regardless.

These compose: A is compatible with any of B/C/D landing later.

## Open questions for the session

- Containment unit: computation, node, component, window root? What does
  "nearest boundary" default to when the app declared none?
- Loudness contract: what exactly does a contained error produce (terminal
  line with owner/component context, get_logs entry, render-tree marker,
  dev overlay?) so quiet rot cannot happen.
- Dev/prod split: is halt ever the right dev behavior, or is contained+loud
  strictly better even in dev (postmortem leans the latter)?
- Reset/recovery semantics: does a poisoned node recover on next successful
  recompute, or stay dead until remount (Errored's reset callback is the
  precedent)?
- Upstream strategy: file the issue upstream first regardless of local path;
  is a universal fork (small) acceptable where a signals fork is not?
- Verify by repro before deciding: the EFFECT_USER console.error path (case
  4), post-resetErrorHalt() consistency (D), and whether boundary claiming
  behaves the same for initial mount vs later update.

## Repro set to build first

Small examples, one per taxonomy case (1-4), each a few lines against a
throwing signal/async memo, runnable headless. They pin current behavior,
become the acceptance tests for whatever containment lands, and settle the
"to verify" items above.