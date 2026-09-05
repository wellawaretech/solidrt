// GPU textures and shaders, reactive (SolidJS) layer: the create* helpers free
// their texture automatically when the reactive owner is disposed. Drive a
// target's uniforms declaratively with `<texture src={id} params={{...}} />`
// (see TextureProps) - the preferred way, deferred to the next real repaint so
// a fast-changing signal stays paced to actual frames; the prop means "the
// target's params" on every kind (on a draw target, its shared params).
// setTargetParams is the imperative exception: reach for it only when there
// is no `<texture>` element to hold a params prop, e.g. a target that only
// feeds another shader as a sampler2D input. The imperative primitives
// (uploadTexture, setTargetParams, destroyTexture, ...) live in the
// `flux:gpu` module.
//
// Sampling is a per-texture property declared at creation: `filter`
// ("linear" default | "nearest"), `wrap` ("clamp" default | "repeat"),
// `mipmap` (default false) and `anisotropy` (default 1 = off; pair it with
// mipmap) on every create* helper. One state for every
// consumer - `<texture>` display and shader sampling both follow it - so a
// nearest texture upscales with hard pixels everywhere (the retro/pixel-art
// path: render small, display big). `mipmap: true` keeps a mip chain the
// runtime regenerates after every upload or render, so shader sampling of a
// minified texture (3d surfaces at distance, a supersampled target) does
// not alias; the display draw samples the full-size level only.
//
// Combining several passes is a render-tree job, not a shader one: stack
// `<texture>` elements and set their `blendMode` (e.g. `blendMode="plus"` for
// an additive pass over a base pass) instead of writing a pass that samples
// both. WITHIN one pipeline draw, `blend: "add"` accumulates overlapping
// geometry additively and `blend: "multiply"` scales it (both
// order-independent, no sorting); `blend: "alpha"` composites over in
// draw-list order (order-dependent: the app or a scene layer sorts); anything
// else draws with GL blending disabled and overwrites.
//
// The pixel contract. Three facts hold for every texture and target:
//
// - Clip space is y-down. `gl_Position` y = -1 is the top of the target, +1
//   the bottom (GL's row 0 is clip y = -1, and Impeller samples row 0 as the
//   top). A vertex stage carrying camera-up geometry must negate y, or fold
//   the flip into its projection, or it draws upside down: Vulkan's
//   convention, not desktop GL's. The fragment path absorbs the same flip
//   already, so `vUV` is 0..1 with top-left origin and a fragment-only shader
//   never sees it.
// - Color is premultiplied alpha. A target's RGB is expected already
//   multiplied by its A - `vec4(rgb * a, a)`, not `vec4(rgb, a)`, which
//   composites as opaque. That is what Impeller composites and what
//   `<texture blendMode>` blends; `clearColor` is premultiplied too, so the
//   default transparent black needs no thought. Uploaded pixels follow the
//   same rule: `decodeImage` premultiplies at the codec boundary (image
//   files store straight alpha) and `encodeImage` undoes it, so pixels
//   inside the app are premultiplied everywhere.
// - Values are non-linear RGBA8, with no color-space concept. Every texture
//   and target holds 8-bit RGBA UNORM exactly as written; nothing converts to
//   or from linear light. `filter: "linear"` averages and the `blend` modes
//   accumulate non-linear values - the usual approximation, stated so
//   shaders written today stay correct. The one exception is opt-in: an
//   "rgba8-srgb" texture decodes to linear light when a shader samples it
//   (and "rgba16f" holds linear HDR values), which is how a linear-space
//   renderer such as `@solidrt/3d` reads its inputs before encoding its
//   output back into this contract.

import { createEffect, createSignal, getOwner, onCleanup, untrack } from "@solidjs/signals"
import * as gpu from "flux:gpu"

// The create* helpers accept { autoFree: false } to opt out of the
// owner-scoped auto-free, for resources whose lifetime is managed by hand
// (rebuilt on signal changes inside a long-lived component, handed across
// owners, ...). Without the opt-out, each rebuild would stack another
// onCleanup on the component owner: a leak until unmount, then a double-free
// against the by-hand destroys.
// `label` is a free-form debug name (WebGPU's label): surfaced by the dev
// tooling's GPU inventory and engine log messages, never interpreted, kept
// across id-stable resizes.
export type CreateOptions = { autoFree?: boolean; label?: string }

// Sampling options every texture-producing create* helper accepts, applied at
// creation as a property of the texture id (there is no set-sampler-later).
export type SamplerOptions = { filter?: gpu.FilterMode; wrap?: gpu.WrapMode; mipmap?: boolean; anisotropy?: number }
export type { FilterMode, WrapMode, TextureBinding, TextureBindings } from "flux:gpu"

// Pixel format option for the pixel-upload creates (createTexture,
// createCubeTexture, createMutableTexture), fixed for the id's lifetime like
// the sampler state. "rgba8" (default), "rgba8-srgb" (decodes to linear
// light on sample), "r8", the float data-texture formats "r32f"/"rgba32f"
// (Float32Array payload, nearest/texelFetch sampling only, no readback) and
// the HDR image format "rgba16f" (Float32Array payload packed to half float,
// filterable) - see TextureFormat in flux:gpu for each format's contract.
// "etc2-rgba8" (compressed) is a reserved future value of the same
// vocabulary.
export type TextureFormatOptions = { format?: gpu.TextureFormat }
export type { TextureFormat } from "flux:gpu"

// The branded id types, one per id space (see flux:gpu): plain numbers at
// runtime, distinct types to the checker, so a cross-space slip like
// destroyBuffer(textureId) fails to compile. Exported so apps can annotate
// storage (`let ids: TextureId[]`).
export type { BufferId, DrawId, ProgramId, RenderPipelineId, ShaderStageId, TextureId } from "flux:gpu"

// Re-exported so callers that depend on @solidrt/core -- like @solidrt/components
// -- need not import flux directly: destroyTexture for the manual-cleanup path
// (textures made outside a reactive scope, e.g. after an await, are not
// auto-freed), uploadTexture to push new pixels into a mutable texture, and
// the target-level verbs. setTargetParams writes a target's params on ANY
// target kind - the non-reactive exception described above, so prefer
// `<texture params={...}>` when a `<texture>` element is already in the
// tree. On a single-program target (fragment texture, pipeline target) the
// names validate strictly against its one program; on a draw target they are
// the SHARED params every entry reads (a camera's view-projection: one write
// per camera move instead of one per mesh), applied before each entry's own
// params so an entry naming the same uniform overrides the shared value, and
// a name only some entries' programs declare applies where declared.
// setTargetTextures is its sampler analog: retarget sampler2D inputs without
// recompiling (on a draw target, shared sources every entry reads - an
// environment map, a LUT - bound where an entry's program declares the name
// and its own bindings do not override it). resizeTexture and setTargetSize
// resize in place at a stable id (so `<texture src>` and sampler bindings
// stay valid); because the id survives, the owner-scoped auto-free
// registered at creation keeps working and no re-registration is needed.
// depthTexture(target) names a draw target's sampleable depth (created with
// `depth: "texture"`): a sampler-only id bound like any texture, owned by
// the target - no auto-free of its own, it dies with the target's.
export {
  depthTexture,
  destroyTexture,
  endBufferWrite,
  resizeTexture,
  setTargetParams,
  setTargetRect,
  setTargetSize,
  setTargetTextures,
  uploadTexture,
} from "flux:gpu"

// Pipeline plumbing re-exported raw: setDraw re-renders a pipeline with an
// updated draw range (vertexCount after its buffer gained or lost dynamic
// geometry, firstVertex for a different window of a shared buffer,
// instanceCount for an instanced population; absent keys keep their current
// value, like params) and/or swapped buffers (instanceBuffer pointed at a
// larger buffer once a population outgrows the old one - the growth
// primitive; replace-only, the range is rechecked); destroyBuffer is the manual
// cleanup path for buffers created outside a reactive scope. renderTarget is
// the explicit render verb for `render: "manual"` targets - targets whose
// pass is state (accumulation, feedback) rather than a pure function of its
// inputs, which the runtime therefore never renders on its own; the app
// steps them, usually from onFrame.
// copyTexture overwrites a manual target with another texture's pixels
// GPU-side (exact, same size): seed a loadOp "load" accumulator, snapshot a
// ping-pong buffer, reset state to a known image.
export { copyTexture, destroyBuffer, renderTarget, setDraw } from "flux:gpu"
export type { BlendMode, BufferUpdate, CullMode, DrawRange, IndexBinding, IndexFormat, IndexRange, InstanceAttribute, InstanceOrder, OrderUpdate, ShaderParams, Topology, VertexAttribute } from "flux:gpu"

// The draw-list verbs, re-exported raw: entries live and die with their draw
// target (see createDrawTarget below), so there is no per-entry lifetime to
// wrap. addDraw adds an entry (appended, or inserted via opts.before) and
// returns its stable DrawId; removeDraw drops one; setDrawParams /
// setDrawTextures / setDrawRange / setDrawBuffers are the per-entry forms of
// setTargetParams / setTargetTextures / setDraw (its range and buffer halves),
// taking (target, draw, value) with identical merge and validation semantics. The per-object hot path is setDrawParams (a
// moved mesh = one call with its new matrix); the per-target one is
// setTargetParams (exported above), which on a draw target writes the SHARED
// params every entry reads. setDrawOrder replaces the whole
// list order with a full permutation of the live ids - the sorting verb
// (opaque front-to-back, transparent back-to-front, re-issued when the
// camera moves). That orders ENTRIES; ordering the instance RECORDS within
// one entry is the instanceOrder creation option (see InstanceOrder), whose
// projected-key direction updates ride setDraw/setDrawRange as
// orderDirection.
export { addDraw, removeDraw, setDrawBuffers, setDrawOrder, setDrawParams, setDrawRange, setDrawTextures } from "flux:gpu"

// The device ceilings (max texture/target size, sampler inputs per pass,
// vertex attributes per pipeline), queried once at startup. Creates and binds
// validate against them and throw naming the limit; read these to size within
// the device instead (e.g. clamp a supersampled target to maxTextureSize).
export { limits } from "flux:gpu"

// The raw shading layer, re-exported as-is - no reactive wrapper, the app
// owns these lifetimes. compileShader compiles one stage from complete GLSL
// ES (or with the standard header via { header: true }); linkProgram links a
// vertex and a fragment stage into a program handle; createRenderPipeline
// pairs a program with draw state (vertex layout, topology, blend, depth -
// how it draws) into a pipeline handle that backs any number of
// createShaderTarget calls (and compiles nothing per pipeline or target);
// destroyShader / destroyProgram / destroyRenderPipeline free by id space,
// any order safe against live users. createShaderTexture/createPipelineTexture
// remain the fused conveniences on top.
export {
  compileShader,
  createRenderPipeline,
  destroyProgram,
  destroyRenderPipeline,
  destroyShader,
  linkProgram,
  programAttributes,
} from "flux:gpu"

/**
 * Tags an inline GLSL source, returning it unchanged. Shaders small enough to
 * belong beside the code that uses them stay in the file; the tag is what makes
 * them legible there, because editors highlight GLSL inside a template literal
 * only when a known tag marks it (the name matters - `glsl` is the one the
 * grammars look for).
 *
 * Interpolated values are stringified verbatim, with no GLSL-aware formatting:
 * `${2}` splices in the int literal `2`, which will not assign to a float. Pass
 * anything that varies as a uniform instead of building it into the source.
 *
 * Raw semantics, so backslashes reach the compiler as written: the GLSL
 * preprocessor continues a line with a trailing `\`, which a cooked template
 * would reject as an invalid escape and silently pass through as `undefined`.
 */
export let glsl = String.raw

// captureSnapshot renders a node to pixels and readTexture reads any
// texture's bytes back; both resolve the same { width, height, data } shape.
// A laid-out node captures its layout box; a `d-*` node captures its painted
// box - its own w/h when set, else the nearest laid-out ancestor's box, its
// x/y offset mapped to the texture origin. A capture creates no texture and
// nothing needs freeing; to display or sample the result, upload it with
// createTexture.
//
// This is the one-shot bake path: draw something only the engine can produce
// (shaped text, an SVG, a themed view), capture it and process the pixels on
// the CPU - baking a glyph atlas is the worked example. Not a rendering path:
// a capture rasterizes the subtree offscreen and reads it back to the CPU,
// costing a readback stall and a paint pass of latency every call. Batch
// captures (one paint pass services many), never run them per frame, and do
// not use them to feed live screen content into a shader - for that the
// source has to update in place (another pipeline's target, a camera texture).
export { captureSnapshot, readTexture } from "flux:gpu"

/**
 * Uploads raw pixels to an immutable GPU texture and returns its id (use it
 * as `<texture src={id} />`). `data` must be exactly `width * height` pixels
 * at the declared format's size, in the view type matching the format
 * (Uint8Array for "rgba8"/"rgba8-srgb"/"r8", Float32Array for "r32f"/
 * "rgba32f"/"rgba16f"); a mismatch throws. RGBA data is uploaded verbatim and composited
 * as premultiplied alpha, like every texture: `decodeImage` and the readback
 * calls already deliver that, and hand-built pixels must too (`rgb * a`). For pixels you intend to mutate and
 * re-upload, use `createMutableTexture` instead. When called inside a
 * reactive scope the texture is freed automatically once that owner is
 * disposed; when called outside one (e.g. after an `await`, where the owner
 * is no longer current) nothing is registered and you must call
 * `destroyTexture` (from flux:gpu) yourself. Pass `{ autoFree: false }` to
 * skip the auto-free and own the disposal yourself even inside a reactive
 * scope.
 */
export function createTexture(
  data: Uint8Array | Float32Array,
  width: number,
  height: number,
  opts?: CreateOptions & SamplerOptions & TextureFormatOptions,
): gpu.TextureId {
  let id = gpu.createTexture(data, width, height, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Uploads a cube map: six square faces of `size * size` pixels in GL order
 * (+X, -X, +Y, -Y, +Z, -Z), each in the view type matching the declared
 * format like `createTexture`, and returns its id. A cube map is sampled by
 * DIRECTION from a `uniform samplerCube` in a shader (`texture(uEnv, dir)`;
 * `textureLod` for an explicit mip level, `mipmap: true` builds the chain
 * from the faces) - the skybox, reflection and environment-lighting
 * primitive. `faces` may instead be an explicit mip chain - an array of
 * six-face arrays, level 0 first, halving down to the 1x1 level (the full
 * chain) - uploaded as given: prefiltered environment levels, with no
 * generated chain and so no renderability requirement on the format (an
 * "rgba16f" chain works on every device). Explicit levels imply `mipmap:
 * true`. Sampler-only: the id binds through `textures` like any other,
 * but `<texture src>` cannot display it and `readTexture`, `copyTexture`,
 * `uploadTexture` and `resizeTexture` throw (it is create-once). `wrap` has
 * no effect - cube filtering is seamless across faces. Binding a cube map
 * to a `sampler2D`, or a 2D texture to a `samplerCube`, throws at the bind.
 * Note GL's cube convention: each face is what a lookup in that direction
 * returns, the cube as seen from OUTSIDE, so a face set authored the Three
 * way (each face as seen from inside) reads mirrored on x unless each
 * image is mirrored at load. Freed like `createTexture` (auto-free under a
 * reactive owner, `{ autoFree: false }` or `destroyTexture` otherwise).
 */
export function createCubeTexture(
  faces: (Uint8Array | Float32Array)[] | (Uint8Array | Float32Array)[][],
  size: number,
  opts?: CreateOptions & SamplerOptions & TextureFormatOptions,
): gpu.TextureId {
  let id = gpu.createCubeTexture(faces, size, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a GPU texture you intend to update over time: seed it with `data`,
 * then call `uploadTexture(id, data)` (from flux:gpu) to push new pixels.
 * `data` must hold at least `width * height` pixels at the declared format's
 * size, in the view type matching the format (Uint8Array for "rgba8"/
 * "rgba8-srgb"/"r8", Float32Array for "r32f"/"rgba32f"/"rgba16f"); it may
 * hold several frames. Like `createTexture`, the texture is freed automatically
 * when the reactive owner is disposed (opt out with `{ autoFree: false }`);
 * created outside a reactive scope you must call `destroyTexture` (from
 * flux:gpu) yourself.
 */
export function createMutableTexture(
  data: Uint8Array | Float32Array,
  width: number,
  height: number,
  opts?: CreateOptions & SamplerOptions & TextureFormatOptions,
): gpu.TextureId {
  let id = gpu.createMutableTexture(data, width, height, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Compiles a GLSL ES 3.00 fragment shader and renders it into a texture,
 * returning the texture id (usable anywhere a normal texture id is, e.g.
 * `<texture src>`) - hence the name: what comes back is a texture, not a
 * shader object. The fragment body may reference `vUV` (0..1, top-left
 * origin), `iResolution`, and any uniform it declares (`float`/`int`
 * scalars from a number, `vec2`/`vec3`/`vec4`/`mat4` from a flat number
 * array); drive their values with `<texture src={id} params={{...}} />`
 * (preferred) or, when there is no `<texture>` element for it, imperatively
 * with `setTargetParams`. `params` is its own argument - it seeds the same
 * live channel those two drive - and takes `null` (or nothing) for a shader
 * without uniforms. A time-driven shader declares its own time uniform
 * (`uniform float uTime;`) and the app drives it like any other value.
 * `opts.textures` binds each declared `uniform sampler2D` to an existing texture id
 * (e.g. a camera or decoded image, or another shader/pipeline target) so the
 * shader can read it; bound inputs are live dependencies, so the shader
 * re-renders whenever a source changes - including a sampled target
 * re-rendering, transitively through chains. Frees the
 * texture and shader program when the reactive owner is disposed (opt out
 * with `{ autoFree: false }`); create outside any reactive scope for
 * app-lifetime shaders. For a shader whose source or inputs change
 * reactively, use {@link createShaderTextureMemo} instead.
 *
 * That preamble (`#version 300 es`, precision, `vUV`, `iResolution`,
 * `fragColor`) is injected only into sources that do not declare their own
 * `#version` line, and declares exactly what the runtime provides - nothing
 * app-driven. A source starting with `#version 300 es` compiles exactly
 * as written, so a shader carrying its own uniform names - one ported from
 * elsewhere - runs unchanged here without dropping to compileShader /
 * linkProgram. The built-in vertex stage still supplies `vUV`; declare
 * `in vec2 vUV;` yourself to read it. One naming trap: GLSL ES reserves
 * `packed` as a keyword, so `vec4 packed = texture(...)` fails with a syntax
 * error that does not name the identifier - pick another name.
 *
 * A bad source is a runtime failure, not one `srt check` catches: the compile
 * throws at this call, so a shader created in a component body takes the app
 * to the error window unless an `<Errored>` closer in the tree claims it.
 */
export function createShaderTexture(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: CreateOptions & SamplerOptions & { textures?: gpu.TextureBindings },
): gpu.TextureId {
  let id = gpu.createShaderTexture(fragmentSrc, width, height, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a render target over a pipeline from `createRenderPipeline` and
 * renders it once, returning the texture id (usable anywhere a normal
 * texture id is, e.g. `<texture src>`; resize with `setTargetSize`, drive
 * uniforms with `<texture params>` or `setTargetParams`). Many targets may
 * share one pipeline, and creating a target compiles nothing. The target
 * brings the per-target half: size, the concrete vertex `buffer` the
 * pipeline's attribute layout describes, the `instanceBuffer` its
 * `instanceAttributes` describe (required exactly when it declares any),
 * the draw range (`vertexCount` defaults to the rest of the buffer from
 * `firstVertex` on, `instanceCount` repeats it as instances told apart by
 * `gl_InstanceID` and defaults to one per instance-buffer record; a
 * fullscreen pass over an attributeless pipeline is `{ vertexCount: 3 }`
 * with a covering-triangle vertex stage), uniforms, and
 * `clearColor`. An `indexBuffer` + `indexFormat` pair makes the draw indexed
 * (shared vertices stored once), with the range in `firstIndex`/`indexCount`
 * spelling - see IndexBinding/IndexRange. Draw state (`attributes`,
 * `instanceAttributes`, `topology`, `blend`, `cull`, `depth`, `depthWrite`)
 * lives on the pipeline
 * and throws here. Frees the target when the reactive owner is disposed (opt
 * out with `autoFree: false`); the pipeline is yours and outlives it.
 *
 * `render: "manual"` makes it a manual target: the runtime never renders it
 * (it starts cleared to `clearColor`), only an explicit `renderTarget(id)`
 * does, in call order - which is what legalizes feedback state stepped by
 * the app. `loadOp: "load"` (manual-only, throws otherwise) keeps the
 * previous contents under each draw - single-target accumulation - while
 * the default `"clear"` clears to `clearColor` per render; state that must
 * read its own pixels (decay, blur, simulation) still ping-pongs across two
 * manual targets, and `copyTexture` seeds either shape.
 *
 * `samples` (2, 4 or 8) multisamples the target's storage so filled
 * geometry gets anti-aliased edges; the texture id still names a
 * single-sample image, so nothing downstream changes. Clamped to the device
 * maximum, falls back to single-sample (with a warning) where the driver
 * refuses, and throws with `loadOp: "load"`.
 */
export function createShaderTarget(
  pipeline: gpu.RenderPipelineId,
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: {
    textures?: gpu.TextureBindings
    buffer?: gpu.BufferId
    instanceBuffer?: gpu.BufferId
    /** One buffer per instance slot of the pipeline (index = the
     * attributes' `slot`); pass this OR `instanceBuffer`, not both. */
    instanceBuffers?: gpu.BufferId[]
    /** Draw the instance records in key order (see InstanceOrder). */
    instanceOrder?: gpu.InstanceOrder
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
    samples?: 1 | 2 | 4 | 8
  } & (gpu.DrawRange | (gpu.IndexBinding & gpu.IndexRange)) &
    CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createShaderTarget(pipeline, width, height, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a draw target: a render target holding an ordered, MUTABLE list of
 * draws, rendered as one pass - clear once (color, and depth when declared),
 * then every entry in list order into the same storage. This is the
 * multi-pass primitive (N meshes x N pipelines sharing one depth buffer -
 * what every 3D API calls a render pass), retained: build the list with
 * `addDraw`, prune it with `removeDraw`, and drive per-entry state with
 * `setDrawParams` / `setDrawTextures` / `setDrawRange`. `depth: true` gives
 * the target the depth storage all entries share (cross-entry occlusion);
 * whether an entry tests/writes it stays pipeline state, and a depth-testing
 * pipeline into a depthless target throws at `addDraw`. `depth: "texture"`
 * makes that storage a sampleable depth texture with its own id,
 * `depthTexture(target)` - the shadow-map / depth-effect input; not with
 * `samples`.
 *
 * `params` seeds the target's SHARED params - values every entry reads,
 * written once per target instead of once per entry (a camera's
 * view-projection is the motivating case: one `setTargetParams` per camera
 * move instead of one `setDrawParams` per mesh). Shared values apply before
 * each entry's own params, so an entry naming the same uniform overrides
 * the shared value; a name only some entries' programs declare is applied
 * where declared and skipped elsewhere. They are target state: entry
 * add/remove/rebuild cannot lose them. `opts.textures` is the sampler
 * analog - shared sources every entry reads (an environment map, a LUT),
 * driven later with `setTargetTextures`, same precedence and coverage
 * rules.
 *
 * `into` makes a sub-target: a draw target rendering into the rectangle at
 * `x`/`y` (top-left origin) of draw target `into`'s storage, so N views or
 * N shadow maps share one texture and ONE pass; the id is a draw target to
 * every verb but not a texture (display and sample the parent, with
 * `srcX`/`srcY` on the leaf), and `setTargetRect` moves it. Auto-free works
 * the same way; destroying the parent takes its tiles.
 *
 * The render contract is unchanged: the list is input data, so an ordinary
 * (`render: "auto"`) draw target re-renders exactly when its entries or
 * their inputs change - a static scene costs zero passes, and one render is
 * one pass regardless of entry count. `render: "manual"` and `loadOp` work
 * as on `createShaderTarget`. Returns the texture id; frees on owner
 * disposal (opt out with `autoFree: false`), taking its entries with it - the
 * entries' pipelines and buffers are yours and outlive it.
 */
export function createDrawTarget(
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: {
    depth?: boolean | "texture"
    textures?: gpu.TextureBindings
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
    samples?: 1 | 2 | 4 | 8
    into?: gpu.TextureId
    x?: number
    y?: number
  } & CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createDrawTarget(width, height, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a cube draw target: a draw target whose output is a `size` x
 * `size` cube map, rendered one face at a time with `renderTarget(id,
 * face)` (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z) - the reflection-probe
 * primitive (`@solidrt/3d`'s `scene.createReflectionProbe` drives it).
 * Manual by contract, one depth renderbuffer for the six faces (`depth:
 * true`), no samples or tiles; `format` rgba8 or rgba8-srgb (encodes on
 * write, decodes on sample); `mipmap: true` allocates the chain and
 * `renderTarget(id, face, level)` renders one level of it (a face render
 * without a level regenerates the chain instead) - the prefiltered
 * environment path. The id binds to a `samplerCube`
 * and cannot be displayed, read back, copied or resized. Each face pass
 * inverts the front-face rule (a GL cube face is the x mirror of a 2D
 * target's image), so render the faces through an x-mirrored projection;
 * see flux:gpu's createCubeDrawTarget. Freed like every target.
 */
export function createCubeDrawTarget(
  size: number,
  params?: gpu.ShaderParams | null,
  opts?: {
    depth?: boolean
    format?: "rgba8" | "rgba8-srgb"
    textures?: gpu.TextureBindings
    clearColor?: [number, number, number, number]
    render?: "manual"
    loadOp?: "clear" | "load"
  } & CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createCubeDrawTarget(size, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/** The reactive shader description `createShaderTextureMemo` builds from.
 * Sampling (`filter`/`wrap`/`mipmap`) is creation-time state, so changing it rebuilds
 * at a fresh id, like a fragment-source or sampler-binding change. */
export type ShaderSpec = {
  fragmentSrc: string
  width: number
  height: number
  params?: gpu.ShaderParams
  textures?: gpu.TextureBindings
} & SamplerOptions

// Shallow name->value equality for params/textures records; treats undefined
// as the empty record. A param value may be a number or a flat number list
// (typed uniforms: a number[] or a Float32Array/Float64Array), so lists
// compare elementwise; a texture binding may be an `{ id, filter?, wrap? }`
// override, compared field by field.
type RecordValue = number | number[] | Float32Array | Float64Array | gpu.TextureBinding
function isNumberList(v: RecordValue | undefined): v is number[] | Float32Array | Float64Array {
  return Array.isArray(v) || v instanceof Float32Array || v instanceof Float64Array
}
function sameValue(a: RecordValue | undefined, b: RecordValue | undefined): boolean {
  if (a === b) return true
  if (isNumberList(a) && isNumberList(b)) {
    if (a.length !== b.length) return false
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false
    return true
  }
  if (typeof a === "object" && typeof b === "object" && !isNumberList(a) && !isNumberList(b)) {
    return a.id === b.id && a.filter === b.filter && a.wrap === b.wrap
  }
  return false
}

function sameRecord(a: Record<string, RecordValue> | undefined, b: Record<string, RecordValue> | undefined): boolean {
  if (a === b) return true
  let ka = a ? Object.keys(a) : []
  let kb = b ? Object.keys(b) : []
  return ka.length === kb.length && ka.every(k => sameValue(a![k], b![k]))
}

/**
 * A fragment shader whose spec is reactive: returns an accessor for the
 * current texture id (use it as `<texture src={id()} />`) and keeps the GPU
 * resource in step with `spec` from then on. Changes that keep the compiled
 * program valid mutate in place at a stable id - a size change routes to
 * `setTargetSize`, a params change to `setTargetParams` - while a change to
 * the fragment source or the sampler bindings rebuilds at a fresh id, updates
 * the accessor, and destroys the old id. That destroy is frame-safe (the
 * runtime reclaims an id only once the render tree no longer references it),
 * so the swap never paints a blank frame. The current id is freed when the
 * owning scope is disposed. Data textures need no analog: `uploadTexture` and
 * `resizeTexture` already cover their reactive changes id-stably.
 *
 * `onError` makes a failed rebuild survivable. Without it a shader that does
 * not compile throws from inside the effect, where no caller can catch it;
 * with it the error is handed to you and the last shader that DID compile
 * stays current - id, size, params and accessor all unchanged - so the app
 * keeps drawing the previous frame's shader instead of tearing down. That is
 * the normal case whenever the source is not known-good: a shader editor, live
 * coding, or a dialect ported from elsewhere. The initial compile is not
 * covered: it throws at the call site, where an ordinary try/catch works and
 * there is no previous shader to fall back to.
 */
export function createShaderTextureMemo(
  spec: () => ShaderSpec,
  opts?: { onError?: (error: unknown) => void },
): () => gpu.TextureId {
  let make = (s: ShaderSpec) =>
    gpu.createShaderTexture(s.fragmentSrc, s.width, s.height, s.params, {
      textures: s.textures,
      filter: s.filter,
      wrap: s.wrap,
      mipmap: s.mipmap,
    })
  let current = untrack(spec)
  let currentId = make(current)
  let [id, setId] = createSignal(currentId)
  createEffect(spec, next => {
    try {
      if (
        next.fragmentSrc === current.fragmentSrc &&
        sameRecord(next.textures, current.textures) &&
        next.filter === current.filter &&
        next.wrap === current.wrap &&
        next.mipmap === current.mipmap
      ) {
        // Program and inputs unchanged: mutate in place, the id stays stable.
        if (next.width !== current.width || next.height !== current.height) {
          gpu.setTargetSize(currentId, next.width, next.height)
        }
        if (!sameRecord(next.params, current.params) && next.params) {
          gpu.setTargetParams(currentId, next.params)
        }
        current = next
        return
      }
      // Compile before touching any state: a throw here must leave `current`,
      // `currentId` and the accessor all still pointing at the last shader
      // that worked, which is what makes onError's keep-last-good real.
      let rebuilt = make(next)
      let old = currentId
      current = next
      currentId = rebuilt
      setId(rebuilt)
      gpu.destroyTexture(old)
    } catch (error) {
      if (!opts?.onError) throw error
      opts.onError(error)
    }
  })
  if (getOwner()) onCleanup(() => gpu.destroyTexture(currentId))
  return id
}

// View any TypedArray or ArrayBuffer as a Uint8Array over the same memory,
// without copying, so vertex data can be authored as Float32Array.
function toUint8(data: ArrayBuffer | ArrayBufferView): Uint8Array {
  if (data instanceof Uint8Array) return data
  if (data instanceof ArrayBuffer) return new Uint8Array(data)
  return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
}

/**
 * Compiles a GLSL ES 3.00 vertex+fragment pipeline and renders it into a
 * texture, returning the texture id (usable anywhere a normal texture id is,
 * e.g. `<texture src>`) - named, like `createShaderTexture`, for what comes
 * back. Unlike `createShaderTexture` the vertex stage is yours:
 * declare `in` attributes matching `opts.attributes` (one interleaved vertex
 * in `opts.buffer`, a {@link createBuffer} id) and your own varyings toward
 * the fragment stage. Clip space is y-down: `gl_Position` y = -1 is the top
 * row of the target and +1 the bottom, so camera-up geometry must negate y
 * (or fold the flip into its projection) to display up. Both sources may
 * reference `iResolution` and any uniform they declare (`float`/`int`
 * scalars from a number, `vec2`/`vec3`/`vec4`/`mat4` from a flat number
 * array); drive values with `<texture src={id} params={{...}} />` or
 * `setTargetParams`, exactly like a fragment shader.
 * `opts.depth` attaches a private depth buffer (cleared + tested per render);
 * `opts.depthWrite: false` (requires depth) keeps the test but stops the
 * draw from writing depth. `opts.blend: "add"` makes the draw accumulate
 * overlapping geometry additively and `"multiply"` makes it scale (darken)
 * what is already there, both order-independent (no sorting) instead of
 * overwriting; `"alpha"` composites over in draw-list order (premultiplied
 * output, back-to-front is the caller's job). A depth-tested blended pass is
 * `{ depth: true, blend: "add", depthWrite: false }` - each option only does
 * what it says, neither implies the other. The draw range (`firstVertex`, `vertexCount`, `instanceCount` -
 * see DrawRange) defaults to the whole buffer drawn once and can be changed
 * later with `setDraw`; `instanceCount` is the standard answer to particles
 * and repeated meshes, N copies of the range told apart by `gl_InstanceID`
 * in the vertex stage. `opts.instanceAttributes` + `opts.instanceBuffer`
 * (declare both or neither) give each instance its own interleaved record -
 * real per-instance state instead of `gl_InstanceID` arithmetic - and
 * `instanceCount` then defaults to one instance per record. An
 * `indexBuffer` + `indexFormat` pair makes the draw
 * indexed (shared vertices stored once), with the range in
 * `firstIndex`/`indexCount` spelling; `opts.cull` discards one face set by
 * winding (counter-clockwise as displayed = front). `opts.render: "manual"` and
 * `opts.loadOp` behave exactly as on {@link createShaderTarget}: step the
 * target with `renderTarget(id)`, and `loadOp: "load"` (manual-only) keeps
 * the previous contents under each draw. Frees the texture and GL program when the reactive
 * owner is disposed (opt out with `autoFree: false`); create outside any reactive
 * scope for app-lifetime pipelines.
 */
export function createPipelineTexture(
  vertexSrc: string,
  fragmentSrc: string,
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: {
    textures?: gpu.TextureBindings
    attributes?: gpu.VertexAttribute[]
    buffer?: gpu.BufferId
    instanceAttributes?: gpu.InstanceAttribute[]
    instanceBuffer?: gpu.BufferId
    /** One buffer per instance slot (index = the attributes' `slot`);
     * pass this OR `instanceBuffer`, not both. */
    instanceBuffers?: gpu.BufferId[]
    /** Draw the instance records in key order (see InstanceOrder). */
    instanceOrder?: gpu.InstanceOrder
    topology?: gpu.Topology
    depth?: boolean
    depthWrite?: boolean
    blend?: gpu.BlendMode
    cull?: gpu.CullMode
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
    samples?: 1 | 2 | 4 | 8
  } & (gpu.DrawRange | (gpu.IndexBinding & gpu.IndexRange)) &
    CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createPipelineTexture(vertexSrc, fragmentSrc, width, height, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a vertex buffer for pipeline attributes from raw data (typically a
 * Float32Array laid out to match the pipeline's interleaved attribute list).
 * Update it later with {@link writeBuffer}; the buffer's byte size is fixed at
 * creation, so reserve room up front for dynamic geometry. Freed automatically
 * when the reactive owner is disposed (opt out with `{ autoFree: false }`);
 * created outside a reactive scope you must call `destroyBuffer` yourself.
 * (Destruction order relative to pipelines does not matter.)
 */
export function createBuffer(data: ArrayBuffer | ArrayBufferView | number, opts?: CreateOptions): gpu.BufferId {
  let id = gpu.createBuffer(typeof data === "number" ? data : toUint8(data), opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyBuffer(id))
  return id
}

/**
 * Opens a zero-copy write into a vertex buffer: returns a Float32Array over
 * runtime-owned memory spanning the whole buffer. Write records in place,
 * then publish with {@link endBufferWrite} - the bytes move to the GPU with
 * no copy on the CPU path, which is the per-frame streaming path (instanced
 * sprites, dynamic geometry). Reach other element types through `.buffer`.
 *
 * Contents are UNSPECIFIED at begin (a recycled block holds what was
 * published the time before last): fill everything you publish. One open
 * write per buffer at a time. The view is detached at end/destroy - retained
 * references become zero-length, so a stale write is inert, never a race.
 */
export function beginBufferWrite(id: gpu.BufferId): Float32Array {
  let ab = gpu.beginBufferWrite(id)
  return new Float32Array(ab, 0, (ab.byteLength / 4) | 0)
}

/**
 * Overwrites part of a vertex buffer at `byteOffset` (default 0). Every
 * pipeline drawing from the buffer re-renders with its last-applied params,
 * so geometry-only changes reach the screen without a params update.
 */
export function writeBuffer(id: gpu.BufferId, data: ArrayBuffer | ArrayBufferView, byteOffset?: number): void {
  gpu.writeBuffer(id, toUint8(data), byteOffset)
}