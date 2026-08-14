---
title: Local pointer coordinates
description: Reintroduce per-node localX/localY on pointer events (already carried through hit testing, dropped in flux marshalling), and cap move hit-tests to one per pointer per frame.
created: 2026-07-25
completed: 2026-07-25
---

# Local pointer coordinates

Reintroduce the local-coordinate variant of pointer events that the pre-rewrite
runtime exposed (`event.localX`/`event.localY`, and the `onPointerMoveLocal`
event). Trigger: porting `~/solidrt-dev/solidrt/examples/spin.tsx` to the new
API. Its whole effect places balls at the pointer position inside a centered,
rotated, scaled view; the new `PointerEvent` exposes only window-relative
`clientX`/`clientY`, so the local frame is unrecoverable at the app level
without hand-inverting the transform chain. Per "design gaps grow the renderer"
this belongs in the runtime, not worked around in the example.

Revision 2026-07-25: no backward compatibility is required, so the original
"live-path locals with client-coordinate fallback" design was dropped for a
stronger contract: locals are always exact, for every pointer event, including
drags routed along the frozen down-path after the pointer left the node, and
enter/leave. The old runtime's event surface (checked against
`~/solidrt-dev/solidrt/packages/core/src/events/dispatch.ts`) also carried
`parentX`/`parentY` (pointer in the previous path node's frame) - the currency
of drag-and-drop, where a node's x/y live in its parent's frame - plus
`target`/`currentTarget`. Those come along; they are derived JS-side from the
same locals array at zero wire cost.

## The data already exists

Hit-testing already computes the pointer in each node's own coordinate frame and
carries it through the whole path:

- `alloy/src/rendertree/hit.rs`: `HitEntry = (u64, XY, XY)` = (node_id,
  parent-space point, **local point after the element's transform**). Filled in
  `hit_recursive` as `let local = element.kind.transform_to_local(point, &ctx)`.
- `View::transform_to_local` (`alloy/src/rendertree/kinds/view.rs`) inverts the
  node's full transform (x/y/rotate/scale) via the inverse matrix - exact for
  affine, an accepted approximation under perspective. So the local point is
  precisely "pointer in this view's own frame", which is what the example needs.

The remaining gaps were (a) marshalling - `flux/src/plugins/gui/input.rs` maps
the path down to `ids` and drops the local points - and (b) events routed along
a stored path rather than the live hit test (frozen-drag moves/ups,
enter/leave), where no locals were computed at all. (b) is closed by observing
that `transform_to_local` does not care about bounds: given any stored
root->leaf id chain, the pointer can be projected into every node's frame by
replaying `hit_recursive`'s descent math along that chain.

This is a core/runtime change only. Pointer events are emitted via
`emit_event` / `srt:events` and typed in `packages/core/src/types.d.ts`; they are
not part of the `flux-types` / `docs/flux.md` surface (grep is clean), so there
is no flux-parity doc to mirror.

## Design

`localX`/`localY` are the pointer position **in the coordinate frame of the node
whose handler is currently running** (its own transform undone). Because the
event bubbles leaf->root through several nodes, the value differs per node and
must be resolved as the event visits each one - it cannot be a single scalar on
the wire. It is carried as arrays parallel to `targets` and collapsed to a
scalar per handler in JS. The contract is unconditional: no event ever falls
back to client coordinates.

Per-handler derived fields, resolved in the same JS walk:

- `parentX`/`parentY`: the previous path entry's local point (`clientX`/`clientY`
  at the root, whose frame is the window). Documented caveat: "parent" is the
  previous node **on the hit path**, which skips `pointerEvents="none"`
  ancestors - the layout parent in normal trees, not in pathological ones. The
  old runtime had the same semantics.
- `currentTarget`: the node id whose handler is running; `target`: the deepest
  path node's id. `parentTarget` from the old runtime is dropped - no consumer.

Frozen down-path routing moves from JS into the engine. Today
`packages/core/src/window.ts` freezes the down path per pointer and routes
moves/ups along it; the gui plugin cannot compute locals for a path it never
sees. Instead the plugin stores the down path per `(PointerType, pointer_id)`
(next to the hover-path state already living in its `EngineState`; the JS map
was keyed by bare `pointerId`, a latent mouse/touch id collision this also
fixes), and every emitted event carries the exact routed targets plus their
locals:

- move/up with an active down: the frozen chain, locals via projection.
- move with no active down, wheel, down: the live hit path, locals already in
  its `HitEntry`s.
- enter/leave: `update_hover` projects along the full old/new chains and picks
  out the `path_diff` subsets, so they too carry true locals.

`window.ts` gets simpler: `downPaths` and the `frozen ?? targets` logic are
deleted; `bubble`/`dispatchOrdered` zip `targets` with the locals arrays and
set the per-node scalars before each handler.

Naming: keep the old `localX`/`localY`. The DOM's nearest analogue is
`offsetX`/`offsetY` (relative to the target's padding edge), but our value is a
transform-inverted content-frame point, not the DOM's, so reusing the DOM name
would mislead. `localX`/`localY` also matches the pre-rewrite API.

## Changes

### 1. Rust - `alloy/src/rendertree/hit.rs`

`locals_along_path(tree, chain, point) -> Vec<XY>`: project a window point
along a root->leaf id chain, replaying `hit_recursive`'s per-step math
(transform_to_local + child layout offset + scroll), no bounds checks. A
missing node truncates the chain (the tree changed under a frozen drag; the
suffix below a dead node is meaningless). Engine-independent tree math; unit
test in `alloy/src/tests/`.

### 2. Rust - `flux/src/plugins/gui/input.rs`

- `EngineState` gains `down_paths: HashMap<PointerKey, Vec<u64>>`; set on
  `PointerDown`, taken on `PointerUp`, consulted on `PointerMove`.
- `build_pointer_obj` takes a `locals: &[XY]` parallel to the ids and emits
  `localX`/`localY` arrays.
- Dispatch per event kind as in Design above. Touch-up leave keeps its current
  shape but with projected locals.

### 3. JS - `packages/core/src/window.ts`

- Delete `downPaths` and the frozen-path selection.
- `bubble()` / `dispatchOrdered()`: before invoking each node's handler, set
  `localX`/`localY`/`parentX`/`parentY`/`currentTarget`/`target` from the
  arrays; keep the shared-event + `stopPropagation` shape.

### 4. Types - `packages/core/src/types.d.ts`

Add to `PointerEvent` (doc-comment policy: document the per-node contract and
the path-parent caveat, not the obvious):

```ts
localX: number
localY: number
parentX: number
parentY: number
currentTarget: number
target: number
```

### 5. Examples

- `examples/spin`: delete the hand-rolled `toLocal()` inversion; handlers move
  onto the rotated view itself and read `event.localX`. A stroke must start on
  the spinner, but the drag stays exact off-element via the frozen-path
  projection - which exercises exactly the revised contract.
- `examples/drag` (new, core-level): cards inside a rotated + scaled container,
  dragged with plain `onPointerDown/Move/Up`: grab offset from `localX` at
  down, position from `parentX - offset` during move. The port of what the old
  `simple-drag.tsx` (`Motion.View draggable`) demonstrated, minus the Motion
  framework.

Follow-up note (separate change): `createPan`'s deltas are window-frame today;
the components-level gesture util is the natural `parentX` consumer if panning
inside transformed containers is wanted.

## Verify

- `cargo check` + alloy unit test for the projection.
- Rebuild runtime + client, then on both connected clients (desktop, Android):
  spin balls track the finger inside the rotating/scaled frame; drag cards
  track the pointer inside the transformed container; multi-touch shows
  distinct colors per finger.
- Gesture regression: components `createPress`/`createPan` consumers (gallery)
  still work - routing semantics are unchanged, only computed engine-side now.
- Re-confirm no `flux-types` / `docs/flux.md` pointer-event surface exists (so no
  parity edit): `grep -rniE "clientX|localX|pointerMove|targets" packages/flux-types docs/flux.md`.

## Related: move hit-test cost / frame-time hit testing

Surfaced while working this path (folded in here since it is the same hit-test
code). On Windows a high-poll-rate mouse (raw moves ~1000 Hz, like a browser's
uncoalesced pointermove stream) made per-event hit testing expensive.

State today:

- `lattice/src/lib.rs` (~L296-348) already coalesces each drain batch:
  `PointerMove` collapses to the latest position per pointer, frame signals to
  the newest. So one batch pays at most one move hit-test per pointer.
- `flux/src/plugins/gui/input.rs` dispatches on event **arrival**, not per
  frame - deliberately, so hover/move keep working when no frame is being
  produced (demand-driven idle). Each move = hit_test + JS emit + bubble +
  flush.
- `lattice/src/plugins/draw.rs` (~L179) runs `refresh_hover` after every
  produced frame, re-hit-testing each live pointer to catch layout moving under
  a stationary cursor. So frame-time hit testing already exists for hover.

The leak: batches are not frame-bounded, so many batches can run per frame, each
doing a hit test. Goal is "at most one move hit-test per pointer per frame."

Constraints on any fix:

- **Demand-driven rendering** is the hard one: arrival-time dispatch is what
  lets idle hover work with no frame in flight. Moving hit testing to frame time
  means input must pump something each event - so it needs a lightweight
  input-only pass (hit-test + dispatch, no full paint unless a JS handler
  dirties state), not just relocating the call, or idle hover turns into a frame
  pump.
- **Discrete events must stay prompt**: `down`/`up`/`wheel` carry ordering and
  latency needs; deferring a `down` by up to a frame hurts press feedback and
  risks reordering vs moves. They are rare, so keep them on arrival and coalesce
  only moves.
- **Measure first**: confirm whether the dominant cost is the Rust hit test or
  the JS round-trip before choosing.

Two staged options (minimal first):

- **Stage 1 (B): rate-limit moves to one dispatch per pointer per frame.** Keep
  arrival-time dispatch, but after dispatching a move for a pointer, suppress
  further moves for it until the next frame boundary while still tracking its
  latest position. Caps hit tests, preserves the "works with no frame" property,
  small diff.
- **Stage 2 (A): hit-test at onLayout/frame time** (the "hit-test after layout"
  idea). Cleanest coalescing and always-fresh layout, but needs the input-only
  pass above to preserve idle, plus keeping discrete events on arrival. Only if
  Stage 1 measurement shows the JS-side cost still dominates.

This is independent of the localX/localY marshalling above (same code, different
concern) and can land separately.

Update 2026-07-25: Stage 1 landed in stronger form as the per-pointer move
dispatch gate in `lattice/src/runtime.rs` (at most one move dispatch in
flight per pointer, latest position wins), after two-finger tablet drags
(240 moves/s) saturated the JS thread and replayed stale moves for seconds.
Frame-time sampling (stage 2's spirit) is now owned by
`okf/plans/frame-pacing.md` stage 3 (input resampling). Windows 1000Hz-mouse
verification done on the winbox 2026-07-26: the gate holds under the fast
mouse; item closed.

## Open items

- `isPrimary` on pointer events: known gap from the component-gestures work,
  orthogonal to coordinates, still open.
