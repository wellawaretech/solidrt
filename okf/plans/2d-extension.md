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
   calls (scaffold AGENTS.md "Where GPU work stops being free"), so a
   tilemap is not ten thousand quads - it is a texture.

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

Deliberately NOT in v1, in staging order: z-ordering (insertion order only),
dirty-range publishing (whole live prefix per dirty frame), baked/tilemap
layers, frame-animation helpers, retro presets, website/docs pages (the API
should settle first; docs/30-extensions/index.md still says "two").
