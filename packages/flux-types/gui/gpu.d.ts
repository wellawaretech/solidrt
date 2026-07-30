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
//
// The render contract. A target's contents are a pure function of its inputs
// (params, bound textures, geometry): the runtime renders it whenever inputs
// change - zero, one, or many times per frame, at its discretion - so a pass
// must not depend on its own previous output or on how often it runs. When a
// pass IS state (accumulation, feedback, simulation), create the target with
// `render: "manual"`: the runtime then never renders it, only an explicit
// renderTarget(id) does, in call order - the app owns the stepping. Targets
// sampling a manual target update after each explicit render; a manual
// target's own params/geometry writes take effect at its next render.
// `loadOp: "load"` (manual-only) keeps the previous contents under each
// draw - single-target accumulation - and copyTexture(src, dst) seeds or
// snapshots a manual target GPU-side. Both compose with renderTarget in
// call order.
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

declare module "flux:gpu" {
  /**
   * A GPU texture handle: what every texture-producing call returns and every
   * texture-consuming site takes (`<texture src>`, sampler bindings, the
   * texture mutators and destroyTexture). At runtime it is a plain number;
   * the brand exists so each id space is its own type and a cross-space slip
   * - `destroyBuffer(textureId)`, `createShaderTarget(programId, ...)` - is a
   * type error instead of an operation on an unrelated live resource (every
   * space counts from 1, so a wrong id is usually a valid id in the wrong
   * space). Ids widen to number freely; only number -> id is blocked.
   */
  export type TextureId = number & { readonly __texture: unique symbol }
  /** The vertex-buffer id space ({@link createBuffer}); see {@link TextureId} for the brand model. */
  export type BufferId = number & { readonly __buffer: unique symbol }
  /** The compiled-stage id space ({@link compileShader}); see {@link TextureId} for the brand model. */
  export type ShaderStageId = number & { readonly __shaderStage: unique symbol }
  /** The linked-program id space ({@link linkProgram}); see {@link TextureId} for the brand model. */
  export type ProgramId = number & { readonly __program: unique symbol }
  /** The render-pipeline id space ({@link createRenderPipeline}); see {@link TextureId} for the brand model. */
  export type RenderPipelineId = number & { readonly __renderPipeline: unique symbol }
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
  export function createTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions): TextureId
  /**
   * Create a texture intended to be updated later via {@link uploadTexture}. The
   * seed buffer must hold at least one frame (width*height*4 bytes) and may hold
   * more (uploadTexture selects a frame by offset).
   */
  export function createMutableTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions): TextureId
  /**
   * Replace a mutable texture's pixels. `data` may hold several frames; `offset`
   * (default 0) selects which frame to upload.
   */
  export function uploadTexture(id: TextureId, data: Uint8Array, offset?: number): void
  /**
   * Replace a texture's storage with a new size at the same id (an id-stable
   * resize): `<texture src>` references and shader sampler bindings keep
   * working, and shaders sampling the texture re-render. `data` seeds the new
   * contents and, like {@link createMutableTexture}, must hold at least one
   * width*height*4 frame. Shader/pipeline target ids are rejected - resize
   * those with {@link setShaderSize}.
   */
  export function resizeTexture(id: TextureId, data: Uint8Array, width: number, height: number): void
  /**
   * Destroy a texture (immutable, mutable, or shader). Frame-safe: the id is
   * reclaimed by the runtime once the render tree no longer references it, so
   * destroying the old id in the same update that repoints `<texture src>` at
   * its replacement never paints a blank frame, whatever order the two land
   * in. A destroyed id that stays mounted keeps drawing (and stays allocated)
   * until it is unmounted or repointed.
   */
  export function destroyTexture(id: TextureId): void
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
    textures?: Record<string, TextureId>,
    opts?: SamplerOptions,
  ): TextureId
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
   *
   * A vertex stage writes into a y-down clip space: `gl_Position` y = -1 is
   * the top row of the target and +1 the bottom, so camera-up geometry must
   * negate y (or fold the flip into its projection) to display up.
   */
  export function compileShader(
    stage: "vertex" | "fragment",
    source: string,
    opts?: { header?: boolean },
  ): ShaderStageId
  /**
   * Link a compiled vertex and fragment stage into a program, returning a
   * program id (its own id space, like buffers - not a texture id). Link
   * errors throw here. The stages remain usable for further links (mix one
   * vertex stage with many fragment stages and vice versa), and may be
   * destroyed right after: a linked program keeps its own compiled copies.
   * Creating targets from the returned handle compiles nothing. Free with
   * {@link destroyProgram}.
   */
  export function linkProgram(vertexShader: ShaderStageId, fragmentShader: ShaderStageId): ProgramId
  /**
   * Destroy a compiled stage by id. Programs linked from it are unaffected.
   */
  export function destroyShader(id: ShaderStageId): void
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
    program: ProgramId,
    opts?: {
      attributes?: VertexAttribute[]
      topology?: Topology
      blend?: BlendMode
      depth?: boolean
      depthWrite?: boolean
    },
  ): RenderPipelineId
  /**
   * Destroy a render pipeline by id. Targets created from it are unaffected:
   * each holds the pipeline until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new targets
   * immediately.
   */
  export function destroyRenderPipeline(id: RenderPipelineId): void
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
   *
   * `render: "manual"` opts the target out of runtime-driven rendering (see
   * the render contract above): it starts cleared to `clearColor` and its
   * pass runs only when {@link renderTarget} is called.
   *
   * `loadOp` chooses what each render finds in the target: `"clear"` (the
   * default) clears to `clearColor` first, `"load"` keeps the previous
   * contents and draws over them - single-target accumulation (with the
   * pipeline's `blend: "add"`, an additive trail; without blending, draws
   * simply land over old pixels). `"load"` requires `render: "manual"` and
   * throws otherwise: on a runtime-rendered target the output would depend
   * on how often the runtime happened to render. Depth (when the pipeline
   * has it) is per-render scratch and always clears; creation, resize, and
   * nothing else reset the color to `clearColor`. State that needs a
   * read-modify-write of its own pixels (decay, blur, simulation) still
   * ping-pongs across two manual targets - a pass can never sample the
   * texture it writes.
   */
  export function createShaderTarget(
    pipeline: RenderPipelineId,
    width: number,
    height: number,
    opts?: {
      params?: ShaderParams
      textures?: Record<string, TextureId>
      buffer?: BufferId
      vertexCount?: number
      clearColor?: [number, number, number, number]
      render?: "auto" | "manual"
      loadOp?: "clear" | "load"
    } & SamplerOptions,
  ): TextureId
  /**
   * Destroy a linked program by id. Pipelines created from it are unaffected:
   * each holds the program until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new pipelines
   * immediately.
   */
  export function destroyProgram(id: ProgramId): void
  /**
   * Update a shader texture's uniforms by name and re-render it (see
   * {@link ShaderParams} for the value shapes). On a manual target nothing
   * renders here; the values apply at its next {@link renderTarget}.
   */
  export function setShaderParams(id: TextureId, params: ShaderParams): void
  /**
   * Rebind a shader texture's sampler2D inputs by uniform name and re-render
   * it with its last-applied params - the sampler analog of
   * {@link setShaderParams}. Bindings not named keep their current source, so
   * a single input can be retargeted (post-process source swap, ping-pong
   * between two data textures) without recompiling the shader. Throws if the
   * shader or a source texture id is unknown, if a binding names the
   * shader's own target (same-pass feedback), or if it would close a
   * sampling cycle among runtime-rendered targets. A cycle through a
   * `render: "manual"` target is legal - the runtime never renders one, so
   * the loop only steps when the app calls {@link renderTarget}.
   */
  export function setShaderTextures(id: TextureId, textures: Record<string, TextureId>): void
  /**
   * Resize a shader or pipeline target texture in place and re-render it: the
   * id, compiled program, last-applied params, and sampler bindings all carry
   * over; only the output size changes. The setDrawCount analog for output
   * size.
   */
  export function setShaderSize(id: TextureId, width: number, height: number): void

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
   * the pipeline's own). Clip space is y-down: `gl_Position` y = -1 is the top
   * row of the target and +1 the bottom, so camera-up geometry must negate y
   * (or fold the flip into its projection) to display up. `attributes`
   * describes one interleaved vertex in `buffer` (a {@link createBuffer} id);
   * omit both for attributeless rendering via gl_VertexID. `vertexCount`
   * defaults to the whole buffer (buffer size / vertex stride). With
   * `depth: true` the pipeline gets a private depth buffer, cleared and tested
   * on every render; `depthWrite: false` (requires `depth: true`) keeps the
   * test but stops the draw from writing depth. `blend` sets the draw's own blending (see
   * {@link BlendMode}); an additive pass over a depth buffer is
   * `{ depth: true, blend: "add", depthWrite: false }`, stated explicitly.
   * The target is cleared to `clearColor` (default transparent black) before
   * each draw. `render: "manual"` and `loadOp` behave exactly as on
   * {@link createShaderTarget}: no runtime-driven renders, step with
   * {@link renderTarget}, and `loadOp: "load"` (manual-only) keeps the
   * previous contents under each draw.
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
      textures?: Record<string, TextureId>
      attributes?: VertexAttribute[]
      buffer?: BufferId
      topology?: Topology
      vertexCount?: number
      depth?: boolean
      depthWrite?: boolean
      blend?: BlendMode
      clearColor?: [number, number, number, number]
      render?: "auto" | "manual"
      loadOp?: "clear" | "load"
    } & SamplerOptions,
  ): TextureId

  /**
   * Create a vertex buffer from raw bytes (interleave attribute data to match
   * the pipeline's attribute list). Buffer ids are their own space, separate
   * from texture ids.
   */
  export function createBuffer(data: Uint8Array): BufferId
  /**
   * Overwrite part of a vertex buffer at `byteOffset` (default 0), within the
   * size it was created with. Pipelines drawing from the buffer re-render
   * with their last-applied params.
   */
  export function writeBuffer(id: BufferId, data: Uint8Array, byteOffset?: number): void
  /** Destroy a vertex buffer. Destroy pipelines drawing from it first. */
  export function destroyBuffer(id: BufferId): void
  /**
   * Set how many vertices a pipeline texture draws and re-render it, e.g.
   * after writing a variable amount of dynamic geometry into its buffer.
   * (On a manual target nothing renders here; the count applies at its next
   * {@link renderTarget}.)
   */
  export function setDrawCount(id: TextureId, count: number): void
  /**
   * Render a `render: "manual"` target once, now. Renders land in call order
   * relative to every other GPU call: a `setShaderParams`/`writeBuffer`
   * issued before is visible to the pass, a {@link readTexture} issued after
   * observes it, and two renders run the pass twice in order. Inputs are
   * fresh: pending runtime-driven renders of sampled targets resolve first.
   * Targets sampling this one update after the render. Throws if the id is
   * not a manual target - the runtime owns rendering the others, and a pass
   * that depends on how often it runs is only well-defined when the app is
   * the one counting. Ping-pong feedback is two manual targets sampling
   * each other, stepped alternately from `onFrame`; binding a target to
   * ITSELF still throws (same-pass GL feedback, undefined pixels regardless
   * of who schedules it).
   */
  export function renderTarget(id: TextureId): void
  /**
   * Overwrite a `render: "manual"` target with another texture's current
   * pixels, GPU-side: the seed/history analog of {@link uploadTexture}
   * (seed a `loadOp: "load"` accumulator, snapshot one ping-pong buffer
   * into another, reset state to a known image). Exact and same-size only -
   * content and row order are preserved, and a size mismatch throws (a
   * scaling copy is an ordinary pass). Copies land in call order like
   * renders: a copy after a render sees that render, a readback after a
   * copy sees the copy, and targets sampling `dst` update afterwards.
   * Throws if either id is unknown, `dst` is not a manual target (the
   * runtime owns those contents), or `src === dst`. `src` may be any
   * texture: uploaded, mutable, a camera frame, or another target's output.
   */
  export function copyTexture(src: TextureId, dst: TextureId): void
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
  export function captureSnapshot(nodeId: number): Promise<{ id: TextureId; width: number; height: number }>
  /**
   * Read back a registered texture's current pixels as RGBA8 (tightly packed,
   * top-to-bottom rows), for any texture id whatever created it (createTexture,
   * createShader, captureSnapshot). Synchronous. Throws if the id is unknown.
   */
  export function readTexture(id: TextureId): { width: number; height: number; data: Uint8Array }
}