---
title: Compressed textures - KTX2 shipped, ETC2 or BC7 on the device
description: One shipped payload (Basis Universal in KTX2, baked with our own encoder) transcoded at load to the device's native block format - ETC2 on GLES 3.0 targets, BC7 where the BPTC extension is reported; built end to end (device formats, the codec with zstd, the model bake and loader, texture settings per application, attribution) and verified on Linux, Windows and Android; settings per file are built (compression on by default, a list of glob entries that raise or exempt files, glob matching and a scan in flux); next are compressed textures outside a model, after the build stage is designed, then the memory a load leaves behind.
created: 2026-07-30
---

# Compressed textures - KTX2 shipped, ETC2 or BC7 on the device

From [gpu-review](../notes/gpu-review.md) (lesson 17), shaped and started
2026-09-29. `createTexture` was RGBA8-class only, so a texture-heavy scene
paid 4 bytes per texel per mip level on every device. GLES 3.0 mandates
ETC2/EAC in core and every desktop exposes BC7, both 1 byte per texel with
the sampler doing the decode for free.

## State (2026-09-30)

| Step | State |
|---|---|
| 1. Device formats on `createTexture` | built, verified on Linux, Windows and Android |
| 2. `transcodeTexture` / `encodeTexture` | built, verified on the same three; zstd on, a libktx file reads, four transcode threads |
| 3. Tool hosting, model bake, loader, glTF | built; a baked Sponza loads on Linux, Windows and the tablet |
| Texture settings per application | built (`solidrt.textures`, `@solidrt/core/textures`) |
| Attribution | built for everything in the binaries (`THIRD-PARTY-NOTICES.txt`) |
| 4. Settings per file | built 2026-09-30, verified on Linux; not run on Windows or Android |
| Self-review list | built 2026-09-30, all of it |
| macOS arm64 | client builds, forge tests pass; not run on device |
| Sponza re-measurement | done on Linux, Windows and the tablet; the judgment by eye is open |

Steps 1 to 3 hold on three platforms; step 4 and the self-review list
are verified on Linux only. What is left is what a developer expects
next, then performance: "Open in this plan" has it in that order (the
user's, 2026-09-29: functionality that must be in before performance,
"better to do first what developers are expecting to get"), and starts
with where to pick up.

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

- Others build their own 3d extensions (stated 2026-09-29). So
  everything a texture bake or a loader needs sits BELOW `@solidrt/3d`,
  in flux and core, and the 3d package is one consumer among others:
  nothing texture-related may exist only there.

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
(Linux, Android)". Corrected (from driver documentation; observed are
the Linux row, BC7 on Windows, and ETC2 with no BC7 on an Adreno 610):

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
- Both calls copy their input. `encodeTexture` runs on four encode
  threads of its own (`forge::ktx2::encode_queued`, 2026-09-30).
  `transcodeTexture` runs on FOUR transcode threads of its own
  (`forge::ktx2::transcode_queued`, decided 2026-09-29), shared by every
  caller in the process and fed in queue order, so a `Promise.all` over
  a scene is bounded whoever wrote it. The threads are a
  `forge::workers::Workers`, a fixed pool for CPU-bound jobs that is not
  specific to textures.
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

### 3. Tool hosting, bake and load (packages/cli, packages/3d) - built

- **Tools under flux.** `srt tool` spawns bun for every
  `<package>/tools/<name>.ts`. A tool named `tools/<name>.flux.ts` runs
  under the flux binary instead, with the `SRT_*` environment the server
  gets, listed under the same `<pkg>/<name>`. Opt-in by file name, no
  scanning of imports. flux runs one plain-JS file (no TypeScript, no
  module loaded from disk), so the tool is bundled into a temp file
  first, the way `srt run` bundles the dev server; the shared step is
  `packages/cli/src/lib/flux-script.ts`. The model tool moved
  (`tools/model.flux.ts`); `environment.ts` and `splat.ts` stay under
  bun (splat needs `node:zlib`'s gunzip for `.spz`, which flux does not
  offer).
- **flux grew for it.** `flux:process` has `exit(code?)` (the process
  ends inside the call, the engine's shutdown hooks run and output
  flushed first, an integer in 0..255). The host allows it
  (`ProcessExit` userdata, set by the flux and fluxrt binaries); it
  throws in a windowed app, which ends through core's `exit()` and its
  quit hooks, and in an isolate. Also
  `flux:path` has `basename`, `dirname`, `extname` with Node's
  semantics; the cores are in `forge::process` and `forge::path`.
- **The bake.** `srt tool 3d/model` (as `tools/model.flux.ts`; the
  `--ktx2` flag it had is gone, see "Settings per file",
  `flux:fs` and `flux:process` in place of `node:fs` and `process.argv`)
  runs `parseGltf` as today, then for each sampled image `decodeImage`
  (premultiplied, the default) and `encodeTexture`: ETC1S with `srgb` for
  the images a material samples as `map` or `emissiveMap`, the slot rule
  `createModel` already uses, UASTC linear for the rest, `mipmap: true`,
  `wrap: "repeat"`, four images at a time (see "Self-review, built").
  The KTX2 bytes replace the image entry and `encodeModel` writes the file
  unchanged. The summary line adds image bytes before and after. Without
  the flag nothing changes. The tool never takes a platform. The
  runtime-free `@solidrt/3d/model` entry (`model-data.ts`) stays
  runtime-free.
- **The load path.** `createModel` stays synchronous. A new async
  `transcodeModelImages(data)` sniffs the KTX2 magic (`isKtx2`, from
  core) and starts a transcode per such image; the runtime runs four at
  a time. As built, `data.images` keeps each image's file
  (`ModelImage.bytes`) and the results go into an optional parallel list,
  `data.textures` (by image index), rather than replacing the entries:
  a parse and every existing consumer keep their types, and transcoded
  data can still be written by `encodeModel`, which never writes
  `textures`. `loadModel` and `loadGltf`, already async, call it before
  `createModel`. `createModel` uploads a ready payload at its format and
  decodes everything else as today; a KTX2 image it meets untranscoded
  throws naming the function to await first. A KTX2 image takes its
  color space and its mip chain from its file. What an image is sampled
  as is ONE rule, `modelImageUses` (sRGB or not, and its kind), used by
  `createModel` for the uploads and by the bake for the encodes, so a
  baked image samples as the same image unbaked: an image a color slot
  and a normal slot share is sRGB and takes the accurate codec. The `.srtm` image block carries the
  KTX2 bytes as-is.
- **Images are records** (2026-09-29). `ModelData.images` is a list of
  `ModelImage`: `bytes`, `name` (the document's name, else the uri of
  its file, else "image<i>": Three's rule for a texture's name) and
  `uri` when the image is a file of its own. The container writes the
  name with the block and not the uri (it embeds the image), which made
  it version 11; version 10 files are rejected like every earlier
  version. `createModel` labels each texture `<label>-<image name>`, so
  the `/gpu` inventory names the image. `data.textures` stays a list
  beside it on purpose: a create that streams would remove it.
- **glTF input.** `parseGltf` accepts `KHR_texture_basisu` in
  `extensionsRequired`, and a texture's
  `extensions.KHR_texture_basisu.source` takes precedence over `source`.
  That closes the KTX2 row of
  [3d-model-loader](../backlog/3d-model-loader.md).
- **Settings per application** (decided 2026-09-29). The codec and
  quality per kind of map (`color`, `normal`, `data`) are the
  `textures` group of the `solidrt` key in the package.json of the
  project the tool runs in, over the defaults; an unknown kind, codec or
  field fails the bake before anything is encoded. The reader is
  `textureSettings` in `packages/core/src/textures.ts`, published as
  the runtime-free entry `@solidrt/core/textures` (with `isKtx2`), so
  any extension's bake reads the same settings. The kinds are defined
  by what the texels ARE (color the eye sees, directions, other linear
  values that tolerate error), not by material slots: the slot mapping
  is the 3d package's (`modelImageUses`). Exact values (a lookup table,
  a distance field) are no kind and are not block compressed. Open
  kinds per extension were rejected: two extensions reading one group
  would each refuse the other's names. Color space, mips and wrap are
  not settings. Not in the scaffold's package.json (the key belongs to
  an extension); documented in packages/3d/AGENTS.md. Settings per
  file are decided and being built: "Settings per file" below.
- **Docs.** packages/3d docs and AGENTS notes: the bake flag, what ships,
  what the device does, the sync `createModel` / async
  `transcodeModelImages` rule.

### 4. Settings per file (packages/core, forge, flux, packages/3d) - built 2026-09-30

Unity, Godot and the Khronos KTX guide all let one texture be raised
above the rest, or left alone; the settings were per kind only.

```json
"solidrt": {
  "textures": {
    "compress": true,
    "color": { "codec": "etc1s", "quality": 0.75 },
    "files": [
      { "match": "assets/sponza/textures/lion_*.png", "codec": "uastc", "quality": 0.95 },
      { "match": ["assets/ui/**", "assets/hero.glb#lut*"], "compress": false }
    ]
  }
}
```

What was decided, each by the user:

- **Compression is ON unless the app says otherwise.** "Nothing
  changes for an app that says nothing" was my argument for off and is
  no argument: nothing is released, and an app that says nothing would
  keep paying four times the texture memory. Unity and Godot compress
  by default. It covers textures whose kind is known (a model's
  images); nothing compresses a UI image, which is not baked.
- **`compress` is a setting**, per app, per kind and per entry, the
  narrower one winning. `--compress` and `--no-compress` stand in for
  the app's own for one run; `--ktx2` is gone. What a kind or an entry
  says about itself holds against the flag.
- **"Not compressed" is `"compress": false`.** `"codec": "none"` names
  a codec that is none, and `null` already means "unset, as if the key
  were absent" in this config (`packages/cli/src/lib/project.ts`), so
  `"codec": null` would read as the kind's codec. An entry with
  `compress: false` and a codec or a quality fails.
- **A list named `files`, its pattern field `match`**, a pattern or a
  list of them, later entries winning field by field: the shape of
  Prettier's and ESLint's overrides, so the order is written down and
  does not rest on the key order of a JSON object. `files` and not
  `overrides` because the same list will declare textures outside a
  model, where nothing is overridden.
- **Patterns are globs, matched by the `glob` crate** (rust-lang, no
  dependencies): `*`, `**`, `?`, `[a-z]`, `[!a-z]`, a literal `*`
  written `[*]`. No `{a,b}`: `match` takes a list. One pattern covers
  a folder; a single file is the rare case.
- **A pattern is matched against the image's path from the project
  root**, not the uri the glTF writes, so two models that each have a
  `textures/base.png` are told apart. This is how Unity and Godot key
  import settings, and it holds for a texture outside a model
  unchanged.
- **An embedded image is `<model path>#<image name>`.** The model's
  path alone names every image embedded in it. A `#` in a file's name
  is written `[#]`.
- **A pattern that names nothing fails the bake**, before anything is
  encoded. A bake sees one model and the settings are the app's, so the
  question goes to the project's files (the scan comes back empty);
  the `#name` part is checked by the bake of that model.

As built:

- **forge, flux.** `glob = "=0.3.3"`. `forge::path`: `matches_glob`,
  `check_glob`, `relative`. `forge::fs::glob(pattern, cwd)`: the files
  a pattern matches, sorted, honoring the assets mount (a packed mount
  is matched against its index). On flux: `matchesGlob(path, pattern)`
  and `relative(from, to)` in `flux:path`, `glob(pattern, { cwd })` in
  `flux:fs`, Node's names. `relative` was not in what the user
  approved; the path from the project root needs it and it is Node's.
  Matching is case-sensitive, a wildcard never stands for a separator,
  a leading dot is an ordinary character; a malformed pattern throws
  naming the character.
- **The scan and the matcher agree**, by test: a file is listed exactly
  when its path matches. The crate's own scan reads a closing `**` as
  the directories below where its matcher reads everything below, so
  the scan asks for the entries of those directories and keeps the
  files.
- **The reader** (`packages/core/src/textures.ts`) stays in the
  runtime-free entry, and so does deciding which setting a texture
  gets: `textureSettingFor(settings, kind, { file, name }, matchesGlob)`
  takes the matcher as an argument, `unmatchedTextureFiles(settings,
  scan)` the scan, `unmatchedTextureNames` the matcher. So the whole of
  it is tested under bun, where I had told the user it would move to
  the runtime side. Settings are frozen, the defaults included, and
  `TextureCodec` is the entry's own type; `packages/core/src/image.ts`
  holds the line that stops compiling when it and the encoder's
  disagree.
- **The bake** (`packages/3d/tools/model.flux.ts`) reads the settings,
  checks the patterns, asks per image and prints what it did, grouped
  by kind, codec and quality, with the images it kept.
- **The CLI rejected the key.** `parseProjectConfig` fails on any key
  of `solidrt` it does not know and `textures` was not one, so an app
  that declared texture settings could not `srt run` or `srt pack`.
  Present since the settings were built. The CLI now takes the key as
  an object and leaves its content to the reader (it does not depend
  on core).

Dropped on the way: a matcher of my own in JS (`@solidrt/core/glob`,
built, tested and deleted); `globset`, whose advantages over `glob` are
`{a,b}` and matching thousands of patterns in one pass.

Verified, 2026-09-30, Linux:

- `cargo test -p forge --lib --features ktx2` 116 (2 ignored), among
  them the pattern language, `relative`, the scan, its agreement with
  the matcher and a packed mount; `cargo test -p flux --features
  compile,ktx2 --test path --test dir --test file --test process
  --test image` 14, 7, 8, 8, 7; `-p flux --lib --features gui,ktx2` 70.
- packages/core `bun test tests` 16; packages/3d `bun test` 40;
  `srt check` on the 3d package, the tool and the probes.
- A scratch project with Sponza and a .glb of three named embedded
  images: the entries raise one color map to UASTC at quality 1, keep
  the four PNGs and the embedded `lut_warm`, lower `face`; a
  misspelled file pattern, a misspelled name and a pattern for a model
  that does not exist fail the bake listing all three; a malformed
  pattern and a wrong codec fail naming the key; `--no-compress` keeps
  everything.
- That Sponza (65 images compressed, 4 kept) loads in the rebuilt
  release client: 585 ms, 65 textures as BC7 and 4 as rgba8.
- The rebuilt release client answers `matchesGlob`, `relative` and
  `glob` from an app.

Not rebuilt: the production runtime (`make runtime`), which was
already older than the flux binary when this started.

What wider scopes add to it, each additive on the above:

- Textures outside a model: an entry also takes `kind`, `mipmap` and
  `wrap`, which no material or sampler decides there.
- Normal maps as two channels: the settings keep their shape, every
  baked normal map is rebaked.

## Open in this plan

### Where to pick up (left 2026-09-30)

Nothing is half done: every change of 2026-09-29 and 2026-09-30 is
built, tested and written up above. Nothing is committed; the user
commits.

**The next step** is the design of the build stage
([asset-build-stage](../backlog/asset-build-stage.md)): a design to
review, no code. The user was asked on 2026-09-30 whether to draft it
and has not answered, so ask first. It comes before "compressed
textures outside a model" because it decides how an app names a baked
file. When the work starts the item moves to `plans/`.

**Asked and not answered:**

1. Whether to draft the build stage's design (above).
2. `relative(from, to)` was added to `flux:path` without being asked
   for (step 4 needs the path from the project root, and it is
   Node's). Keep it or take it out.
3. The two under "Proposed, awaiting the user's decision" below.
4. The order under "Functionality a developer expects" is still a
   suggestion.

**For the user to do, none of it blocking:**

- Every `.srtm` baked before 2026-09-29 is version 10 and is refused:
  re-bake with `srt tool 3d/model`. The one in the repo
  (`probes/model-reuse/reuse.srtm`) is done.
- A bake now compresses by default and takes about a minute for
  Sponza; `--no-compress` is the quick one.
- `make runtime`: the production runtime in `dist/` is older than the
  flux binary and the client, which are rebuilt and staged.
- The git index holds `packages/core/src/glob.ts` and its test as
  added, the working tree has them deleted (they were staged by
  something other than this session, which never stages). Several
  other files of this work are staged in part the same way.
- `make client` regenerated `lattice/resources/bsod/bsod.srt.js` and
  `lattice/resources/player/index.srt.js`.
- Windows and Android have not run step 4: `matchesGlob` treats "/"
  and "\\" alike on Windows by the crate's word, not by a run here.

**Traps met on the way:**

- This plan was edited by another session while this one worked (the
  section "Smells in the code of steps 1 and 2" arrived mid-session).
  Re-read before editing, and edit by replacing text, never by writing
  the file back whole.
- Texture settings patterns are matched under flux only. Bun's matcher
  reads `{a,b}` and backslash escapes, ours does not; the tests under
  bun use patterns both read alike.
- Large scratch goes to `target/scratch/`, not the session scratchpad
  (RAM). The machine had 4 to 5 GB free all session: builds ran with
  `CARGO_BUILD_JOBS=4`, test builds with `CARGO_PROFILE_DEV_DEBUG=0
  CARGO_PROFILE_TEST_DEBUG=0`.

### The build stage (decided 2026-09-30: its own item)

[asset-build-stage](../backlog/asset-build-stage.md). It is designed
before "compressed textures outside a model" is built, because it
decides how an app names a baked file, and built after tool discovery.

### Proposed, awaiting the user's decision

Nothing here is decided. Each was put to the user on 2026-09-29 and not
answered yet; ask before building.

1. **Notices in a packed app.** The platform packages carry
   `THIRD-PARTY-NOTICES.txt` (see "Attribution"); `srt pack` does not
   add it to what it writes, which leaves the developer to ship it.
   Suggested: next to the executable for the single-file and folder
   forms, inside the APK's assets for an APK. Where it goes per output
   form is a packaging decision.
2. **Sponza: which asset.** The user called the Khronos sample on disk
   (`~/solidrt/demoes/sponza/model/Sponza`) "a very old version", and
   separately pointed at that demo for its lighting. Whether the size
   figures should be redone on another asset was asked and not
   answered. What does not depend on the asset (a quarter of the
   texture memory, the load path) is measured.

### Functionality a developer expects, in the suggested order

The order is a suggestion put to the user, not yet confirmed.

1. **Compressed textures outside a model**, after the build stage is
   designed ([asset-build-stage](../backlog/asset-build-stage.md)),
   which decides how an app names a baked file. Only the model bake
   exists. A 2d tile set or a standalone image under `assets/` has the
   primitives (`encodeTexture`, `transcodeTexture`, `createTexture`)
   and nothing above them: no tool that bakes it, no loader that takes
   it. The settings and `isKtx2` are already in core for this.
2. **Tools of third-party extensions in `srt tool`.** Discovery looks
   under `node_modules/@solidrt/*` only (okf/ideas.md has the line).
   Suggested rule: the project's direct dependencies that depend on
   `@solidrt/core` and ship a `tools/` folder. Scanning every installed
   package would list unrelated scripts.
3. **Straight alpha in KTX2 files from other tools.** See "Foreign
   alpha": a translucent `KHR_texture_basisu` material blends slightly
   wrong until something premultiplies.
4. **Untrusted input.** A fetched glTF feeds network bytes into a C++
   parser; no review or fuzzing was done.
5. **macOS on device.** The client builds; whether ANGLE over Metal
   lists the BPTC extension, and the probes, are open. The builder
   cannot do it: a client started over ssh finds no display while the
   console session is another user's. It needs a run from a session
   logged in at the Mac.
6. **The quality of the defaults, by eye.** The defaults follow the
   Khronos KTX guide and glTF-Transform (ETC1S 0.75 for color, UASTC
   0.9 for normal and data; `data` was first ETC1S on a recommendation
   made before that guidance was looked up). The comparison images
   were made in the Sponza demo's scene (`probes/sponza-compare/`) and
   live in a session scratchpad, so they are gone; remake them for the
   judgment. `encodeTexture`'s own default of 0.5 was picked before
   anyone knew quality drives RDO, and effort is upstream's default,
   unexamined.

### Performance and size, after the above

1. **A compressed load holds every payload at once.** On Windows and
   Android a loaded compressed model leaves the process 100 to 200 MB
   larger than the textures account for; see "What a load leaves
   behind, per platform". glibc is handled (the mmap threshold is
   pinned at startup). The fix for every platform is a create that
   takes the KTX2 and uploads each texture as its transcode finishes,
   so a few payloads are alive at a time and none passes through the
   JS heap ("Copies on the way to the GPU" below). A cache of
   transcoded bytes does not fix it.
2. **Opaque textures as ETC2 without alpha**, **ASTC as a target**,
   **normal maps as two channels**: below, under "Found, outside this
   plan". ASTC is what Three prefers for UASTC sources.
3. **The runtime's weight.** The whole zstd library and the encoder
   are linked into every runtime; one needs the decoder and the
   transcoder. The runtime's size with zstd is not measured, only the
   object is (see "Binary size").
4. **Encoder output across machines.** Repeat encodes are
   byte-identical on one machine. The probe's four files came out the
   same size from MSVC x86_64 and NDK clang arm64 (607, 509, 610, 509
   bytes); their bytes were not compared. The one-payload rule does
   not need it, a reproducible build would.

### Left unexplained

- A client's memory before any load varies between runs of the same
  build: 185 or 225 MB on Linux, 117 or 279 MB on the tablet. Readings
  in this plan are compared as what a load adds for that reason.
- On every remote client the first debug query after a launch or a
  load timed out for up to a minute, then answered.

### Smells in the code of steps 1 and 2

Written down 2026-09-29 by the session that wrote that code, each
confirmed present at commit 2f2e41d2. None changes behavior today and
none was approved as work. Step 3's own list was approved and is built
("Self-review, built"); two entries here changed with it and say so.

Layering:

- **A forge plugin reads gui state.** `flux/src/forge_plugins/image.rs`
  calls `alloy_plugins::try_gui` under `cfg(feature = "gui")` to pick the
  default transcode target. The placement rule says the crate marshalled
  decides where a plugin lives; this one marshals forge and consults
  alloy.
- **The plugin does the sizing.** `create_texture` in
  `flux/src/alloy_plugins/gpu.rs` computes the expected payload
  (`chain_byte_len` or `byte_len`) and words the error; alloy's
  `create_texture_at` does not check the length at all, only
  `new_compressed` on the raster thread does, as a backstop. The rule is
  thin plugins, domain logic in the owning module. The rgba8 path had
  this shape before; the chain made it larger.
- **Compressed textures are adopted into Impeller.** The raster create
  path hands every 2D texture to `gl::adopt_texture`, which describes it
  to Impeller as RGBA8888. Cube maps skip adoption because they are
  sampler-only; compressed textures are sampler-only too and do not skip
  it. Nothing has gone wrong, and nothing was tested that would show it.

Duplication:

- **The block math exists twice**, `BLOCK_EDGE` / `BLOCK_BYTES` and the
  chain arithmetic in `alloy/src/gpu/texture.rs` and again in
  `forge/src/ktx2/mod.rs` (forge cannot depend on alloy). Only a test
  that feeds a forge payload to an alloy create would catch them
  drifting, and that test is the probe, not a unit test.
- **The format names cross crates as strings.** `Target::format_name` in
  forge spells alloy's `TextureFormat` names by hand, and
  `TranscodedTexture.format` in `image.d.ts` is a literal union copied
  from `TextureFormat` in `gpu.d.ts`. Three vocabularies kept equal by
  attention.
- **`BASE_CAPABILITIES` is two lists** under opposite `cfg`s in
  `flux/src/plugins/mod.rs`; a capability added to one and not the other
  compiles.
- **The texture calls have two bodies each**, the real one and a
  throwing stub under `cfg(not(feature = "ktx2"))`, with signatures that
  must be kept alike by hand.
- **Option parsing is repeated.** `string_opt` was added beside
  `premultiplied_opt`, and `encode_texture` re-reads `img.data`,
  `img.width`, `img.height` the way `encode_image` does. (2026-09-30:
  `string_opt` moved to the shared toolkit, `plugins/marshal.rs`, which
  `flux:fs` uses too; the rest stands.)
- **`KTX2 ?= 1` is declared in two Makefiles** (lattice and flux).

The bindings:

- **No version check.** The libvpx bindings pin an ABI version that init
  verifies; `ktx2/ffi.rs` declares `bt_init` and `bu_init` and trusts
  the submodule. Upstream exports `bt_get_version` / `bu_get_version`; a
  pinned constant compared at init would turn a submodule bump that
  changes a signature into an error instead of undefined behavior.
- **The `unsafe` blocks are whole function bodies** (`transcode` and
  `encode` in `forge/src/ktx2/mod.rs`), so the safe checks and the
  foreign calls are not told apart by the block.
- **Handles and pointers are `u64`**, as upstream's API has them, so the
  compiler cannot tell a file handle from a parameter block from a
  buffer address.
- **The source list is copied from upstream's CMakeLists.txt** into
  `forge/build.rs` (32 files). A submodule bump that adds a file fails
  at link, one that drops a file fails at compile; neither is checked
  before.
- **Encoders we never call are compiled**: UASTC HDR, ASTC HDR and LDR,
  XUBC7, the EXR and DDS writers, the PNG and JPEG readers. The encoder
  library does not link without them as upstream lays it out.

Smaller:

- **`byte_len` has an `unreachable!`** arm for the compressed formats
  (they return earlier), a panic path in a function every create calls.
- **`gl_storage` returns a layout and a type for compressed formats**
  that nothing may use.
- **`NDEBUG` is defined in every profile**, so upstream's asserts are
  off in debug builds too, where they would be wanted.
- **The readback refusal lists every reason in one sentence** (float,
  compressed, sRGB) whatever the format was.
- **An error changed order.** `create_texture` now parses the sampler
  before checking the size, so a call wrong in both reports the sampler
  first, where it reported the size.
- **The encoder takes every core inside a blocking-pool thread**, and
  concurrent `encodeTexture` calls multiply that. (2026-09-30: no
  longer so. Encodes run four at a time on threads of their own, and
  one encode does not take every core: "Self-review, built" has the
  measurement.)
- **`byteLength` is arithmetic**, the format's size for the declared
  dimensions, not what the driver allocated: on a driver that expands
  ETC2 it understates by four.

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
- `forge::workers`: `cargo test -p forge --lib workers` (2): no more
  jobs at once than threads, a panicking job fails alone.
- packages/core: `bun test tests/textures.test.ts` (5), which also
  proves the entry imports no runtime.
- `@solidrt/core/glob`: `bun test tests/glob.test.ts` (10): each piece
  of the pattern language against the paths it matches and the ones it
  does not, under bun, so the entry imports no runtime.
- Image records, 2026-09-29: packages/3d `bun test` passes (40), the
  check rig covering the name rule (the document's name, the uri, the
  index), the uri of a file image, and names through the container
  without uris; `srt check` passes on the package, the tool and the
  probes. Release client, Linux: the Khronos Sponza baked by the tool
  (version 11, 69 images) loads in `probes/ktx2-model-probe.tsx` and
  the `/gpu` inventory lists its textures as
  `probe-<file name of the image>`.
- flux additions: `cargo test -p forge --lib path` (4), `cargo test -p
  flux --features compile --test path --test process` (11 and 7; the
  `exit` tests run the flux binary).
- packages/3d: `bun test` passes (40), with the check rig covering
  `KHR_texture_basisu` (the KTX2 source wins over the fallback, in the
  prefetch list and the parse) and the container round trip of KTX2
  bytes; `srt check` passes on the package and on the tool.
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

### Sponza, baked (2026-09-29, Linux, release client)

The Khronos Sponza sample: 25 materials, 69 sampled images, 1024^2 and
2048^2. `srt tool 3d/model Sponza.gltf --ktx2`, quality 0.75 for ETC1S
and 0.9 for UASTC, loaded by `probes/ktx2-model-probe.tsx` on Mesa Intel
(the device picked BC7).

| | PNG/JPEG bake | `--ktx2` bake |
|---|---|---|
| `.srtm` | 49569 KiB | 46637 KiB |
| images in it | 41984 KiB | 39052 KiB |
| of which color maps (25) | 17.4 MB | 4.8 MB |
| of which data maps (44) | 23.6 MB | 33.4 MB |
| bake time | 1.3 s | 130 s (7.6 min of CPU) |
| texture bytes, `/gpu` | 362.7 MB | 90.7 MB |
| `loadModel` | 1041 ms | 449 ms |
| client RSS after load | 241 to 262 MB | 364 to 410 MB |

- GPU texture memory is a quarter, as designed, and the load is faster.
- The shipped size does not drop: color maps shrink to a quarter, data
  maps GROW, because UASTC at 0.9 is larger than the JPEGs it replaces,
  and 44 of the 69 images are data maps. The expectation of "shipped
  images under 10 MB" counted the 25 base color maps the parser opened
  then.
- Client RSS is HIGHER with the compressed bake and does not come down
  within a minute. Explained under "Parallel transcodes keep their
  memory"; not fixed.
- Both bakes render the same picture at a glance; the judgment on
  quality is the user's.

The table above is the first bake, UASTC 0.9 for every non-color map,
which is what the defaults are. Against `data` set to ETC1S 0.75, same
asset:

| kind | images | source | `data` set to ETC1S 0.75 | defaults |
|---|---|---|---|---|
| color | 25 | 17828 KiB | 4882 KiB | 4882 KiB |
| normal | 24 | 15827 KiB | 22110 KiB | 22110 KiB |
| data | 20 | 8329 KiB | 2526 KiB | 12061 KiB |
| `.srtm` | | 49569 KiB | 37102 KiB | 46637 KiB |

Texture bytes are 90.7 MB either way. The two bakes against the
uncompressed one, rendered in the Sponza demo's own scene
(`probes/sponza-compare/`, a copy of its sources run on the repo's
packages, five fixed poses, clock frozen), PSNR of the rendered frame:

| pose | `data` as ETC1S | defaults |
|---|---|---|
| nave | 31.4 dB | 33.3 dB |
| curtains | 30.9 dB | 32.9 dB |
| floor | 34.5 dB | 36.5 dB |
| lion | 42.1 dB | 42.2 dB |
| aisle | 41.4 dB | 41.7 dB |

ETC1S roughness costs about 2 dB of the rendered frame where glossy
surfaces fill the view and nothing elsewhere, for 9.5 MB of download.

### Parallel transcodes keep their memory (2026-09-29, Linux, glibc)

`probes/ktx2-memory-probe.tsx` runs the load's steps one by one and
reads the process RSS after each, with at most N transcodes in flight
(0: all 69 at once, what `transcodeModelImages` does). Sponza, MB:

| in flight | transcode time | before | peak | settled |
|---|---|---|---|---|
| all (3 runs) | 210 ms | 194 to 271 | 359 to 438 | same as peak |
| 8 (3 runs) | 220 to 281 ms | 195 to 234 | 310 to 348 | same as peak |
| 6 (2 runs) | 254 to 322 ms | 196 to 235 | 308 to 349 | 258, 349 |
| 4 (4 runs) | 340 to 399 ms | 194 to 195 | 305 to 307 | 234 to 250 |
| 3 (2 runs) | 469 to 484 ms | 195 | 304 to 305 | 230 to 236 |
| 1 (2 runs) | 1060 ms | 234 to 235 | 342 to 344 | 265 to 267 |
| the PNG/JPEG bake | none | 238 | 240 | 240 |

- The transcoded payloads are not leaked: with 4 or fewer in flight the
  process settles where the uncompressed bake does once the load drops
  its references.
- With many in flight the memory is never given back. The runtime uses
  glibc malloc (no allocator of its own), which keeps an arena per
  allocating thread, and each transcode allocates on a blocking pool
  thread; with few in flight the pool reuses a few threads.
- The peak of about 110 MB above the start is the payloads themselves
  (90 MB, all needed at once by the synchronous `createModel`) and their
  copies; only a create that streams would lower it ("Copies on the way
  to the GPU").
- Four in flight costs about 150 ms of load here and still beats the
  uncompressed load (449 ms in all against 1041 ms).
- glibc only. Android, Windows and macOS have other allocators and are
  not measured; the tablet is where it counts.

The cause, found by setting glibc's tunables from the environment with
all 69 in flight (three runs each, MB):

| setting | peak | settled |
|---|---|---|
| stock | 332 to 372 | same as peak |
| `MALLOC_MMAP_THRESHOLD_=131072` | 297 to 298 | 204 to 206 |
| `MALLOC_TRIM_THRESHOLD_=131072` | 296 to 338 | 204 to 245 |
| `MALLOC_ARENA_MAX=2` | 298 to 308 | 256 to 260 |

glibc serves a large allocation by mmap and returns it to the system on
free, but it RAISES the size it calls large each time such a block is
freed (up to 32 MiB), after which texture-sized buffers come from the
arena heaps, which are trimmed only from the top. Setting the threshold
explicitly, at its own default, switches that adjustment off. It is not
the number of threads: arenas capped at two still keep 70 MB.

With the four runtime threads as built (four runs each): stock settles
44 to 61 MB above the pre-load level in three runs and keeps all 110 MB
in one; with the threshold pinned it settles 13 to 17 MB above in all
four. The pre-load level itself varies by 40 MB between runs (185 or
225 MB), with every bake, which is what made single readings look
erratic.

Cost of pinning, measured on the Sponza fly-through (10 s windows, two
runs each): frame work 34.4 ms stock, 33.8 ms pinned, no difference.
That scene is GPU-bound on this machine, so it does not rule out a cost
for an app that allocates large buffers every frame on the CPU.

BUILT 2026-09-29 (decided by the user): `forge::process::
return_large_allocations` pins the threshold with `mallopt`, called at
the start of lattice's `start` and of the flux and fluxrt binaries.
Built in, four runs: 13 to 17 MB above the pre-load level every time.
Its cost on CPU-side work, Starlings (large per-frame buffer writes),
six runs each: frame work 1.8 to 9.9 ms stock, 1.5 to 3.3 ms pinned,
missed presents 0 to 54 stock, 0 to 53 pinned. No difference this
machine can resolve.

A trim when the transcode queue drains was considered and dropped: the
payloads are freed later, on the JS thread, after `createModel` has
uploaded them.

### What a load leaves behind, per platform (2026-09-29)

`probes/ktx2-memory-probe.tsx` on fresh clients, the model kept alive,
process RSS in MB, two runs each (the default bake against the
PNG/JPEG one; both hold 69 textures, 90.7 and 362.7 MB):

| client | bake | start | after the create | a minute later |
|---|---|---|---|---|
| Windows (RTX 3070, D3D11) | compressed | 137 to 140 | 381 to 383 | 290 to 293 |
| Windows | uncompressed | 137 | 433 to 436 | 189 to 192 |
| tablet (Adreno 610) | compressed | 117 to 279 | 385 to 494 | 382 to 447 |
| tablet | uncompressed | 117 | 455 to 456 | 452 to 458 |

- RSS is not texture memory. On Windows the textures live in VRAM and
  leave the process; on the tablet they are shared memory and stay in.
- Windows: the uncompressed load ends 53 MB above its start, the
  compressed one 153 MB above. The difference is about the size of the
  payloads (91 MB).
- Tablet: the compressed load ends 70 MB below the uncompressed one,
  where the textures alone are 272 MB smaller. About 200 MB is
  something else.
- The cause is the shape of the load, not the transcode: every payload
  is alive at once (transcoder buffer, its copy in a JS typed array,
  the copy in the raster command) until the synchronous `createModel`
  has taken them all, where the uncompressed path decodes, uploads and
  frees one image after another and reuses the same memory. Each
  platform's allocator then keeps what that peak cost. A cache of
  transcoded bytes would not change it: the same bytes pass through
  the same buffers, only read instead of computed.
- The start level itself varied on the tablet (117 or 279), as it did
  on Linux; readings are compared as what a load adds.

The baked model on the three platforms (load, format picked, texture
bytes from `/gpu`): Linux 554 ms BC7, Windows 508 ms BC7, tablet 1310 ms
ETC2, 90.7 MB on each; uncompressed 1250, 746 and 3158 to 3384 ms,
362.7 MB. `flux:process` exit throws in the windowed client on all
three.

### Self-review, built (2026-09-30)

- **Encodes are bounded.** `forge::ktx2::encode_queued` runs
  `ENCODE_THREADS` (4, never more than the cores) encodes at a time on
  a `Workers` of their own; `encodeTexture` goes through it. This plan
  said an encode takes every core, which the measurement does not
  bear out (`measure_encodes_at_once`, ignored, in
  `forge/src/tests/ktx2.rs`; release, 16 cores, eight Sponza maps,
  two runs):

  | at once | UASTC 0.9, normal maps | ETC1S 0.75, color maps | ETC1S, most memory |
  |---|---|---|---|
  | 1 | 19.0 to 21.6 s | 8.6 to 8.9 s | 0.25 GB |
  | 2 | 12.8 to 14.1 s | 4.5 to 4.9 s | 0.39 GB |
  | 4 | 10.4 to 11.9 s | 2.7 to 3.0 s | 0.64 GB |
  | 8 | 11.1 s, 35.1 s | 2.4 to 2.5 s | 1.0 GB |

  ETC1S works an image on little more than one core, UASTC spreads one
  over several; both stop gaining at four while the memory keeps
  growing. The 35.1 s is one run on a laptop that throttles when hot.
- **The bake runs four images at a time** (`IMAGES_IN_FLIGHT` in
  `tools/model.flux.ts`), each decoded when its turn comes: an encode
  that waits in the runtime's queue holds its pixels, so starting all
  69 would hold 69 decoded images. Sponza with `--ktx2`: 64.7 s where
  it was 130 s, the same bytes per kind (4882, 22110 and 12061 KiB).
  It loads in the release client (531 ms, 69 textures, BC7).
- **A panic's message reaches the caller.** `Workers::run` errs with
  "the job panicked: <what it said>", for a literal and a formatted
  message alike.
- **One environment for a flux script.** `fluxScriptEnv` in
  `packages/cli/src/lib/flux-script.ts`, used by `srt run` and by a
  tool under flux. Both verified after the change.
- **The settings entry takes no type from the runtime and its
  defaults are frozen**: done with the reader, see "Settings per
  file".
- Suites, all passing: `cargo test -p forge --lib --features ktx2` 104
  (2 ignored, the measurements), `-p flux --test image --features
  ktx2` 7, `-p forge --lib workers` 2.
- Not rebuilt: the release client. It runs the transcode path, which
  did not change; the bake runs under the flux binary, which is
  rebuilt and staged.

### Attribution (2026-09-29)

Nothing credited third-party software before this: the platform
packages shipped the MIT `LICENSE` alone, so Basis Universal had no
mechanism to join. Built for everything in the binaries, not for Basis
alone:

- `scripts/build-third-party-notices.ts` writes
  `THIRD-PARTY-NOTICES.txt` for a Rust target: the crates the runtime
  links (from `cargo tree` over normal edges at the dist features; 391
  to 399 by target), the native libraries and assets from a table in
  the script (SDL, SDL_mixer, Impeller, ANGLE on Windows and macOS,
  QuickJS, libffi, Basis Universal with its NOTICE, Zstandard, libvpx
  off Android, Opus, libc++ on Android, the Noto fonts), and the
  JavaScript libraries the built-in apps bundle. Identical texts print
  once. About 2.1 MB, of which 1.3 MB is the Flutter engine's license
  file, which is what the Impeller prebuilt names as its license.
- The dist goals run it (`stage-notices` in `lattice/Makefile`), the
  seven platform packages list the file, the release workflow uploads
  it. Generated, so gitignored like the binaries.
- `third-party-licenses/` holds the three texts no file in reach of
  the build carries (Impeller, ANGLE, libc++), with where each is from
  and when to refresh it.
- `cargo metadata` alone over-reports: it lists optional dependencies
  no feature turns on (SDL_image and SDL_ttf, which are not linked).
- Known limits: 22 to 31 crates ship no license file in their package
  and are listed under their license expression, without their own
  copyright line; the libraries bundled inside SDL_mixer beyond
  stb_vorbis are not listed (the build links only the WAV and
  stb_vorbis decoders); one linked crate is MPL-2.0 (`attohttpc`, by
  way of iroh's port mapping), whose terms are met by naming its
  source, which the notice does.
- Not run: a full `make dist`, so the step is verified by a dry run
  of the goals and by running the script for four targets.

### Platforms (2026-09-29, commit 8da4e734)

- Windows x86_64 MSVC (RTX 3070, ANGLE over D3D11, GLES 3.0): the client
  builds and links with Basis and zstd. `limits.bc7Textures` is true,
  so the BPTC extension is listed. `ktx2-texture-probe`: the device
  picked `bc7-rgba8`, gray 120 reads back 120, sRGB (200, 100, 50) reads
  back (147, 33, 8) from ETC1S and (147, 32, 8) from UASTC, 5488 bytes
  each. `compressed-texture-probe`: ETC2 138 of 138, the mip level 70 of
  70, BC7 137 of 137, every gate throws, inventory 64 / 112 / 64 bytes.
  ETC2 there is accepted; whether the driver expands it is not
  observable from GL.
- File sizes from the Windows encoder: ETC1S 607 and 610 bytes, the same
  as on Linux; UASTC 509 bytes with zstd, against 5824 before it.
- macOS arm64 (Xcode 27): the client builds and links (against the
  system `libc++`). `cargo test --release -p forge --lib --features
  ktx2 ktx2` passes, 7 tests, so Basis and zstd compile and run there
  and the libktx file reads. The test profile with debug info turned off
  hits the Xcode 27 strip failure (E0463); use `--release`.
- Android arm64 (SM-T500, Adreno 610, GLES 3.2): `limits.bc7Textures`
  is false. `ktx2-texture-probe`: the device picked `etc2-rgba8`, gray
  120 reads back 120, sRGB (200, 100, 50) reads back (148, 33, 8) from
  ETC1S and (146, 33, 8) from UASTC, 5488 bytes each; file sizes as on
  Windows. `compressed-texture-probe`: ETC2 138 of 138, the mip level 70
  of 70, a `bc7-*` create throws naming the limit, every other gate
  throws, inventory 64 / 112 bytes.
- Android arm64 (NDK): builds and passes the 16 KB alignment check. The
  first build linked `c++_static` and did not load on the device:
  `dlopen failed: cannot locate symbol "__gxx_personality_v0"`, the
  static runtime without its `c++abi`. forge now links the NDK's shared
  runtime (cc's default), which every APK already stages and whisper
  uses; `libmain.so` needs `libc++_shared.so` and exports no Basis or
  zstd symbol.

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
