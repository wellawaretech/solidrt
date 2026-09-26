---
title: Gaussian splat rendering
description: Captured 3DGS scenes (phone scans, photogrammetry successors) are a growing content class nothing here can display. The viewer is proven by a probe (300k splats in 19 ms at full resolution on a 2022 phone) and the shape settled 2026-09-26 - a pack-time bake to a .srts record with the 3D covariance precomputed, a generic instanceOrder knob on the 3d meshes riding gpu-instance-order's retained projected key with the scene feeding direction, SplatMesh over createRecordMesh, and SH bands via an id-indexed data texture.
created: 2026-08-24
---

# Gaussian splat rendering

## Symptom

3D Gaussian splatting is becoming the default format for captured scenes -
phone scans, drone captures, product spins - and none of it can be shown.
The format is simple (a point list: position, covariance as scale +
rotation quaternion, opacity, spherical-harmonic color), the renderer is
not a mesh pipeline, and every platform that matters to capture content
already has viewers. This is a content-class gap, not a feature-parity
one.

## What the render path needs, against what exists

The standard non-compute approach (every WebGL viewer): each splat is an
instanced camera-facing quad; the vertex stage projects the 3D covariance
to a 2D conic, the fragment evaluates the gaussian falloff times opacity,
and everything blends back-to-front with depth-write off.

Already landed, and proven together by the probe in Findings: instanced
materials with per-record attributes (`shaderMaterialClass({
instanceBuffers })` + `createRecordMesh`, records being opaque data rather
than scene nodes), instance streams in any vertex format
([3d-instance-records-as-bytes](../done/3d-instance-records-as-bytes.md)),
custom GLSL with the shared camera params, `transparent: true` with
premultiplied output and depth-test-without-write, the scene transparent
sort placing the cloud against other transparent meshes, and bounds-based
picking for a record mesh given explicit bounds.

The within-cloud order also exists in full:
[gpu-instance-order](../done/gpu-instance-order.md) landed all three
stages (field/projected keys, gather-at-publish, `retain: true` with the
core-side re-sort on an `orderDirection` update and the
unchanged-permutation gate). What is missing is only the exposure - no 3d
mesh passes `instanceOrder` through - so the JS-sort interim the original
staging carried is dead: measured useless in motion at any size worth
showing (Findings), and nothing depends on it. The plan goes straight to
the retained projected key.

The remaining wall is the per-splat vertex-side work: ~21 ms per million
splats on the Findings phone, untouched by resolution. The answer moves
into the file format (covariance baked at conversion), with a measured
escalation path.

## What the shipping viewers get wrong

Surveyed 2026-09-26: antimatter15/splat, mkkellogg's GaussianSplats3D
(three.js), Babylon's GaussianSplattingMesh, PlayCanvas. Four shared
mistakes, each answered structurally here rather than by effort:

- **Async sorting.** All sort on a worker (or WASM), decoupled from the
  frame; the order lags the camera by several frames and fast orbits show
  popping. Ours sorts core-side in Rust, synchronously with the flush: the
  order a frame draws is the order its camera implies. And the
  parked-camera gate (permutation compare, nothing uploads) is a profile
  none of them has.
- **Splats as a parallel world.** mkkellogg's viewer wraps the renderer
  rather than being a mesh; Babylon special-cases its render ordering.
  Splats then compose badly with meshes, UI and picking. Ours is an
  ordinary record mesh: depth-tested against opaque geometry, placed by
  the transparent entry sort, picked by bounds, transformed by its node.
  Nothing splat-shaped enters the scene model.
- **Runtime format parsing.** Viewers parse `.ply` at runtime -
  multi-second stalls, 2x memory spikes, a format zoo in the client. Ours
  converts at pack time under Bun (the
  [3d-model-loader](3d-model-loader.md) direction); runtime is one fetch
  and one buffer upload of bytes that already are the records.
- **The raw training record in the vertex stage.** Everyone ships
  scale + quaternion and rebuilds the covariance at each of the quad's
  four corners; that is the measured vertex wall. A bake step can
  precompute it once per splat, which no bake-less viewer can.

One thing they do that must not be casually "fixed": they blend in sRGB,
and captures are TRAINED against sRGB blending, so linear-light blending
(our scene contract) can diverge from the trained appearance. That is a
side-by-side acceptance check, not a foregone correction (Findings
already flags it as uncompared).

## The container and bake tool

`srt tool 3d/splat <in.ply|in.splat|in.spz> [-o out.srts] [--sh 0..3]
[--keep-orientation]`, mirroring `3d/model`: the parser lives in a
runtime-free `@solidrt/3d/splat` entry (the `model-data.ts` shape), shared
by the tool and any app bake script.

- Inputs: `.ply` (the trainers' output), `.splat` (the de-facto
  interchange), `.spz` (Niantic's compressed format, what phone scanning
  apps export; gunzip + fixed-point decode). Not `.ksplat` - one viewer's
  private cache format.
- The `.srts` record, 28 bytes: position `float32x3` (12) + 3D covariance
  upper triangle as six `float16` (12) + sRGB color + opacity `unorm8x4`
  (4). Positions stay float32 deliberately - half positions are Babylon's
  known precision mistake on large scenes; covariance quantizes to fp16
  robustly. This is the old stage 3's cheapest option promoted into the
  format: it deletes the quaternion decode, the rotation matrix build and
  the M*Mt from all four corners, and lands below `.splat`'s 32 bytes.
- Header: count, layout/SH degree, explicit bounds (picking wants them),
  record region offsets.
- Orientation baked to y-up by default (COLMAP captures are y-down; the
  probe needed a half-turn FLIP). Rotating positions and covariances at
  bake is exact and free, and it removes the footgun every viewer pushes
  onto the app: a baked splat drops into a scene like any model.
  `--keep-orientation` opts out.
- Records written importance-sorted (size times opacity), as `.splat`
  files are, so a count prefix is a usable scene at any N.
- SH degree 1-3 (`--sh`): the coefficients beyond DC go in a half-float
  data texture (riding
  [gpu-float-texture-formats](../done/gpu-float-texture-formats.md))
  indexed by a per-record splat id (`uint` attribute, +4 bytes) - not
  extra instance streams. The id travels with the record through any
  permutation, so the SH texture never republishes on a re-sort; sibling
  streams would republish 90 B/splat (SH3) on every camera move. Default
  degree 0; SH1 is +18 MB at 1M splats, SH3 +90 MB.

`.srts` is ours; no compatibility constraints while this lands.

## The ordering knob (generic, splats are the first consumer)

`instanceOrder` on `createRecordMesh` and `createInstancedMesh` - the
package-level face of core's settled `InstanceOrder`, speaking attribute
names instead of byte offsets:

```ts
instanceOrder?: ({ position: string; retain?: boolean } | { field: string })
  & { descending?: boolean; stream?: number }
```

Any transparent instanced population wants it (a particle population is
projected-key + gather, per the settled core design). What it takes:

- The mesh resolves the attribute name to the core offset/buffer index
  and passes `instanceOrder` through to the entry.
- An ordered mesh's publishes (the load upload included) switch from
  `writeBuffer` (throws on an ordered buffer) to the lease
  (`beginBufferWrite`/`endBufferWrite`).
- The direction feed is the scene's, not the app's. The spatial core
  already holds each sorted target's view (eye and forward, what the
  entry-level draw sort keys from since 2026-09-22); depth-ordered
  entries get `orderDirection` fed from that same view, mapped through
  the inverse of the entry's model rotation (the probe did this mapping
  by hand; automatic, or every consumer re-derives it wrong). Gated by a
  named angle threshold (`ORDER_DIRECTION_EPS`, set by measurement): a
  projected key is dot(position, direction), translation-invariant, so
  only rotation triggers a re-key at all, and with retain an unchanged
  permutation uploads nothing. Zero per-frame JS; a parked camera costs
  nothing.

## The viewer surface

- `@solidrt/3d/splat` (runtime-free): `SplatData`, `parseSplat` (the
  three input formats), `encodeSplat` (`.srts` out), `loadSplat` (a
  zero-copy `SplatData` view over fetched `.srts` bytes, the `loadModel`
  shape).
- `createSplatMesh(data, opts): SplatMesh` - a `RecordMesh`
  specialization: the stock splat material (the probe's, minus the
  covariance build), bounds from the header, `instanceOrder: { position:
  "iCenter", retain: true, descending: true }`. A `<SplatMesh>` component
  wraps it with the standard node props, next to `<RecordMesh>`. The
  material class stays exported for apps that fork the shader - primitive
  accessible, convenience on top.
- The probe's tuning constants carry over as the named constants they
  already are (the e^-4 extent, the 0.3 px^2 dilation, the sigma cap, the
  frustum margin, the min alpha).

## Measurement gates

- Baked covariance against the probe's half-resolution column (the vertex
  wall). If ~21 ms/M does not drop enough for 1M on the Pixel 7, the
  escalation is a transform-feedback projection pass (once per splat per
  camera change, GLES 3.0, no compute) - which no WebGL viewer ships. The
  index-texture layout stays the last resort: it changes what the
  ordering orders (an index stream, not records).
- The core re-sort at 1M on the phone (Rust radix + 28 MB gather +
  upload): this sets `ORDER_DIRECTION_EPS`, measured, not guessed.
- Linear vs sRGB blending side by side against a reference viewer, judged
  by a human. If linear visibly diverges from the trained appearance,
  decide then - the one place "correct" and "faithful to the capture"
  can disagree.
- The fill share: the half-float scene buffer moves twice the blend bytes
  of the viewers' rgba8 targets (7-10 ms of the phone frame is fill).
  Measure; a splat-private render target would break scene composition
  (the parallel-world mistake), so it needs to be very much worth it.
- Retake the desktop reference run with the window visible (the first run
  was throttled to 15 fps and useless).

## Stages

Each stage has standalone value.

- **A - the ordering knob.** `instanceOrder` on both mesh kinds, the
  lease publish switch, the scene direction feed. Verified by a probe
  re-ordering a transparent instanced population with zero JS.
- **B - the viewer.** `@solidrt/3d/splat`, the bake tool, `loadSplat`,
  the material, `createSplatMesh`/`<SplatMesh>`. Done looks like: a few
  hundred thousand splats orbiting smoothly on desktop and on the
  Findings phone (release client), order updates with zero per-frame JS,
  parked camera renders nothing new, verified through the probe's bench
  ladder.
- **C - the vertex wall.** The covariance-bake measurement, and the
  transform-feedback escalation only if the numbers demand it.
- **D - SH bands.** `--sh` at bake, the id-indexed data texture,
  view-direction evaluation in the vertex stage (camera position is in
  the shared params). Skinning wants the same texture machinery, so this
  pays twice.

## Not in this item

Training or editing splats, compute-tile rasterization (no compute on the
GLES 3.0 floor; a 3.1 probe-up could revisit, but the core CPU sort makes
it a nice-to-have), streaming/LOD for city-scale captures, runtime-fetched
user splats (the runtime-content problem is general, not splat-specific).

## Findings

Appended during the work, per the rule in [../README.md](../README.md); cut
into `notes/` when this closes.

- Stage A landed 2026-09-26 (uncommitted): `instanceOrder` on
  `createRecordMesh`/`createInstancedMesh` and both components
  (`{ position: name, retain?, descending? } | { field: name }`, resolved
  against the material's instance layouts at creation, matched to the
  entry's pipeline buffer by layout key at attach); ordered publishes
  through the lease; and the core-side direction feed - `orderFeed` on
  `bindDraw` marks the scene entry's sink, and a flush pass in the
  spatial core writes the view forward mapped into the node's model
  frame through a new `SinkWriter::write_order_direction`, which the
  context routes to `set_instance_order_direction` +
  `rematerialize_retained_order`. Verified: `cargo test -p alloy --lib`
  (608, three new feed tests), `cargo test -p flux --lib --features
  gui`, `srt check` on the 3d package, and `probes/order-3d-probe.tsx`
  end to end on a fresh release client - `/buffer` reads the gathered
  records back far-to-near for the front camera and exactly reversed
  after a `view` flip to behind, with no record publish from JS between
  the readbacks.
- The mapping needs no inverse: dot(W p + t, f) = dot(p, W3x3^T f) +
  const, so the model-frame key direction is the world's transposed
  upper 3x3 times the view forward - exact under any scale, and
  translation-invariant, so only ROTATION (camera or mesh) ever re-sorts
  a projected-key population; the feed re-fires past
  `ORDER_DIRECTION_EPS_COS` (~2 degrees, set by feel - measure the
  re-sort plus republish cost at splat scale in stage C and revisit).
- One ordered entry per buffer (the core registry's rule) decides the
  multi-target story: the SCENE target's entry declares the order and
  the feed; views and shadow passes bind the same gathered buffers
  unordered and draw whatever order the scene's camera decided. A mesh
  layered out of the scene target keeps its last order. Per-view record
  order is structurally impossible with one buffer - not a gap to fix.
- The ordered publish contract that fell out: the sorted population is
  the PUBLISHED byte length, so an ordered mesh publishes the live set
  whole, `[0, count * stride)`, on any dirty range (a partial write has
  no stable position under a permutation), `setRecordCount` republishes
  on every change (the first n gathered records are NOT slots 0..n, so
  a shrink without a republish draws the old population's nearest
  records), and a (re)attached ordered mesh republishes whole to seed
  the entry's freshly built order registry (mirrors do not survive
  removeDraw). A failed direction write stops the feed but keeps the
  sink - the cull path's release-on-false would kill uModel over a lost
  order.
- `alloy/examples/draw_ordered.rs` (uncommitted) no longer compiles, and
  not from this work: it predates the open-vertex-layouts API churn
  (`PipelineDesc.attributes`, `DrawSpec.buffer`,
  `BufferUpdate.instance_buffer`). The lib tests and the JS probes carry
  its coverage meanwhile.

- Stage B landed 2026-09-26 (uncommitted): `@solidrt/3d/splat`
  (src/splat-data.ts, runtime-free like model-data) with `parseSplat`
  (.ply / .splat / gunzipped .spz v2-v3), the covariance bake
  (importance sort, y-up flip by conjugation - negate the xy and xz
  entries - fp16 clamped at 65504), `encodeSplat`/`decodeSplat` for the
  .srts container ("SRTS" u32 | version | jsonLength | json | records,
  version 1); `srt tool 3d/splat` (tools/splat.ts, gunzips .spz
  itself); runtime-side `loadSplat` + `createSplatMesh`/`<SplatMesh>`
  over the stock material (SPLAT_VERTEX/SPLAT_FRAGMENT exported for
  forks, one shared instance + quad). Verified: 10 new bun tests
  (parsers against hand-built inputs incl. spz smallest-three,
  closed-form covariances, flip signs, container round trip), `srt
  check`, and probes/splat-mesh-probe.tsx on the train capture -
  loadSplat 1,026,508 splats in 24 ms, photoreal from both sides,
  /buffer windowed readbacks (head + tail, 64 KiB cap paged by offset)
  showing the gathered order exactly reversed by a half-turn `view`
  flip with zero JS record writes, parked camera 0 frames in a 5 s
  stats window, JS p50 0.3-0.4 ms while orbiting at every rung.
- The spz spec was re-derived from Niantic's load-spz.cc at
  implementation time, not from memory: colorScale 0.15, scales
  exp(u8/16 - 10), alpha sigmoid-encoded as u8, positions 24-bit fixed,
  v2 rotations (x,y,z)/127.5 - 1 with w derived non-negative, v3
  smallest-three with 9-bit magnitudes scaled 1/sqrt(2) and component 3
  read from the LOWEST bits (the test's first packer had the bit order
  backwards - worth keeping as the regression it caught). spz stores
  y-up (RUB) already, so only .ply/.splat get the flip; spz v1 (f16
  positions, never released) and v4+ (zstd streams) are rejected by
  name.
- flux:fs resolves a relative path to per-app storage; only `assets/`
  reaches the project tree, so the probe's bake lives at
  assets/train.srts (untracked, ~27 MiB) - a bake shipped with an app
  goes under assets/ for the same reason.
- uViewport became a STANDARD shared param written with the camera
  (cameraParams now takes the target size; scene target, views and
  probe faces each feed their own) rather than the app write the stage 1
  probe did - any pixel-space material gets it for free, and a view
  renders splat footprints at its own size. An app's own
  `setParams({ uViewport })` (the old probe) is overwritten on the next
  camera move; harmless when equal, a trap if an app relied on lying
  about the size.
- Desktop laptop rungs (integrated GPU, thermally clamped - NOT the
  reference desktop): 100k 42 fps, 300k 20 fps, 1M 8 fps at 1707x960,
  all GPU-bound with sub-half-ms JS, and the ~2 deg feed epsilon at
  orbit speed folds ~9 re-sorts+republishes per second at 1M into these
  numbers already.
- Pixel 7 leg, same day (fresh release client, this build): loadSplat
  reads the 1M .srts in 45 ms on device, photoreal through
  <SplatMesh>. GPU ms/frame (gpuFrameExecMsPerFrame, orbiting, order
  LIVE - the core re-sorts and republishes the 28 MB per ~2 deg):
  full res 21.8 / 35.3 / 52.2 at 100k / 300k / 1M, half res 13.1 /
  19.4 / 34.1. The stage 1 table is NOT comparable across days: the
  old stale-order probe re-run the same hour read 18.3 / 34.4 / 49.4
  full res (vs its recorded 12.7 / 19.2 / 34.8) - the phone was in
  use, warm and on USB, inflating everything ~1.4x. Absolute phone
  numbers only mean something same-session; the ladder pair
  (splat-probe.tsx stale vs splat-mesh-probe.tsx live) is the valid
  A/B and should be re-run on a cold, idle device for the record.
- The same-day A/B: live core ordering costs ~1-3.5 ms/frame over
  stale order at this orbit speed (full res: +3.5 at 100k, +0.9 at
  300k, +2.8 at 1M) - the whole price of a correct back-to-front order
  every frame, sort + gather + 28 MB upload included.
- Preliminary stage C signal, needs the clean re-measure: the half-res
  per-million growth is ~21.5 ms/M on the OLD shader (quat decode +
  covariance build per corner) and ~22.7 ms/M on the BAKED one, same
  day - the bake did not measurably drop the per-splat vertex cost, so
  the wall looks like per-instance setup (4-vertex instances, the cost
  packages/3d/AGENTS.md names) rather than vertex ALU. If a clean
  device confirms it, transform feedback attacks the wrong term and
  the lever is fewer instances per splat (the index/geometry
  restructuring), which changes stage C's escalation order.
- Android reaped the foreground client once during the 1M rungs under
  system memory pressure (another app foregrounding at that moment;
  no tombstone, "has died: fg TOP" + mem-pressure reschedules) - the
  first on-device sighting of the triple record residency's weight
  (~115 MB for the cloud alone with the fetched bytes still viewed).
  Real-world backing for the hand-off record form the stage A review
  recommends.
- The .splat and .srts probes cannot share a capture cache: the old
  probe fetched over HTTP with force-cache, the new one reads the baked
  file - re-baking after a splat-data change is `curl` + one tool run
  (the header comment carries both commands).
- The triple record residency
  (notes/gaussian-splats-stage-a-review.md) stands unresolved: a splat
  mesh keeps the JS stream mirror, the core's retained mirror and the
  GPU buffer (~84 MB at 1M splats). The hand-off record form (transfer
  to the engine, no JS mirror) is additive to this API - createSplatMesh
  would adopt it without a signature change - so it was consciously not
  blocked on here; it stays the review's open recommendation.

- Stage 1 probe, 2026-09-26 (`probes/splat-probe.tsx`, local): the public
  "train" capture (1,026,508 splats, the 3DGS paper's Tanks and Temples
  scene) in the antimatter15 `.splat` format, fetched from Hugging Face
  with `cache: "force-cache"` (33 MB: 3.7 s on first load, 0.3 s from the
  disk cache). Its record - position float32x3, scale float32x3, sRGB
  color + opacity unorm8x4, rotation w,x,y,z unorm8x4 (`v * 128 + 128`) -
  IS a record-mesh instance layout, so the fetched bytes are the records.
  The file is sorted by importance (size times opacity, background splats
  first), so its first N records are the scene at N and every subset
  keeps the worst overdraw. The material: a 4-vertex quad per record; the
  Jacobian of the pixel position taken straight from `uViewProj * uModel`
  (d(clip.xy / clip.w) / dp), so no view matrix or focal uniforms, only
  `uViewport` through `scene.setParams`; the 3DGS 0.3 px^2 dilation; the
  2D covariance eigen-decomposed into the quad's axes; a falloff cut at
  e^-4; premultiplied output. Renders photoreal on desktop and phone. The
  capture is COLMAP-oriented (y down), stood up by half a turn about x.
- Pixel 7 (Tensor G2, Mali-G710 MP7, GLES 3.2, 1080x2400, 90 Hz),
  installed release client, camera orbiting with the order left stale so
  no JS runs in the frame. GPU is the stats window's
  `gpuFrameExecMsPerFrame` (the compositor's per-frame GPU span); this
  Mali's pass timers read 0, and fps is quantized by the cadence hold, so
  GPU time is the figure:

  | splats | target | GPU ms/frame | fps |
  |---|---|---|---|
  | 100k | 1079x2399 | 12.7 | 63 |
  | 300k | 1079x2399 | 19.2 | 39 |
  | 1.03M | 1079x2399 | 34.8 | 21 |
  | 100k | 539x1200 | 5.9 | 73 |
  | 300k | 539x1200 | 8.8 | 74 |
  | 1.03M | 539x1200 | 25.1 | 25 |

  Quartering the pixels saves 7-10 ms at every count (the fill share),
  while the half-resolution column grows ~21 ms per million splats: the
  per-splat vertex-side work, the wall at a million. The measurement does
  not split that into shader ALU (four corners repeating the math) versus
  per-instance setup (the cost `packages/3d/AGENTS.md` names for
  instances under ~100 vertices); a trivial vertex stage at the same
  instance count would, by subtraction.
- The JS counting sort (16-bit depth keys, whole-record gather) on the
  phone: 256 ms at 100k, 474 ms at 300k, 1.29 s at 1M, blocking the JS
  thread; desktop QuickJS is about the same (1.1 s at 1M). Fine on settle
  up to ~100k, useless in motion at any size worth showing.
- The scene buffer is half float, so blending moves twice the bytes per
  pixel of the rgba8 targets the web viewers draw into: a candidate lever
  for the fill share, unmeasured.
- Colors are decoded to linear and blended in linear light (the scene
  buffer's contract) where the reference renderers blend in sRGB; not
  compared side by side.
- Away from the capture path a capture is full of floaters: from 6 units
  off the train's center the view was a smear of large foreground splats,
  from 3 units clean. An app showing a capture keeps its camera near the
  path the capture was taken from.
- Not measured: the low-end tier (the Adreno 610 tablet and Mali-T860 TV
  of [3d-low-end-gpu-performance](3d-low-end-gpu-performance.md)) - splats
  target desktop and phones from the last few years. The desktop reference
  run was unusable (the window was throttled to 15 fps and GPU time stayed
  flat across counts); retake it with the window visible.
