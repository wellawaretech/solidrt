// Low-level GPU textures and fragment shaders (gui-enabled runtime only). The
// imperative primitive; @solidrt/core's gpu helpers add reactive auto-cleanup on
// top. Texture ids are the public token (used as `<texture src>` and shader
// sampler inputs), so there is no handle to hide here.

declare module "flux:gpu" {
  /**
   * Create an immutable texture from an RGBA8 pixel buffer (exactly
   * width*height*4 bytes). Returns the texture id.
   */
  export function createTexture(data: Uint8Array, width: number, height: number): number
  /**
   * Create a texture intended to be updated later via {@link uploadTexture}. The
   * seed buffer must hold at least one frame (width*height*4 bytes) and may hold
   * more (uploadTexture selects a frame by offset).
   */
  export function createMutableTexture(data: Uint8Array, width: number, height: number): number
  /**
   * Replace a mutable texture's pixels. `data` may hold several frames; `offset`
   * (default 0) selects which frame to upload.
   */
  export function uploadTexture(id: number, data: Uint8Array, offset?: number): void
  /**
   * Replace a texture's storage with a new size at the same id (an id-stable
   * resize): `<texture src>` references and shader sampler bindings keep
   * working, and shaders sampling the texture re-render. `data` seeds the new
   * contents and, like {@link createMutableTexture}, must hold at least one
   * width*height*4 frame. Shader/pipeline target ids are rejected - resize
   * those with {@link setShaderSize}.
   */
  export function resizeTexture(id: number, data: Uint8Array, width: number, height: number): void
  /** Destroy a texture (immutable, mutable, or shader). */
  export function destroyTexture(id: number): void
  /**
   * Compile a GLSL ES fragment shader into an offscreen texture of the given
   * size. `params` sets float uniforms by name; `textures` binds sampler2D
   * uniforms to texture ids. Returns the resulting texture id.
   */
  export function createShader(
    fragmentSrc: string,
    width: number,
    height: number,
    params?: Record<string, number>,
    textures?: Record<string, number>,
  ): number
  /** Update a shader texture's float uniforms by name and re-render it. */
  export function setShaderParams(id: number, params: Record<string, number>): void
  /**
   * Resize a shader or pipeline target texture in place and re-render it: the
   * id, compiled program, last-applied params, and sampler bindings all carry
   * over; only the output size changes. The setDrawCount analog for output
   * size.
   */
  export function setShaderSize(id: number, width: number, height: number): void

  export type Topology = "points" | "lines" | "line-strip" | "triangles" | "triangle-strip"
  /**
   * One float attribute of an interleaved vertex. The attribute list's order
   * defines the byte layout; locations are resolved by name against the
   * vertex shader's `in` declarations.
   */
  export type VertexAttribute = { name: string; format: "f32" | "vec2" | "vec3" | "vec4" }

  /**
   * Compile a GLSL ES vertex+fragment pipeline into an offscreen texture of
   * the given size and render it once. Sources without a `#version` line get
   * a 300 es preamble declaring `iResolution`/`iTime` (no vUV: varyings are
   * the pipeline's own). `attributes` describes one interleaved vertex in
   * `buffer` (a {@link createBuffer} id); omit both for attributeless
   * rendering via gl_VertexID. `vertexCount` defaults to the whole buffer
   * (buffer size / vertex stride). With `depth: true` the pipeline gets a
   * private depth buffer, cleared and tested on every render. The target is
   * cleared to `clearColor` (default transparent black) before each draw.
   * Returns a texture id: display it with `<texture src>`, drive uniforms via
   * the `params` prop or {@link setShaderParams}, destroy with
   * {@link destroyTexture}.
   */
  export function createPipeline(
    vertexSrc: string,
    fragmentSrc: string,
    width: number,
    height: number,
    opts?: {
      params?: Record<string, number>
      textures?: Record<string, number>
      attributes?: VertexAttribute[]
      buffer?: number
      topology?: Topology
      vertexCount?: number
      depth?: boolean
      clearColor?: [number, number, number, number]
    },
  ): number

  /**
   * Create a vertex buffer from raw bytes (interleave attribute data to match
   * the pipeline's attribute list). Buffer ids are their own space, separate
   * from texture ids.
   */
  export function createBuffer(data: Uint8Array): number
  /**
   * Overwrite part of a vertex buffer at `byteOffset` (default 0), within the
   * size it was created with. Pipelines drawing from the buffer re-render
   * with their last-applied params.
   */
  export function writeBuffer(id: number, data: Uint8Array, byteOffset?: number): void
  /** Destroy a vertex buffer. Destroy pipelines drawing from it first. */
  export function destroyBuffer(id: number): void
  /**
   * Set how many vertices a pipeline texture draws and re-render it, e.g.
   * after writing a variable amount of dynamic geometry into its buffer.
   */
  export function setDrawCount(id: number, count: number): void
  /**
   * Capture a render-tree node's subtree into a new GPU texture, resolving once
   * it has been rendered on the next paint pass. The node must be attached to
   * the live tree (a detached node is never painted, so its capture rejects).
   * Rendered at the current display scale, so `width`/`height` are the texture's
   * actual pixel dimensions (ceil(logicalSize * displayScale)), not logical
   * points. Each call returns an independent id you must {@link destroyTexture}
   * when done. Use the returned id anywhere a texture id is accepted
   * (`<texture src>`, a shader sampler input, {@link readTexture}).
   */
  export function captureSnapshot(nodeId: number): Promise<{ id: number; width: number; height: number }>
  /**
   * Read back a registered texture's current pixels as RGBA8 (tightly packed,
   * top-to-bottom rows), for any texture id whatever created it (createTexture,
   * createShader, captureSnapshot). Synchronous. Throws if the id is unknown.
   */
  export function readTexture(id: number): { width: number; height: number; data: Uint8Array }
}