---
title: HDR scene buffer - tone map in a resolve pass
description: "Done 2026-09-11: every scene and view target is a linear buffer (half float where renderable) plus one resolve pass that exposes, tone maps, dithers and encodes; the per-fragment OUTPUT stage is gone, clearColor is an sRGB color option, probes are buffers without a resolve, and the resolve is the post-effect slot (resolve/setResolve, hdrTexture), with the stock bloom option, the AgX and Neutral curves and the app-resolved atlas recipe landed the same day. Left open: the low-end cost measurement."
created: 2026-09-06
completed: 2026-09-11
---

# HDR scene buffer - tone map in a resolve pass

Symptom: three consequences documented in packages/3d/AGENTS.md (Color)
since the linear color pipeline landed (3d-environment 3b) - a
`transparent: true` mesh blends over already-encoded pixels, the
clearColor bypasses exposure and tone mapping, and anything that would
read the scene's radiance (bloom, auto exposure, a depth-of-field over
HDR) has nothing to read: the fragment already produced display bytes.

Cause: the scene target is rgba8 and displayed raw by the runtime, so the
output stage (`OUTPUT`'s `outputColor`) runs per fragment. That is Three's
default path (no EffectComposer) and was the right first step. Godot and
Unity render into a half-float color buffer and tone map in one
full-screen pass at the end; Three does the same once a composer is in
play (its `OutputPass`). Godot's mobile renderer does it too (a 10-bit
HDR buffer, tone mapped in a subpass): every engine pays the pass.

What 3d-environment 4c put in place: `format: "rgba16f"` on 2D draw
targets, sampler-only, behind `limits.halfFloatRenderable`; the probes
already render linear radiance into it through the `LINEAR_OUTPUT`
target params, and a covering-triangle pass (the prefilter's shape) is
the resolve primitive.

## Proposal: one pipeline, no mode

Decided against an opt-in `hdr` flag beside today's per-fragment encode:
two pipelines means two clearColor semantics, two direct-fragColor
contracts, a runtime `uOutputEncode` branch in every fragment and the
probes as a permanent special case. No backwards compatibility is owed,
so the linear buffer and the resolve become the only path. Everything
below is library work in `packages/3d`; the engine has every primitive it
needs (rgba16f draw targets, auto shader targets whose sampled textures
are live dependencies, target-level shared params).

### Every target is a buffer plus a resolve

The target the meshes draw into is the SCENE BUFFER: today's
`createDrawTarget` call with `format` set by the device - rgba16f where
half float is renderable, rgba8 elsewhere (the `probeFormat()` rule,
renamed to the one buffer-format rule and used by scenes, views and
probes alike). The fallback still blends in linear space and still tone
maps the clear; it only loses radiance above 1 (an 8-bit target clamps
on write). No per-target knob, no throw: the renderer decides, as it
does for probes.

The RESOLVE is one auto shader target of the same size, rgba8: a
covering triangle (the `FACE_VERTEX` shape) whose fragment samples the
buffer once and writes exposure, tone mapping and the display encode,
premultiplied alpha carried through. It re-renders exactly when the
buffer does (a sampled target is a live dependency), so a static scene
still costs zero passes. Its id is what `scene.texture` / `view.texture`
names - the displayable output, so display, `output`, fill and readback
are untouched. The buffer is `scene.hdrTexture` / `view.hdrTexture`
(sampler-only), beside `texture` and `depthTexture`.

`setSize` and fill resize both targets; `dispose` destroys both plus the
resolve program and pipeline (per scene, like the background's).
`depthTexture` stays the buffer's depth. `samples` is the engine's
multisample resolve at the format, then the tone-map resolve - the
standard order; the depth-"texture"-with-samples rule is unchanged.

A probe is a buffer WITHOUT a resolve: `LINEAR_OUTPUT` and its ownNames
claim are deleted, the prefilter, `bakeBackground` and the .srte path
are unchanged. A tiled view (`into`) renders linear into the app's atlas
and the app resolves the atlas once with the exported resolve source
(below); one rule instead of an exception.

### The output stage leaves the fragments

`OUTPUT` is deleted from the fragment contract: `uOutputEncode`,
`uToneMapping`, `uExposure` and `outputColor` go. A fragment ends with
PREMULTIPLIED LINEAR light in fragColor - materials, backgrounds
(`vRay` skies, the skybox form), probes, all one contract. `sceneOutput`
keeps only fog (its signature stays; a tier-3 fragment is unchanged).
The stock materials' generated tails lose the encode. Exposure and tone
mapping are uniforms of the resolve target alone, so `setToneMapping` /
`setExposure` are one target write each, never a fan-out; the scene's
`setParams` fan-out reaches the resolve too (names merge, zero coverage
tolerated), so a custom resolve reads any scene-wide name like the
background does.

A `shaderMaterial` or background that wrote encoded fragColor renders
brighter until it writes linear; that is a documented contract change,
not a migration path (the same fragment already rendered wrong into
every probe).

### clearColor is a real color option

The clear is linear light in the buffer and the resolve tone maps it, so
`clearColor` becomes what every other `[r, g, b]` option in the library
is: sRGB, decoded on write (`srgbToLinear`, premultiplied by its alpha).
With `toneMapping: "none"` and exposure 1 the decode and encode
round-trip and the backdrop is pixel-identical to today; with a curve on
it is tone mapped like a background is. The "draw the backdrop as a
background" warning is deleted.

### The resolve is the post-effect slot

Godot's glow lives in the tonemap pass; Three's bloom reads the HDR
render and `OutputPass` tone maps after it. No composer (still a
non-goal); two things instead:

- `resolve?: string | { source: string; textures?: TextureBindings }` on
  `SceneOptions`/`ViewOptions`, live via `scene.setResolve` (a
  textures-only change rewrites the bindings in place, no recompile).
  The source gets the shader-target fragment contract (vUV, iResolution,
  fragColor), the resolve set declared (`uExposure`, `uToneMapping`,
  the tone-map and encode helpers, `uniform sampler2D uScene` - like the
  background declares vRay). The default source, exported from `/glsl`
  so an app resolving its own atlas or extending the default starts
  from it, is one sample through `resolveColor(rgb, alpha)`. A bloom
  resolve declares `uniform sampler2D uBloom;`, adds it to the sample
  and ends in the same helper. On the component, `resolve` is a function
  of the buffer id, called once untracked exactly like `output`
  (`resolve={hdr => ({ source, textures: { uBloom: chain(hdr) } })}`),
  because the chain's passes need the id before the scene has a resolve
  to give them.
- `output(texture)` keeps its meaning: how the displayable leaf is
  composed, after the resolve. An LDR effect (vignette, aberration)
  stays there; a radiance-reading one moves into the resolve.

### Stock effects on the resolve

With the resolve unconditional, the two effects every engine ships as
stock are small:

- `bloom?: { threshold?: number; intensity?: number; radius?: number }`
  on `SceneOptions`, live via `scene.setBloom`, the reactive prop -
  Godot's glow on the Environment, Unity's Bloom on the Volume, Three's
  UnrealBloomPass. A threshold pass over the buffer, a half-resolution
  two-tap separable blur chain (`radius` = the number of halvings), one
  extra sample in the resolve. Off by default; when off, no chain
  exists and the resolve is the plain one. Composes with a custom
  `resolve` (the chain's result is bound as `uBloom` there).
- Debanding: an ordered dither at the encode (Godot's `use_debanding`),
  always on - a gradient through 8-bit bands otherwise, and the resolve
  is the one place where the encode now lives.
- AgX and Neutral tone mapping join "aces" as `ToneMapping` values on
  the resolve, where a curve costs nothing per material. Auto exposure
  stays out until asked.

### Cost and measurement

One full-screen pass and double color bandwidth on every target,
everywhere, including the TV and the Pi (and 8 bytes per sample under
MSAA: a 1080p 4x buffer is ~66 MiB). Every engine pays it, but measure
it in stage 1, before the change lands as the only path: `/stats` GPU
ms of `examples/standard.tsx` at 1080p, old pipeline against new, on the
Intel/Mesa laptop, the Android tablet, the TV and the Pi. The numbers go
in the done note. If a device cannot afford the pass at its native
resolution, the answer is a smaller buffer (render at 0.75x, display at
full, which the resolve already does for free), not a second pipeline.

### Staging

1. The pipeline: buffer plus resolve on scene and view targets, the
   device-driven buffer format, OUTPUT removed from the fragments,
   `sceneOutput` fog-only, probes as buffers without a resolve, the
   sRGB clearColor, resize and dispose, `hdrTexture`, the measurement.
   Value on its own: linear-space transparency and a tone-mapped
   backdrop.
2. The `resolve` slot, `setResolve`, the exported default source, the
   tiled-view atlas recipe, and `examples/bloom.tsx` hand-built from the
   slot (the sky-lit 40.0 sun disc blooming).
3. Stock `bloom`, debanding, AgX and Neutral; the example moves to the
   stock option and keeps the custom-resolve form as its second half.

### Verification

`probes/3d-hdr-scene-probe.tsx`, read back through the displayable
`scene.texture` (encoded, the existing readback trap):

- Two `transparent` quads at alpha 0.5 over black: the bytes match
  linear-space blending, not encoded-space (the old numbers recorded
  before the change).
- An opaque scene, tone mapping none, exposure 1: pixel-identical to the
  old pipeline within half-float rounding (+-1).
- clearColor under aces reads tone mapped.
- A 4.0 emissive read through a pass sampling `hdrTexture` scaled by
  0.125 reads 128 (radiance survives the buffer); through
  `scene.texture` it reads 255.
- A probe's faces still hold radiance (the prefilter probe's existing
  checks pass unchanged).
- `setSize` keeps the format; dispose is clean; a custom resolve sees a
  `scene.setParams` name; the bloom example's `/gpu` inventory shows the
  chain and the resolve at the expected draw counts.

### Documentation

`packages/3d/AGENTS.md`: the Color section is rewritten (the three
consequences become history), Views (`hdrTexture`, `resolve`, the atlas
recipe), Output composition (resolve vs output), Background (fragments
write linear), the Views trap "`scene.texture` IS the draw target id"
(no longer: `setParams` is the only spelling), the Materials trap for
direct writers, a Bloom section beside Fog. Roadmap item 17's box
closes; its paragraph moves to AGENTS.md per the roadmap's rule.

## Findings

Cut into [3d-scene-buffer-resolve](../notes/3d-scene-buffer-resolve.md).

## Follow-ups landed the same day

The tiled-view recipe became `examples/scene-atlas.tsx` with two exports
it needed: `resolveFragment(source?)` from `/glsl` (the stock resolve
declarations over a source, for a shader texture of the app's own) and
`resolveParams({ toneMapping?, exposure? })`, plus `bufferFormat` made
public. The `resolve` option takes a function of the buffer id at the
core level (`ResolveInput`), so the components pass it through and a
custom resolve compiles once.

Also the same day: the stock `bloom` (SceneOptions / ViewOptions /
setBloom, views following the scene's like fog; the chain lives with
the resolve in `packages/3d/src/resolve.ts`, `uBloom` rebinds to a 1x1
placeholder when switched off because a target binding cannot be
removed) and "agx" / "neutral" as `ToneMapping` values, both checked by
`probes/3d-hdr-scene-probe.tsx` against reference math.

## Left open

The resolve cost on the tablet, the TV and the Pi (measured on the
Intel/Mesa laptop only).
