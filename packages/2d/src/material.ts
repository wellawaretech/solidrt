// Materials: GLSL paired with pipeline state, attached to a LAYER - the
// layer is the batch (its atlases bound together, N quads in one draw per
// view), so the material sits on the thing that is one draw entry, as
// @solidrt/3d's sits on the mesh. Two 2d sprites with different materials
// are two layers, which is also how you order them (an additive glow
// layer draws over the scene layer). Deduped hard, the 3d way: one
// program per material CLASS and atlas list met (the fragment set is
// generated per atlas list, spriteSource), one render pipeline per
// instance-layout list met on top (the node layer binds a pose and a
// sprite buffer, the records and tile layers one record buffer; a
// pipeline is program + attribute list, so the program never recompiles
// for another layer kind), created lazily at first use and kept for the
// class's lifetime. A material INSTANCE is the params and sampler
// bindings it contributes to every view and chunk target of its layer.
//
// Three tiers of custom look, the same three as @solidrt/3d's: a custom
// vertex stage under the stock fragment (shaderMaterialClass with
// unlitFragment from ./glsl), a `surface` function inside the stock
// material (unlit's prelude/surface slot over the Sprite struct), or a
// fragment of your own over the layer's set (shaderMaterialClass). Per-
// sprite data for any of them is the material's `instanceBuffers`: an
// app-owned style record per sprite in any vertex format, written with
// setInstanceStyle / instanceAttribute / records(layer, stream), the 3d
// vocabulary unchanged.
//
// `blend` is pipeline state and lives here, as in 3d; the deliberate
// asymmetry is that 3d's `blend` implies `transparent` and the scene's
// back-to-front sort, while a layer has no depth and draws in record
// order, so nothing sorts.
import { compileShader, createBuffer, createRenderPipeline, destroyBuffer, destroyProgram, destroyRenderPipeline, destroyShader, encodeRecord, checkLayout, layoutComponents, layoutKey, linkProgram } from "@solidrt/core/gpu"
import type { BlendMode, BufferId, ProgramId, RenderPipelineId, ShaderParams, TextureBindings, VertexAttribute, VertexBufferLayout } from "@solidrt/core/gpu"
import { forwardedName, LAYER_ATTRIBUTE_NAMES, spriteSource, unlitFragment, unlitVertex, VERTEX_UNIFORMS } from "./glsl.ts"
import type { AtlasShading, UnlitSourceOptions } from "./glsl.ts"

/** What a layer draws with once its material met its atlases and its
 * instance layouts: the pipeline and the unit quad every instance
 * reuses (the class's, shared by every layer of the class). */
export type SpritePipeline = { pipeline: RenderPipelineId; quad: BufferId }

export type Material = {
  /**
   * The pipeline this material draws a layer with, for the layer's
   * atlases (the fragment set is generated for them: one sampler per
   * atlas, the distance-field decodes) and its instance layouts (the
   * layer's own records, the material's instance buffers after them) -
   * lazily created, one program per atlas list and one pipeline per
   * layout list met. The layers call it at creation.
   */
  pipeline(atlases: readonly AtlasShading[], layouts: readonly VertexAttribute[][], label: string): SpritePipeline
  /** The uniform values this material seeds on every target of its
   * layer (a view, a tile chunk); the layer's own params apply over them. */
  params: ShaderParams
  /** Sampler bindings beyond the atlases (a palette LUT, a noise
   * texture), bound on every target of the layer. They share the
   * device's sampler budget with the atlases (limits.maxTextureUnits). */
  textures?: TextureBindings
  /** How the layer's sprites blend into its targets: pipeline state,
   * fixed with the material. */
  blend: BlendMode
  /** The instance buffers the pipeline declares after the layer's own
   * (shaderMaterialClass's `instanceBuffers`): one per-sprite style
   * record stream each, app-owned, in any vertex format. */
  instanceBuffers?: VertexBufferLayout[]
  /** The style record a fresh sprite starts with (one value per
   * component of the FIRST instance buffer's attributes, in order);
   * absent = zeros. */
  instanceStyle?: ArrayLike<number>
  /** Present on materials that own their pipeline (shaderMaterial). */
  dispose?(): void
}

export type ShaderMaterialClassOptions = {
  /**
   * Vertex stage GLSL. MUST declare and use `uniform vec4 uCamera`,
   * `uniform vec4 uCameraRot` and `uniform vec2 uViewport` (the view's
   * camera mapping, written per view) - a source mentioning any of them
   * not at all throws right here - and write the varyings the fragment
   * set reads (SPRITE_VARYINGS from `@solidrt/2d/glsl`, SPRITE_VERTEX_BODY
   * writes them). Declare any of the layer's record attributes by name
   * (iPos, iRot, iScale, iUv, iTint, iScreenPx, iAtlas, iOutline - the
   * same names on every layer kind) and your own instance buffers';
   * undeclared ones are skipped. SPRITE_VERTEX is the stock stage.
   */
  vertex: string
  /**
   * Fragment stage GLSL over the layer's SET, which the layer prepends:
   * the varyings, the atlas samplers, `uTint`, the `Sprite` struct,
   * `spriteOf()` and `spriteSample(uv)` (spriteSource in `./glsl`) are
   * declared for you - declare none of them. Start main with `Sprite s
   * = spriteOf();` (the derivatives every tap uses are taken there);
   * `unlitFragment(options)` is the stock one.
   */
  fragment: string
  /**
   * Instance buffers: one layout per per-sprite buffer (`{ attributes
   * }`: any vertex format, tightly packed in list order; stepMode is
   * "instance" by definition here), bound after the layer's own records.
   * The vertex stage reads the attributes as `in` variables beside the
   * record's, and each drawn sprite gets one record from each buffer:
   * the material's STYLE record(s), written with setInstanceStyle (the
   * first buffer), instanceAttribute + updateRecords (any), or
   * records(layer, stream) in bulk. A palette index is a `uint8x4` at
   * four bytes, a dissolve amount a `float16x2`. Names may not reuse the
   * layer's own (LAYER_ATTRIBUTE_NAMES).
   */
  instanceBuffers?: VertexBufferLayout[]
  /** The style record a fresh sprite starts with (one value per
   * component of the first instance buffer's attributes, in order);
   * default zeros. The identity of whatever the stage does with the
   * record - white for a multiplied tint. */
  instanceStyle?: ArrayLike<number>
  /**
   * How the sprites blend into the layer's targets, core gpu's
   * BlendMode; default "alpha" (premultiplied source-over in draw order,
   * the fragment writing premultiplied output). "add" for glows,
   * explosions and additive particles, "multiply" to darken, "none" to
   * overwrite.
   */
  blend?: BlendMode
  label?: string
}

/** The instance half of a shader material: uniform seeds and sampler
 * bindings for one parameterisation of a class's program. */
export type ShaderMaterialInstanceOptions = {
  /** Uniform seeds beyond the standard set (a prelude's uniforms);
   * update per layer later with setLayerParams. */
  params?: ShaderParams
  /** Sampler bindings beyond the atlases; names may not be an atlas
   * sampler's (uAtlasN). */
  textures?: TextureBindings
}

export type ShaderMaterialOptions = ShaderMaterialClassOptions & ShaderMaterialInstanceOptions

/**
 * One program and pipeline, many parameterisations: the class/instance
 * split unlit has internally, for your own GLSL. `instance()` returns a
 * Material sharing the class's pipelines with its own params/textures -
 * the class compiles once per atlas list met, and dispose() is on the
 * class alone (instances hold nothing of their own).
 */
export type ShaderMaterialClass = {
  instance(opts?: ShaderMaterialInstanceOptions): Material
  /** Destroy the shared programs, pipelines and quad. Layers still
   * drawing with an instance draw nothing valid afterwards. */
  dispose(): void
}

// The atlas sampler names a material texture may not take.
const ATLAS_UNIFORM = /^uAtlas\d+$/

/** The key of an atlas list for the program cache: the shading of each
 * atlas in order (a colour atlas, or a field with its range), which is
 * exactly what spriteSource varies on. */
function atlasKey(atlases: readonly AtlasShading[]): string {
  return atlases.map(a => (a.sdf === undefined ? "rgb" : "sdf" + a.sdf.range)).join(",")
}

/** The texture bindings' validation, shared by the instance forms:
 * a name an atlas sampler takes is a throw. */
function checkTextures(site: string, textures: TextureBindings | undefined): void {
  if (textures === undefined) return
  for (let name of Object.keys(textures)) {
    if (ATLAS_UNIFORM.test(name)) throw new Error(`${site}: texture '${name}' is an atlas sampler's name; the layer binds those itself`)
  }
}

/**
 * A material class from your own GLSL: sources without a `#version` line
 * get the standard pipeline preamble (`fragColor`, `iResolution`), the
 * fragment behind the layer's generated set. Two calls with identical
 * sources compile two programs - there is no dedupe by source value (a
 * hidden cache keyed by content is the anti-pattern the GPU layer avoids
 * throughout); the class IS the app-owned split. Create one per program
 * at app scope, `instance()` per look, and `dispose()` the class when
 * the app is done with the look for good.
 */
export function shaderMaterialClass(opts: ShaderMaterialClassOptions): ShaderMaterialClass {
  // The camera contract, checked where the mistake is made: a vertex
  // stage that never mentions the mapping uniforms cannot place sprites,
  // and with shared params skipping undeclared names the omission would
  // otherwise surface as a blank view, not an error.
  for (let name of VERTEX_UNIFORMS) {
    if (!new RegExp("\\b" + name + "\\b").test(opts.vertex)) {
      throw new Error(`shaderMaterial vertex stage must declare and use '${name}' (the camera mapping; see SPRITE_VARYINGS in @solidrt/2d/glsl)`)
    }
  }
  // Records are tightly packed like the layer's own; explicit strides and
  // offsets are the engine's, reachable through core's own pipeline API.
  // An unknown format would otherwise make a NaN stride at the layer.
  for (let layout of opts.instanceBuffers ?? []) {
    if (layout.stepMode === "vertex") throw new Error("shaderMaterial instanceBuffers are instance-step by definition; drop stepMode")
    if (layout.arrayStride !== undefined || layout.attributes.some(a => a.offset !== undefined)) {
      throw new Error("shaderMaterial instanceBuffers are tightly packed records; drop arrayStride and offset")
    }
    checkLayout(layout.attributes, "shaderMaterial instanceBuffers")
    for (let attr of layout.attributes) {
      if (LAYER_ATTRIBUTE_NAMES.includes(attr.name)) throw new Error(`shaderMaterial instance attribute '${attr.name}' is one of the layer's own record attributes; pick another name`)
    }
  }
  // An empty list declares nothing - same as absent.
  let instanceBuffers = opts.instanceBuffers?.length ? opts.instanceBuffers.map(b => ({ stepMode: "instance" as const, attributes: b.attributes.map(a => ({ ...a })) })) : undefined
  if (opts.instanceStyle !== undefined) {
    if (instanceBuffers === undefined) throw new Error("shaderMaterial instanceStyle needs an instance buffer to describe")
    // Encoded once here so a wrong length throws at creation, not at the
    // first addSprite.
    encodeRecord(instanceBuffers[0]!.attributes, opts.instanceStyle, "shaderMaterial instanceStyle")
  }
  let blend = opts.blend ?? "alpha"
  let label = opts.label ?? "shader-material"
  let quad: BufferId | undefined
  let programs = new Map<string, ProgramId>()
  let pipelines = new Map<string, RenderPipelineId>()
  let quadFor = (): BufferId => {
    // One unit quad (triangle strip), reused by every instance of every
    // layer of the class.
    if (quad === undefined) quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), { label: `${label}-quad`, autoFree: false })
    return quad
  }
  let programFor = (atlases: readonly AtlasShading[]): ProgramId => {
    let key = atlasKey(atlases)
    let program = programs.get(key)
    if (program === undefined) {
      let vs = compileShader("vertex", opts.vertex, { header: true })
      let fs = compileShader("fragment", spriteSource(atlases) + opts.fragment, { header: true })
      program = linkProgram(vs, fs, { label })
      destroyShader(vs)
      destroyShader(fs)
      programs.set(key, program)
    }
    return program
  }
  let pipelineFor = (atlases: readonly AtlasShading[], layouts: readonly VertexAttribute[][], layerLabel: string): SpritePipeline => {
    let key = atlasKey(atlases) + "|" + layouts.map(l => layoutKey(l)).join("|")
    let pipeline = pipelines.get(key)
    if (pipeline === undefined) {
      pipeline = createRenderPipeline(programFor(atlases), {
        label: `${label}-${layerLabel}`,
        topology: "triangle-strip",
        buffers: [{ attributes: [{ name: "aPos", format: "float32x2" }] }, ...layouts.map(attributes => ({ stepMode: "instance" as const, attributes })), ...(instanceBuffers ?? [])],
        blend,
      })
      pipelines.set(key, pipeline)
    }
    return { pipeline, quad: quadFor() }
  }
  return {
    instance(inst = {}) {
      checkTextures("shaderMaterial instance", inst.textures)
      return {
        pipeline: pipelineFor,
        params: inst.params ?? {},
        textures: inst.textures,
        blend,
        instanceBuffers,
        instanceStyle: opts.instanceStyle,
      }
    },
    dispose() {
      for (let pipeline of pipelines.values()) destroyRenderPipeline(pipeline)
      pipelines.clear()
      for (let program of programs.values()) destroyProgram(program)
      programs.clear()
      if (quad !== undefined) {
        destroyBuffer(quad)
        quad = undefined
      }
    },
  }
}

/**
 * A material from your own GLSL: the custom-look escape hatch, first-class
 * next to unlit. A class with a single instance - `shaderMaterialClass()`
 * is the form for one program with many parameterisations. The INSTANCE
 * is the pipeline handle: two calls with identical sources compile two
 * programs. Create one per look at app scope, share it across layers,
 * and `dispose()` it if the app is done with the look for good.
 */
export function shaderMaterial(opts: ShaderMaterialOptions): Material {
  let cls = shaderMaterialClass(opts)
  let material = cls.instance(opts)
  material.dispose = cls.dispose
  return material
}

export type UnlitOptions = Omit<UnlitSourceOptions, "forwards"> & {
  /** See ShaderMaterialClassOptions.blend; default "alpha". */
  blend?: BlendMode
  /** Uniform seeds for the prelude's uniforms (the layer's params apply
   * over them; update later with `layer.setParams`). */
  params?: ShaderParams
  /** Sampler bindings the prelude declares (a palette LUT, a noise
   * texture), beyond the atlases. */
  textures?: TextureBindings
  /**
   * Per-sprite style record(s) the surface reads, see
   * ShaderMaterialClassOptions.instanceBuffers. The stock vertex stage
   * forwards every attribute unchanged as a flat varying - `iPalette`
   * reaches the prelude and the surface as `vPalette` (forwardedName),
   * so the names follow the layer's own `i` + capital convention - and
   * the app writes the records with setInstanceStyle, instanceAttribute
   * or records(layer, stream). A palette index per sprite, a flash
   * amount, a dissolve threshold: the data a look varies on.
   */
  instanceBuffers?: VertexBufferLayout[]
  /** See ShaderMaterialClassOptions.instanceStyle. */
  instanceStyle?: ArrayLike<number>
  label?: string
}

// One shaderMaterialClass per unlit option combination (blend x prelude x
// surface x instance layouts), cached for the app's lifetime like 3d's;
// one program per atlas list and one pipeline per layout list inside
// each.
let unlitClasses = new Map<string, ShaderMaterialClass>()

/**
 * The stock material, the one every layer draws with unless told
 * otherwise: the atlas texel times the sprite's tint (a distance-field
 * atlas decoded to its fill and outline), times the layer tint, blended
 * with `blend`. `prelude`/`surface` is the tier-2 slot: `void
 * surface(inout Sprite s)` called with the struct filled and before the
 * layer tint - a palette lookup, a hit flash, a dissolve, a scrolling
 * `spriteSample` - with the prelude's uniforms as layer params and the
 * `instanceBuffers` records as flat varyings. One program per distinct
 * source, keyed like every option; the name says the shading model,
 * Unity's Sprite-Unlit, which a 2d `lit` would differ on.
 */
export function unlit(opts: UnlitOptions = {}): Material {
  let blend = opts.blend ?? "alpha"
  let prelude = opts.prelude ?? ""
  let surface = opts.surface ?? ""
  let instanceBuffers = opts.instanceBuffers?.length ? opts.instanceBuffers : undefined
  let forwards = instanceBuffers?.flatMap(b => b.attributes) ?? []
  for (let attr of forwards) forwardedName(attr.name)
  let key = [blend, prelude, surface, forwards.map(a => a.name + ":" + a.format).join(","), opts.instanceStyle === undefined ? "" : Array.from(opts.instanceStyle).join(" ")].join("|")
  let cls = unlitClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: unlitVertex(forwards),
      fragment: unlitFragment({ prelude, surface, forwards }),
      instanceBuffers,
      instanceStyle: opts.instanceStyle,
      blend,
      label: opts.label ?? "unlit",
    })
    unlitClasses.set(key, cls)
  }
  return cls.instance({ params: opts.params, textures: opts.textures })
}

/** The sampler count a material binds beyond the atlases: what
 * checkAtlases subtracts from the device's budget. Internal. */
export function materialSamplers(material: Material): number {
  return material.textures === undefined ? 0 : Object.keys(material.textures).length
}

/** The style record a fresh sprite of `material` starts with in its
 * instance buffer `i`: the material's instanceStyle encoded for the
 * first buffer, zeros for every other. Internal - the layers copy it
 * into every new slot. */
export function materialBlank(material: Material, i: number): Uint8Array {
  let layout = material.instanceBuffers![i]!.attributes
  let values = i === 0 && material.instanceStyle !== undefined ? material.instanceStyle : new Array<number>(layoutComponents(layout)).fill(0)
  return encodeRecord(layout, values, "instanceStyle")
}
