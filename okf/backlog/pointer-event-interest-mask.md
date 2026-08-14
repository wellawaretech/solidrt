---
title: Per-node event-interest mask for pointer dispatch
description: Rust marshals the full root-to-leaf hit path into JS for every pointer event because only the JS handler registry knows which nodes listen; a per-element event-kind bitmask lets dispatch deliver only listening nodes and (staged) skip empty emissions entirely, making input over handler-free regions free.
created: 2026-08-01
---

# Per-node event-interest mask for pointer dispatch

## Problem

The JS handler registry (`handlers` in core's core.ts, fed by `applyProp`'s
`/^on[A-Z]/` intercept in renderer.ts) is the only place that knows which
nodes listen to which events. Rust owns the tree but has no interest
information, so `input.rs` (flux/src/plugins/gui) marshals the FULL
root-to-leaf hit path for every pointer event: five parallel JS arrays
(targets, localX/Y, parentX/Y) plus a ~13-field object, ~5d+13 rquickjs
value writes at depth d. The JS side then rest-spreads the object and walks
all d entries doing map lookups, typically to run one handler - or none.

Because only JS knows interest, Rust can never skip anything: a mouse
resting or wandering over a handler-free region wakes JS with a fully
marshalled path per move (coalesced to engine-drain rate by
`pending_moves`, but unbounded in duration). That cuts against the
runtime's demand-driven ethos (no-polling event loop, idle-tick gating).
The cost per event is microseconds - nobody has profiled move dispatch as
a problem; the case is structural: the mask is the primitive that makes
dead-zone input genuinely free and is where any future Rust-side gesture
or hover logic would hang.

## Verified constraints (2026-08-01)

Consumers of the delivered `targets` array across core + components are
exactly two: the dispatch walk itself (window.ts `dispatchPath`) and the
pointerDown focus logic (`raw.targets.includes(focused)` for outside-tap
blur / keyboard activation). Nothing else reads it - press.ts works purely
through per-node handlers and bubble order, so order-preserving pruning
does not touch the arena. So the full path is load-bearing ONLY on
pointerDown, which is once per tap.

Emission cannot be skipped from bus state: environment.ts subscribes to
raw `pointerMove`/`pointerDown` for the mouseSeen/touchSeen fallback, and
window.ts is always subscribed, so `has_listeners` is always true.
Enter/leave/up/wheel have no consumer beyond handler dispatch and can be
skipped outright when the filtered delivery list is empty.

## Design

A per-element event-interest bitmask: down/up/move/enter/leave/wheel,
6 bits.

- Lives on `Element` (alloy rendertree, next to `interaction`). "Which
  event kinds this node observes" is engine-neutral, so rendertree's
  engine-independence holds; it dies with the element, no parallel map to
  leak. Written via a new rendertree method + one `flux:rendertree`
  export.
- JS side: ~10 lines in `applyProp`'s existing `on[A-Z]` intercept, which
  already sees every handler add/remove. Compare-and-skip keeps it to a
  couple of FFI writes per interactive node mount.
- Store full, deliver pruned. Both stored paths stay FULL:
  - The frozen down path: `locals_along_path` projects through every
    ancestor's transform/scroll, truncation-on-removal depends on the full
    chain, and a down handler that installs a move handler mid-gesture
    (lazy drag wiring) must start receiving moves - dispatch-time mask
    checks give that for free.
  - The hovered path: diffing pruned paths fires spurious enters when a
    node gains a handler mid-hover. Diff full paths, filter only the
    delivered `left`/`entered` subsets (`pick_locals` already handles
    arbitrary subsets).
- `e.target` stays the deepest node of the full path, computed before
  filtering - semantics unchanged.
- Per event kind: pointerDown delivers the full path always (focus logic +
  seen-flags) and is never emission-skipped; move/up/wheel deliver
  mask-filtered; enter/leave diff full then filter delivery; up/wheel/
  enter/leave skip emission when the filtered list is empty; move skips
  only after the stage-3 seen-flag migration.

## Failure mode to design for

Two sources of truth (JS handler map, Rust mask) can diverge. The
asymmetry helps: an overstated mask is harmless (the JS walk already skips
nodes without handlers), only an understated mask silently drops events.
Keep the writer one small testable function and the JS dispatch loop
defensive as-is. Optional dev-mode cross-check: deliver the full path,
assert the pruned set matches the handler map.

## Stages

1. Dispatch counters (mirrors `SETPROP_COUNT` in gui/tree.rs): events
   emitted / targets delivered / handlers actually run, per frame. Turns
   the microseconds claim into a number and gives stage 2 a before/after.
   Immediately doable, independent of the rest.
2. Mask + pruned delivery arrays. Always still emits; pure marshalling
   reduction, zero contract change, semantics identical by construction.
3. Skip empty emissions: up/wheel/enter/leave immediately; moves after
   migrating environment.ts's seen-flags off raw move/down events to a
   Rust-emitted sticky (fold into `inputDevices` or a `pointerSeen`
   sticky; `note()` already self-unsubscribes once satisfied).

## End-state alternative: handlers stored Rust-side

The stages above keep the JS handler map and teach Rust interest bits. The
terminal version of the same trajectory moves the pointer handlers
themselves into Rust: a plugin-side `HashMap<(nodeId, kind),
Persistent<Function>>` in context userdata, dispatch looks up and calls
only listening nodes directly - no bus emit, no JS-side walk, no parallel
arrays, and the registry IS the mask (the divergence failure mode above
disappears; skip-when-empty is the natural behavior rather than a stage).
Calling JS handlers from Rust is the established pattern, not new ground:
the event bus (`ListenerMap`, modules/events.rs), rAF (`RafCallbacks`,
gui/raf.rs), and timers all store `Persistent<Function>` in userdata and
restore + call, reporting throws via `report_uncaught`.

Constraints that shape it:

- Persistent discipline (the flux:wasm lesson, flux/CLAUDE.md): Persistents
  live in a context-userdata registry keyed by id, never in an rquickjs
  class. The registry cannot live on `Element` (rendertree stays
  engine-independent), so node destroy must clear its entries explicitly -
  that map is the new leak surface.
- stopPropagation crosses the boundary: Rust must observe a flag a handler
  sets mid-walk (native closure on the event object writing an
  `Rc<Cell<bool>>`, or a property read-back after each call).
- Reentrancy: handlers call setFocus, write signals, trigger effects that
  call setProperty back into Rust. Dispatch must keep the current shape -
  compute path + locals, drop all tree RefCell borrows, THEN call handlers
  one at a time - or a handler touching the tree panics the borrow.
- Partial coverage by design: key/text/focus dispatch routes along the
  focus chain and focus policy stays in JS (below), so the JS handler map
  survives for those. End state is two registries split by routing family
  (hit-tested events in Rust, focus-routed events in JS) - principled, but
  "single source of truth" is per-family, not global.
- Dividing line this respects: Rust may own dispatch (calling registered
  functions at the right moment - it already does everywhere below the
  renderer), JS owns reactivity (what those calls mean). For the same
  reason, storing SIGNAL SETTERS in Rust was considered and rejected: a
  setter is just a function so it is mechanically identical, but it bakes
  a Solid dependency into flux (which runs plain JS apps), the facts it
  would carry are rare window events where the bus hop costs nothing, and
  the sticky-replay pattern would have to be reinvented. Rust emits facts;
  JS makes them reactive.

Justify with the stage-1 counters before building: the mask gets most of
the marshalling win with a much smaller blast radius, so this step needs
the remaining delta (per-event JS walk + emit overhead) to show up in
numbers.

## Deliberately out of scope

- Moving focus to Rust: rejected. Focus is entangled with JS-only policy
  (text-session eligibility reads the handler map, session policy reads
  sticky inputDevices facts, per-node IME hints, synchronous
  onFocus/onBlur that press.ts's reactive `focused` depends on). JS owns
  focus policy and already pushes Rust the one fact it needs
  (`setTextInputActive`). With pointerDown unpruned, Rust never needs the
  focused id.
- `focusable` stays JS-side; focus-nav's cost is geometry batching
  (a `getBoundingBoxes(ids)` bulk query), a separate concern.
- No change to the JS destroy walk: it clears per-node JS maps (handlers,
  focusables, textHints) that must stay in JS regardless.
