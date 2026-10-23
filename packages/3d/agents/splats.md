# @solidrt/3d - Gaussian splats

Captured Gaussian splat scenes: parsing, loading and drawing. Read
../AGENTS.md first: it carries the model this file builds on.

Captured 3D Gaussian splat scenes (phone scans, photogrammetry
successors) as ordinary content, the model-loading split repeated:

- `@solidrt/3d/splat` (src/splat-data.ts) is the runtime-free data side,
  the model-data rule: `parseSplat(bytes, { format?, keepOrientation?,
  shDegree?, antialiased? })` reads a trainer's `.ply`, the de-facto
  `.splat` interchange or a GUNZIPPED `.spz` (Niantic, version 2 or 3;
  the entry stays decompression-free, so inflate first) and bakes it -
  the records partitioned spatially into chunks - a k-d split down to
  runs of SPLAT_CHUNK_MIN..SPLAT_CHUNK_MAX (16..256) consecutive records,
  each a compact box, split finer where a 256-splat box would move its
  smallest splat past SPLAT_STEP_VISIBLE, at most SPLAT_CHUNK_LIMIT
  (65536) chunks - quantized against their chunk and ordered by
  IMPORTANCE inside (projected area times opacity: the two largest
  scales times the alpha, so a prefix of a chunk's run is its most
  important splats), each chunk's SPLAT_RAMP_SAMPLES (16) importance
  samples along its run as a ramp block (`ramp`) and its first record as
  a run block (`runs`) beside the chunk table, y-down captures (.ply,
  .splat) stood up to y-up (positions, rotations and SH rotated
  together; `keepOrientation` opts out, `.spz` is y-up already), bounds
  measured, and the loss reported (`report`: the chunk count and the
  smallest chunk, the chunk position step over each splat's largest axis
  as median, 99th percentile and the share above SPLAT_STEP_VISIBLE, plus
  the SH coefficients clamped). `encodeSplat`/`decodeSplat` round-trip
  the `.sol3s` container (the record, color, chunk, run, ramp and SH
  blocks, next bullet); decode is a header parse plus byte VIEWS,
  nothing per-splat;
  `decodeRecord`/`decodeSh` are the reference decoders (per record: for
  tests and tools, not whole clouds). `sol tool 3d/splat <in> [-o
  out.sol3s] [--sh 0..3] [--keep-orientation] [--antialiased]` is the
  parse and bake under bun (tools/splat.ts), printing the report; an
  app bake script uses the same entry.
- The `.sol3s` record (version 4) is 16 bytes = one rgba32ui texel, a
  chunked record in the field's shape (Unity's importer, PlayCanvas's
  compressed ply) with the position at 16 bits per axis where the field
  has 11 (11 moved a capture's sub-millimetre detail by multiples of its
  size): the position as 16-bit unorms over the chunk's position box in
  two words (x | y << 16, then z | chunk index << 16), the rotation as a
  smallest-three quaternion (the largest's index on top, three 10-bit
  components), the scale as 11/10/11-bit unorms over the chunk's
  log-scale box (SPLAT_ATTRIBUTES as a byte layout). The sRGB color +
  opacity are a color block beside the records (rgba8, SPLAT_COLOR_BYTES
  per splat), the chunk table 12 floats per chunk (position min/max,
  log-scale min/max; SPLAT_CHUNK_TEXELS = 3 rgba32f texels), the run
  block a u32 per chunk, the ramp block SPLAT_RAMP_SAMPLES floats per
  chunk. `--sh 1..3` bakes
  that many spherical-harmonic bands beyond the base color from a
  trainer's `.ply` or an `.spz` (`.splat` has none), capped at the
  capture's, stored as `--sh-encoding` says (`shEncoding` on
  parseSplat, SplatShEncoding, in the header's SH block): `"pq"`, the
  default, is product quantization - each coefficient's rgb becomes a
  luma plane (green) and a chroma plane (red and blue as differences
  from green, exact on the byte lattice), the planes are cut by band
  into SPLAT_SH_SUBSPACES (luma and chroma of band 1, of band 2, and of
  band 3 in two halves each: 3, 6, 5, 10, 3, 4, 6, 8 bytes, one palette
  texel each), each clustered at bake by k-means into
  SPLAT_SH_PALETTE_SIZE (256) entries, and a splat stores one label
  byte per subspace (SPLAT_SH_LABEL_BYTES: 2/4/8 at degree 1/2/3)
  beside one palette block (256 entries x subspaces rgba32ui texels,
  32 KB at degree 3, `shPalette`); `"u8"` is a byte per coefficient,
  spz's (byte - 128) / 128, in 1/2/3 texels per splat (SPLAT_SH_TEXELS;
  coefficient-major with rgb interleaved), 16/32/48 bytes per splat,
  exact for an spz. Luma and chroma are separate codebooks because a
  capture's SH is mostly achromatic and one codebook over rgb spends
  its codes on the gray axis and averages the rare hue shifts away
  (okf/design/splats.md, D11). The pq bake seeds (k-means++) and
  iterates (Lloyd) on a 64k sample, then over the whole cloud to
  convergence, deterministically; it reports the clustering error per
  subspace (`report.shPalette`: rms per coefficient and the 99th
  percentile of the per-splat worst coefficient, in byte steps, and
  the chroma differences clamped) and costs about 35 s at 786k splats
  and degree 3. SH costs texture either way, so the default degree is
  0 and a bake opts in. The header's `antialiased` flag marks a capture
  trained with the Mip-Splatting compensation. At 1M splats a cloud is
  24 MB on disk at SH0 (32 at SH3 as pq, 72 as u8) and the bake runs in
  about a second without SH.
- On the runtime, `loadSplat(path)` reads a `.sol3s` (fetch + decode,
  the loadModel shape) and `createSplatMesh(data, { count?,
  lodThreshold?, material?, label? })` / `<SplatMesh>` shows it: a
  RecordMesh in the INDEXED form (see "Instance order" in instancing.md) with a PACKED
  position key - the engine keeps each splat's packed position words in
  storage order (8 bytes per splat) and the chunk table with its runs
  and ramps, and decodes the depth key from them - AND the record block
  uploaded once, in place and in row
  bands, as the `rgba32ui` record texture the vertex stage fetches by id
  (`uSplatRecords`, the chunk table as `uSplatChunks`, the colors as
  `uSplatColors`, plus `uSplatSh` when baked; bound per mesh, so a forked
  material keeps them),
  SPLAT_GROUP (16) splats per drawn instance of a merged-quad geometry,
  the header bounds as the cull and picking box, and the id stream
  SELECTED and sorted back to front core-side under the scene's view
  (the scene feeds the mesh's model-to-clip matrix): a chunk whose box
  is out of view is skipped whole, a visible chunk is cut where its
  ramp falls under `lodThreshold` pixels of contribution at the chunk's
  projected scale (a far chunk thins, a near one keeps its micro-splats),
  `count` is a budget the threshold rises to meet, and what passed sorts
  - the cloud composes as a mesh (depth-tested against opaque geometry,
  placed by the transparent entry sort, transformed by its node), a
  camera turn re-sorts and a camera move re-selects, republishing 4
  bytes per drawn splat with zero per-frame JS - on the engine's sort
  thread above ~16k splats, so the frame never waits and the order lags
  the camera by one sort (tens of ms at 1M on a phone; a test `settle`s
  before reading it) - and a parked camera uploads nothing. The draw
  range the GPU sees is the selection's (the engine sets it when a sort
  lands; `_instances.count` stays the budget). `setRecordCount` and
  `setRecordLod` dial both live. Without a view (no scene flush yet) the
  budget alone thins, by importance across the cloud.
  Write-once: treat the SplatData as consumed and drop it; at 1M splats
  the cloud is resident as the engine's 8 MB key copy plus 20 MB of
  record and color texture (8 MB of labels plus the 32 KB palette more
  at SH3 as pq - `uSplatShLabels`, an `rg8ui` texture at degree 1 and
  one or two `rgba8ui` texels per splat above, and `uSplatShPalette`;
  48 MB more as u8, `uSplatSh`) plus
  a 4 MB id buffer, and disposeInstances frees all of it.
  `splatMaterialClass({ shDegree, shEncoding, antialiased })` is the
  stock class per kind (a shared instance each; a SplatData is a valid
  argument), and `splatVertexSource(opts)` / SPLAT_FRAGMENT /
  SPLAT_ID_ATTRIBUTES are exported for forks; `count` rounds up to whole
  groups.
- The vertex stage decodes the record against its chunk (the chunk
  index from the record's second word; the mix over
  the boxes, exp of the log scale, the quaternion's fourth component
  from the unit norm), rebuilds the covariance R S St Rt (measured free:
  vertex ALU is invisible next to the fetch and the fill), projects it
  through the Jacobian of `uViewProj * uModel` (no focal/view uniforms;
  any node transform shapes the footprint with the same matrix that
  moves the center), reads the target size from the standard
  `uViewport`, and evaluates the SH bands per corner from the view
  direction (`uCamPos` through the transposed model rotation - exact
  under rotation and uniform scale) - under pq the splat's label texels
  then one cache-resident palette texel per subspace, green plus the
  chroma differences per coefficient, under u8 the splat's own
  coefficient texels (`decodeSh` is the reference for both). Two of the reference rasterizer's
  rules ride along: the fragment caps alpha at SPLAT_ALPHA_MAX (0.99),
  and an antialiased bake scales opacity by the footprint's determinant
  ratio before and after the LOW_PASS dilation. The data textures' row
  width is the device's (up to 4096), compiled into the source.
- Why this shape: on the Pixel 7 at 1M splats the one-quad-per-instance
  form spent two thirds of its vertex side on the instancing frontend;
  the indexed form takes the full-res frame from 45 to 31 ms and the
  vertex side from 24 to 8 ms, the per-vertex fetch and random id order
  cost under a millisecond, and SH1-3 are near free (the bands cost
  memory, not time). What remains at 1M is fill (overdraw). The table
  is in okf/done/gaussian-splats.md.
- Two capture caveats worth knowing at the app level: captures are
  TRAINED against sRGB blending, so show them in a display-space scene
  (`blendSpace: "display"`) - a linear scene lays a milky veil over
  stacked translucents - and a capture is only clean near the camera
  path it was taken from (elsewhere it is floaters), so keep the camera
  near that path.
