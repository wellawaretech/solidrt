---
title: A splat cloud's SH3 texture is 48 bytes per splat where a palette holds it in 3
description: The .sol3s SH block stores a byte per coefficient (16/32/48 bytes per splat at degree 1/2/3, 48 MB at 1M splats with SH3) while sogs and Unity's importer quantize the 45-coefficient vectors to a k-means palette at 2-3 bytes per splat; a product-quantized encoding behind the SH block's `encoding` field would cut the dominant texture 12x, at the price of a bake-time k-means to engineer and a quality judgement of its own.
created: 2026-10-18
---

# A splat cloud's SH3 texture is 48 bytes per splat where a palette holds it in 3

## Symptom

okf/done/splat-record-compression.md left this as phase 2: at 1M
splats with SH3 the SH texture is 48 MB of the cloud's 84 MB resident,
and the lizard capture's file is 54 MiB of which 36 are SH bytes.
sogs (PlayCanvas) and Unity's importer cluster the SH vectors into a
palette (64k entries in sogs, on a GPU; Unity's in Burst jobs over
minutes) and store a label per splat.

## Shape

- Product quantization at bake: three 15-dimensional subspaces (the
  three bands, or three channel groups), 256 centroids each by k-means
  over the cloud's vectors - 3 bytes per splat plus a 46 KB codebook,
  SH3 from 48 MB to 3 MB. Plain TypeScript at bake scale: 1M x 256 x 15
  per iteration, seconds to a minute under bun.
- The header's SH block already carries `encoding: "u8"`; a `"pq"`
  encoding adds the codebook block and a label texture. The vertex
  stage fetches the three labels and the codebook rows (a small
  texture, cache-resident).
- Judge it as this item was judged: the snapshot diff against the u8
  bake and a human flip on a glossy capture (the lizard's metal).

## Findings

- 2026-10-18, built as planned: the 8-bit integer texture formats in
  alloy (`r8ui`, `rg8ui`, `rgba8ui`; `TextureFormat::is_integer` for
  the sampler and filter rules, `is_uint32` for the payload), the pq bake
  (four subspaces 9/15/9/12 bytes, 256 codes each, k-means++ on a 64k
  sample, Lloyd on the sample then the whole cloud, the centroids rounded
  into the byte domain before the final assignment), the labels as a
  1/2/4-byte integer texture and the palette as 256 x subspaces rgba32ui
  texels, `shBlock` per encoding, `splatMaterialClass({ shDegree,
  shEncoding, antialiased })`. 18 bun, 4 app (the SH shading test under
  both encodings, the u8 path's first app test), 63 alloy texture and
  validate, 83 flux gui tests green.
- The lizard at SH3: 54.1 MiB as u8, 21.2 MiB as pq, the bake 89 s under
  bun (1.3 s as u8). Resident: the 37.7 MB `uSplatSh` becomes 3.1 MB of
  `rgba8ui` labels plus a 64 KB palette texture. The report: rms per
  coefficient 3.6 / 5.3 / 2.7 / 4.0 byte steps per subspace, 99% of
  splats within 14 / 26 / 17 / 20.
- Snapshot diff, laptop, 1707x960 scene, linear blend, default pose and
  a close pose (distance 1.3): mean 1.38 and 1.57 / 255, max 57, about a
  quarter of the pixels exact; the lizard's head tile mean 1.0, max 32,
  and its 2x crop pair indistinguishable. The worst tile both times is
  the background foliage at the top right: a band of view-dependent
  PURPLE in the u8 render is GRAY under pq (scratch/captures/pair-*.png).
- Why: the per-splat error tail. Over the cloud (scratch script against
  the two bakes, slots aligned) the worst coefficient error per splat is
  16 steps at the median (the lizard's spz quantizes bands 2-3 on a
  16-step lattice and the centroids sit between lattice points), 27 at
  p99, 45 at p99.99, 64 at the maximum; 31k splats (4%) above 20 steps.
  The worst vectors are the CHROMATIC ones: their u8 coefficients differ
  across rgb by 0.1-0.3 and the pq reconstruction is near-gray - the
  cloud's SH mass is achromatic (rgb nearly equal), so 256 mass-driven
  codes per subspace average the rare chroma away. A failure of the code
  count against the distribution's tail, not of the subspace split or the
  metric: the bulk is at the sRGB acceptance pair's residue (1.2-1.4/255).
- The probe's stage memo keyed on the splat count, so a `scene` load of a
  bake with the same count (the two lizards) kept the old mesh; the first
  "pq" snapshots were the u8 mesh and the GPU inventory said so (the
  37.7 MB `splat-sh` still resident, no labels). Fixed: the stage is
  keyed on the data object too.
- Found on the way and fixed the same day: `Scene` with `blendSpace:
  "display"` threw `scene.setBloom: a blendSpace "display" scene has no
  resolve stage to bloom on` when the component's deferred bloom effect
  ran with no prop (the probe's rung remount made it run on mount), so
  the probe's `bench { blend: "display" }` rung was dead in HEAD. The
  scene now takes bloom OFF as a no-op where there is no resolve stage
  (the scene and a tiled view alike) and still refuses bloom ON by name;
  tests/display-scene.test.tsx covers the contract and a display scene
  mounted through the component under an async boundary; the probe's
  display rung verified live.
- Revised the same day on that finding. Candidates measured on the
  lizard's u8 vectors with a direction-free (Parseval) luma and chroma
  error per splat and the chroma norm kept (scratch scripts, deleted):

  | split | bytes | luma err steps p50 / p99 | chroma err p50 / p99 | chroma kept p50 |
  |---|---|---|---|---|
  | bands 4 x 256 (built first) | 4 | 15.8 / 57.9 | 14.8 / 31.2 | 0.3 |
  | tree VQ 256 x 256, whole vector | 2 | (rms 4.96, tail max 45) | - | lost alike |
  | Y15 Co15 Cg15 | 3 | 36.1 / 94.6 (dir-max) | kept | 0.9 |
  | Y per band + CoCg per band | 6 | 26.8 / 77.6 | 0.0 / 21.1 | 1.0 |
  | Y per band, band 3 split + CoCg per band | 7 | 15.7 / 55.3 | 0.0 / 21.2 | 1.0 |

  Luma and chroma as separate codebooks keep the chroma at no luma cost
  once band 3's luma has two subspaces (it carries most of the luma
  error: one subspace over its 7 coefficients doubled it). Built as 8
  subspaces (chroma band 3 split like luma's, so 2/4/8 label bytes by
  degree with no spare byte), the planes as green and the red/blue
  differences from green (exact on the byte lattice where YCoCg rounds
  three times; a difference past the byte range clamps and is counted).
- The revised lizard: 24.2 MiB, the bake 34 s (smaller subspaces); rms
  per coefficient 2.6 / 0.7 / 4.6 / 1.6 / 0.4 / 2.5 / 0.3 / 1.0 steps,
  99% of splats within 11 / 8 / 20 / 16 / 1 / 15 / 1 / 16; resident
  6.3 MB of `rgba8ui` labels (two texels per splat) plus the 64 KB
  palette texture against the 37.7 MB u8 block. Snapshot diff against
  u8, same poses: mean 0.67/255 default (was 1.38) and 0.90 close (was
  1.57), max 35 and 41 (were 57), the head tile 0.7 (was 1.0), the
  foliage's purple intact in the crop pair. What remains visible in the
  worst tiles: a saturated cyan floater blob at the top left of the
  default pose is a little desaturated (tile mean 1.9/255); the body
  scales in the close pose's worst tile (mean 1.8) read the same. Pairs
  for the human flip: scratch/captures/pair-default.png (foliage),
  pair-head-default.png, pair-worst-default.png (the cyan blob),
  pair-worst-close.png, and the full renders u8-*.png / pq-*.png.
- Owed: the human flip on the pairs, and the Pixel 7 noraster rung at
  786k SH3, u8 against pq, under the protocol (the device attached at
  the time was the Samsung tablet, not the Pixel).
- Phone rows, 2026-10-18 evening. Tablet (SM-T500, Adreno 610, 2.7 GB)
  first: the splat material did not draw at all there - the program's
  attribute reflection listed two nameless FLOAT attributes (the driver's
  hidden gl_VertexID/instancing inputs) and the layout check refused
  them; fixed in alloy (`reflect_attributes` skips nameless entries like
  `gl_`-prefixed ones). The component's deferred prop effects run on
  mount on that client, so `setExposure(1)` threw on the display scene
  the way `setBloom(null)` had: exposure 1 and tone mapping "none" are
  no-ops on a display scene now (tests/display-scene.test.tsx). With
  both fixed the tablet draws the train (SH degree 0) and loads every
  lizard bake without an error, but a readback bisect showed it draws
  NO SH cloud at any degree under either encoding: the first SH program
  of a process renders nothing (the human saw a black window with the
  HUD), every later one reflects zero active uniforms. Not the SH vertex
  path (read so at the time; disproven on the device 2026-10-19): the
  heavy scene pass trips a KGSL GPU fault that dead-ends the context,
  the SH degree-0 lizard and the train fault too at full resolution, and
  a quarter render scale draws the SH3 lizard; filed as
  okf/backlog/splat-adreno-gpu-fault.md. The palette is not involved and
  the Pixel draws all of it. The tablet also killed the client twice
  without a native crash, read as lmkd at the time (a server reload with
  two engines holding a cloud each, and a few cloud loads in a row); at
  least some such deaths follow the same GPU fault, though the 2.7 GB
  device has little headroom for swapping lizard-scale clouds either
  (about 600 MB available with one 786k cloud resident), which the
  palette is the lever on. It
  exposes no live GPU clock to the shell, so no row from it would have
  been clock-pinned anyway.
- Pixel 7, the protocol (hold off, cold skin, orbit, 10 s windows, the
  Mali clock sampled five times), the lizard at 786k SH3, portrait
  1079x2399, two passes:

  | rung | u8 | pq | clocks |
  |---|---|---|---|
  | noraster (vertex side), GPU ms/frame | 8.73, 8.65 | 8.48, 8.64 | 701-762 MHz (60 fps, idling) |
  | stock, display blend, full res, GPU ms/frame | 21.14, 21.16 | 21.76, 21.80 | 848 MHz every sample |
  | stock, fps / missed presents per 10 s | 44.8 / 448 | 44.1 / 459 | |

  The vertex-only rung is a wash within noise (sub-vsync, so a ceiling
  at a lower clock); at full fill the palette costs 0.6 ms (3%): the two
  label texels plus eight palette texels are slightly more binning work
  than three coefficient texels, and the SH texture's 37.7 MB was never
  the vertex side's wall. The palette's return is memory (6.3 MB + 64 KB
  resident against 37.7 MB; 24 against 54 MiB on disk), not frame time.
  The sort thread read 25-68 ms per sort on the phone, as before.
