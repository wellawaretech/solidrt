---
title: Signal-to-setProperty path, hop by hop
description: What runs between a Solid signal setter in app code and the Rust property setter it feeds - compiled effect shape, Solid 2 write/flush/recompute, the core renderer glue, the flux binding, apply_jsx and damage - and where the time goes, measured on the release runtime with probes/signal-bench.tsx; about 10 us per animated element per frame, three quarters of it Solid (setSignal + recompute + reads), the renderer glue and the FFI crossing about 0.4 us each per write.
created: 2026-08-19
---

# Signal-to-setProperty path, hop by hop

Read 2026-08-19 from the code as it stands: `@solidjs/signals` 2.0.0-rc.0
prod build (`node_modules/@solidjs/signals/dist/prod/core/*.js`, field
names mangled), `@solidjs/universal`, `packages/core/src/renderer.ts`,
`flux/src/alloy_plugins/tree.rs` + `properties/`, `alloy/src/rendertree/tree.rs`.
Reference case: `<d-rect x={x()} width={x()*2} color={c()}>` with `x`
written once per frame from a `requestAnimationFrame` callback.
`probes/compile-jsx.ts` prints the compiled shape for any TSX file;
`probes/signal-path-probe.tsx` is the reference case.

Companion: `ffi-crossing-costs.md` (mount path measured; one crossing is
about 0.25 us). This note is the map for measuring the update path.

## 0. Compiled shape

babel-preset-solid (universal generate) emits one render effect per JSX
element covering all of that element's dynamic props; static props go to
`createElement` once:

```js
var _el$ = _$createElement("d-rect", { y: 10, height: 20, onPointerDown: ... });
_$effect(() => ({ e: x(), t: x() * 2, a: c() }),
  ({ e, t, a }, _p$) => {
    e !== _p$?.e && _$setProp(_el$, "x", e, _p$?.e);
    t !== _p$?.t && _$setProp(_el$, "width", t, _p$?.t);
    a !== _p$?.a && _$setProp(_el$, "color", a, _p$?.a);
  });
```

Consequences: the compute phase allocates a fresh object per run and reads
every bound signal (here `x()` twice); the apply phase compares every bound
prop and calls `setProp` only for changed ones. Per-frame JS cost scales
with props bound per dirty element; FFI count scales with props changed.
`_$effect` is the universal renderer's `createRenderEffect(fn, effectFn,
{transparent: true, sync: true})`.

## 1. Signal write: `setX(v)` -> `setSignal` (core.js)

`setSignal.bind(null, node)`. Per call: transition check, override check,
`typeof v === "function"`, `isEqual`, `queuePendingNode` (push onto the
batch array), store the pending value, `insertSubs`, `schedule`.
`insertSubs` (scheduler.js) walks the signal's subscriber list and
`enqueueSub`s each into a height-bucketed dirty heap (O(1) per
subscriber). `schedule` sets a flag and queues a microtask `flush` (the
rAF path drains via `runFrame`'s explicit `flush()` first; the microtask
then finds nothing). Cost: O(subscribers) per write, small constant.

## 2. Flush: `runFrame` -> `flush()` -> `GlobalQueue.flush`

window.ts `runFrame`: animation-frame callbacks, then `flush()`, then
`renderFrame()`. Plain flush (no transitions): `commitPendingNodes`
(pending -> value for every written signal), `runHeap(dirtyQueue,
recompute)`, `commitPendingNodes` again, `clock++`, `run(EFFECT_RENDER)`,
`run(EFFECT_USER)`.

## 3. Effect compute: `recompute` (core.js)

The fattest JS function on the path; runs once per dirty element effect
(and per dirty memo above it) per frame. Per run: `deleteFromHeap`,
disposal bookkeeping (skipped when the effect owns nothing), a dozen
flag/field reads, save/restore of `context`/`tracking`/`stale`/
`latestReadActive`, then the compute fn. Each signal read in it is
`read.bind(null, node)` -> `read()`: a fast path behind about nine
conditions, then `link(source, observer)` which on a re-run usually hits
the same-dep-same-position early return.

The compiled effect returns an object, which would route the value through
`handleAsync` (promise/iterable detection) - but the universal renderer
creates its effects with `sync: true`, and a `CONFIG_SYNC` node takes the
direct `c = e.ce(c)` branch, so compiled JSX effects skip it. Plain
`createRenderEffect`/`createEffect` without `sync` do pay it (measured
below: about 2 us per effect per run). Then `clearStatus` guard, `trimStaleDeps`, comparator (effects have none, so
"changed"), set modified + `enqueue(EFFECT_RENDER, runner)`, commit
value, pending-node gate.

Rough size: a few hundred interpreted bytecodes per element effect per
frame. In QuickJS that is single-digit microseconds; at 1000 dirty
elements it is milliseconds.

## 4. Effect apply: `Queue.run` -> `runEffect` (effect.js) -> compiled body

Each queued bound runner: two guards, error-arm check, prev-cleanup call,
the compiled effect fn (destructure, one strict compare per bound prop,
`setProp` for changed), try/finally. `setProp` (universal.js) forwards
to the renderer's `setProperty` hook.

## 5. Renderer glue: `applyProp` -> `setTreeProperty` (renderer.ts)

Per write, in order: null check; a route decided by property name (event
handler, focusable, textInputHints, or plain tree write); then
`setTreeProperty`'s try/catch around the binding (a try block costs
nothing until it throws). As measured (2026-08-19) the classifier was a
`/^on[A-Z]/` regex exec plus name compares per write, ~0.4 us of the
per-write cost; routing is a function of the name alone, so it is now
cached in a per-name Map (routeFor), one Map get per write. Color parsing
moved to Rust with native transitions (colord removed), so no string
parse remains here.

## 6. FFI: `tree.setProperty(id, name, value)` (flux alloy_plugins/tree.rs)

rquickjs `Function::new` closure `(u64, String, Value)`. Per call:
- call trampoline, `u64` from number, the property NAME as a fresh
  `String` (alloc + copy; the interned-keys idea in
  backlog/ffi-write-batching.md), `Value` refcount clone;
- `SETPROP_COUNT` bump; `to_prop_value` (numbers free, strings allocate,
  arrays/objects recurse);
- `RefCell::borrow_mut` + `try_edit`:
  - `node_mut`: HashMap lookup (an eager `expect(&format!(..))` used to
    allocate the message on every call, hit or miss; fixed 2026-08-19 to
    `unwrap_or_else`, together with the `node()` sibling);
  - `apply_jsx` (properties/mod.rs): about six failed `name ==` compares
    (position, repaintBoundary, float, clear, pointerEvents, ...),
    `detached_only_geometry`, the per-kind `match name`, then
    `paint::apply`, then `layout::apply`. Short-string compares; tens of
    ns total, not where time goes;
  - `apply_damage`: `bump_revision`, `envelope.clear()`, and for
    `Damage::Paint` an `invalidate_paint` walk to the ROOT per write
    (each ancestor: `paint_cache.borrow_mut()`, `take()`,
    `envelope.clear()`) - O(depth) per write where O(depth) per frame
    would do; `sync_span_parent`: two lookups;
- `request_frame`: atomic store.

## Measured (2026-08-19, release linux client, QuickJS, Intel RPL-P laptop)

Harness: `probes/signal-bench.tsx`, N = 1000 `<d-rect>` each bound to its
own signal (`x` and `y` from it), every signal written every frame from one
`onFrame` callback, so 1000 setter calls, 1000 dirty element effects and
2000 `setProperty` writes per frame. The callback times its setter loop,
then calls `flush()` itself and times that, and logs medians over 120
frames; the native time inside the `setProperty` handler came from a
temporary `Instant` timer around the handler body (flux
alloy_plugins/tree.rs), drained per frame next to `SETPROP_COUNT` and
exposed as a `/stats` field for the measurement only (removed afterwards;
re-add the same way if the native share needs re-measuring). Variants via the
bench's MODE (`direct`: no signals, the frame loop calls the renderer's
`setProp` itself; `graph`: sync render effects of the compiled shape with
no element bound) and two temporary renderer.ts substitutions (B: the
`setProperty` hook calls `tree.setProperty` directly, skipping
`applyProp`; A: the hook is a no-op).

Trap: every frame is padded with a busy-wait to a fixed 12 ms load. Without
it CPU frequency scaling made the cheap variants look 3-4x cheaper (direct
mode A: 2.0 ms unpadded vs 0.55 ms padded) and the native time of
identical writes differed 2x between modes. Unpadded numbers are not
comparable across variants.

Medians, ms per frame, N = 1000 (2000 writes):

| variant                         | setter loop | flush | native setProperty |
|---------------------------------|------------:|------:|-------------------:|
| real path                       |        1.65 |  8.15 |      0.73 (2000 w) |
| B: skip applyProp               |        1.66 |  7.30 |               0.44 |
| A: no FFI (hook no-op)          |        1.75 |  6.40 |                  0 |
| graph, 3 reads + object (sync)  |        1.58 |  5.85 |                  0 |
| graph, 1 read, scalar (sync)    |        1.56 |  3.85 |                  0 |
| graph, 3 reads, NOT sync        |        2.3  |  ~7.9 (unpadded) |       0 |
| direct: 2000 setProp, real      |        2.35 |     0 |               1.12 |
| direct, B                       |        1.55 |     0 |               0.73 |
| direct, A                       |        0.55 |     0 |                  0 |

Per animated element per frame (about 9.8 us total), derived:
- `setSignal` + subscriber walk + heap insert: about 1.65 us per write.
  More than an FFI crossing.
- `recompute` bookkeeping + `runEffect` + queue: about 3 us per effect.
- each signal `read()` (bind trampoline + fast-path checks + `link`):
  about 0.8-1 us; the compiled effect reads every bound signal every run
  (`x()` twice here), plus about 0.4 us for the result object,
  destructure and compares.
- `applyProp` glue (regex, name compares, try): about 0.4 us per write.
- FFI crossing as seen from JS: about 0.45 us per write (rquickjs call +
  arg marshalling + the native 0.35 us of decode, dispatch, apply,
  damage).
- Frame loop body + universal `setProp` + hook call: about 0.28 us per
  write.

Shares of the real path: Solid (setter + recompute + reads + effect run)
about 75%, renderer glue about 9%, FFI about 9%, loop/call overhead the
rest. The flux binding and `apply_jsx` are not the problem; neither is the
`/^on[A-Z]/` regex on its own (its removal is worth about 0.4 us per
write, real but second-order). A non-sync effect costs about 2 us more per
run than a sync one (the `handleAsync` branch); app-level `createEffect`
users pay this, the renderer does not.

What this means for the update path: the cost is per dirty element
effect, dominated by Solid's generic machinery (read/link/recompute), in
an interpreter. Options, unranked: fewer reads per effect (the compiled
shape reads each bound signal per run; memoizing or reading once is in
babel-preset-solid's hands), a leaner effect kind for renderer bindings
(upstream), or moving animation-frequency motion out of signals entirely
(compositor-side animation / detached primitives written directly:
`direct` mode is 2.35 ms where signals cost 9.8 ms for the same writes,
and the Rust side would take them at 0.35 us each).

## After native transitions (2026-08-19)

With transitions declared on the elements, the bench's transition mode
(targets once a second, spring 700 ms) moves the whole path off the frame:
jsMs 0.05-0.1, setPropsPerFrame ~1. The frame cost is paint, and it scales
with nodes PAINTED, not nodes animated:

- N=1000 (all on screen): frame p50 1.8 ms, paint 1.6 ms, nodesPainted
  1001, 60 fps.
- N=4000 (grid wider than the window): p50 3.9 ms, paint 3.3 ms,
  nodesPainted ~2080 (culling drops the off-screen half), 58 fps.

So paint costs ~1.6 us per painted d-rect per frame, linear; layout and
the Rust tick are noise (layoutMs 0.01). Ceiling at 60 Hz: roughly 10k
painted animating nodes per frame. The next lever, if ever needed, is
paint-side (display-list build cost per node, culling, boundary
placement), not the write path; every animating node repaints by
definition (Damage::Paint on d-* geometry), so repaint boundaries only
protect static content around them, and transform/opacity animation on
views (Damage::Compose) skips even that.

Boundary effect measured (probes/boundary-bench.tsx: 4000 static d-rects
in one view + 50 continuously animating siblings): without a boundary the
root rebuild re-records everything - nodesPainted 4052, paint ~4.3 ms;
with repaintBoundary on the static view the walk replays its cached
recording - nodesPainted 51, paint 0.07 ms, frame p50 0.11 ms. So the
rule for animation-heavy screens: animators do not benefit from their own
boundary (their interior is damaged every frame), the static bulk around
them does, ~linearly in what the boundary fences off.
