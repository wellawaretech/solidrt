// Low-level GPU textures and shaders (gui-enabled runtime only). The
// imperative primitive; @solidrt/core's gpu helpers add reactive auto-cleanup
// on top. Three id spaces, each destroyed by its own destroyer: texture ids
// (the public token used as `<texture src>` and sampler inputs ->
// destroyTexture), buffer ids (-> destroyBuffer), and the raw shading layer's
// shader-stage ids (-> destroyShader) and program ids (-> destroyProgram).
// Layering: compileShader/linkProgram are the raw GL primitives (complete
// sources, explicit header opt-in); createShader/createPipeline are fused
// conveniences (compile + link + target in one call, curated preamble).
//
// Sampling state is fixed, not an option: every texture id samples with linear
// filtering (there is no nearest/point magnification, and no mipmaps exist).
// Wrapping differs by origin - shader and pipeline render targets are
// clamp-to-edge, while createTexture/createMutableTexture textures repeat
// outside 0..1.
//
// Compositing several targets is a render-tree job, not a shader one: stack
// `<texture>` elements and set their `blendMode` (the full Skia set, e.g.
// "plus" for an additive pass over a base pass) instead of writing a pass that
// samples both. Blending WITHIN one draw is unavailable - a target's own draw
// runs with GL blending disabled.

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
  /**
   * Destroy a texture (immutable, mutable, or shader). Frame-safe: the id is
   * reclaimed by the runtime once the render tree no longer references it, so
   * destroying the old id in the same update that repoints `<texture src>` at
   * its replacement never paints a blank frame, whatever order the two land
   * in. A destroyed id that stays mounted keeps drawing (and stays allocated)
   * until it is unmounted or repointed.
   */
  export function destroyTexture(id: number): void
  /**
   * Compile a GLSL ES fragment shader into an offscreen texture of the given
   * size. `params` sets float uniforms by name; `textures` binds sampler2D
   * uniforms to texture ids. Returns the resulting texture id. The fused
   * convenience: one call compiles a program and creates a target over it,
   * and the program lives and dies with the target. To share one compile
   * across targets (or hold a program with no target yet), use the raw layer:
   * {@link compileShader} + {@link linkProgram} + {@link createShaderTarget}.
   *
   * The preamble (`#version 300 es`, precision, `vUV`, `iResolution`, `iTime`,
   * `fragColor`) is injected only into sources that do not declare their own
   * `#version` line. A source that starts with `#version 300 es` is compiled
   * exactly as written, so a shader with its own uniform names (a port from
   * elsewhere) needs no rewriting and no drop to the raw layer. The built-in
   * vertex stage still supplies `vUV` to a complete source; declare
   * `in vec2 vUV;` yourself to read it. Same rule on {@link createPipeline}.
   */
  export function createShader(
    fragmentSrc: string,
    width: number,
    height: number,
    params?: Record<string, number>,
    textures?: Record<string, number>,
  ): number
  /**
   * Compile a single shader stage from raw GLSL ES: the primitive under
   * {@link linkProgram}, GL's own model (a "shader" is one stage; linking
   * stages yields a program). The source is complete - it declares its own
   * `#version 300 es`, precision, varyings and uniforms; nothing is injected.
   * With `header: true` the standard header is prepended explicitly: `#version
   * 300 es`, `precision highp float;`, `uniform vec2 iResolution;`, `uniform
   * float iTime;`, plus `out vec4 fragColor;` for a fragment stage (the same
   * text {@link createPipeline} injects). Do not combine `header` with your
   * own `#version` line. Returns a shader (stage) id in its own id space;
   * compile errors throw here, synchronously, at a call site the app chose.
   * Free with {@link destroyShader}.
   */
  export function compileShader(
    stage: "vertex" | "fragment",
    source: string,
    opts?: { header?: boolean },
  ): number
  /**
   * Link a compiled vertex and fragment stage into a program, returning a
   * program id (its own id space, like buffers - not a texture id). Link
   * errors throw here. The stages remain usable for further links (mix one
   * vertex stage with many fragment stages and vice versa), and may be
   * destroyed right after: a linked program keeps its own compiled copies.
   * Creating targets from the returned handle compiles nothing. Free with
   * {@link destroyProgram}.
   */
  export function linkProgram(vertexShader: number, fragmentShader: number): number
  /**
   * Destroy a compiled stage by id. Programs linked from it are unaffected.
   */
  export function destroyShader(id: number): void
  /**
   * Create a render target over a linked program and render it once: the
   * target half of {@link createPipeline}. Returns a texture id exactly like
   * createShader/createPipeline do (drive uniforms via the `params` prop or
   * {@link setShaderParams}, resize with {@link setShaderSize}, destroy with
   * {@link destroyTexture}). Many targets may share one program. A raw-linked
   * program carries its own vertex stage, so the mesh options apply:
   * `attributes`/`buffer` for vertex input (omit for attributeless rendering
   * via gl_VertexID - a fullscreen pass is `vertexCount: 3` with a
   * covering-triangle vertex stage), `topology`, `vertexCount`, `depth`,
   * `clearColor`, all as in {@link createPipeline}.
   */
  export function createShaderTarget(
    program: number,
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
   * Destroy a linked program by id. Targets created from it are unaffected:
   * each holds the program until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new targets
   * immediately.
   */
  export function destroyProgram(id: number): void
  /** Update a shader texture's float uniforms by name and re-render it. */
  export function setShaderParams(id: number, params: Record<string, number>): void
  /**
   * Rebind a shader texture's sampler2D inputs by uniform name and re-render
   * it with its last-applied params - the sampler analog of
   * {@link setShaderParams}. Bindings not named keep their current source, so
   * a single input can be retargeted (post-process source swap, ping-pong
   * between two data textures) without recompiling the shader. Throws if the
   * shader or a source texture id is unknown, or a sampler would source the
   * shader's own target.
   */
  export function setShaderTextures(id: number, textures: Record<string, number>): void
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
   *
   * Intended for one-shot bakes and inspection: turning something the engine
   * can draw but the app cannot compute - shaped text, an SVG, a themed view -
   * into pixels, usually to hand to {@link readTexture} and process on the CPU.
   * Baking a glyph atlas by laying out cells, capturing them and keeping the
   * coverage channel is the worked example. Tests and freeze-frames are the
   * same shape.
   *
   * Not a rendering primitive. Every call rasterizes the subtree into a fresh
   * offscreen MSAA target, reads the pixels back to the CPU and uploads them
   * again as a new texture: a full GPU -> CPU -> GPU round trip plus a paint
   * pass of latency, per call, with nothing incremental about it. Batch what
   * you capture (many nodes captured together are serviced by one paint pass),
   * and do not drive it per frame or reach for it to feed live content into a
   * shader - an effect over what is beneath it, a backdrop filter. Content that
   * must stay current has to come from a source that updates in place: another
   * pipeline's render target, a camera texture, a mutable texture.
   */
  export function captureSnapshot(nodeId: number): Promise<{ id: number; width: number; height: number }>
  /**
   * Read back a registered texture's current pixels as RGBA8 (tightly packed,
   * top-to-bottom rows), for any texture id whatever created it (createTexture,
   * createShader, captureSnapshot). Synchronous. Throws if the id is unknown.
   */
  export function readTexture(id: number): { width: number; height: number; data: Uint8Array }
}