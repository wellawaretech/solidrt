// GPU textures and shaders, reactive (SolidJS) layer: the create* helpers free
// their texture automatically when the reactive owner is disposed. Drive a
// shader's uniforms declaratively with `<texture src={id} params={{...}} />`
// (see TextureProps) - the preferred way, deferred to the next real repaint so
// a fast-changing signal stays paced to actual frames. setShaderParams is the
// imperative exception: reach for it only when there is no `<texture>` element
// to hold a params prop, e.g. a shader that only feeds another shader as a
// sampler2D input. The imperative primitives (uploadTexture, setShaderParams,
// destroyTexture, ...) live in the `flux:gpu` module.

import { createEffect, createSignal, getOwner, onCleanup, untrack } from "@solidjs/signals"
import * as gpu from "flux:gpu"

// The create* helpers accept { manual: true } to opt out of the owner-scoped
// auto-free, for resources whose lifetime is managed by hand (rebuilt on
// signal changes inside a long-lived component, handed across owners, ...).
// Without it, each rebuild would stack another onCleanup on the component
// owner: a leak until unmount, then a double-free against manual destroys.
export type CreateOptions = { manual?: boolean }

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
export type { Topology, VertexAttribute } from "flux:gpu"

// captureSnapshot renders a node to a texture and readTexture reads any
// texture's bytes back. Re-exported raw (no reactive auto-cleanup wrapper):
// captureSnapshot resolves asynchronously, by which point the reactive owner is
// no longer current, so the caller owns the returned id and frees it with
// destroyTexture (as with any texture created after an await).
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
export function createTexture(data: Uint8Array, width: number, height: number, opts?: CreateOptions): number {
  let id = gpu.createTexture(data, width, height)
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
export function createMutableTexture(data: Uint8Array, width: number, height: number, opts?: CreateOptions): number {
  let id = gpu.createMutableTexture(data, width, height)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Compiles a GLSL ES 3.00 fragment shader and renders it into a texture,
 * returning the texture id (usable anywhere a normal texture id is, e.g.
 * `<texture src>`). The fragment body may reference `vUV` (0..1, top-left
 * origin), `iResolution`, `iTime`, and any `uniform float` it declares; drive
 * their values with `<texture src={id} params={{...}} />` (preferred) or, when
 * there is no `<texture>` element for it, imperatively with `setShaderParams`.
 * `textures` binds each declared `uniform sampler2D` to an existing texture id
 * (e.g. a camera or decoded image) so the shader can read it; those inputs are
 * re-sampled on every params update, so live sources stay current. Frees the
 * texture and shader program when the reactive owner is disposed (opt out
 * with `{ manual: true }`); create outside any reactive scope for
 * app-lifetime shaders. For a shader whose source or inputs change
 * reactively, use {@link createShaderMemo} instead.
 */
export function createShader(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: Record<string, number>,
  textures?: Record<string, number>,
  opts?: CreateOptions,
): number {
  let id = gpu.createShader(fragmentSrc, width, height, params, textures)
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/** The reactive shader description `createShaderMemo` builds from. */
export type ShaderSpec = {
  fragmentSrc: string
  width: number
  height: number
  params?: Record<string, number>
  textures?: Record<string, number>
}

// Shallow name->number equality for params/textures records; treats undefined
// as the empty record.
function sameRecord(a: Record<string, number> | undefined, b: Record<string, number> | undefined): boolean {
  if (a === b) return true
  let ka = a ? Object.keys(a) : []
  let kb = b ? Object.keys(b) : []
  return ka.length === kb.length && ka.every(k => a![k] === b![k])
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
 */
export function createShaderMemo(spec: () => ShaderSpec): () => number {
  let current = untrack(spec)
  let currentId = gpu.createShader(current.fragmentSrc, current.width, current.height, current.params, current.textures)
  let [id, setId] = createSignal(currentId)
  createEffect(spec, next => {
    if (next.fragmentSrc === current.fragmentSrc && sameRecord(next.textures, current.textures)) {
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
    let old = currentId
    current = next
    currentId = gpu.createShader(next.fragmentSrc, next.width, next.height, next.params, next.textures)
    setId(currentId)
    gpu.destroyTexture(old)
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
 * the fragment stage. Both sources may reference `iResolution`/`iTime` and any
 * `uniform float` they declare; drive values with `<texture src={id}
 * params={{...}} />` or `setShaderParams`, exactly like a fragment shader.
 * `opts.depth` attaches a private depth buffer (cleared + tested per render);
 * `opts.vertexCount` defaults to the whole buffer and can be changed later
 * with `setDrawCount`. Frees the texture and GL program when the reactive
 * owner is disposed (opt out with `opts.manual`); create outside any reactive
 * scope for app-lifetime pipelines.
 */
export function createPipeline(
  vertexSrc: string,
  fragmentSrc: string,
  width: number,
  height: number,
  opts?: {
    params?: Record<string, number>
    textures?: Record<string, number>
    attributes?: gpu.VertexAttribute[]
    buffer?: number
    topology?: gpu.Topology
    vertexCount?: number
    depth?: boolean
    clearColor?: [number, number, number, number]
  } & CreateOptions,
): number {
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
export function createBuffer(data: ArrayBuffer | ArrayBufferView, opts?: CreateOptions): number {
  let id = gpu.createBuffer(toUint8(data))
  if (!opts?.manual && getOwner()) onCleanup(() => gpu.destroyBuffer(id))
  return id
}

/**
 * Overwrites part of a vertex buffer at `byteOffset` (default 0). Every
 * pipeline drawing from the buffer re-renders with its last-applied params,
 * so geometry-only changes reach the screen without a params update.
 */
export function writeBuffer(id: number, data: ArrayBuffer | ArrayBufferView, byteOffset?: number): void {
  gpu.writeBuffer(id, toUint8(data), byteOffset)
}