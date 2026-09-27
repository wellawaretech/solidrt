---
title: Gaussian splat rendering
description: Captured 3DGS scenes (phone scans, photogrammetry successors) are a growing content class nothing here can display. The viewer is proven by a probe (300k splats in 19 ms at full resolution on a 2022 phone) and the shape settled 2026-09-26 - a pack-time bake to a .srts record with the 3D covariance precomputed, a generic instanceOrder knob on the 3d meshes riding gpu-instance-order's retained projected key with the scene feeding direction, SplatMesh over createRecordMesh, and SH bands via an id-indexed data texture.
created: 2026-08-24
completed: 2026-09-27
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
- **C - the vertex wall.** DONE 2026-09-27 as measurement (Findings):
  the wall is the instanced-draw frontend, vertex ALU is invisible, so
  the transform-feedback escalation is dead and the real lever - K
  splats per instance with the record fetched by computed id - folds
  into stage D, whose data texture it shares.
- **D - SH bands and the instance restructure.** DONE 2026-09-27, in two
  steps (Findings): the per-vertex fetch priced first on the phone
  (the estimator's 5-8 ms/M confirmed at 7.4-8.0), then the build - the
  `rgba32ui` data-texture format, the core's index materialization of
  an instance order, `.srts` version 2 with `--sh` at bake, and the
  splat runtime over them (16 splats per instance, the record and SH
  textures fetched by id, SH evaluated per corner from the shared
  camera position). What remains at 1M full res on the phone is fill:
  [splat-overdraw-fill](../backlog/splat-overdraw-fill.md).

## Not in this item

Training or editing splats, compute-tile rasterization (no compute on the
GLES 3.0 floor; a 3.1 probe-up could revisit, but the core CPU sort makes
it a nice-to-have), streaming/LOD for city-scale captures, runtime-fetched
user splats (the runtime-content problem is general, not splat-specific).

## Findings

Cut into [../notes/gaussian-splats.md](../notes/gaussian-splats.md).
