---
title: Compressed textures - KTX2 shipped, ETC2 or BC7 on the device
description: One shipped payload (Basis Universal in KTX2, baked with our own encoder) transcoded at load to the device's native block format - ETC2 on GLES 3.0 targets, BC7 where the BPTC extension is reported; the device formats and the codec (forge::ktx2, flux:image, zstd included) are built and verified on Linux, open are the other platforms, the model bake and loader, and the quality a bake should use.
created: 2026-07-30
---

# Compressed textures - KTX2 shipped, ETC2 or BC7 on the device

From [gpu-review](../notes/gpu-review.md) (lesson 17), shaped and started
2026-09-29. `createTexture` was RGBA8-class only, so a texture-heavy scene
paid 4 bytes per texel per mip level on every device. GLES 3.0 mandates
ETC2/EAC in core and every desktop exposes BC7, both 1 byte per texel with
the sampler doing the decode for free.

## State (2026-09-29)

| Step | State |
|---|---|
| 1. Device formats on `createTexture` | built, verified in the release client on Linux |
| 2. `transcodeTexture` / `encodeTexture` | built, verified end to end on Linux; zstd on, a libktx file reads |
| 3. Tool hosting, model bake, loader, glTF | not started |
| Other platforms (Android, Windows, macOS) | not built, not run |
| Sponza re-measurement | waits on step 3 |

Read "Open in this plan" before continuing: its first item (the other
platforms) decides whether what is built holds outside this machine.

## Field report: Sponza (2026-08-28)

The Khronos glTF sample Sponza (25 materials, 69 images of which the parser
opens the 25 base-color maps, 1024^2 to 2048^2 JPEGs, ~18 MB of source)
lands as 136.5 MB of RGBA8 base level and ~182 MB with the mip chain;
client RSS sat at ~235 MB. Nothing broke and the scene ran at 60 fps, but a
texture-heavy model has no lever other than shipping smaller images.
Measured on Windows (ANGLE / D3D11, RTX 3070).

Expected after step 3: GPU texture memory ~45 MB, the shipped images under
10 MB, the same GPU figure on every platform because ETC2 RGBA and BC7 are
both 16 bytes per 4x4 block.

## The rules that shape the plan

- An app's payload is the same bytes on every platform: a Windows runtime
  receives exactly what the Android, Linux and macOS runtimes receive. So
  no bake ever targets a device format, nothing ships two copies, and the
  choice of block format happens at load, on the device, from one shipped
  form.
- The toolchain is ours. A texture bake is a core workflow, so its encoder
  ships with `srt` and the developer installs nothing else. An external
  `ktx` CLI was considered and rejected: a third-party install step in
  the build path is what the no-npx rule exists to avoid.

## Platform reality

The original note said ETC2 is "guaranteed native on the GL targets
(Linux, Android)". Corrected (from driver documentation, only the Linux
row is observed):

| Platform | ETC2 | BC7 (`EXT_texture_compression_bptc`) |
|---|---|---|
| Android (Adreno, Mali, PowerVR), Raspberry Pi, the TVs | native, GLES 3.0 core | absent |
| Windows, ANGLE over D3D11 | accepted, expanded to RGBA8 on the CPU at upload | native |
| macOS, ANGLE over Metal | native on Apple silicon, expanded on Intel | native |
| Linux desktop, Mesa | native on Intel iris only; AMD and NVIDIA drivers expand it | native |

ETC2 alone wins on mobile and loses the memory cut on every desktop. Two
device formats are the minimum that holds on both sides, and under the
one-payload rule that means a transcode at load, not two assets. Every
device that reports BC7 is a desktop that would expand ETC2, so the rule
"BC7 where reported, ETC2 otherwise" needs no native-or-expanded probe (GL
has none).

## What ships

KTX2 files holding Basis Universal texel data with a full mip chain and the
sRGB flag: the container and codec that glTF's `KHR_texture_basisu`, Three's
KTX2Loader, Unity and Godot use. One per image, embedded as the image
blocks of a `.srtm` model file or standalone under `assets/`. Two codecs,
chosen per image by the bake:

- ETC1S for color maps (base color, emissive): lossy, JPEG-class quality
  and size, around 1 bit per texel (measured: 607 bytes for a solid 64x64
  with its chain).
- UASTC for data maps (normal, metallic-roughness, occlusion): high
  quality, 8 bits per texel in memory, zstd-compressed in the file.
  Measured on Sponza's maps at the default quality: 1.9 to 5.9 bits per
  texel; see "UASTC and ETC1S across the quality scale".

## Design as built

Three layers, primitive first, each usable without the one above it.

### 1. Device formats on createTexture (alloy, flux:gpu) - built

| value | GL storage | availability |
|---|---|---|
| `etc2-rgba8` | `COMPRESSED_RGBA8_ETC2_EAC` | GLES 3.0 core, always |
| `etc2-rgba8-srgb` | `COMPRESSED_SRGB8_ALPHA8_ETC2_EAC` | GLES 3.0 core, always |
| `bc7-rgba8` | `COMPRESSED_RGBA_BPTC_UNORM` | `limits.bc7Textures` |
| `bc7-rgba8-srgb` | `COMPRESSED_SRGB_ALPHA_BPTC_UNORM` | `limits.bc7Textures` |

- `TextureFormat::byte_len` is `ceil(w/4) * ceil(h/4) * 16` for these;
  `chain_byte_len` sums the levels; `is_compressed` names the class.
- The payload is one `Uint8Array`. With `mipmap: true` it holds the FULL
  chain down to 1x1, level-major; without, the base level alone.
  `glGenerateMipmap` is illegal on compressed storage, so the chain comes
  from the file. The size error names the expected bytes and level count.
- `GpuTexture::new_compressed` issues `glCompressedTexImage2D` per level.
- Create-once and sample-only, like a cube map: `createMutableTexture`,
  `uploadTexture`, `resizeTexture`, `readTexture`, `copyTexture` and cube
  maps refuse the formats. Filtering, mips and anisotropy apply.
- `limits.bc7Textures` from the BPTC extension query (EXT or ARB
  spelling); a `bc7-*` create without it throws naming the limit.
- `etc2-*` is the only compressed format portable to SHIP raw; `bc7-*` is
  a load-time target.
- The `/gpu` texture records carry `byteLength` (texel storage at the
  format, chain and cube faces included; the record had no size before),
  so summing it is a scene's texture memory.

Files: `alloy/src/gpu/texture.rs`, `gpu/limits.rs`, `gpu/resources.rs`,
`gl/texture.rs`, `gl/context.rs`, `context/texture.rs`,
`context/capture.rs`, `raster/resources.rs`, `tests/texture.rs`;
`flux/src/alloy_plugins/gpu.rs`; `lattice/src/go/connection.rs`;
`packages/flux-types/gui/gpu.d.ts`; `packages/core/src/gpu.ts`;
`packages/cli/agents/debugging.md`, `packages/cli/src/mcp/main.ts`.

### 2. transcodeTexture and encodeTexture (forge, flux:image) - built

```ts
transcodeTexture(ktx2: Uint8Array, options?: { target?: "etc2-rgba8" | "bc7-rgba8" | "rgba8" }):
  Promise<{ data: Uint8Array; width: number; height: number; format: ...; mipmap: boolean }>

encodeTexture(img: DecodedImage, options: { codec: "etc1s" | "uastc"; srgb?: boolean; mipmap?: boolean; wrap?: "clamp" | "repeat"; quality?: number }):
  Promise<Uint8Array>
```

- The codec is Binomial's Basis Universal v2.50, a submodule at
  `forge/vendor/basis_universal` pinned to tag `v2_50`, compiled by
  `forge/build.rs` through `cc` (C++17, `-fno-strict-aliasing`, `NDEBUG`
  so a malformed file fails the call instead of aborting, no SSE, no
  OpenCL, the legacy output formats off). Zstd supercompression is on:
  upstream's single-file `zstd/zstd.c` from the same submodule, compiled
  as C in a `cc` build of its own (`basisu_zstd`). `encode` sets the
  zstd flag on every UASTC file, no option: it is lossless.
- `forge::ktx2` (`forge/src/ktx2/`) is the safe surface: `transcode`,
  `encode`, `is_ktx2`, `Target`, `Codec`, `MipWrap`. `ktx2/ffi.rs` is the
  one `unsafe` block: hand-written `extern "C"` declarations of upstream's
  own C API (`encoder/basisu_wasm_transcoder_api.h`,
  `encoder/basisu_wasm_api.h`; "wasm" is historical, the u64 "offsets"
  are pointers natively). No C++ of ours, no binding crate, no container
  parser crate: the C API parses KTX2.
- `transcode` accepts ETC1S and UASTC LDR 4x4, with or without zstd, a
  plain 2D texture, one level or the full chain; everything else is a
  named error.
- With no `target` the plugin picks the device's format from the gui
  limits (`bc7-rgba8` where `limits.bc7Textures`, else `etc2-rgba8`); a
  runtime with no GPU throws and wants the target named. The returned
  `format` carries `-srgb` when the file says so. `"rgba8"` is for
  inspection; no device needs it as a fallback.
- `encode` takes PREMULTIPLIED RGBA8, a `decodeImage` result as it is:
  every texel a sampler returns is premultiplied by the pixel contract,
  blocks cannot be premultiplied after the fact, and mips filter
  correctly only on premultiplied color. `wrap` decides how the mip
  filter treats the edges (the model bake passes repeat, how
  `createModel` samples). `quality` 0..1 maps linearly onto upstream's
  1..100, effort is upstream's default. On that unified scale upstream
  derives UASTC's rate-distortion optimization from the quality itself
  (lambda = 20 * (1 - q)^1.3, off at 100) and zeroes the separate
  low-level RDO argument, so RDO is on at every quality below 1 and
  there is no second knob to pass.
- Both calls copy their input and run on the blocking pool.
- ONE cargo feature, `ktx2` (forge, flux, lattice), encoder and
  transcoder both in every build: decided 2026-09-29, "for now". The
  capability `ktx2` reports it; on a build without it both calls exist
  and throw. `KTX2 ?= 1` in lattice/Makefile (every goal) and
  flux/Makefile (the flux binary hosts the bake).

Files: `forge/Cargo.toml`, `forge/build.rs`, `forge/src/lib.rs`,
`forge/src/ktx2/`, `forge/src/tests/ktx2.rs`; `flux/Cargo.toml`,
`flux/Makefile`, `flux/src/forge_plugins/image.rs`,
`flux/src/plugins/mod.rs`, `flux/tests/image.rs`; `lattice/Cargo.toml`,
`lattice/Makefile`; `packages/flux-types/modules/image.d.ts`;
`packages/core/src/image.ts`, `src/index.ts`; `docs/runtime/index.md`.

### 3. Tool hosting, bake and load (packages/cli, packages/3d) - not started

- **Tools under flux.** `srt tool` spawns bun for every
  `<package>/tools/<name>.ts`. A tool named `tools/<name>.flux.ts` runs
  under the flux binary instead (the spawn in `packages/cli/src/main.ts`
  that hosts the server, with the same `SRT_*` environment), listed under
  the same `<pkg>/<name>`. Opt-in by file name, no scanning of imports.
  The model tool moves; `environment.ts` and `splat.ts` stay under bun
  (splat needs `node:zlib`'s gunzip for `.spz`, which flux does not
  offer).
- **The bake.** `srt tool 3d/model --ktx2` (as `tools/model.flux.ts`,
  `flux:fs` and `flux:process` in place of `node:fs` and `process.argv`)
  runs `parseGltf` as today, then for each sampled image `decodeImage`
  (premultiplied, the default) and `encodeTexture`: ETC1S with `srgb` for
  the images a material samples as `map` or `emissiveMap`, the slot rule
  `createModel` already uses, UASTC linear for the rest, `mipmap: true`,
  `wrap: "repeat"`, ONE image at a time (the encoder takes every core).
  The KTX2 bytes replace the image entry and `encodeModel` writes the file
  unchanged. The summary line adds image bytes before and after. Without
  the flag nothing changes. The tool never takes a platform. The
  runtime-free `@solidrt/3d/model` entry (`model-data.ts`) stays
  runtime-free.
- **The load path.** `createModel` stays synchronous. A new async
  `transcodeModelImages(data)` walks `data.images`, sniffs the KTX2 magic
  and replaces each KTX2 entry with the `transcodeTexture` result, so an
  image entry is either encoded PNG/JPEG bytes or a ready payload.
  `loadModel` and `loadGltf`, already async, call it before `createModel`.
  `createModel` uploads a ready payload at its format and decodes
  everything else as today; a KTX2 entry it meets untranscoded throws
  naming the function to await first. The `.srtm` image block carries the
  KTX2 bytes as-is; version 10 files read identically, so the container
  version does not move.
- **glTF input.** `parseGltf` accepts `KHR_texture_basisu` in
  `extensionsRequired`, and a texture's
  `extensions.KHR_texture_basisu.source` takes precedence over `source`.
  That closes the KTX2 row of
  [3d-model-loader](../backlog/3d-model-loader.md).
- **Docs.** packages/3d docs and AGENTS notes: the bake flag, what ships,
  what the device does, the sync `createModel` / async
  `transcodeModelImages` rule.

## Open in this plan

In the order they should be settled:

1. **Only Linux x86_64 has built or run this.** Android (the NDK build,
   and the choice to link `c++_static`, which is an ODR hazard if another
   library in the process brings `libc++_shared`, as speech does),
   Windows MSVC and macOS are unbuilt; that now includes `zstd.c` as a
   second `cc` build. Unverified on device: that ANGLE over D3D11 and
   over Metal list the BPTC extension, the ETC2 path on a mobile GPU,
   both probes on each.
2. **Step 3**, as designed above.
3. **The quality a bake uses, per map kind.** The figures are in "UASTC
   and ETC1S across the quality scale"; the choice is a judgment by eye
   on real textures, which the figures cannot make. The default 0.5 puts
   a normal map at 35.7 dB with RDO at lambda 8, and that default was
   picked before anyone knew quality drove RDO. Effort is fixed at
   upstream's default and unexamined.
4. **Encoder output across machines.** Repeat encodes are byte-identical
   on one machine. Across compilers and architectures is unchecked. The
   one-payload rule does not need it (a bake runs once, its output
   ships), a reproducible build would.
5. **Attribution.** Basis Universal is Apache 2.0: distributed binaries
   owe its NOTICE, and the zstd inside it is BSD (`zstd/LICENSE`). Check
   how libvpx and opus are credited in a packed app and add both the
   same way. The Khronos test fixture is Apache 2.0 and is not
   distributed in any binary.
6. **The runtime's size with zstd** is not measured: only the object is
   (see "Binary size").
7. **Sponza, re-measured**: bake with `--ktx2`, load on Windows (the
   original site), Linux and the Adreno 610 tablet; record shipped size,
   bake time, texture bytes from `/gpu`, client RSS, transcode time per
   platform; a side-by-side against the RGBA8 bake for the user's eye.

## Found, outside this plan

Each is additive on what is built. None is started; the ones about
weight live in [runtime-optimization](../backlog/runtime-optimization.md).

- **Opaque textures pay for an alpha channel they do not have.** ETC2
  RGBA8 is 16 bytes per block; ETC2 RGB8 is 8. Every Sponza base color
  map is opaque, so mobile holds twice what it needs. `etc2-rgb8` (and
  its sRGB twin) as targets, picked when the file has no alpha, halves
  it. BC7 is 16 bytes either way, so the GPU figure then differs between
  mobile and desktop; the payload does not.
- **Normal maps as two channels.** The practice everywhere else is RG
  only, transcoded to EAC RG11 or BC5, Z rebuilt in the shader: better
  quality per bit than RGBA blocks. Both targets are switched off in the
  build today, and the materials sample RGB.
- **ASTC** as a third target: present on most current Android GPUs,
  better than ETC2 at the same size. Its transcoder tables are compiled
  out.
- **Copies on the way to the GPU.** A transcoded payload is copied out of
  the blocking task into a JS typed array, again into the raster command
  and again by the driver; a scene's worth passes through the JS heap. A
  create that takes the KTX2 and hands the transcoder's buffer to the
  raster thread would skip the heap. That is the convenience layer the
  primitive-first design leaves room for.
- **Uploads run on the raster thread.** A scene's textures at load are
  one long stall there. A quarter of today's RGBA8 uploads, and still a
  hitch; the pixel-unpack-buffer path `update_texture` uses does not
  cover creates.
- **UI images.** Displaying a compressed texture through `<texture src>`
  is declared out of contract and was never tried, so `createImage` and
  `Image` cannot use any of this. Converting every packaged image at pack
  time would need that path, and a per-asset opt-out: block compression
  damages sharp art and text.
- **Untrusted input.** A fetched glTF feeds network bytes into a C++
  parser. Upstream ships a SECURITY.md and pins are ours to bump; no
  review or fuzzing was done here.
- **A device-side cache of transcoded bytes.** Possible without new
  surface; pays only where the transcode is slower than the flash read
  (UASTC to ETC2 on a slow CPU). Measure on the tablet and the TV first.
  Never a hidden default cache.
- **Context loss** drops compressed textures like every other: alloy
  keeps no bytes. [gpu-context-loss](../backlog/gpu-context-loss.md).
- Mutable or resizable compressed textures, compressed cube maps, a
  `.flux.ts` port of the environment and splat tools (needs gzip in
  flux).

## Verification done

- alloy: `cargo test -p alloy --lib texture` passes, with tests for block
  and chain sizing and the BPTC gate.
- forge: `cargo test -p forge --lib --features ktx2 ktx2`, seven tests:
  each codec to each target, decoded color, the sRGB flag, byte-identical
  repeat encodes, refusals, a UASTC file smaller than its payload, and a
  file from another writer (`tests/data/khronos_uastc_zstd.ktx2`:
  `ktx_document_uastc_rdo_4_zstd_5.ktx2` of the Khronos KTX-Software
  test resources, written by `ktx create` of libktx 5.0, 1024x1024 with
  11 levels, UASTC with RDO and zstd) to all three targets.
- The full suites, 2026-09-29, all passing: `cargo test -p forge --lib
  --features ktx2` 98, `-p alloy --lib` 616, `-p flux --lib --features
  gui,ktx2` 69, `-p flux --test image --features ktx2` 7.
- flux: `cargo test -p flux --test image --features ktx2 texture`, two
  tests: the round trip from JS, and the refusals (no GPU to pick a
  target, not a KTX2, unknown target, missing codec, short payload).
- Release client, Linux, `probes/compressed-texture-probe.tsx` (debug
  command `check`), hand-built solid blocks read back through rgba8
  passes: ETC2 138 of an expected 138, the uploaded mip level 70 of 70,
  BC7 137 of 137, every refusal gate throws, inventory sizes 64 / 112
  (chain) / 64 bytes.
- Release client, Linux, `probes/ktx2-texture-probe.tsx` (debug command
  `check`), pixels through `encodeTexture`, `transcodeTexture` with no
  target, `createTexture`, read back at full size and minified through
  the chain: the device picked `bc7-rgba8`; a linear gray of 120 reads
  back 120 from both codecs; an sRGB (200, 100, 50) reads back
  (148, 33, 8), its linear-light value; each 64x64 texture holds 5488
  bytes with its chain against 21845 as rgba8.

Under memory pressure build the tests with `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0`: the debug-info link of the Basis objects is
what runs the machine out.

## Findings

(cut into notes/ on completion)

### UASTC and ETC1S across the quality scale (2026-09-29)

Sponza maps, 1024x1024 with the full chain, `wrap: "repeat"`, release
build, x86_64. Every one is 1398128 bytes on the GPU. PSNR is over the
color channels of the base level against the decoded source, which for
the JPEG sources already carries JPEG's own error. Reproduce with the
ignored test `measure_quality` in `forge/src/tests/ktx2.rs`
(`KTX2_MEASURE_IMAGES`, `KTX2_MEASURE_CODEC`).

UASTC with zstd, file bytes and PSNR:

| quality | normal map A | normal map B | metallic-roughness |
|---|---|---|---|
| 0.25 | 747443, 34.5 dB | 246553, 37.3 dB | 583908, 32.1 dB |
| 0.5 | 769038, 35.7 dB | 248853, 38.0 dB | 591500, 32.6 dB |
| 0.75 | 830920, 37.6 dB | 256653, 39.7 dB | 638651, 34.5 dB |
| 0.9 | 967132, 40.1 dB | 276609, 43.2 dB | 803210, 39.6 dB |
| 1.0 (RDO off) | 1296213, 42.9 dB | 485560, 48.6 dB | 1072315, 45.9 dB |
| source JPEG | 413837 | 211322 | 668962 |

ETC1S, file bytes and PSNR:

| quality | base color (PNG source, 2427921) | base color (JPEG source, 572245) |
|---|---|---|
| 0.25 | 217972, 37.4 dB | 149470, 30.8 dB |
| 0.5 | 250422, 38.1 dB | 183961, 31.9 dB |
| 0.75 | 292309, 38.9 dB | 226680, 33.1 dB |
| 1.0 | 319659, 39.2 dB | 260461, 33.9 dB |

What the figures say, short of the judgment by eye:

- zstd alone (quality 1.0) takes UASTC from 8 bits per texel of base
  level to 3.7 to 9.9 for the file with its chain: noisy maps barely
  compress without RDO.
- Below 0.75 UASTC gives up quality for almost no bytes: 0.5 against
  0.75 is 3 to 8 percent smaller and 2 dB worse. 0.9 is the knee on two
  of the three maps.
- A UASTC map ships larger than its JPEG at any quality that keeps it
  above 40 dB. The win for data maps is GPU memory, not download.
- ETC1S moves little across the scale: 0.25 to 1.0 is 1.5 to 3 dB.

An encode takes 0.6 to 2.5 s per 1024x1024 map on this machine; RDO is
most of it (0.7 s with it off).

### Why not the binding crate

`basis-universal` 0.3.1 is the crate Bevy uses, vendored C++ with nothing
fetched at build, 431k downloads. It was rejected on coverage, not on
trust: last commit 2023-11-02, Basis 1.16, and its wrapper has no KTX2 at
all - only `.basis` files and a low-level UASTC slice transcoder, so ETC1S
inside KTX2 cannot be transcoded through it, and the encoder cannot write
KTX2. Upstream is active (v2.50, 2026-08-03, Apache 2.0) and ships the C
API forge binds. Not adopted either: `basisu_c_sys` (single publisher,
young), `ktx2-rw`, the `ktx2` parser crate (not needed).

### Binary size (2026-09-29, x86_64)

| what | size |
|---|---|
| transcoder, every output format (v2.50, compiled alone, -O2) | 1323 KiB |
| transcoder, legacy formats off (what forge builds) | 978 KiB |
| transcoder as linked through the 0.3.1 crate (Basis 1.16), fat LTO, stripped | 881 KiB |
| encoder on top of that, same measurement | 924 KiB |
| `solidrt-go` at `release`, unstripped, before and after `ktx2` | 71.26 to 75.42 MB |
| zstd, the whole library (`zstd.c`, compiled alone, -O2), text | 669 KiB |
| zstd, the decoder alone (`zstddeclib.c`, same measurement) | 131 KiB |

The zstd rows are object sizes before the linker drops what is
unreferenced; forge links the whole library because encoder and
transcoder share one feature. A runtime that only transcodes needs the
decoder alone, which is the split
[runtime-optimization](../backlog/runtime-optimization.md) would make.

The transcoder is a transcoder to some 25 formats from five codecs, not a
decoder of one. ETC1S reaches each target by table lookup (150 to 180 KiB
of table source per target family), and since 1.16 the file gained UASTC
HDR, ASTC HDR, XUASTC, a full ASTC LDR decoder and a DDS reader. The
switches for those (`BASISD_SUPPORT_UASTC_HDR`, `_XUASTC`, `_ASTC`) do not
compile when set to 0 in v2.50, so they cannot be configured out, and the
dispatch is a runtime switch on the target, so the linker cannot drop
them either. The `release-opt` figure for the runtime is not measured.

### The submodule is heavy

444 MB at depth 1: webgl 87, test_files 80, python 58, bin 49; `encoder/`
and `transcoder/` are 12. It follows the libvpx and opus convention. If
that weighs on every checkout, the alternative is a vendored copy of the
two directories with LICENSE and NOTICE.

### Foreign alpha

A KTX2 from elsewhere carries straight alpha (the glTF rule) and
`transcodeTexture` hands its texels through as they are, so a translucent
`KHR_texture_basisu` material blends slightly wrong until its shader
premultiplies. Opaque maps, the common case, are unaffected.

### Naming

forge already has `tests/texture.rs` (the video texture player), so the
module is `forge::ktx2` and its tests `tests/ktx2.rs`.
