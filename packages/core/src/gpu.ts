// GPU textures and shaders, reactive (SolidJS) layer: the create* helpers free
// their texture automatically when the reactive owner is disposed. Drive a
// shader's uniforms declaratively with `<texture src={id} params={{...}} />`
// (see TextureProps) - the preferred way, deferred to the next real repaint so
// a fast-changing signal stays paced to actual frames. setShaderParams is the
// imperative exception: reach for it only when there is no `<texture>` element
// to hold a params prop, e.g. a shader that only feeds another shader as a
// sampler2D input. The imperative primitives (uploadTexture, setShaderParams,
// destroyTexture, ...) live in the `flux:gpu` module.
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

import { createEffect, createSignal, getOwner, onCleanup, untrack } from "@solidjs/signals"
import * as gpu from "flux:gpu"

// The create* helpers accept { manual: true } to opt out of the owner-scoped
// auto-free, for resources whose lifetime is managed by hand (rebuilt on
// signal changes inside a long-lived component, handed across owners, ...).
// Without it, each rebuild would stack another onCleanup on the component
// owner: a leak until unmount, then a double-free against manual destroys.
export type CreateOptions = { manual?: boolean }

// Sampling options every texture-producing create* helper accepts, applied at
// creation as a property of the texture id (there is no set-sampler-later).
export type SamplerOptions = { filter?: gpu.FilterMode; wrap?: gpu.WrapMode }
export type { FilterMode, WrapMode } from "flux:gpu"

// The branded id types, one per id space (see flux:gpu): plain numbers at
// runtime, distinct types to the checker, so a cross-space slip like
// destroyBuffer(textureId) fails to compile. Exported so apps can annotate
// storage (`let ids: TextureId[]`).
export type { BufferId, ProgramId, RenderPipelineId, ShaderStageId, TextureId } from "flux:gpu"

// Re-exported so callers that depend on @solidrt/core -- like @solidrt/components
// -- need not import flux directly: destroyTexture for the manual-cleanup path
// (textures made outside a reactive scope, e.g. after an await, are not
// auto-freed), uploadTexture to push new pixels into a mutable texture, and
// setShaderParams as the non-reactive exception described above - prefer
// `<texture params={...}>` when a `<texture>` element is already in the tree.
// resizeTexture and setShaderSize resize in place at a stable id (so
// `<texture src>` and sampler bindings stay valid); because the id survives,
// the owner-scoped auto-free registered at creation keeps working and no
// re-registration is needed. setShaderTextures is the sampler analog of
// setShaderParams: retarget a shader's sampler2D inputs without recompiling.
export {
  destroyTexture,
  resizeTexture,
  setShaderParams,
  setShaderSize,
  setShaderTextures,
  uploadTexture,
} from "flux:gpu"

// Pipeline plumbing re-exported raw: setDrawCount re-renders a pipeline after
// its buffer gained or lost dynamic geometry; destroyBuffer is the manual
// cleanup path for buffers created outside a reactive scope.
export { destroyBuffer, setDrawCount } from "flux:gpu"
export type { BlendMode, ShaderParams, Topology, VertexAttribute } from "flux:gpu"

// The raw shading layer, re-exported as-is - no reactive wrapper, the app
// owns these lifetimes. compileShader compiles one stage from complete GLSL
// ES (or with the standard header via { header: true }); linkProgram links a
// vertex and a fragment stage into a program handle; createRenderPipeline
// pairs a program with draw state (vertex layout, topology, blend, depth -
// how it draws) into a pipeline handle that backs any number of
// createShaderTarget calls (and compiles nothing per pipeline or target);
// destroyShader / destroyProgram / destroyRenderPipeline free by id space,
// any order safe against live users. createShader/createPipeline remain the
// fused conveniences on top.
export {
  compileShader,
  createRenderPipeline,
  destroyProgram,
  destroyRenderPipeline,
  destroyShader,
  linkProgram,
} from "flux:gpu"

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
 * Uploads raw RGBA8 pixels to an immutable GPU texture and returns its id (use
 * it as `<texture src={id} />`). `data` must be exactly `width * height * 4`
 * bytes; a mismatch throws. For pixels you intend to mutate and re-upload, use
 * `createMutableTexture` instead. When called inside a reactive scope the
 * texture is freed automatically once that owner is disposed; when called
 * outside one (e.g. after an `await`, where the owner is no longer current)
 * nothing is registered and you must call `destroyTexture` (from flux:gpu)
 * yourself. Pass `{ manual: true }` to skip the auto-free and own the
 * disposal yourself even inside a reactive scope.
 */
export function createTexture(
  data: Uint8Array,
  width: number,
  height: number,
  opts?: CreateOptions & SamplerOptions,
): gpu.TextureId {
  let id = gpu.createTexture(data, width, height, opts)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a GPU texture you intend to update over time: seed it with `data`,
 * then call `uploadTexture(id, data)` (from flux:gpu) to push new pixels. `data`
 * is RGBA8 and must hold at least `width * height * 4` bytes (it may hold several
 * frames). Like `createTexture`, the texture is freed automatically when the
 * reactive owner is disposed (opt out with `{ manual: true }`); created
 * outside a reactive scope you must call `destroyTexture` (from flux:gpu)
 * yourself.
 */
export function createMutableTexture(
  data: Uint8Array,
  width: number,
  height: number,
  opts?: CreateOptions & SamplerOptions,
): gpu.TextureId {
  let id = gpu.createMutableTexture(data, width, height, opts)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Compiles a GLSL ES 3.00 fragment shader and renders it into a texture,
 * returning the texture id (usable anywhere a normal texture id is, e.g.
 * `<texture src>`). The fragment body may reference `vUV` (0..1, top-left
 * origin), `iResolution`, `iTime`, and any uniform it declares (`float`/`int`
 * scalars from a number, `vec2`/`vec3`/`vec4`/`mat4` from a flat number
 * array); drive their values with `<texture src={id} params={{...}} />`
 * (preferred) or, when there is no `<texture>` element for it, imperatively
 * with `setShaderParams`.
 * `textures` binds each declared `uniform sampler2D` to an existing texture id
 * (e.g. a camera or decoded image, or another shader/pipeline target) so the
 * shader can read it; bound inputs are live dependencies, so the shader
 * re-renders whenever a source changes - including a sampled target
 * re-rendering, transitively through chains. Frees the
 * texture and shader program when the reactive owner is disposed (opt out
 * with `{ manual: true }`); create outside any reactive scope for
 * app-lifetime shaders. For a shader whose source or inputs change
 * reactively, use {@link createShaderMemo} instead.
 *
 * That preamble (`#version 300 es`, precision, `vUV`, `iResolution`, `iTime`,
 * `fragColor`) is injected only into sources that do not declare their own
 * `#version` line. A source starting with `#version 300 es` compiles exactly
 * as written, so a shader carrying its own uniform names - one ported from
 * elsewhere - runs unchanged here without dropping to compileShader /
 * linkProgram. The built-in vertex stage still supplies `vUV`; declare
 * `in vec2 vUV;` yourself to read it.
 */
export function createShader(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: gpu.ShaderParams,
  textures?: Record<string, gpu.TextureId>,
  opts?: CreateOptions & SamplerOptions,
): gpu.TextureId {
  let id = gpu.createShader(fragmentSrc, width, height, params, textures, opts)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a render target over a pipeline from `createRenderPipeline` and
 * renders it once, returning the texture id (usable anywhere a normal
 * texture id is, e.g. `<texture src>`; resize with `setShaderSize`, drive
 * uniforms with `<texture params>` or `setShaderParams`). Many targets may
 * share one pipeline, and creating a target compiles nothing. The target
 * brings the per-target half: size, the concrete vertex `buffer` the
 * pipeline's attribute layout describes, `vertexCount` (defaults to the
 * whole buffer; a fullscreen pass over an attributeless pipeline is
 * `{ vertexCount: 3 }` with a covering-triangle vertex stage), uniforms, and
 * `clearColor`. Draw state (`attributes`, `topology`, `blend`, `depth`,
 * `depthWrite`) lives on the pipeline and throws here. Frees the target when
 * the reactive owner is disposed (opt out with `opts.manual`); the pipeline
 * is yours and outlives it.
 */
export function createShaderTarget(
  pipeline: gpu.RenderPipelineId,
  width: number,
  height: number,
  opts?: {
    params?: gpu.ShaderParams
    textures?: Record<string, gpu.TextureId>
    buffer?: gpu.BufferId
    vertexCount?: number
    clearColor?: [number, number, number, number]
  } & CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createShaderTarget(pipeline, width, height, opts)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/** The reactive shader description `createShaderMemo` builds from. Sampling
 * (`filter`/`wrap`) is creation-time state, so changing it rebuilds at a
 * fresh id, like a fragment-source or sampler-binding change. */
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
 * `setShaderSize`, a params change to `setShaderParams` - while a change to
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
export function createShaderMemo(
  spec: () => ShaderSpec,
  opts?: { onError?: (error: unknown) => void },
): () => gpu.TextureId {
  let make = (s: ShaderSpec) =>
    gpu.createShader(s.fragmentSrc, s.width, s.height, s.params, s.textures, { filter: s.filter, wrap: s.wrap })
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
          gpu.setShaderSize(currentId, next.width, next.height)
        }
        if (!sameRecord(next.params, current.params) && next.params) {
          gpu.setShaderParams(currentId, next.params)
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
 * e.g. `<texture src>`). Unlike `createShader` the vertex stage is yours:
 * declare `in` attributes matching `opts.attributes` (one interleaved vertex
 * in `opts.buffer`, a {@link createBuffer} id) and your own varyings toward
 * the fragment stage. Both sources may reference `iResolution`/`iTime` and
 * any uniform they declare (`float`/`int` scalars from a number,
 * `vec2`/`vec3`/`vec4`/`mat4` from a flat number array); drive values with
 * `<texture src={id} params={{...}} />` or `setShaderParams`, exactly like a
 * fragment shader.
 * `opts.depth` attaches a private depth buffer (cleared + tested per render);
 * `opts.depthWrite: false` (requires depth) keeps the test but stops the
 * draw from writing depth. `opts.blend: "add"` makes the draw accumulate
 * overlapping geometry additively (order-independent, no sorting) instead of
 * overwriting; a depth-tested additive pass is `{ depth: true, blend: "add",
 * depthWrite: false }` - each option only does what it says, neither implies
 * the other. `opts.vertexCount` defaults to the whole buffer and can be
 * changed later with `setDrawCount`. Frees the texture and GL program when the reactive
 * owner is disposed (opt out with `opts.manual`); create outside any reactive
 * scope for app-lifetime pipelines.
 */
export function createPipeline(
  vertexSrc: string,
  fragmentSrc: string,
  width: number,
  height: number,
  opts?: {
    params?: gpu.ShaderParams
    textures?: Record<string, gpu.TextureId>
    attributes?: gpu.VertexAttribute[]
    buffer?: gpu.BufferId
    topology?: gpu.Topology
    vertexCount?: number
    depth?: boolean
    depthWrite?: boolean
    blend?: gpu.BlendMode
    clearColor?: [number, number, number, number]
  } & CreateOptions &
    SamplerOptions,
): gpu.TextureId {
  let id = gpu.createPipeline(vertexSrc, fragmentSrc, width, height, opts)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a vertex buffer for pipeline attributes from raw data (typically a
 * Float32Array laid out to match the pipeline's interleaved attribute list).
 * Update it later with {@link writeBuffer}; the buffer's byte size is fixed at
 * creation, so reserve room up front for dynamic geometry. Freed automatically
 * when the reactive owner is disposed (opt out with `{ manual: true }`);
 * created outside a reactive scope you must call `destroyBuffer` yourself.
 * Destroy pipelines before their buffer.
 */
export function createBuffer(data: ArrayBuffer | ArrayBufferView, opts?: CreateOptions): gpu.BufferId {
  let id = gpu.createBuffer(toUint8(data))
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyBuffer(id))
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