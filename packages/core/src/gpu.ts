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
// ("linear" default | "nearest") and `wrap` ("clamp" default | "repeat") on
// every create* helper. One state for every consumer - `<texture>` display
// and shader sampling both follow it - so a nearest texture upscales with
// hard pixels everywhere (the retro/pixel-art path: render small, display
// big). No mipmaps exist.
//
// Combining several passes is a render-tree job, not a shader one: stack
// `<texture>` elements and set their `blendMode` (e.g. `blendMode="plus"` for
// an additive pass over a base pass) instead of writing a pass that samples
// both. WITHIN one pipeline draw, `blend: "add"` accumulates overlapping
// geometry additively (order-independent, no sorting); anything else draws
// with GL blending disabled and overwrites.
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
//   default transparent black needs no thought.
// - Values are non-linear RGBA8, with no color-space concept. Every texture
//   and target holds 8-bit RGBA UNORM exactly as written; nothing converts to
//   or from linear light. `filter: "linear"` averages and `blend: "add"`
//   accumulates non-linear values - the usual approximation, stated so
//   shaders written today stay correct if a format vocabulary arrives.

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
export type SamplerOptions = { filter?: gpu.FilterMode; wrap?: gpu.WrapMode }
export type { FilterMode, WrapMode } from "flux:gpu"

// Pixel format option for the pixel-upload creates (createTexture,
// createMutableTexture), fixed for the id's lifetime like the sampler state.
// "rgba8" (default) or "r8" - see TextureFormat in flux:gpu for the r8
// contract (1 byte/pixel, sampled as `(v, 0, 0, 1)`, any width).
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
export {
  destroyTexture,
  resizeTexture,
  setTargetParams,
  setTargetSize,
  setTargetTextures,
  uploadTexture,
} from "flux:gpu"

// Pipeline plumbing re-exported raw: setDraw re-renders a pipeline with an
// updated draw range (vertexCount after its buffer gained or lost dynamic
// geometry, firstVertex for a different window of a shared buffer,
// instanceCount for an instanced population; absent keys keep their current
// value, like params); destroyBuffer is the manual
// cleanup path for buffers created outside a reactive scope. renderTarget is
// the explicit render verb for `render: "manual"` targets - targets whose
// pass is state (accumulation, feedback) rather than a pure function of its
// inputs, which the runtime therefore never renders on its own; the app
// steps them, usually from onFrame.
// copyTexture overwrites a manual target with another texture's pixels
// GPU-side (exact, same size): seed a loadOp "load" accumulator, snapshot a
// ping-pong buffer, reset state to a known image.
export { copyTexture, destroyBuffer, renderTarget, setDraw } from "flux:gpu"
export type { BlendMode, CullMode, DrawRange, IndexBinding, IndexFormat, IndexRange, ShaderParams, Topology, VertexAttribute } from "flux:gpu"

// The draw-list verbs, re-exported raw: entries live and die with their draw
// target (see createDrawTarget below), so there is no per-entry lifetime to
// wrap. addDraw adds an entry (appended, or inserted via opts.before) and
// returns its stable DrawId; removeDraw drops one; setDrawParams /
// setDrawTextures / setDrawRange are the per-entry forms of setTargetParams /
// setTargetTextures / setDraw, taking (target, draw, value) with identical
// merge and validation semantics. The per-object hot path is setDrawParams (a
// moved mesh = one call with its new matrix); the per-target one is
// setTargetParams (exported above), which on a draw target writes the SHARED
// params every entry reads. setDrawOrder replaces the whole
// list order with a full permutation of the live ids - the sorting verb
// (opaque front-to-back, transparent back-to-front, re-issued when the
// camera moves).
export { addDraw, removeDraw, setDrawOrder, setDrawParams, setDrawRange, setDrawTextures } from "flux:gpu"

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

// captureSnapshot renders a node to a texture and readTexture reads any
// texture's bytes back. A laid-out node captures its layout box; a `d-*` node
// captures its painted box - its own w/h when set, else the nearest laid-out
// ancestor's box, its x/y offset mapped to the texture origin. Re-exported raw
// (no reactive auto-cleanup wrapper):
// captureSnapshot resolves asynchronously, by which point the reactive owner is
// no longer current, so the caller owns the returned id and frees it with
// destroyTexture (as with any texture created after an await).
//
// Together they are the one-shot bake path: draw something only the engine can
// produce (shaped text, an SVG, a themed view), capture it, read the pixels and
// process them on the CPU - baking a glyph atlas is the worked example. Not a
// rendering path: a capture rasterizes the subtree offscreen, reads it back to
// the CPU and re-uploads it, costing a full GPU -> CPU -> GPU round trip and a
// paint pass of latency every call. Batch captures (one paint pass services
// many), never run them per frame, and do not use them to feed live screen
// content into a shader - for that the source has to update in place (another
// pipeline's target, a camera texture).
export { captureSnapshot, readTexture } from "flux:gpu"

/**
 * Uploads raw pixels to an immutable GPU texture and returns its id (use it
 * as `<texture src={id} />`). `data` must be exactly `width * height` pixels
 * at the declared format's size (`* 4` bytes for the default "rgba8", `* 1`
 * for "r8"); a mismatch throws. For pixels you intend to mutate and
 * re-upload, use `createMutableTexture` instead. When called inside a
 * reactive scope the texture is freed automatically once that owner is
 * disposed; when called outside one (e.g. after an `await`, where the owner
 * is no longer current) nothing is registered and you must call
 * `destroyTexture` (from flux:gpu) yourself. Pass `{ autoFree: false }` to
 * skip the auto-free and own the disposal yourself even inside a reactive
 * scope.
 */
export function createTexture(
  data: Uint8Array,
  width: number,
  height: number,
  opts?: CreateOptions & SamplerOptions & TextureFormatOptions,
): gpu.TextureId {
  let id = gpu.createTexture(data, width, height, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a GPU texture you intend to update over time: seed it with `data`,
 * then call `uploadTexture(id, data)` (from flux:gpu) to push new pixels.
 * `data` must hold at least `width * height` pixels at the declared format's
 * size (`* 4` bytes for the default "rgba8", `* 1` for "r8"; it may hold
 * several frames). Like `createTexture`, the texture is freed automatically
 * when the reactive owner is disposed (opt out with `{ autoFree: false }`);
 * created outside a reactive scope you must call `destroyTexture` (from
 * flux:gpu) yourself.
 */
export function createMutableTexture(
  data: Uint8Array,
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
 */
export function createShaderTexture(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: CreateOptions & SamplerOptions & { textures?: Record<string, gpu.TextureId> },
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
 */
export function createShaderTarget(
  pipeline: gpu.RenderPipelineId,
  width: number,
  height: number,
  params?: gpu.ShaderParams | null,
  opts?: {
    textures?: Record<string, gpu.TextureId>
    buffer?: gpu.BufferId
    instanceBuffer?: gpu.BufferId
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
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
 * pipeline into a depthless target throws at `addDraw`.
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
    depth?: boolean
    textures?: Record<string, gpu.TextureId>
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
  } & CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createDrawTarget(width, height, params, opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/** The reactive shader description `createShaderTextureMemo` builds from.
 * Sampling (`filter`/`wrap`) is creation-time state, so changing it rebuilds
 * at a fresh id, like a fragment-source or sampler-binding change. */
export type ShaderSpec = {
  fragmentSrc: string
  width: number
  height: number
  params?: gpu.ShaderParams
  textures?: Record<string, gpu.TextureId>
} & SamplerOptions

// Shallow name->value equality for params/textures records; treats undefined
// as the empty record. A param value may be a number or a flat number array
// (typed uniforms), so arrays compare elementwise.
function sameValue(a: number | number[] | undefined, b: number | number[] | undefined): boolean {
  if (a === b) return true
  if (!Array.isArray(a) || !Array.isArray(b)) return false
  return a.length === b.length && a.every((v, i) => v === b[i])
}

function sameRecord(
  a: Record<string, number | number[]> | undefined,
  b: Record<string, number | number[]> | undefined,
): boolean {
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
    gpu.createShaderTexture(s.fragmentSrc, s.width, s.height, s.params, { textures: s.textures, filter: s.filter, wrap: s.wrap })
  let current = untrack(spec)
  let currentId = make(current)
  let [id, setId] = createSignal(currentId)
  createEffect(spec, next => {
    try {
      if (
        next.fragmentSrc === current.fragmentSrc &&
        sameRecord(next.textures, current.textures) &&
        next.filter === current.filter &&
        next.wrap === current.wrap
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
 * overlapping geometry additively (order-independent, no sorting) instead of
 * overwriting; a depth-tested additive pass is `{ depth: true, blend: "add",
 * depthWrite: false }` - each option only does what it says, neither implies
 * the other. The draw range (`firstVertex`, `vertexCount`, `instanceCount` -
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
    textures?: Record<string, gpu.TextureId>
    attributes?: gpu.VertexAttribute[]
    buffer?: gpu.BufferId
    instanceAttributes?: gpu.VertexAttribute[]
    instanceBuffer?: gpu.BufferId
    topology?: gpu.Topology
    depth?: boolean
    depthWrite?: boolean
    blend?: gpu.BlendMode
    cull?: gpu.CullMode
    clearColor?: [number, number, number, number]
    render?: "auto" | "manual"
    loadOp?: "clear" | "load"
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
export function createBuffer(data: ArrayBuffer | ArrayBufferView, opts?: CreateOptions): gpu.BufferId {
  let id = gpu.createBuffer(toUint8(data), opts)
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => gpu.destroyBuffer(id))
  return id
}

/**
 * Overwrites part of a vertex buffer at `byteOffset` (default 0). Every
 * pipeline drawing from the buffer re-renders with its last-applied params,
 * so geometry-only changes reach the screen without a params update.
 */
export function writeBuffer(id: gpu.BufferId, data: ArrayBuffer | ArrayBufferView, byteOffset?: number): void {
  gpu.writeBuffer(id, toUint8(data), byteOffset)
}