---
title: Runtime optimization - what the binary weighs and how to shrink it
description: The runtime is 61 MB at release (about 40 MB at release-opt) and size already decides what ships - speech recognition is compiled out of every build for it; this collects the measured weights and the ways down, starting with a per-crate size audit nobody has run yet.
created: 2026-09-29
---

# Runtime optimization - what the binary weighs and how to shrink it

The runtime binary is one file that carries everything: `solidrt` is 61 MB
and `solidrt-go` 71 MB at the `release` profile on linux-x64 (2026-09-29,
unstripped), about 40 MB at `release-opt` (fat LTO, one codegen unit,
stripped; 39.7 MB measured 2026-07-19). Every capability added lands in
every app's download on every platform, and size has already started
deciding what ships:

- **Speech recognition is not compiled in.** Whisper plus the wake-word
  stack (whisper.cpp, tract, the ONNX models the wake-word crate embeds)
  is about 12 MB of code. `SPEECH ?= 0` in lattice/Makefile for every
  goal, dist builds force it off even against `SPEECH=1`, and nothing
  that ships can use `@solidrt/core/speech`. The feature works, and size is
  one reason it is out rather than the only one: the stack puts two
  inference engines in one build, which model ships by default was never
  settled, and the wake-word path needs a livekit fork. So the 12 MB is not
  a size threshold anything else can be judged against.
- **Video ships a decoder only.** libvpx's VP9 encoder is 1.28 MB of
  linked binary (1732 KiB with it, 448 KiB without) and is configured out
  in forge/build.rs.
- **Compressed textures cost about 1.8 MB** as landed
  ([gpu-compressed-textures](../plans/gpu-compressed-textures.md)):
  encoder and transcoder both in every runtime, for now.

This item is the list of ways down. Nothing here is started.

## Measure first

No per-crate breakdown of the runtime exists; every figure above came
from a one-off investigation. The first step is an audit of the
`release-opt` runtime by crate and by section, kept as a note and re-run
when a dependency lands, so the rest of this list is ranked by measured
weight instead of by what was looked at last. Candidates the audit should
settle, none measured yet: the TLS and HTTP stack, the p2p stack, sqlite,
the image formats, SVG, Impeller.

The wasm engine is measured (2026-10-03, linux-x64-gnu builder,
`release-opt`, four builds of `make runtime` on one working-tree base):

| what | size |
|---|---|
| wasmi 2.0.0 as linked today, exclusive of shared `wasmparser` | 1.39 MB |
| wasmi's `simd` feature, on since 2026-10-04 | +0.61 MB |
| swapping wasmi for wasmtime 48.0.5 with Cranelift | +7.80 MB |

The swap figure is the one that matters for
[wasm-native-execution](../done/wasm-native-execution.md), which carries the full
table and the caveats. Two method notes worth keeping: an isolated probe
crate predicted the in-tree swap to within half a megabyte, because
Cranelift's bulk is ISLE-generated code that is all reachable and so
survives fat LTO; and the delta compresses better than the binary average
(zstd-19 0.247 against 0.333), so it is +1.93 MB on the wire and the full
+7.80 MB on disk.

Two results from earlier audits are worth keeping in mind as method:
a default feature of one crate (rxing pulling `image` with every format)
weighed on the whole workspace through feature unification, and a linker
flag (`/OPT:NOREF,NOICF` on Windows) cost 14 MB on its own. Both were
found by measuring, neither by reading code.

## Ways down, by subject

### Compressed textures (Basis Universal)

Measured 2026-09-29, x86_64:

| what | size |
|---|---|
| transcoder, every output format (v2.50, compiled alone, -O2) | 1323 KiB |
| transcoder, legacy formats off (what forge builds) | 978 KiB |
| transcoder, as linked through the 0.3.1 crate (Basis 1.16) | 881 KiB |
| encoder on top of that (same measurement) | 924 KiB |
| `solidrt-go` at `release` (unstripped), before and after `ktx2` | 71.26 MB to 75.42 MB, +4.15 MB |

The last row is symbols included and no LTO, so it overstates what a
published `release-opt` build gains; that figure is not measured yet.

- **Encoder out of the production runtime.** Only a bake encodes, and a
  bake runs on the developer's machine. Split forge's `ktx2` feature into
  `ktx2` (transcoder) and `ktx2-encode`, the second on for the flux
  binary that hosts the bake and off for the runtimes; `encodeTexture`
  throws where it is absent. About 0.9 MB. The same split takes zstd
  from the whole library (`zstd/zstd.c`, 669 KiB of text compiled alone)
  to its decoder (`zstd/zstddeclib.c`, 131 KiB) in the runtimes.
- **Transcoder down to the four paths in use.** We call ETC1S and UASTC,
  each to ETC2 and BC7. The transcoder also carries UASTC HDR, ASTC HDR,
  XUASTC (its own entropy coder and deblocking filter), a full ASTC LDR
  decoder and a DDS reader, and the switches for those no longer compile
  in v2.50, so they cannot be configured out. Options: a patch we carry
  across release bumps (a 46,000 line file), or asking upstream to repair
  the switches, which costs nothing to try first. Estimated 400 to 500
  KiB left, not measured.
- **The conversion tables are the floor.** ETC1S reaches each target by
  table lookup; the BC7 table alone is 174 KiB of source. That is the
  price of a fast transcode and not a target.

### Speech recognition

The engine cannot shrink to fit (the models are the feature), so the way
back in is not to make it smaller but to stop carrying it in every
runtime:

- **A capability shipped beside the runtime, loaded on demand.** The
  runtime stays speech-free; an app that declares the capability gets the
  engine as a separate library in its package, loaded at first use. This
  is the general shape for anything heavy and rare, and the first thing
  to design here, because it also answers the encoder, a future VP9
  encoder, and whatever comes next. Open: where the library lives per
  platform (Android packages native libraries its own way), how the
  capability is declared, how `Flux.capabilities` reports something
  present but not loaded.
- **Its home is undecided** (lattice/src/plugins/speech.rs is outside the
  forge/flux layering). Moving it and making it loadable are one piece of
  work.

### Build configuration

- `release-opt` is the publish profile and takes the runtime from about
  60 MB to about 40 MB; see
  [make-goals-and-dist-profile](make-goals-and-dist-profile.md). Open
  there: verifying the darwin and Android publish builds.
- Feature unification: audit every dependency's default features the way
  rxing was (`default-features = false` plus the features named), since
  one crate's defaults are paid by the workspace.
- Per-capability build knobs exist (`VIDEO=0`, `SPEECH=1`); they are for
  us, not for an app. An app cannot ask for a runtime without a
  capability it does not use, which is the same question as loading on
  demand, seen from the other side.

## Not the goal

Shrinking by removing web-standard surface, or by swapping a
battle-tested dependency for a smaller unproven one. Reliability decides
a dependency (CLAUDE.md); size ranks the work, it does not pick the
library.
