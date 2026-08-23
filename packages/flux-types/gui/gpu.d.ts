// Low-level GPU textures and shaders (gui-enabled runtime only). The
// imperative primitive; @solidrt/core's gpu helpers add reactive auto-cleanup
// on top. Each id space has its own destroyer: texture ids (the public token
// used as `<texture src>` and sampler inputs -> destroyTexture), buffer ids
// (-> destroyBuffer), and the raw shading layer's shader-stage ids
// (-> destroyShader), program ids (-> destroyProgram), and render-pipeline
// ids (-> destroyRenderPipeline).
// Layering: compileShader/linkProgram are the raw GL primitives (complete
// sources, explicit header opt-in); createRenderPipeline pairs a program with
// draw state (topology, blend, cull, depth, vertex layout - how it draws);
// createShaderTarget builds a texture-backed target over a pipeline (size,
// buffer, uniforms, clear - where it draws); createDrawTarget holds an
// ordered, mutable LIST of such draws in one target (addDraw/removeDraw +
// per-entry setters, sharing one depth buffer) - the multi-pass render pass.
// createShaderTexture/createPipelineTexture are fused conveniences (compile +
// link + pipeline + target in one call, curated preamble) - named for what
// they return.
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
   * A draw-entry handle on a draw target ({@link addDraw}); see
   * {@link TextureId} for the brand model. Target-scoped and stable: an id
   * keeps naming its entry across other adds and removes (never an index),
   * and a removed entry's id errors from then on rather than aliasing.
   */
  export type DrawId = number & { readonly __draw: unique symbol }
  /**
   * Shader uniform values by name. A number drives a scalar uniform (`float`,
   * or `int`/`bool`, truncated); a flat number array drives a typed uniform
   * whose declared GLSL type sets the expected length: 2/3/4 for
   * `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`. An array uniform
   * (`vec3 uLight[4]`) goes by its bare name and takes one flat array of
   * element length times array size (12 here; a light list or palette is one
   * write). Dispatch follows the
   * shader's own declaration, and every write is validated against it at the
   * call site: a name with no active uniform, a value whose length does not
   * fit the declared type, a `sampler2D` named here (samplers bind via
   * `textures`), or a value that is not a number / number array throws.
   * A uniform that is declared but optimized out by the compiler is accepted
   * with a warning and the write is skipped, so one param object can drive
   * shader variants that do not all use every uniform.
   * An `undefined` value is skipped, so conditional spreads stay usable.
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
   * A free-form debug name every create accepts (WebGPU's label): surfaced by
   * the dev server's GPU inventory (get_gpu_resources) and in engine log
   * messages, so a chain of targets reads as "bloom-h samples particle-verts"
   * instead of anonymous ids. Not unique, never interpreted; set at create,
   * kept across id-stable resizes ({@link resizeTexture},
   * {@link setTargetSize}).
   */
  export type LabelOption = { label?: string }
  /**
   * Pixel format of an uploaded texture, declared at creation and fixed for
   * the id's lifetime (like the sampler state). "rgba8" (default) is 4 bytes
   * per pixel. "r8" is the single-channel format - 1 byte per pixel - for
   * palette-indexed or grayscale content: upload raw indices and look the
   * color up in the shader, so palette effects are a palette-texture write
   * instead of touching every pixel. Any width works (no 4-byte row padding;
   * the engine uploads r8 at unpack alignment 1). A shader samples an r8
   * texture as `(v, 0, 0, 1)` - read `.r`; displaying one via `<texture src>`
   * shows that same red-channel reading. Shader/pipeline targets and
   * readbacks stay RGBA8.
   */
  export type TextureFormat = "rgba8" | "r8"
  export type TextureFormatOption = { format?: TextureFormat }
  /**
   * This device's hard ceilings, queried once at startup: process constants.
   * Every create and bind validates against them at the call site, so an
   * oversize target throws naming the limit instead of failing later as a
   * driver error, and a binding list past the unit cap throws instead of
   * silently sampling garbage. Values at or below these are safe on this
   * device; the GLES 3.0 floors (2048 / 16 / 16) are the portable baseline
   * every device guarantees.
   */
  export let limits: {
    /** Largest width/height of any texture or render target, in pixels (>= 2048). */
    maxTextureSize: number
    /**
     * Sampler inputs one pass may bind (>= 16): a target's `textures`
     * entries; on a window shader the runtime-filled `uSource` (and
     * `uPrevious` when declared) count toward it too.
     */
    maxTextureUnits: number
    /** Vertex attributes one pipeline may declare (>= 16). */
    maxVertexAttribs: number
  }
  /**
   * Create an immutable texture from a pixel buffer (exactly
   * width*height*bytesPerPixel bytes: *4 for the default "rgba8" format, *1
   * for "r8"). Returns the texture id.
   */
  export function createTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions & TextureFormatOption & LabelOption): TextureId
  /**
   * Create a texture intended to be updated later via {@link uploadTexture}. The
   * seed buffer must hold at least one frame (width*height bytes at the
   * declared format's pixel size) and may hold more (uploadTexture selects a
   * frame by offset).
   */
  export function createMutableTexture(data: Uint8Array, width: number, height: number, opts?: SamplerOptions & TextureFormatOption & LabelOption): TextureId
  /**
   * Replace a mutable texture's pixels; the frame size follows the format the
   * id was created with. `data` may hold several frames; `offset` (default 0)
   * selects which frame to upload.
   */
  export function uploadTexture(id: TextureId, data: Uint8Array, offset?: number): void
  /**
   * Replace a texture's storage with a new size at the same id (an id-stable
   * resize): `<texture src>` references and shader sampler bindings keep
   * working, and shaders sampling the texture re-render. `data` seeds the new
   * contents and, like {@link createMutableTexture}, must hold at least one
   * frame at the id's format (which survives the resize, like the sampler
   * state). Render target ids are rejected - resize those with
   * {@link setTargetSize}.
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
   * size. `params` sets initial uniforms by name (see {@link ShaderParams}
   * for the value shapes and the validation contract - a typo'd name throws
   * here, at the create). It is its own argument, not an option, because it
   * is the initial value of a live channel - the same values the `<texture
   * params>` prop and {@link setTargetParams} drive later; pass `null` (or
   * omit it) for a shader with none. `opts.textures` binds sampler2D
   * uniforms to texture ids - any texture id, including another
   * shader/pipeline target's output, under a name that must be an active
   * `sampler2D` uniform. Bound targets are live dependencies: when a source
   * re-renders (its params, geometry, or data change), every target sampling
   * it re-renders too, transitively through chains, before the next frame or
   * readback - no per-frame uniform write is needed to keep a chain current.
   * Returns the resulting texture id. The fused convenience: one call
   * compiles a program and creates a target over it, and the program lives
   * and dies with the target. To share one compile across targets (or hold a
   * program with no target yet), use the raw layer: {@link compileShader} +
   * {@link linkProgram} + {@link createRenderPipeline} +
   * {@link createShaderTarget}.
   *
   * The preamble (`#version 300 es`, precision, `vUV`, `iResolution`,
   * `fragColor`) is injected only into sources that do not declare their own
   * `#version` line, and declares exactly what the runtime provides - an
   * app-driven uniform (a time value, say) is the source's own declaration,
   * driven through params like any other. A source that starts with
   * `#version 300 es` is compiled exactly as written, so a shader with its
   * own uniform names (a port from elsewhere) needs no rewriting and no drop
   * to the raw layer. The built-in vertex stage still supplies `vUV` to a
   * complete source; declare `in vec2 vUV;` yourself to read it. Same rule on
   * {@link createPipelineTexture}. A complete source may also declare
   * `iResolution` as vec3 (a common convention in ported shaders); it is
   * then filled as `(w, h, 1.0)`. One naming trap: GLSL ES reserves `packed` as a keyword,
   * so `vec4 packed = texture(...)` fails with a syntax error that does not
   * name the identifier - pick another name.
   */
  export function createShaderTexture(
    fragmentSrc: string,
    width: number,
    height: number,
    params?: ShaderParams | null,
    opts?: { textures?: Record<string, TextureId> } & SamplerOptions & LabelOption,
  ): TextureId
  /**
   * Compile a single shader stage from raw GLSL ES: the primitive under
   * {@link linkProgram}, GL's own model (a "shader" is one stage; linking
   * stages yields a program). The source is complete - it declares its own
   * `#version 300 es`, precision, varyings and uniforms; nothing is injected.
   * With `header: true` the standard header is prepended explicitly: `#version
   * 300 es`, `precision highp float;`, `uniform vec2 iResolution;`, plus
   * `out vec4 fragColor;` for a fragment stage (the same text
   * {@link createPipelineTexture} injects). Do not combine `header` with your
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
  export function linkProgram(vertexShader: ShaderStageId, fragmentShader: ShaderStageId, opts?: LabelOption): ProgramId
  /**
   * Destroy a compiled stage by id. Programs linked from it are unaffected.
   */
  export function destroyShader(id: ShaderStageId): void
  /**
   * Pair a linked program with draw state, returning a render pipeline id
   * (its own id space, like programs and buffers - not a texture id): the
   * pipeline state object of every modern GPU API. The pipeline owns HOW its
   * targets draw - `attributes` (the interleaved vertex layout; omit for
   * attributeless rendering via gl_VertexID), `instanceAttributes` (the
   * per-instance layout, fetched from each entry's `instanceBuffer` - see
   * the option's doc), `topology`, `blend`, `cull`, `depth`, `depthWrite`
   * (`false` requires `depth: true`) - while each target brings its own
   * size, buffers, uniforms, and clear. Both layouts share one attribute
   * namespace (each name is one `in` of the vertex stage), so a name in
   * both lists throws. Creating a pipeline compiles nothing, and many
   * pipelines may share one program. The vocabulary is validated here, so a
   * bad word throws at this call site. Free with
   * {@link destroyRenderPipeline}; the program is yours and outlives it.
   */
  export function createRenderPipeline(
    program: ProgramId,
    opts?: {
      attributes?: VertexAttribute[]
      /**
       * One interleaved record per INSTANCE (WebGPU's `stepMode:
       * "instance"`): these attributes read from the entry's
       * `instanceBuffer` and advance per instance instead of per vertex, so
       * every vertex of instance N sees record N - real per-instance state
       * (offsets, colors, a packed transform) with no `gl_InstanceID`
       * arithmetic. Declaring any makes `instanceBuffer` required on every
       * entry drawn with this pipeline. A mat4 per instance is its four
       * vec4 columns, reassembled in the shader (attributes have no matrix
       * formats, as in WebGPU). Instance N always reads record N of the
       * entry's buffer, from record 0 (ES 3.0 has no base instance), so
       * several independently culled groups cannot share one buffer as
       * sub-ranges: give each group its own `instanceBuffer` and entry, and
       * cull it by `instanceCount`.
       */
      instanceAttributes?: VertexAttribute[]
      topology?: Topology
      blend?: BlendMode
      cull?: CullMode
      depth?: boolean
      depthWrite?: boolean
    } & LabelOption,
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
   * render it once: the target half of {@link createPipelineTexture}. Returns
   * a texture id exactly like the fused creates do (drive uniforms
   * via the `params` prop or {@link setTargetParams}, resize with
   * {@link setTargetSize}, destroy with {@link destroyTexture}). Many targets
   * may share one pipeline, and creating a target compiles nothing. `buffer`
   * supplies the concrete vertex buffer the pipeline's attribute layout
   * describes (required when the pipeline declares attributes), and
   * `instanceBuffer` the per-instance records its `instanceAttributes`
   * describe (required exactly when it declares any); the
   * {@link DrawRange} keys pick what is drawn from them - `vertexCount`
   * defaults to the rest of the buffer from `firstVertex` on,
   * `instanceCount` to one instance per instance-buffer record (1 without
   * one) - and a fetch past either buffer's end throws here. A fullscreen
   * pass over an attributeless pipeline is `vertexCount: 3` with a
   * covering-triangle vertex stage. Draw-state keys
   * (`attributes`, `topology`, `blend`, `depth`, `depthWrite`) belong to the
   * pipeline and throw here. `params` and `textures` are validated against
   * the pipeline's program (see {@link ShaderParams}).
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
    params?: ShaderParams | null,
    opts?: {
      textures?: Record<string, TextureId>
      buffer?: BufferId
      instanceBuffer?: BufferId
      clearColor?: [number, number, number, number]
      render?: "auto" | "manual"
      loadOp?: "clear" | "load"
    } & (DrawRange | (IndexBinding & IndexRange)) &
      SamplerOptions &
      LabelOption,
  ): TextureId
  /**
   * Destroy a linked program by id. Pipelines created from it are unaffected:
   * each holds the program until it is itself destroyed, so either
   * destruction order is safe. The id stops being usable for new pipelines
   * immediately.
   */
  export function destroyProgram(id: ProgramId): void
  export type Topology = "points" | "lines" | "line-strip" | "triangles" | "triangle-strip"
  /**
   * Blending for a pipeline's own draw. "none" (default) overwrites:
   * overlapping geometry resolves by depth or draw order. "add" accumulates
   * (glBlendFunc(ONE, ONE)): order-independent, so geometry needs no sorting
   * - the additive half of translucency (point splats, glow passes). A
   * depth-tested additive pass usually pairs with `depthWrite: false`; with
   * writes on, unsorted geometry depth-rejects its own later fragments and
   * accumulation becomes draw-order-dependent. That pairing is the app's to
   * state - neither option implies the other. "multiply" scales
   * (glBlendFunc(DST_COLOR, ZERO)): each fragment multiplies what is already
   * in the target, all four channels, so it darkens where "add" brightens -
   * a projected shadow, a dust pass. Order-independent like "add", same
   * `depthWrite: false` pairing. On the premultiplied target a uniform factor
   * across rgb and alpha fades the existing pixels; alpha 1 with rgb below 1
   * darkens color only - so a strength-weighted shadow is
   * `vec4(mix(vec3(1.0), shadowColor, strength), 1.0)`, not alpha =
   * strength (that fades instead). "alpha" composites OVER
   * (glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA)): classic translucency, with the
   * fragment written premultiplied like every target pixel -
   * `vec4(color * a, a)`, never straight rgb with a loose alpha. It is the
   * one order-DEPENDENT mode: the result follows draw-list order, so
   * translucent geometry must land back-to-front - by draw-list ordering
   * (`before`, `setDrawOrder`) or a sorting layer above - and normally after
   * the opaque draws with `depthWrite: false`, so it depth-tests against them
   * without occluding what it only tints. Nothing sorts for you here.
   */
  export type BlendMode = "none" | "add" | "multiply" | "alpha"
  /**
   * Face culling for a pipeline's draws. "none" (default) rasters both faces
   * - the two-sided fallback open surfaces need. "back" discards faces wound
   * away from the camera, halving a closed mesh's fragment work; "front"
   * discards the other set (shadow and inside-out tricks). The winding rule
   * is WebGPU's, fixed: counter-clockwise AS DISPLAYED (screen coordinates,
   * y down) = front. Measured after every flip, so it just works: a mesh
   * exported counter-clockwise-front for a y-up world, drawn through a
   * standard right-handed camera (looking down -z) with the usual y
   * negation for display, culls correctly with "back". If "back" shows you
   * the mesh's inside anyway, the winding reaching the screen is mirrored -
   * either the exporter winds clockwise, or the hand-rolled projection is
   * left-handed (the classic: camera looking toward +z without mirroring
   * x). Fix the rig, flip the exporter, or use "front".
   */
  export type CullMode = "none" | "back" | "front"
  /**
   * One float attribute of an interleaved record - a vertex of `attributes`
   * or an instance record of `instanceAttributes`. The list's order defines
   * the byte layout; locations are resolved by name against the vertex
   * shader's `in` declarations.
   */
  export type VertexAttribute = { name: string; format: "f32" | "vec2" | "vec3" | "vec4" }
  /**
   * A pipeline target's draw as data, WebGPU-style: `firstVertex` +
   * `vertexCount` pick the vertex range `[firstVertex, firstVertex +
   * vertexCount)` of the buffer, `instanceCount` draws that range as N
   * instances (`glDrawArraysInstanced`) told apart by `gl_InstanceID` (and
   * by their `instanceAttributes` records, when the pipeline declares any).
   * All keys optional: at create, `firstVertex` defaults to 0, `vertexCount`
   * to the rest of the buffer, and `instanceCount` to one instance per
   * record of the entry's `instanceBuffer` - 1 without one, the plain draw;
   * in {@link setDraw}, absent keys keep their current value.
   * `instanceCount: 0` draws nothing - a cheap off switch. With an instance
   * buffer bound, `instanceCount` is bounds-checked against it like every
   * fetch (instances 0..N-1 each read one record). Two GL facts worth
   * knowing: `gl_VertexID` includes `firstVertex` (as in WebGPU), and
   * `gl_InstanceID` always counts from 0 - ES 3.0 has no base instance, so
   * instance N reads record N of the entry's `instanceBuffer` and a group
   * that is culled independently needs its own buffer and entry.
   */
  export type DrawRange = { firstVertex?: number; vertexCount?: number; instanceCount?: number }
  /**
   * The element type of an index buffer: "uint16" halves index bandwidth and
   * addresses meshes up to 65535 vertices, "uint32" covers the rest -
   * WebGPU's two formats exactly.
   */
  export type IndexFormat = "uint16" | "uint32"
  /**
   * An entry's index binding: any {@link createBuffer} buffer plus its
   * element type (the buffer is typeless bytes, so the format must be
   * declared - as WebGPU does at setIndexBuffer). One buffer kind serves
   * both roles; there is no separate index-buffer create. With a binding
   * present the draw is `glDrawElements`: vertices are fetched through the
   * index VALUES, so shared vertices are stored (and shaded) once, and the
   * range speaks {@link IndexRange} instead of {@link DrawRange}. The
   * index-buffer fetch is bounds-checked like every range; the index values
   * themselves are not checked against the vertex buffer (that would mean
   * reading them back) - an out-of-range index is the same undefined fetch
   * raw GL gives you.
   */
  export type IndexBinding = { indexBuffer: BufferId; indexFormat: IndexFormat }
  /**
   * The index-counted spelling of a draw range, for indexed entries
   * (WebGPU's drawIndexed vocabulary): `firstIndex` + `indexCount` pick the
   * range of the INDEX buffer, `instanceCount` as in {@link DrawRange}.
   * Same defaults and merge rules; the vertex-named keys throw on an
   * indexed entry (and these throw on a plain one), so a range never
   * silently counts the wrong thing. `gl_VertexID` reads the index value;
   * there is no base vertex (ES 3.0, like ES 3.0's missing base instance).
   */
  export type IndexRange = { firstIndex?: number; indexCount?: number; instanceCount?: number }

  /**
   * Compile a GLSL ES vertex+fragment pipeline into an offscreen texture of
   * the given size and render it once. Sources without a `#version` line get
   * a 300 es preamble declaring `iResolution` (no vUV: varyings are the
   * pipeline's own; app-driven uniforms are the source's own declarations).
   * Clip space is y-down: `gl_Position` y = -1 is the top
   * row of the target and +1 the bottom, so camera-up geometry must negate y
   * (or fold the flip into its projection) to display up. `attributes`
   * describes one interleaved vertex in `buffer` (a {@link createBuffer} id);
   * omit both for attributeless rendering via gl_VertexID.
   * `instanceAttributes` describes one per-instance record in
   * `instanceBuffer` (see {@link createRenderPipeline}; declare both or
   * neither). The {@link DrawRange} keys pick what is drawn: `vertexCount`
   * defaults to the rest of the buffer from `firstVertex` on,
   * `instanceCount` draws the range as N instances told apart by
   * `gl_InstanceID` and defaults to one per instance-buffer record; a fetch
   * past either buffer's end throws. With
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
   * the `params` prop or {@link setTargetParams}, destroy with
   * {@link destroyTexture}.
   */
  export function createPipelineTexture(
    vertexSrc: string,
    fragmentSrc: string,
    width: number,
    height: number,
    params?: ShaderParams | null,
    opts?: {
      textures?: Record<string, TextureId>
      attributes?: VertexAttribute[]
      buffer?: BufferId
      /** See {@link createRenderPipeline}'s `instanceAttributes`. */
      instanceAttributes?: VertexAttribute[]
      instanceBuffer?: BufferId
      topology?: Topology
      depth?: boolean
      depthWrite?: boolean
      blend?: BlendMode
      cull?: CullMode
      clearColor?: [number, number, number, number]
      render?: "auto" | "manual"
      loadOp?: "clear" | "load"
    } & (DrawRange | (IndexBinding & IndexRange)) &
      SamplerOptions &
      LabelOption,
  ): TextureId

  /**
   * Create a vertex buffer from raw bytes (interleave attribute data to match
   * the pipeline's attribute list), or from a byte length alone - a zeroed
   * buffer, the natural create when the contents arrive through the write
   * lease ({@link beginBufferWrite}). Buffer ids are their own space,
   * separate from texture ids. Size is fixed for the id's lifetime: reserve
   * the maximum up front and publish a prefix.
   */
  export function createBuffer(data: Uint8Array | number, opts?: LabelOption): BufferId
  /**
   * Open a zero-copy write into a vertex buffer: returns an ArrayBuffer over
   * runtime-owned memory exactly the buffer's size. Write into it in place
   * (wrap it in a Float32Array or any view), then publish with
   * {@link endBufferWrite} - no copy happens anywhere on the CPU path.
   *
   * Contents are UNSPECIFIED at begin: a recycled block holds what was
   * published the time before last, so fill everything you publish. One open
   * write per buffer id at a time (a second begin throws). The view is
   * detached at end/destroy - a retained reference becomes zero-length, and
   * writes through it are inert, never a race.
   */
  export function beginBufferWrite(id: BufferId): ArrayBuffer
  /**
   * Publish the open write's first `byteLength` bytes at offset 0 (default:
   * the whole buffer) and close the lease. `byteLength` 0 cancels: the lease
   * closes and nothing is published. Always closes the lease, error or not;
   * throws when no write is open or `byteLength` exceeds the buffer size.
   * Pipelines drawing from the buffer re-render, like {@link writeBuffer}.
   */
  export function endBufferWrite(id: BufferId, byteLength?: number): void
  /**
   * Overwrite part of a vertex buffer at `byteOffset` (default 0), within the
   * size it was created with. Pipelines drawing from the buffer re-render
   * with their last-applied params.
   */
  export function writeBuffer(id: BufferId, data: Uint8Array, byteOffset?: number): void
  /**
   * Destroy a vertex buffer. Pipeline textures drawing from it hold their own
   * reference, so destruction order does not matter; further writes to the id
   * throw.
   */
  export function destroyBuffer(id: BufferId): void
  /**
   * Update a pipeline texture's draw range and re-render it: `vertexCount`
   * after writing a variable amount of dynamic geometry into its buffer,
   * `firstVertex` to draw a different window of a shared buffer,
   * `instanceCount` to grow or shrink an instanced population. Keys absent
   * from `draw` keep their current value, like params. Throws if a value is
   * negative or the merged range's vertex fetch would run past the end of
   * the target's buffer ((firstVertex + vertexCount) x vertex stride >
   * buffer size) - the out-of-bounds draw GL itself never checks; a target
   * without vertex fetch (attributeless) accepts any non-negative range.
   * (On a manual target nothing renders here; the range applies at its next
   * {@link renderTarget}.) An indexed target takes the {@link IndexRange}
   * spelling instead, bounds-checked against its index buffer; the pair
   * that does not match the target's mode throws.
   */
  export function setDraw(id: TextureId, draw: DrawRange | IndexRange): void
  /**
   * Create a draw target: a render target whose contents are an ordered,
   * mutable LIST of draws - one render clears once, then executes every
   * entry in list order into the same storage. The multi-pass shape of every
   * 3D API (N meshes, N pipelines, one shared depth buffer), retained: where
   * WebGPU re-encodes a render pass every frame, this target holds the pass
   * as state and re-renders on demand. Entries are added and removed at any
   * time ({@link addDraw}/{@link removeDraw}) and updated per entry
   * ({@link setDrawParams}, {@link setDrawTextures}, {@link setDrawRange}).
   *
   * `depth: true` gives the target its own depth storage, shared by every
   * entry and cleared once per render - this is what makes cross-entry
   * occlusion work. It is the storage half of the depth story; whether an
   * entry tests/writes depth is its pipeline's `depth`/`depthWrite` state,
   * and adding a depth-testing pipeline to a target without storage throws.
   *
   * `params` seeds the target's SHARED params - the target-level values
   * every entry reads, the same live channel {@link setTargetParams} drives
   * later (positional like every create's params; see there for the
   * precedence and validation contract). `opts.textures` seeds the shared
   * sampler bindings the same way, the channel {@link setTargetTextures}
   * drives (in opts like every create's textures). At creation there are no
   * entries to validate against, so names are accepted as-is and simply
   * apply to whichever later entries' programs declare them.
   *
   * The render contract is unchanged: the list is input data like params, so
   * "render twice = render once" still holds and the default `render:
   * "auto"` target re-renders exactly when its entries or their inputs
   * change - a static scene costs zero passes, however many entries it
   * holds, and one render is ONE pass however many entries it draws.
   * `render: "manual"` and `loadOp: "load"` compose exactly as on
   * {@link createShaderTarget}. With no entries a render is the clear alone.
   * Returns a texture id (display, resize, destroy like any target; entries
   * die with it).
   */
  export function createDrawTarget(
    width: number,
    height: number,
    params?: ShaderParams | null,
    opts?: {
      depth?: boolean
      textures?: Record<string, TextureId>
      clearColor?: [number, number, number, number]
      render?: "auto" | "manual"
      loadOp?: "clear" | "load"
    } & SamplerOptions &
      LabelOption,
  ): TextureId
  /**
   * Append a draw entry to a draw target: `pipeline` draws `opts.buffer`
   * (required when the pipeline declares attributes) with its own `params`
   * and `textures`, last in list order - the same per-entry shape
   * {@link createShaderTarget} takes, addressed to one entry of the list.
   * Returns the entry's {@link DrawId}, the handle every per-entry update
   * takes. Everything validates here at the call site: unknown ids, depth
   * compatibility (see {@link createDrawTarget}), uniform names and arities,
   * the vertex-fetch bound, per-entry texture-unit count, and sampling
   * cycles. List order is draw order - later entries land over earlier ones
   * where depth does not decide - so painter-style layering is append order,
   * and per-entry `params` is where per-object state (a model matrix) lives.
   * `before` inserts the entry immediately before an existing one instead
   * of appending (it must name a live entry); for wholesale reordering use
   * {@link setDrawOrder}. An {@link IndexBinding} makes the entry draw
   * indexed - real meshes share most vertices, and indexing stores and
   * shades each one once - with the range in {@link IndexRange} spelling.
   * `instanceBuffer` supplies the per-instance records the pipeline's
   * `instanceAttributes` describe (required exactly when it declares any);
   * `instanceCount` then defaults to one instance per record.
   *
   * Seed every uniform the entry's program declares - here, via the
   * target's shared params, or with a later write. GL uniform state lives
   * on the program object, so a declared name nothing writes holds
   * whatever the last draw through that program applied, from any entry
   * or target sharing it - not zero (only a freshly linked program reads
   * the link-time zero). Coverage is deliberately not validated here:
   * adding entries first and setting shared values after is legal.
   */
  export function addDraw(
    target: TextureId,
    pipeline: RenderPipelineId,
    params?: ShaderParams | null,
    opts?: {
      textures?: Record<string, TextureId>
      buffer?: BufferId
      instanceBuffer?: BufferId
      before?: DrawId
    } & (DrawRange | (IndexBinding & IndexRange)),
  ): DrawId
  /**
   * Remove a draw entry from a draw target. Remaining entries keep their
   * order and ids; the removed id errors from then on (ids are never
   * reused). The entry's pipeline and buffer are yours and unaffected.
   */
  export function removeDraw(target: TextureId, draw: DrawId): void
  /**
   * Update one draw entry's uniforms by name: {@link setTargetParams}
   * addressed to a single entry, same merge and validation contract. The
   * per-object hot path - a moved mesh is one setDrawParams with its new
   * model matrix.
   */
  export function setDrawParams(target: TextureId, draw: DrawId, params: ShaderParams): void
  /**
   * Update a target's target-level uniforms by name, on any target kind,
   * with the usual merge-by-name (see {@link ShaderParams} for value shapes;
   * a bad name or a mismatched length throws here, on the line that wrote
   * it). On a single-program target (a fragment texture or a pipeline
   * target) the target level IS its one pass: every name validates against
   * that program and the target re-renders. On a manual target nothing
   * renders here; the values apply at its next {@link renderTarget}.
   *
   * On a draw target these are the SHARED params: values every entry reads
   * - a camera's view-projection above all - written once per target
   * instead of once per entry. Shared values apply at render before each
   * entry's own params, so an entry naming the same uniform overrides the
   * shared value (specific beats general), and they are target state: entry
   * add/remove/rebuild cannot lose them. A draw target legitimately mixes
   * material classes, so coverage may be partial: a name only some entries'
   * programs declare is applied where declared and skipped elsewhere - down
   * to zero coverage: a name no current entry declares is stored and skips
   * everywhere until a declaring entry arrives, so shared state does not
   * depend on write order (a seed before entries and a write after are the
   * same state). Validation is arity where declared: a name must match the
   * declared component count in every entry program that declares it; an
   * entry added later whose program lacks an already-set name is never a
   * retroactive error, the value just skips it.
   */
  export function setTargetParams(target: TextureId, params: ShaderParams): void
  /**
   * Rebind a target's target-level sampler2D inputs by uniform name, on any
   * target kind - {@link setTargetParams}'s sampler analog. Bindings not
   * named keep their current source, so a single input can be retargeted
   * (post-process source swap, ping-pong between two data textures) without
   * recompiling anything. Bound sources are live dependencies: the target
   * re-renders when one changes. Every path throws if the target or a
   * source texture id is unknown, if a binding names the target's own
   * texture (same-pass feedback), or if it would close a sampling cycle
   * among runtime-rendered targets; a cycle through a `render: "manual"`
   * target is legal - the runtime never renders one, so the loop only steps
   * when the app calls {@link renderTarget}. On a single-program target each
   * name must be an active `sampler2D` of its one program.
   *
   * On a draw target these are the SHARED bindings: sources every entry
   * reads - an environment map, a shadow map, a LUT - bound once per
   * target, with the shared-params rules throughout: an entry's own binding
   * for the same name wins; a name only some entries' programs declare
   * binds where declared and is skipped elsewhere, down to zero coverage
   * (an undeclared name is stored, joins the sampler graph, and binds when
   * a declaring entry arrives); shared bindings are target state that entry
   * add/remove/rebuild cannot lose. Each name must be a sampler2D
   * everywhere it is declared, and each entry's effective inputs (its own
   * plus the applicable shared ones) must fit the device's texture units.
   */
  export function setTargetTextures(target: TextureId, textures: Record<string, TextureId>): void
  /**
   * Resize a render target of any kind in place and re-render it: the id,
   * compiled programs, last-applied params, sampler bindings, and draw
   * state all carry over; only the output size changes. The setDraw analog
   * for output size. (Pixel textures resize with {@link resizeTexture},
   * which carries seed pixels instead.)
   */
  export function setTargetSize(id: TextureId, width: number, height: number): void
  /**
   * Rebind one draw entry's sampler2D inputs by uniform name:
   * {@link setTargetTextures} addressed to a single entry, same merge,
   * validation, and cycle rules. Entries bind independently - two entries
   * may bind the same uniform name to different sources.
   */
  export function setDrawTextures(target: TextureId, draw: DrawId, textures: Record<string, TextureId>): void
  /**
   * Update one draw entry's draw range: {@link setDraw} addressed to a
   * single entry, same partial merge, bounds validation, and vocabulary
   * rule (an indexed entry speaks {@link IndexRange}).
   */
  export function setDrawRange(target: TextureId, draw: DrawId, update: DrawRange | IndexRange): void
  /**
   * Reorder a draw target's list. `order` must name every current entry
   * exactly once - a full permutation of the live {@link DrawId}s; a
   * missing, duplicate, or unknown id throws, naming the problem. List
   * order is draw order, which makes this the sorting verb: sort opaque
   * entries front-to-back (early depth rejection) and transparent ones
   * back-to-front, and re-issue the order when the camera moves. Entry
   * state (params, textures, ranges) rides along untouched; ids are
   * unaffected. Like every draw-list write it re-renders an auto target
   * once at the next flush, and folds silently on a manual one.
   */
  export function setDrawOrder(target: TextureId, order: DrawId[]): void
  /**
   * Render a `render: "manual"` target once, now. Renders land in call order
   * relative to every other GPU call: a `setTargetParams`/`writeBuffer`
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
   * Capture a render-tree node's subtree as RGBA8 pixels (tightly packed,
   * top-to-bottom rows - the {@link readTexture} result shape), resolving once
   * it has been rendered on the next paint pass. The node must be attached to
   * the live tree (an unmounted node is never painted, so its capture rejects)
   * and paint a non-zero box. A laid-out node captures its layout box. A `d-*`
   * node has no layout box - that is what detached means - so it captures its
   * painted box instead: its own `w`/`h` when set, else the nearest laid-out
   * ancestor's box (the same box the render tree reports for it), with its
   * `x`/`y` paint offset mapped to the texture origin.
   * Rendered at the current display scale, so `width`/`height` are actual
   * pixel dimensions (ceil(logicalSize * displayScale)), not logical points.
   * No texture is created and there is nothing to free; to display or sample
   * the result, upload it with {@link createTexture}.
   *
   * Intended for one-shot bakes and inspection: turning something the engine
   * can draw but the app cannot compute - shaped text, an SVG, a themed view -
   * into pixels the app processes on the CPU. Baking a glyph atlas by laying
   * out cells, capturing them and keeping the coverage channel is the worked
   * example. Tests and freeze-frames are the same shape.
   *
   * Not a rendering primitive. Every call rasterizes the subtree into a fresh
   * offscreen MSAA target and reads the pixels back to the CPU: a GPU -> CPU
   * readback stall plus a paint pass of latency, per call, with nothing
   * incremental about it. Batch what you capture (many nodes captured together
   * are serviced by one paint pass), and do not drive it per frame or reach
   * for it to feed live content into a shader - an effect over what is beneath
   * it, a backdrop filter. Content that must stay current has to come from a
   * source that updates in place: another pipeline's render target, a camera
   * texture, a mutable texture.
   */
  export function captureSnapshot(nodeId: number): Promise<{ width: number; height: number; data: Uint8Array }>
  /**
   * Read back a registered texture's current pixels as RGBA8 (tightly packed,
   * top-to-bottom rows), for any texture id whatever created it (createTexture,
   * createShaderTexture, a render target). Synchronous. Throws if the id is
   * unknown.
   */
  export function readTexture(id: TextureId): { width: number; height: number; data: Uint8Array }
}