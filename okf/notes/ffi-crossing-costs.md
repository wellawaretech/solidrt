---
title: FFI crossing costs, measured
description: What one JS-to-Rust property write costs on the release runtime (about 0.25 us including decode, string dispatch and apply), what share of a 3000-node mount is FFI (about 10 ms of 118), and why batching creation into one crossing per node changed nothing - mount time goes to per-component JS; the per-frame update path is not measured here.
created: 2026-08-18
---

# FFI crossing costs, measured

Measured 2026-08-18 on the release linux client (QuickJS, Intel RPL-P
laptop), against `ac230d1`. Two harnesses, both throwaway apps run under
`srt run` with the numbers read from `/__control__/logs`:

- a microbench that imports `flux:rendertree` directly and times raw
  `createNode`/`setProperty` loops with `performance.now()`, best of 5;
- a mount bench that renders 3000 `<d-rect>` components with 8 static
  props each through Solid + the core renderer and times the synchronous
  `render()` call, ~60 samples per build via reloads.

Both were run against HEAD (one crossing per prop) and against a build
with batched creation (`createNode(id, kind, propsObject)`: one crossing
per node at mount, props applied in Rust in insertion order, rejections
returned as a list). Numbers were stable to within a few percent across
repetitions.

## Raw crossing costs (3000 nodes, 8 props)

| loop | crossings | HEAD | batched build |
|---|---|---|---|
| createNode, no props | 3,000 | 2.4-3.8 ms | 2.8-4.4 ms |
| 24,000 x setProperty, one numeric name | 24,000 | 6.1 ms | 6.1 ms |
| 24,000 x setProperty, 8 mixed names | 24,000 | 7.8 ms | 7.8 ms |
| createNode + 8 x setProperty | 27,000 | 10.2 ms | 10.3 ms |
| createNode with one shared 8-prop object | 3,000 | - | 7.1 ms |
| createNode with a freshly built 8-prop object | 3,000 | - | 10.4 ms |
| empty JS loop, 24,000 iterations (baseline) | 0 | 1.7 ms | 1.7 ms |

So one `setProperty` crossing costs about **0.25 us** all in: rquickjs
call, `to_prop_value` decode, the chained string dispatch in `apply_jsx`,
the edit, the frame latch. A bare `createNode` is about 1 us including node
creation. Building an 8-key object in QuickJS costs about 1.1 us, i.e.
roughly the same as the 8 crossings it would replace, which is why the
"fresh object" row equals the per-prop row.

## The mount that matters

`render()` of the 3000-component app: **HEAD 118.5 ms median; batched
build ~123 ms** (within noise, if anything slightly worse). Crossings for
that mount went from ~27,000 to ~3,000; wall time did not move.

FFI is therefore ~10 ms of a 118 ms mount. The remaining ~108 ms, about
36 us per component, is Solid's component/effect machinery, the universal
renderer, and the core renderer's proxy-node bookkeeping, all interpreted
by QuickJS.

## Consequences

- Batched creation is not worth it: it attacks a ~0.25 us per write cost
  and had to build a second filtered props object in JS (events, focus,
  text-input hints and color parsing stay JS-side), which ate the raw win.
  It was reverted. The variant that would make it pay (pass the JSX props
  object straight through and let Rust skip the JS-owned names) buys ~3 ms
  per 3000 nodes at the price of pushing renderer policy into the plugin
  layer.
- For mount, the lever is per-component JS cost: what Solid does per
  component and effect on QuickJS, and what the core renderer adds per
  node. Caveat: these runs used the dev bundle (Solid's `dist/dev.js`);
  a production bundle may be materially cheaper per component, which is
  worth measuring on the same harness.
- This says nothing about the update path. A signal-heavy app spends its
  frames in effects writing props, not creating nodes; how many crossings
  such a frame makes, and what share of `jsMs` they are, is unmeasured.
  The per-crossing figure here (0.25 us) is the constant to plug in once
  a limit-pushing app provides the counts (`setPropsPerFrame` in
  `/stats`). The one-call drain, interned keys and shared-buffer stages of
  okf/backlog/ffi-write-batching.md wait on that number.
