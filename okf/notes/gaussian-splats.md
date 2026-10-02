---
title: Gaussian splat measurements and traps
description: What building the splat viewer established - the instancing frontend as the vertex wall and the id-indexed fetch that collapses it, the linear-vs-sRGB blend divergence and the display blend space, the staging-block leak, the phone's DVFS and thermal measurement traps, and the spz and ply layouts re-derived from their readers.
created: 2026-09-27
---

# Gaussian splat measurements and traps

Cut from [../done/gaussian-splats.md](../done/gaussian-splats.md) at its
close; each bullet is dated by the work it came out of.

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
  gui`, `sol check` on the 3d package, and `probes/order-3d-probe.tsx`
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
  .sol3s container ("SOLS" u32 | version | jsonLength | json | records,
  version 1); `sol tool 3d/splat` (tools/splat.ts, gunzips .spz
  itself); runtime-side `loadSplat` + `createSplatMesh`/`<SplatMesh>`
  over the stock material (SPLAT_VERTEX/SPLAT_FRAGMENT exported for
  forks, one shared instance + quad). Verified: 10 new bun tests
  (parsers against hand-built inputs incl. spz smallest-three,
  closed-form covariances, flip signs, container round trip), `sol
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
  assets/train.sol3s (untracked, ~27 MiB) - a bake shipped with an app
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
  reads the 1M .sol3s in 45 ms on device, photoreal through
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
- The .splat and .sol3s probes cannot share a capture cache: the old
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

- The phone kills were NOT the residency: a staging-block leak, found
  2026-09-26 by sampling `dumpsys meminfo` during the 1M orbit. The
  lease pool's recycle channel was drained only in `begin_buffer_write`
  (the JS publish path); the camera-driven republish path
  (rematerialize -> republish_slots -> take_free) never drained it, so
  with JS parked and the camera turning, every consumed ~28.7 MB block
  sat in the unbounded channel forever and every re-sort minted a new
  one: ~8.6 republishes/s x 28.7 MB ~= 200 MB/s of native heap, 2.1 GB
  after 10 s, kernel kill at ~4.8 GB PSS around 20 s - and rasterQueue
  never left 1-3 (the raster thread kept up; nothing recovered on
  orbit-off, pinning "retained, not in flight"). Latent since
  gpu-instance-order stage 3 (`ordered_instance_publish` had the same
  gap); splats were the first consumer big and fast enough to die of
  it. Fixed by a `drain_recycled_blocks` helper called at every
  block-taking entry point; verified by 48 s of 1M desktop orbit at a
  byte-flat RSS (271,240 kB) where the old build grew ~200 MB/s. The
  class of bug is exactly what the stage A review's "no context-level
  Rust test" gap covers - the wiring between context entry points is
  probe-verified only.
- The record transfer form landed the same day (the review's hand-off
  recommendation): core order mirrors moved from entry-lifetime to
  BUFFER-lifetime (`InstanceOrders.mirrors` keyed by buffer id, freed
  with the buffer), `transferRecords(buffer, bytes)` hands the full
  record set to the core (owned mirror: held vs published split), an
  attach over an owned mirror SEEDS the first ordered publish
  core-side (insert returns "seed", target.rs runs rematerialize after
  its borrows drop), and an instanceCount change on an owned mirror
  republishes the re-sorted prefix (`set_instance_order_count` +
  rematerialize, riding the same two-step as orderDirection - which is
  also the path `setDrawCount` from the spatial core takes). JS:
  `transfer: true` on createRecordMesh (single-stream materials; order
  implies retain), guards on setRecords/updateRecords/
  instanceAttribute/growth, no stream mirror allocated;
  createSplatMesh defaults transfer ON. A consequence for every
  retained ordered mesh: re-attach (setLayers re-admission included)
  seeds from the surviving core mirror, so the stage A rule "a
  (re)attached ordered mesh republishes whole from JS" is gone for
  them (markLiveRecords self-guards).
- Transfer verification (desktop, release client): the seeded first
  order is byte-identical to the old JS-publish path (same head
  records), a half-turn `view` flip re-sorts from the engine mirror
  with no JS anywhere, `layers` off/on re-attaches with the order
  intact, the live `count` dial works, and `poke` (setRecords) throws
  naming the transfer. Splat residency at 1M drops from four copies
  (~115 MB with the fetched bytes held) to engine mirror + GPU
  (~57 MB) once the app drops the SplatData - the probe's bench memo
  still holds it (remounts need it), so the probe understates the
  saving.
- Acceptance on the Pixel 7 (fixed build, same session): native heap
  FLAT at ~138 MB through 90 s of continuous 1M orbit - the run that
  hit 2.1 GB in 10 s and died by ~25 s on the leaky build; desktop
  likewise byte-flat (271,240 kB RSS over 48 s). Parked native heap
  198 -> 141 MB: the transferred-away JS mirror plus the blocks the
  old build had already leaked at mount. 300k full-res orbits at
  26.5 ms GPU vs the leaky build's 35.3 the same morning - recycled
  blocks instead of a fresh zeroed allocation per re-sort, plus
  same-day drift. One bounded cost recorded: GL driver memory grows
  231 -> 444 MB over the orbit and stays (the Mali DDK retains ghost
  generations for the in-flight re-uploads, ~7 x 28.7 MB at 1M, and
  its pools never shrink) - not a leak, but another number for stage
  C's move-indices-not-data question.

- Stage C measured 2026-09-27, on a cold Pixel 7 (27.9 C battery at
  start, thermal status 0 throughout) - and the first result is the
  measurement trap that invalidates casual phone numbers: the Mali's
  DVFS clock ranged 202-848 MHz across rungs (readable without root at
  /sys/devices/platform/28000000.mali/cur_freq), tracking utilization,
  and the cadence hold feeds it (a light rung holds presents, the
  governor downclocks, the per-frame span inflates - a 100k rung read
  6.98 ms at an unrecorded clock and 15.74 ms at ~251 MHz the same
  session, no throttle involved). This, not just device warmth, is the
  ~1.4x drift stage B recorded. Every phone GPU figure from now on
  carries the sampled clock; only same-clock rows compare directly,
  and cycles (ms x MHz) are the cross-clock currency for core-bound
  terms only (memory-bound work does not scale with core clock: the
  same raster-free rung read 16.0M cycles at 848 and 13.1M at 572).
- The vertex-wall split (a `shader` rung on splat-mesh-probe: "stock" /
  "noraster" = full ALU with the position redirected past the clip
  volume, zero raster / "setup" = attributes fetched, corners culled by
  a constant, zero ALU). The decisive same-clock row, 1M half-res at
  572 MHz: stock 34.5 / noraster 22.9 / setup 23.8 ms. Vertex ALU is
  INVISIBLE - noraster equals setup within noise at every count and
  every K - so the covariance bake was necessarily cost-neutral on the
  vertex side (it still bought the smaller record), and the
  transform-feedback escalation is dead: it attacks ALU, which was
  never the term. Fill at 1M: 11.6 ms half-res (572), ~28 ms full-res
  (848).
- The frontend is the wall, and it is the INSTANCING, not the record
  fetch (a `quads` estimator rung: a plain RecordMesh drawing
  floor(count/K) instances of K merged quads - same total vertices,
  same per-vertex ALU, 1/K the instances, no order feed). Noraster at
  1M half-res: K=1 16.0M cycles, K=4 6.2M, K=16 4.2M. The two-point
  fit: ~12.6 cycles per INSTANCE + ~0.6 cycles per VERTEX, so the
  per-instance frontend is ~79% of the vertex side and collapses when
  K splats share an instance. At max clock that is ~15 ms/M
  frontend + ~4 ms/M vertex throughput - the ~21 ms/M growth every
  earlier table showed, now attributed. The point-cloud precedent in
  okf/done/gpu-vertex-fill-attribution.md (1.2M instances of a
  3-vertex primitive, 4x slower than one indexed geometry) said this
  generically; the estimator confirms it at splat scale.
- The full-res record, cold with clocks: stock 22.0 ms @ 701 MHz
  (100k), 32.7 @ 762 (300k), 47.1 @ 848 (1M). At 1M full-res the frame
  is ~15 ms frontend + ~4 ms verts + ~28 ms fill, all at max clock: at
  full resolution FILL is the bigger wall, at half resolution the
  frontend is. The two levers stand in that order at each resolution,
  and the frontend restructure alone does not buy 1M full-res 60 fps.
- The order feed re-checked at same clock (1M half-res, 572 MHz):
  SplatMesh with the live order vs the estimator's order-free K=1 is
  ~1.6 ms/frame - inside stage B's 1-3.5 ms A/B. ORDER_DIRECTION_EPS
  stays at ~2 degrees; revisit only if the restructure changes what
  re-sorts (an index stream re-sort uploads 4 MB at 1M, not 28 MB - an
  argument FOR the index form, not against it).
- The escalation, if the numbers are ever demanded at 1M scale, is
  therefore: K splats per instance, the record fetched by computed id
  (gl_InstanceID * K + gl_VertexID / 4) from an id-indexed data
  texture - the same machinery stage D's SH bands want - with the
  ordering moved to what the plan called the last resort, the index
  stream. Estimator ceiling: vertex side ~20 ms/M down to ~5-8 ms/M
  (the added per-vertex texture fetch is not in the estimator and is
  the first thing a prototype must price). Not started: stage B's
  acceptance (a few hundred thousand splats orbiting smoothly on the
  phone) holds without it, so this is a scope decision, not a next
  step. Update: folded into stage D by the user the same day.
- The sRGB acceptance pair, 2026-09-27, produced without a browser
  viewer: an "srgb" fork rung on splat-mesh-probe (the stock vertex
  minus srgbToLinear, so the blend runs on sRGB-encoded values as the
  reference viewers blend), its capture read back through one sRGB
  DECODE offline - exact because the default resolve is exposure 1,
  tone mapping none, one linearToSrgb encode (the fork's double encode
  cancels against the decode; only the 1/255 resolve dither survives).
  The two blends measurably diverge on the train capture: sampled mean
  delta 15-18/255, peaks ~70/255, concentrated where translucent
  splats stack - the linear blend lays a milky veil over the
  near-camera foreground that the sRGB blend does not have, clearly
  visible in the head-on pose, subtle in the side pose (a slightly
  brighter ground). VERDICT (the user, same day, on the side-by-side
  pair): the sRGB blend is the right look - no veil; adopt it for
  splat rendering. The fix cannot be per-material in a shared buffer
  (the material would have to encode its output and the one resolve
  then double-encodes those pixels; there is no per-pixel tag), so
  the shape to design is a per-SCENE blend space: fragments encode at
  the shared output, the resolve drops to sample + dither (display
  space is LDR: exposure/tone mapping do not compose with encoded
  blending), the splat material's decode goes away in that mode - and
  a display-space scene no longer needs the half-float buffer, so
  rgba8 halves the blend bytes and answers the fill-share gate in the
  same stroke. Not designed or implemented yet.
- The desktop reference retake, 2026-09-27, on the winbox (RTX 3070
  through ANGLE D3D11, current tree overlaid and built there, window
  visible on the desktop at 60 Hz - the stage 1 run's 15 fps throttle
  gone): the scene pass at 1M splats is 4.8 ms at 1280x720 and 13.9 ms
  at a 4K-equivalent target (3840x2160, full 1.9 GHz boost), 100k/300k
  at 3.4/4.7 ms. The quads sweep has the same instancing-frontend
  shape as the phone (K=1 -> 16 drops the normalized cost ~10x) at
  absolute costs that never matter. There is no desktop wall; the
  phone drives the design. Two reads for the record: NVIDIA's clocks
  bounce 200-1900 MHz at these low utilizations, so per-rung
  normalization is noisy there (sample the clock, but trust magnitudes,
  not deltas), and `gpuFrameExecMsPerFrame` reads ~0 on ANGLE D3D11 -
  the per-target pass timer is the desktop figure.
- The sRGB verdict's fix landed 2026-09-27: `blendSpace: "linear" |
  "display"` on SceneOptions and `<Scene>`, fixed at creation. Display
  space is the web's rendering model as one scene-level mode: fragments
  encode to sRGB at the shared tail (BLEND_SPACE in `@solidrt/3d/glsl`,
  composed by sceneOutput, the skybox, and the splat vertex, which
  skips its record decode there), BLENDING runs on encoded values, the
  buffer is rgba8 and IS the displayed image - no resolve pass at all
  (`texture` === `hdrTexture`), views inherit the mode, and resolve /
  bloom / toneMapping / exposure / reflection probes throw
  (probes/blend-space-throws-probe.tsx covers all eleven cases). A
  custom fragment in a display scene must end in blendSpaceOutput or it
  blends in the wrong space - documented on BLEND_SPACE and sceneOutput.
- Display-mode acceptance and the regression that almost shipped: the
  engine render is within 1.2-1.4/255 mean of the human-approved
  offline reference (the srgb fork + one decode; residue = rgba8
  quantization vs the fork path's resolve dither), and a linear scene
  mounted AFTER a display one is byte-identical to the pre-change stock
  capture - but only after a fix: a shared program's uniform keeps its
  last drawn value across targets, so "an unwritten uniform reads 0"
  is false the moment any scene writes it. First linear-after-display
  run rendered the splats undecoded (byte-equal to the fork capture).
  Every scene now writes uBlendSpace explicitly, 0 included. The trap
  generalizes: any scene-mode uniform on a shared program must be
  written by BOTH modes.
- The fill lever, measured on the laptop desktop (magnitudes only):
  1M full-res stock scene pass 29.8 ms linear -> 19.4 ms display
  (-35%), plus the 0.5 ms resolve pass gone - the rgba8 buffer halves
  the blend bytes as predicted on an immediate-mode GPU.
- The phone A/B, same day (cold Pixel 7, same session, same clock):
  display buys ~NOTHING on the Mali - 1M full-res 41.96 -> 40.94 ms
  both at 848 MHz, half-res within noise. A tile-based GPU blends
  on-tile: the blend bytes never cross external memory, so halving the
  buffer format does not touch the phone's fill term. The ~28 ms "fill
  share" at 1M full-res is fragment throughput and overdraw (falloff
  eval, discard, per-tile blend ops), not bandwidth - so the fill
  lever on phones is OVERDRAW reduction (tighter extents, a higher
  min-alpha cut), not the buffer format, and the display mode's value
  on phones is correctness (the trained look) plus the dropped resolve
  pass, not fill. Stage D's frontend restructure stays the phone's
  perf lever. (Absolute drift note: this morning's 1M full-res read
  47.1 at the same clock in another session - 12% session drift, the
  same-session-only rule again.)
- Stage D step 1, 2026-09-27: the per-vertex fetch PRICED on the phone
  before any build (the stage's gate), with a `fetch` rung on
  splat-mesh-probe - K quads per instance, K u32 ids per instance record
  (identity order, or a seeded shuffle), the splat record fetched by id
  from a data texture (two rgba32f texels per splat: center + packed
  rgb, then the six covariance halves + opacity as bit patterns - the
  shipping form's two-texel fetch on today's formats), and an `sh`
  degree over a synthetic texture at the shipping layout (2/3/6 texels
  per splat, halves packed pairwise) evaluated by the real SH code per
  corner; no order feed on the fetch rungs. Cold Pixel 7, cadence hold
  pinned OFF, 1,026,508 splats, full res 1079x2399, half res 539x1199,
  10 s windows over the same orbit range, GPU = gpuFrameExecMsPerFrame,
  rows at the sampled 848 MHz unless noted:

  | 1M splats, GPU ms/frame | full-res stock | half-res noraster |
  |---|---|---|
  | SplatMesh K=1 (shipped, order live) | 45.2 (three reads) | 23.8 (720-763 MHz) |
  | quads K=1 / K=16 (attribute records, no order) | - | 19.9 / 7.1 (830 / 597 MHz) |
  | fetch K=1 | 45.2 (661 MHz) | 19.9 |
  | fetch K=4 / 8 / 16 / 32 | 31.2 / 31.3 / 31.6 / - | 8.0 / - / 7.4 / 8.0 (716-796 MHz) |
  | fetch K=4 / 8 / 16, shuffled ids | 31.9 / 31.4 / 32.0 | - / - / 8.0 (767 MHz) |
  | fetch K=16 sh1 / sh2 / sh3 | 31.7 / 32.1 / 32.0 | 7.4 / - / 7.4 |
  | fetch K=16 sh1 / sh3, shuffled ids | 32.5 / 33.6 | - / 7.0 |

  300k full-res stock: SplatMesh 25.9 (741 MHz), fetch K=16 24.8 (779),
  fetch K=8 21.8 (848). The reads: (1) the restructure takes the 1M
  full-res frame from 45.2 to 31.3 ms (-30%) and the vertex side from
  23.8 to 7.4-8.0 ms (-67%) - the frontend collapse the estimator
  predicted (5-8 ms/M), now measured on the real fetch form. (2) The
  per-vertex fetch itself is free: fetch K=1 equals quads K=1 (19.9 vs
  19.9), and fetch K=16 sits 0.3-0.9 ms over quads K=16. (3) Random id
  order (the sorted stream's worst case) costs 0.4-0.7 ms at full res.
  (4) K=4, 8 and 16 are equal within noise; K=16 goes in (fewest
  instances, the widest margin on a weaker frontend). (5) SH1 and SH2
  are free; SH3 costs 0.3 ms sequential and 1.5 ms shuffled at full
  res and nothing on the vertex side - SH3's price is its 99 MB
  texture, not time. (6) What remains at 1M full res is ~24 ms of fill:
  with the vertex side gone, overdraw is the whole phone wall, as the
  display-mode A/B said. (7) 300k full res gains 4 ms (fill-bound).
- Two measurement traps found the same day, both now in the protocol.
  The CADENCE HOLD, not DVFS alone, is what downclocks light rungs: a
  hold of 3-4 refreshes leaves the GPU idle most of the frame, the
  governor drops to 250-370 MHz and the reading inflates (quads K=16
  read 13.1 ms at 301 MHz, 7.1 at 597) - pinned off through a new
  `sol_env` intent extra on the go client (MainActivity.java:
  "NAME=value;..." set into the process environment before SDL_main,
  so `--es sol_env SOLIDRT_CADENCE_HOLD=off` reaches lattice like a shell
  export; APK rebuilt, the .so unchanged). And THERMAL DRIFT inside a
  block below status 1: the second and later rungs of a block run at
  762 MHz with the skin above ~36 C (the status-1 threshold is 39 C)
  and read ~25% high (fetch K=16 39.8 and 35.7 vs 31.6 cold, SH3 41-44
  vs 32); status 0 is not "cold". Every rung now starts cold (park,
  wait for skin < 35.5 C) and only 848 MHz rows compare. Two smaller
  ones: the orbit makes overdraw pose-dependent, so every rung
  remounts at azimuth 0 and a fixed settle + window covers the same
  range (the /stats window caps at 10 s); and with the hold off, rungs
  under the 11 ms vsync period idle and downclock (the noraster rows
  at 600-800 MHz), so their ms are ceilings, not same-clock figures.
- Stage D step 2 landed 2026-09-27, the build the pricing justified.
  (a) The `rgba32ui` texture format: alloy `TextureFormat::Rgba32ui`
  (RGBA32UI / RGBA_INTEGER / UNSIGNED_INT, 16 bytes per texel,
  nearest-only and sample-only like the 32-bit floats), `usampler2D`
  reflected as `UniformKind::USampler2D` with the binding rules both
  ways (an integer texture only behind an integer sampler, and the
  reverse), a Uint32Array payload at the flux boundary. (b) Index
  materialization of an instance order in the core:
  `InstanceOrder.indices = Some(record stride)` (`indices: { stride }`
  at the JS boundary) makes the entry's one instance buffer hold the
  sorted record INDICES as u32, K per instance record, written WHOLE on
  every re-sort (`materialize_indices`: the ids, then `INDEX_NONE` to
  the buffer's end, so a shrink leaves no stale id); the records come
  only through `transferRecords` (the lease and sink publishes throw),
  the published prefix is instanceCount x K records, and the hand-off
  size rule moved from transferRecords to attach, where the form is
  known (gathered records must fit the buffer, an index stream must
  hold their ids). (c) `.sol3s` version 2: the 32-byte two-texel record
  (center + packed rgba8 | six covariance halves + a spare word) and an
  optional SH block (2/3/6 texels per splat at degree 1/2/3, halves
  packed pairwise, coefficient-major with rgb interleaved; a ply's
  f_rest read channel-major at c x D + k, an spz's block
  coefficient-major exactly as load-spz.cc packs it; the y-up flip as
  per-basis signs), `--sh 0..3` on the tool, default 0. (d) The
  runtime: createRecordMesh's indexed form `instanceOrder: { position,
  records: LAYOUT, descending }` (transfer implied, `group` = ids per
  instance record, `drawCount` = ceil(count / group),
  `MeshInstances.textures` freed by disposeInstances); createSplatMesh
  over it with SPLAT_GROUP = 16 merged quads, four uvec4 id attributes,
  `uSplatRecords` / `uSplatSh` bound per MESH (the uBones pattern, so a
  forked material keeps them), `splatMaterialClass(shDegree)` cached
  per degree with the device's texture width compiled in, the SH
  evaluated per corner from uCamPos through the transposed model
  rotation. One GLSL ES 3.00 trap on the way: `unpackUnorm4x8` is a
  3.10 built-in (Mesa refused it), so the packed color unpacks by hand.
- Step 2 verification: 611 alloy lib tests (index parse and
  materialization, the format, the sampler rules), 69 flux gui tests,
  13 bun tests (the SH parse, packing, flip signs and container), `sol
  check`. Desktop release client: the 1M train renders solid under the
  engine's order; the id stream read back through /buffer runs
  far-to-near after four view flips and a layers off/on (the largest
  inversion 6.6e-6, f32 rounding against a f64 check), sentinel-filled
  past the count, the count dial follows (100k = 6250 instances) and a
  rewrite throws naming the transfer. Niantic's hornedlizard.spz
  (786,233 splats, SH degree 3) baked with `--sh 3` renders with its
  view-dependent sheen. Pixel 7 (the new APK, cold, hold off): 1M
  full-res 35.8 ms against the old form's 45.2 (the prototype's 31.6
  had no live order feed, and this window dipped to 701-762 MHz),
  half-res vertex side 10.1 ms at 552 MHz against 23.8, 300k full-res
  23.5 against 25.9, the lizard SH3 at full res 24.6 ms; no errors,
  546 MB PSS with the 96 MB SH3 texture resident.
- A pre-existing bug fixed on the way: the scene's layer switch
  (`_setLayers`) walked instance nodes for every populated mesh and
  crashed on a record mesh (`nodes` is null there); guarded.
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
