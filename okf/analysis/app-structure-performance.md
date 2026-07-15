---
type: analysis
title: App structure and performance under QuickJS
timestamp: 2026-07-15T00:00:00Z
---

# App structure and performance under QuickJS

Structural review of how a SolidRT app executes, as of 2026-07-15: which layer
does what, what crosses the JS/native boundary and when, where performance goes
given a non-JIT interpreter (QuickJS), and whether moving SolidJS itself into
Rust would help. Grounded in packages/core/src/renderer.ts, core.ts, window.ts,
scroll.ts; flux/src/plugins/gui/tree.rs, input.rs, raf.rs, properties/;
lattice/src/runtime.rs; and the components package (spinner, scroll-view).

## The execution structure

A SolidRT app is five layers:

1. App TSX compiles via the Solid universal renderer into calls on the hooks in
   packages/core/src/renderer.ts (createElement, setProperty, insertNode, ...).
   The Solid 2 reactive graph (signals, memos, effects) runs entirely in
   QuickJS.
2. A ProxyNode mirror tree in JS caches id/parent/children, so tree queries
   (getParentNode, getNextSibling) never cross the FFI; only mutations do. Each
   mutation is one call into flux:rendertree. Node removal detaches immediately
   and defers destruction to an end-of-tick microtask sweep so moves survive.
3. The flux gui plugin (tree.rs) marshals each JS value into an engine-free
   PropValue, string-dispatches through properties::apply_jsx, and threads the
   resulting Damage (layout / paint / transform / none) back into the tree.
4. alloy's rendertree owns everything retained: taffy layout, Impeller paint,
   repaint boundaries, hit testing, compositing.
5. The frame loop is demand-driven. Every FFI mutation calls request_frame; on
   a produced frame the runner emits `render` into JS, whose handler
   (window.ts runFrame) runs onFrame callbacks, flush()es the Solid graph, then
   renderFrame() does layout+paint natively. Input dispatches on arrival, not
   per frame: SDL event, native hit test, one JS event object carrying the
   target-id path, JS-side bubbling with stopPropagation. Hover is re-diffed
   natively after each produced frame (moving content under a stationary
   pointer).

Division of labor: JS decides WHAT changes, Rust executes the frame. That is
the correct architecture for a slow interpreter, and most of the right calls
are already made:

- Fine-grained reactivity: no vdom, no diffing; only genuinely-changed props
  cross the boundary.
- ProxyNode caching: zero FFI reads for tree structure.
- Native hit testing; only the resulting id path crosses.
- PointerMove coalescing caps input dispatch at frame rate.
- Damage classification: a transform-class write costs no re-record (repaint
  boundary matrices are hoisted and applied at composite).
- Demand-driven frames: idle costs nothing; onFrame/rAF registration is itself
  the standing frame request.

## Findings, ranked by how much QuickJS amplifies them

### a. Per-prop FFI writes with string keys (the structural hot path)

Every setProperty marshals the property name as a fresh Rust String, then
apply_jsx walks chained `match name` string comparisons (element kind module,
then paint, then layout; an unknown layout prop is compared against every name
in three modules before erroring). Mounting a node with 10 props is 11 FFI
calls; a 200-item list with 5 nodes each is thousands of calls, each with
string allocation and linear dispatch. Mount/startup is where QuickJS (no JIT,
interpreting every Solid component closure) already hurts most, and this
multiplies it.

Cheap fixes, both mechanical:

- Intern property names to small integer ids: JS resolves name->id once per
  unique name (one Map lookup thereafter), native side dispatches on the id via
  a table instead of string matching.
- Batched creation: createNode(id, kind, propsObject) so mounting costs one
  crossing per node instead of one per prop. The universal renderer already
  hands static props to createElement as a single object; today they are
  fanned out into per-prop FFI calls.

### b. Per-frame animation runs through JS

The Spinner pattern: onFrame callback -> signal write -> effect ->
setProperty("rotate") -> FFI, every frame, per animated node. One spinner is
fine; 20 animated elements pay 20 closure invocations + graph flushes + FFI
calls per frame in an interpreter. This is the biggest strategic gap. Flutter
and CSS solve it with compositor-side animations: JS declares a target /
duration / spring once, the native side ticks the value and applies
transform-class damage. The Damage::Transform infrastructure that makes the
native tick nearly free already exists.

### c. Scroll physics will hit the same wall once momentum lands

Wheel events today are event-frequency (fine). scroll-view.tsx notes momentum
is not built yet; implemented in JS it becomes another per-frame JS loop on the
single most latency-visible gesture. Additionally createScroll re-clamps via
two getBoundingBox FFI reads per layout per scroll view - small, but a native
scroll offset (with a JS-observable position) would eliminate both. Build
momentum natively from the start rather than porting it later.

### d. Event-object garbage

Each pointer event builds a fresh Object + targets Array on the Rust side, then
`{ targets, ...e }` in the JS dispatch clones it again, plus a stopPropagation
closure per event. Move-heavy interaction generates steady garbage, and
QuickJS GC pauses land on the one thread that also triggers frames. Coalescing
caps the rate, so this is second-order, but the double clone via rest-spread is
free to remove.

### e. One thread for app logic and frame triggering

A slow effect or a GC pause directly delays renderFrame(). No structural fix
today (demand-driven rendering already minimizes exposure), but it strengthens
the case for b and c: what remains on the JS thread should be
interaction-frequency work only.

### Aside (correctness, found while reading)

apply_jsx and its shared decoders (f32_of, str_of, decode_radius, the
`position` match) panic! on malformed property VALUES, contradicting the flux
"never panic on JS input" rule; a typo'd property NAME throws a catchable JS
error, a typo'd value aborts the process. Also flagged in the flux crate
review (flux-crate-review.md).

## Should SolidJS move into Rust? Mostly no - move the frames, not the framework

Split by what "SolidJS code" means:

- The reactive graph + component logic: keep in JS. Porting the signal/effect
  runtime to Rust while user code stays JS makes things slower, not faster:
  signal reads are the hottest operation in Solid, and each would become an
  FFI round trip costing more than an interpreted property read. Compiling
  user TSX to native is a different product (that is Flutter/Dart) and
  forfeits hot reload and the JS ecosystem, which are the reasons SolidRT
  exists. App logic runs at interaction frequency, not frame frequency;
  QuickJS is adequate for it.
- Anything that runs every frame: yes, move to Rust; the architecture is
  already ~80% there. Layout, paint, hit testing, compositing, damage are
  native. The remaining per-frame JS is animation (b) and future scroll
  physics (c), both with well-understood declarative native designs.
- The escape hatch is already built: the redesign confines rquickjs to the
  plugin layer and keeps rendertree engine-independent. If app logic itself
  ever becomes the bottleneck, swapping in a faster engine is a bounded
  project; porting Solid to Rust is a rewrite. Protect that isolation.

## Proposed order (candidates for backlog when picked up)

1. Batched node creation + interned property keys (a) - mechanical, helps
   every app's mount time.
2. Native declarative animation riding the existing Damage system (b).
3. Native scroll physics when momentum is built (c) - build it natively
   first, do not port a JS implementation.

Measure before and after each step: the SETPROP_COUNT debug overlay plus
record/playback give a deterministic harness; capture a baseline trace of a
component-heavy example first.