// Low-level GPU textures and shaders (gui-enabled runtime only). The
// imperative primitive; @solidrt/core's gpu helpers add reactive auto-cleanup
// on top. Each id space has its own destroyer: texture ids (the public token
// used as `<texture src>` and sampler inputs -> destroyTexture), buffer ids
// (-> destroyBuffer), and the raw shading layer's shader-stage ids
// (-> destroyShader), program ids (-> destroyProgram), and render-pipeline
// ids (-> destroyRenderPipeline).
// Layering: compileShader/linkProgram are the raw GL primitives (complete
// sources, explicit header opt-in); createRenderPipeline pairs a program with
// draw state (topology, blend, depth, vertex layout - how it draws);
// createShaderTarget builds a texture-backed target over a pipeline (size,
// buffer, uniforms, clear - where it draws). createShader/createPipeline are
// fused conveniences (compile + link + pipeline + target in one call, curated
// preamble).
//
// Sampling is a per-texture property declared at creation: every create path
// accepts `{ filter?, wrap? }` ("linear"/"nearest", "clamp"/"repeat";
// defaults linear + clamp for every origin). The state follows the id
// everywhere it is sampled - shader passes and `<texture>` display alike -
// and survives id-stable resizes. It cannot be changed after creation. No
// mipmaps exist.
//
// Compositing several targets is a render-tree job, not a shader one: stack
// `<texture>` elements and set their `blendMode` (the full Skia set, e.g.
// "plus" for an additive pass over a base pass) instead of writing a pass that
// samples both. WITHIN one pipeline draw, `blend: "add"` accumulates
// overlapping geometry additively; anything else (a fragment target, or a
// pipeline without the option) draws with GL blending disabled and overwrites.

declare module "flux:gpu" {
  /**
   * Shader uniform values by name. A number drives a scalar uniform (`float`,
   * or `int`/`bool`, truncated); a flat number array drives a typed uniform
   * whose declared GLSL type sets the expected length: 2/3/4 for
   * `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`. Dispatch follows the
   * shader's own declaration; a value whose length does not fit it is skipped
   * with a runtime warning, as is a name with no active uniform.
   */
  export type ShaderParams = Record<string, number | number[]>
  /** Magnification/minification filter; "linear" (default) or hard-pixel "nearest". */
  export type FilterMode = "linear" | "nearest"
  /** Sampling outside 0..1: "clamp" (default, extend edge pixels) or "repeat" (tile). */
  export type WrapMode = "clamp" | "repeat"
  /**
   * Per-texture sampling, declared at creation and fixed for the id's
   * lifetime. Applies wherever the texture is sampled: shader/pipeline
   * sampler2D inputs AND `<texture src>` display (a "nearest" texture
   * upscales with hard pixels on screen - the pixel-art path). `wrap` only
   * matters to shaders sampling outside 0..1; the display draw never tiles.
   */
  export type SamplerOptions = { filter?: FilterMode; wrap?: WrapMode }
  /**
   * Create an immutable texture from an RGBA8 pixel buffer (exactly
   * width*height*4 bytes). Returns the texture id.
   */
  export function createTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions): number
  /**
   * Create a texture intended to be updated later via {@link uploadTexture}. The
   * seed buffer must hold at least one frame (width*height*4 bytes) and may hold
   * more (uploadTexture selects a frame by offset).
   */
  export function createMutableTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions): number
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
   * size. `params` sets uniforms by name (see {@link ShaderParams} for the
   * value shapes); `textures` binds sampler2D uniforms to texture ids - any
   * texture id, including another shader/pipeline target's output. Bound
   * targets are live dependencies: when a source re-renders (its params,
   * geometry, or data change), every target sampling it re-renders too,
   * transitively through chains, before the next frame or readback - no
   * per-frame uniform write is needed to keep a chain current. Returns the resulting texture id. The fused
   * convenience: one call compiles a program and creates a target over it,
   * and the program lives and dies with the target. To share one compile
   * across targets (or hold a program with no target yet), use the raw layer:
   * {@link compileShader} + {@link linkProgram} + {@link createRenderPipeline}
   * + {@link createShaderTarget}.
   *
   * The preamble (`#version 300 es`, precision, `vUV`, `iResolution`, `iTime`,
   * `fragColor`) is injected only into sources that do not declare their own
   * `#version` line. A source that starts with `#version 300 es` is compiled
   * exactly as written, so a shader with its own uniform names (a port from
   * elsewhere) needs no rewriting and no drop to the raw layer. The built-in
   * vertex stage still supplies `vUV` to a complete source; declare
   * `in vec2 vUV;` yourself to read it. Same rule on {@link createPipeline}.
   * A complete source may also declare `iResolution` as vec3 (the Shadertoy
   * shape); it is then filled as `(w, h, 1.0)`.
   */
  export function createShader(
    fragmentSrc: string,
    width: number,
    height: number,
    params?: ShaderParams,
    textures?: Record<string, number>,
    opts?: SamplerOptions,
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
   * Pair a linked program with draw state, returning a render pipeline id
   * (its own id space, like programs and buffers - not a texture id): the
   * pipeline state object of every modern GPU API. The pipeline owns HOW its
   * targets draw - `attributes` (the interleaved vertex layout; omit for
   * attributeless rendering via gl_VertexID), `topology`, `blend`, `depth`,
   * `depthWrite` (`false` requires `depth: true`) - while each target brings
   * its own size, buffer, uniforms, and clear. Creating a pipeline compiles
   * nothing, and many pipelines may share one program. The vocabulary is
   * validated here, so a bad word throws at this call site. Free with
   * {@link destroyRenderPipeline}; the program is yours and outlives it.
   */
  export function createRenderPipeline(
    program: number,
    opts?: {
      attributes?: VertexAttribute[]
      topology?: Topology
      blend?: BlendMode
      depth?: boolean
      depthWrite?: boolean
    },
  ): number
  /**
   * Destroy a render pipeline by id. Targets created from it are unaffected:
   * each holds the pipeline until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new targets
   * immediately.
   */
  export function destroyRenderPipeline(id: number): void
  /**
   * Create a render target over a {@link createRenderPipeline} pipeline and
   * render it once: the target half of {@link createPipeline}. Returns a
   * texture id exactly like createShader/createPipeline do (drive uniforms
   * via the `params` prop or {@link setShaderParams}, resize with
   * {@link setShaderSize}, destroy with {@link destroyTexture}). Many targets
   * may share one pipeline, and creating a target compiles nothing. `buffer`
   * supplies the concrete vertex buffer the pipeline's attribute layout
   * describes (required when the pipeline declares attributes);
   * `vertexCount` defaults to the whole buffer, and a fullscreen pass over an
   * attributeless pipeline is `vertexCount: 3` with a covering-triangle
   * vertex stage. Draw-state keys (`attributes`, `topology`, `blend`,
   * `depth`, `depthWrite`) belong to the pipeline and throw here.
   */
  export function createShaderTarget(
    pipeline: number,
    width: number,
    height: number,
    opts?: {
      params?: ShaderParams
      textures?: Record<string, number>
      buffer?: number
      vertexCount?: number
      clearColor?: [number, number, number, number]
    } & SamplerOptions,
  ): number
  /**
   * Destroy a linked program by id. Pipelines created from it are unaffected:
   * each holds the program until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new pipelines
   * immediately.
   */
  export function destroyProgram(id: number): void
  /**
   * Update a shader texture's uniforms by name and re-render it (see
   * {@link ShaderParams} for the value shapes).
   */
  export function setShaderParams(id: number, params: ShaderParams): void
  /**
   * Rebind a shader texture's sampler2D inputs by uniform name and re-render
   * it with its last-applied params - the sampler analog of
   * {@link setShaderParams}. Bindings not named keep their current source, so
   * a single input can be retargeted (post-process source swap, ping-pong
   * between two data textures) without recompiling the shader. Throws if the
   * shader or a source texture id is unknown, or a binding would create a
   * sampling cycle among targets (binding a shader's own target is the
   * shortest case).
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
   * Blending for a pipeline's own draw. "none" (default) overwrites:
   * overlapping geometry resolves by depth or draw order. "add" accumulates
   * (glBlendFunc(ONE, ONE)): order-independent, so geometry needs no sorting
   * - the additive half of translucency (point splats, glow passes). A
   * depth-tested additive pass usually pairs with `depthWrite: false`; with
   * writes on, unsorted geometry depth-rejects its own later fragments and
   * accumulation becomes draw-order-dependent. That pairing is the app's to
   * state - neither option implies the other.
   */
  export type BlendMode = "none" | "add"
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
   * private depth buffer, cleared and tested on every render; `depthWrite:
   * false` (requires `depth: true`) keeps the test but stops the draw from
   * writing depth. `blend` sets the draw's own blending (see
   * {@link BlendMode}); an additive pass over a depth buffer is
   * `{ depth: true, blend: "add", depthWrite: false }`, stated explicitly.
   * The target is cleared to `clearColor` (default transparent black) before
   * each draw.
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
      params?: ShaderParams
      textures?: Record<string, number>
      attributes?: VertexAttribute[]
      buffer?: number
      topology?: Topology
      vertexCount?: number
      depth?: boolean
      depthWrite?: boolean
      blend?: BlendMode
      clearColor?: [number, number, number, number]
    } & SamplerOptions,
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
   * the live tree (an unmounted node is never painted, so its capture rejects)
   * and paint a non-zero box. A laid-out node captures its layout box. A `d-*`
   * node has no layout box - that is what detached means - so it captures its
   * painted box instead: its own `w`/`h` when set, else the nearest laid-out
   * ancestor's box (the same box the render tree reports for it), with its
   * `x`/`y` paint offset mapped to the texture origin.
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