---
title: The 2D extension (@solidrt/2d) and the buffer write lease
description: An instanced sprite layer as the third extension, built on a new zero-copy GPU buffer write lease in core; tiers, measurements, and the design decisions
created: 2026-08-19
---

# The 2D extension (@solidrt/2d) and the buffer write lease

Give SolidRT a 2D extension alongside @solidrt/3d: sprites, atlases, and
eventually tilemaps and retro presets. Positioning: not a game engine - the
2D layer of an app runtime, strongest where a 2D canvas lives inside a real
application (level editors, node graphs, board games, tools), with retro
games as a showcase rather than the product.

## The tier model

Benchmarks (2026-08-19, release client, fullscreen 2560x1440, d-texture
sprites from one atlas, motion via native x/y springs) settled a three-tier
architecture:

1. **Sprites as elements** (`d-texture` + srcX/Y/W/H): full participation in
   transitions, gesture arena, and inspection. Measured ~0.65us paint and
   ~15KB memory per node, zero layout cost; 60fps ceilings on a desktop RTX
   3070 machine: ~10k animating (springs), ~19k static. The right tool into
   the low thousands.
2. **The instanced sprite layer** (@solidrt/2d v1): one atlas, one instance
   buffer, N quads in one draw. For dense per-frame-animated populations,
   where tier 1's limit - two setProperty FFI calls per moved sprite per
   frame - is the bottleneck (QuickJS measured ~8us per call; native
   transitions cover retarget-style motion but not per-frame physics).
3. **Baked layers** (staged, okf/backlog/2d-baked-layers.md): static bulk as
   ONE quad. On tiled GPUs the budget is primitive count, not pixels or draw
   calls (@solidrt/core agents/performance.md, "Where GPU work stops being
   free"), so a tilemap is not ten thousand quads - it is a texture.

## The buffer write lease (core, landed with this plan)

Tier 2 requires per-frame sprite data to reach the GPU without per-sprite
FFI writes and without copies. Options compared (full write-up preserved in
the plan session; summary):

- **A. begin/end lease + publish fence** - `beginBufferWrite(id)` hands JS a
  Float32Array over a runtime-owned pooled block, `endBufferWrite` publishes
  by MOVING the block across the raster channel (the UpdateYuv precedent).
  Zero copies, correctness by ownership handback, one-shot writes work,
  and it is literally the deferred stage-4 API of
  okf/done/texture-upload-staging.md (mapped PBOs slot under it unchanged).
- **B. Registered streaming buffer sampled at frame time** - no per-commit
  call at all; raster samples registered blocks during Frame handling,
  demand carried by the raf contract. Decisive finding: the "skip upload on
  unstable seqlock generation" guard is unimplementable as stated - the GPU
  buffer is already overwritten when instability is detected - so sound-B
  needs a raster-side scratch memcpy and ends up with MORE copies than A,
  plus a hand-rolled seqlock and a writes-outside-a-frame-flow footgun.
- **C. pooled writeBuffer** - cannot be zero-copy at all (JS-heap bytes must
  be copied once before the borrow ends); its pool is A's pool.

Decision: **A now, B recorded as the follow-on**
(okf/backlog/gpu-stream-buffers.md). Landed as: `WriteLeases` pool in
alloy/src/gpu/lease.rs, `RasterCmd::WriteBufferLease` with an embedded
recycle sender, `Context::begin_buffer_write`/`end_buffer_write`,
`beginBufferWrite`/`endBufferWrite` in flux:gpu (the JS view minted with the
wasm.rs no-free-callback pattern; detach-first on end/destroy), a
`createBuffer(byteLength)` zeroed overload, and core wrappers. Verified by
cargo tests (alloy/src/tests/gpu_lease.rs) and a pixel-level check on the
playback client (packages/core/checks/gpu-lease-check.tsx - flux:gpu is
behind the gui feature, so GPU checks run via `srt render`, not the headless
flux binary).

## v1 scope (landed)

packages/2d: frames.ts + pick.ts (pure, checked headless via
packages/2d/checks/), atlas.ts (decode + nearest option - createImage never
forwards sampler options), layer.ts (the retained layer: canonical
Float32Array, microtask flush through the lease, camera as shared params,
rotated-rect picking with capture/hover), components.tsx (SpriteLayer/Sprite
faces). Registered in the root workspace and the CI typecheck list (adding
the previously-missing packages/3d as a drive-by).

Deliberately NOT in v1: z-ordering (insertion order only), dirty-range
publishing (whole live prefix per dirty frame), baked/tilemap layers,
frame-animation helpers, retro presets, website/docs pages (the API should
settle first; docs/30-extensions/index.md still says "two").

## Staging after v1

Re-ordered 2026-08-22, after a pass over the package against what a 2D-heavy
application needs. The original order took the tier model straight down
(baked layers next, then presets); using the package first surfaces cheaper
things that block apps sooner. Tiering is a performance argument, and none of
the items below is one - they are gaps in the layer as an API.

1. **Capacity growth** (okf/backlog/2d-layer-capacity-growth.md). The only
   item here that is a runtime failure rather than a tradeoff: past
   `capacity`, `addSprite` throws and the app has no recovery. First.
2. **A sort key** (okf/done/2d-sprite-sort-key.md). Supersedes the
   "z-ordering" line above. Raising a sprite or depth-sorting a population by
   y is remove-plus-re-add today, i.e. per-element churn in the one package
   whose premise is that per-element costs are what kill you.
3. **The frame animation helper** - `createAnimation(frames, fps)`, pulled
   out of the retro presets item, which was gating a handful of lines every
   sprite population wants behind a demo kit.
4. **Baked layers** (okf/backlog/2d-baked-layers.md), unchanged in substance
   and still the big one: primitive count is the budget on tiled GPUs. Now
   also carries the spatial index, since culling and picking want the same
   grid the chunking already implies.
5. **Atlas limits** (okf/backlog/2d-atlas-limits.md). One immutable
   pre-packed sheet per layer: a second sheet costs a whole second render
   target, and images arriving at runtime have no way in. Normal for an
   application, unusual for a game, which is why v1 did not feel it.
6. **The display-scale decision**
   (okf/backlog/2d-layer-display-scale.md). Layer output is upsampled on a
   HiDPI screen unless the app sizes and zooms for it by hand. Wants deciding
   once across @solidrt/2d and @solidrt/3d, not implementing twice.

Dirty-range publishing stays deferred and measurement-gated: one moved sprite
republishing the whole live prefix is a single memcpy - ~520KB at 10k
sprites, microseconds - and keeping one write path is worth more than the
saving until a profile says otherwise.

What stays OUT of the package, restated because it keeps coming up: shapes,
text, gestures and selection models. The layer composites as an ordinary
texture inside the render tree, so labels, handles and chrome are core
elements drawn on top. Core is where the platform's 2D lives; this package is
tier 2 of that story, not a parallel renderer. Gaps found in core while
looking at 2D-heavy applications belong in core's own items (shadows,
clip-to-path and masks, dashes beyond `line`, pattern/image fill - see
okf/backlog/texture-tile-mode.md for the last one's engine side).
