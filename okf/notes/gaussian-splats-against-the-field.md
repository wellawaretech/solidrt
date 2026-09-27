---
title: The splat viewer against the field
description: Where the shipped splat viewer stands against the WebGL viewers (antimatter15, GaussianSplats3D, PlayCanvas, Babylon) and the compute renderers (the reference CUDA rasterizer, Unity's, the WebGPU ports) - the data layout is the WebGL standard, the in-engine sort and the 16-splat instance are ahead of it, the compute class wins on per-frame sorting, front-to-back tile blending and compression, and GLES 3.1 compute is not on the table, so the fill wall is answered by overdraw cuts only.
created: 2026-09-27
---

# The splat viewer against the field

Written 2026-09-27 at the close of okf/done/gaussian-splats.md, answering
"is this how other engines do it, is it best in class, where were the
shortcuts". The measurements behind it are in
[gaussian-splats.md](gaussian-splats.md). Constraint first: the GLES 3.0
floor is the renderer's floor, and **GLES 3.1 (compute) is not on the
table** - every "best in class" gap below that needs compute stays a
documented limitation, not a plan.

## What is the standard form, not ours

The shipped data layout - two `rgba32ui` texels per splat (center + packed
rgba8 | six covariance halves), fetched by a depth-sorted index stream -
is antimatter15's layout, and GaussianSplats3D, PlayCanvas and Babylon use
the same shape (data textures plus a per-instance sorted index from a
worker sort). The plan's survey claimed the viewers "rebuild the
covariance at each of the quad's four corners"; that was wrong for the
ones that matter - antimatter15 and GaussianSplats3D bake it at LOAD time
in the worker. Our bake moves it to pack time, which buys load time and
no load-time memory spike, not per-frame work (stage C measured vertex
ALU as invisible anyway).

## Where this is ahead of the WebGL class

- The sort runs in the engine, synchronous with the flush: the drawn
  order is the current camera's. Every surveyed viewer sorts on a worker
  and lags by frames, the popping on fast orbits.
- A parked camera uploads nothing (the unchanged-permutation gate), and a
  re-sort moves 4 bytes per splat.
- 16 splats per drawn instance. The viewers draw one quad per instance,
  which on Mali is two thirds of the vertex side (12.6 cycles per
  instance); desktop GPUs never showed them that cost.
- The cloud is an ordinary mesh: depth-tested against geometry, picked by
  its bounds, transformed by its node, blended in the scene's blend space
  instead of a splat-private target.

## Where the compute class is ahead, and stays ahead

The reference CUDA rasterizer, aras-p's Unity implementation and the
WebGPU ports (SuperSplat, the gsplat viewers) win on three structural
points, all of which need compute:

- A GPU radix sort every frame - exact, off the CPU, no gate and no lag.
- Tile blending front-to-back with early termination: a fully occluded
  splat costs nothing. Our back-to-front alpha blend pays for every
  occluded fragment; that is the ~24 ms fill at 1M full res on the Pixel
  7 (the display-mode A/B showed the buffer format does not touch it on
  a tiler). With 3.1 off the table this wall is answered only by drawing
  less: [../backlog/splat-overdraw-fill.md](../backlog/splat-overdraw-fill.md).
- Compression, which is not compute-bound but the compute viewers all
  have: spz, PlayCanvas's sogs and ksplat hold a splat in 8-16 bytes
  (chunked quantized positions, 8-bit SH). Ours is 32 bytes plus
  uncompressed half-float SH: ~130 MB resident for a 1M SH3 cloud where
  theirs is ~30 MB.
  [../backlog/splat-record-compression.md](../backlog/splat-record-compression.md).

## Shortcuts in the shipped implementation

- The records are resident twice: the engine's sort-key copy (32 bytes
  per splat) and the texture. The key needs only the 12-byte position.
  [../backlog/splat-key-copy.md](../backlog/splat-key-copy.md).
- The sort blocks the UI thread. Its per-re-sort spike at 1M was never
  isolated (the JS p50 hides a periodic spike) - the one cost of this
  design that is not measured.
  [../backlog/splat-sort-spike.md](../backlog/splat-sort-spike.md).
- The order refreshes past a ~2 degree gate (ORDER_DIRECTION_EPS_COS), so
  inside the gate it is stale. The viewers are lagged instead; neither is
  exact. Lowering the gate is cheap now that a re-sort moves 4 MB (see
  okf/tiny.md).
- The rasterizer's fidelity knobs: the reference's 0.99 per-splat alpha
  cap is not applied, and the antialiasing dilation carries no
  Mip-Splatting compensation, so a capture trained in an antialiased
  mode reads slightly too opaque at small sizes.
  [../backlog/splat-raster-fidelity.md](../backlog/splat-raster-fidelity.md).
- The count rounds up to groups of 16 (at most 15 importance-sorted tail
  splats extra; cosmetic).
- No level of detail or streaming, as the plan scoped out; the
  importance-sorted prefix is the only dial.
