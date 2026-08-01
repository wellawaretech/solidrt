---
type: backlog-item
title: Per-node event-interest mask for pointer dispatch
description: Rust marshals the full root-to-leaf hit path into JS for every pointer event because only the JS handler registry knows which nodes listen; a per-element event-kind bitmask lets dispatch deliver only listening nodes and (staged) skip empty emissions entirely, making input over handler-free regions free.
status: open
timestamp: 2026-08-01T00:00:00Z
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
