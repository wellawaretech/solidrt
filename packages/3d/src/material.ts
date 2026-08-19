// Materials pair GLSL with pipeline state, deduped hard: one program and
// one render pipeline per material CLASS (unlit color, unlit textured),
// created lazily at first use and kept for the app's lifetime. A material
// INSTANCE is just the per-entry uniform values (and sampler bindings) it
// contributes when a mesh becomes a draw entry - so a thousand meshes with
// a thousand colors still share one pipeline.
//
// Colors are straight [r, g, b, a?] 0..1 at the API and premultiplied here
// once, at the boundary (the engine's pixel contract). An alpha below 1
// blends only on a `transparent: true` material (Three's rule: the flag is
// explicit, alpha alone still draws opaque). Transparent materials build
// their pipeline with blend "alpha" and depthWrite off, and the scene draws
// their meshes after the opaque ones, sorted back-to-front per mesh.
//
// Custom looks get the same split through shaderMaterialClass (one
// program, instance() per parameterisation); shaderMaterial is a class with
// a single instance. The raw layer (compileShader / createRenderPipeline in
// @solidrt/core/gpu) stays first-class beneath both.

import {
  compileShader,
  createRenderPipeline,
  destroyProgram,
  destroyRenderPipeline,
  destroyShader,
  glsl,
  linkProgram,
} from "@solidrt/core/gpu"
import type {
  BlendMode,
  CullMode,
  ProgramId,
  RenderPipelineId,
  ShaderParams,
  ShaderStageId,
  TextureId,
  Topology,
  VertexAttribute,
} from "@solidrt/core/gpu"
import { VERTEX_LAYOUTS } from "./geometry.ts"
import type { VertexLayout } from "./geometry.ts"

export type Material = {
  /** The pipeline this material draws with (lazily created). */
  pipeline(): RenderPipelineId
  /** Per-entry uniform values this material contributes at addDraw. */
  params: ShaderParams
  /** Per-entry sampler bindings, when the material samples textures. */
  textures?: Record<string, TextureId>
  /** True when the vertex stage declares `uNormal`: the scene then writes
   * the world matrix's inverse-transpose alongside uModel for meshes using
   * this material (set automatically by shaderMaterial). */
  normalMatrix?: boolean
  /** The vertex layout the pipeline is built for; absent means "standard".
   * shaderMaterial sets "colored" when the vertex stage reads `aColor`. A
   * mesh whose geometry layout differs is rejected at add() - the strides
   * disagree, so a mismatch would render garbage, not just miss a channel. */
  layout?: VertexLayout
  /** True when the pipeline blends over (blend "alpha", depthWrite off):
   * the scene draws this material's meshes after every opaque one, sorted
   * back-to-front by mesh origin, and re-sorts them when the camera moves. */
  transparent?: boolean
  /** Per-instance attributes, when the material's pipeline declares them
   * (shaderMaterialClass's `instanceAttributes`). Such a material draws
   * instanced meshes only - createInstancedMesh supplies the record buffer,
   * and createMesh meshes are rejected at add(). */
  instanceAttributes?: VertexAttribute[]
  /** Present on materials that own their pipeline (shaderMaterial). */
  dispose?(): void
}

// One vertex stage serves every unlit class: model then view-projection,
// plus the UV varying. uModel is per-entry (the scene writes it when the
// mesh moves), uViewProj is target-shared (one write per camera move) - the
// split is what keeps camera motion O(1) instead of O(meshes), and the
// extra per-vertex mat4 multiply is free on the GPU. aNormal from the
// shared layout is deliberately not declared - inactive attributes are
// skipped and only the stride accounts for them.
const VERTEX_SRC = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    gl_Position = uViewProj * uModel * vec4(aPos, 1.0);
    vUv = aUV;
  }
`

const FRAGMENT_COLOR_SRC = glsl`
  uniform vec4 uColor;
  void main() {
    fragColor = uColor;
  }
`

const FRAGMENT_MAP_SRC = glsl`
  in vec2 vUv;
  uniform sampler2D uMap;
  uniform vec4 uColor;
  void main() {
    fragColor = texture(uMap, vUv) * uColor;
  }
`

let sharedVertex: ShaderStageId | undefined
let pipelines: Partial<Record<UnlitClass, RenderPipelineId>> = {}

// One pipeline per unlit CLASS: fragment kind x transparency, since blend
// state is pipeline state.
type UnlitClass = "color" | "map" | "color-transparent" | "map-transparent"

function pipelineFor(cls: UnlitClass): RenderPipelineId {
  let existing = pipelines[cls]
  if (existing !== undefined) return existing
  if (sharedVertex === undefined) sharedVertex = compileShader("vertex", VERTEX_SRC, { header: true })
  let transparent = cls.endsWith("-transparent")
  let fragment = compileShader("fragment", cls.startsWith("color") ? FRAGMENT_COLOR_SRC : FRAGMENT_MAP_SRC, {
    header: true,
  })
  let program = linkProgram(sharedVertex, fragment, { label: "scene-unlit-" + cls })
  let pipeline = createRenderPipeline(program, {
    attributes: VERTEX_LAYOUTS.standard,
    depth: true,
    depthWrite: transparent ? false : undefined,
    blend: transparent ? "alpha" : undefined,
    cull: "back",
    label: "scene-unlit-" + cls,
  })
  pipelines[cls] = pipeline
  return pipeline
}

export type UnlitOptions = {
  /** Straight [r, g, b] or [r, g, b, a], 0..1. Default white. */
  color?: [number, number, number] | [number, number, number, number]
  /** A texture id to sample (tinted by `color` when both are given). */
  map?: TextureId
  /** Blend over what is behind (color alpha and map alpha both count).
   * Without it an alpha below 1 still draws opaque. See Material.transparent. */
  transparent?: boolean
}

/**
 * An unlit material: flat color, textured when `map` is given. Unlit is
 * the complete v1 set - lit materials arrive with uniform arrays (the
 * light list); see the scene-graph research note.
 */
export function unlit(opts: UnlitOptions = {}): Material {
  let color = opts.color ?? [1, 1, 1]
  let a = color.length === 4 ? color[3] : 1
  let uColor = [color[0] * a, color[1] * a, color[2] * a, a]
  let transparent = opts.transparent === true
  if (opts.map !== undefined) {
    return {
      pipeline: () => pipelineFor(transparent ? "map-transparent" : "map"),
      params: { uColor },
      textures: { uMap: opts.map },
      transparent,
    }
  }
  return { pipeline: () => pipelineFor(transparent ? "color-transparent" : "color"), params: { uColor }, transparent }
}

// Mirrors the engine's own preamble rule: a source carrying its own
// #version line is compiled exactly as written.
function needsHeader(source: string): boolean {
  return !source.trimStart().startsWith("#version")
}

// The scene-background pass (scene.setBackground). The vertex stage is the
// engine's own attributeless fullscreen triangle (gl_VertexID, no vertex
// buffer), emitting the SAME vUV the shader-target contract provides: 0..1
// with origin at the displayed top-left - so a backdrop fragment written
// for createShaderTexture ports verbatim.
const BACKGROUND_VERTEX = glsl`
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    vUV = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

// Pipeline fragments get no vUV from the engine preamble (a pipeline's
// varyings are its own), so the background slot injects the full
// shader-target fragment contract itself: vUV, fragColor, iResolution.
const BACKGROUND_FRAGMENT_PREAMBLE =
  "#version 300 es\nprecision highp float;\nin vec2 vUV;\nout vec4 fragColor;\nuniform vec2 iResolution;\n"

/** The scene's background pipeline (internal - reached via
 * scene.setBackground): depth-free, attributeless, drawn as entry zero of
 * the scene pass. */
export function backgroundPipeline(fragment: string, label: string): { pipeline: RenderPipelineId; program: ProgramId } {
  let vs = compileShader("vertex", BACKGROUND_VERTEX, { header: true })
  let fs = compileShader(
    "fragment",
    needsHeader(fragment) ? BACKGROUND_FRAGMENT_PREAMBLE + fragment : fragment,
    { header: false },
  )
  let program = linkProgram(vs, fs, { label })
  destroyShader(vs)
  destroyShader(fs)
  let pipeline = createRenderPipeline(program, { label })
  return { pipeline, program }
}

/** The class half of a shader material: sources and pipeline state, the
 * things one compiled program fixes. */
export type ShaderMaterialClassOptions = {
  /**
   * Vertex stage GLSL. MUST declare and use `uniform mat4 uModel` (the
   * mesh's world matrix, written per entry whenever the mesh moves) and
   * `uniform mat4 uViewProj` (the camera's view-projection, shared by the
   * whole scene target and written once per camera move) - transform with
   * `uViewProj * uModel * vec4(aPos, 1.0)`; a source mentioning neither
   * throws right here. The rest of the standard uniform set is opt-in by
   * declare-and-use: `uniform mat4 uNormal` (either stage) receives the
   * world inverse-transpose beside uModel - take `mat3(uNormal)` for
   * normals, correct under non-uniform scale - and `uniform vec3 uCamPos`
   * the camera's world position, shared like uViewProj (the specular /
   * fresnel view vector: `uCamPos - worldPos`). Declare any of the
   * layout's `in` attributes (aPos vec3, aNormal vec3, aUV vec2);
   * undeclared ones are skipped. Reading `in vec4 aColor` opts the
   * material into the "colored" 12-float layout - the per-vertex data
   * channel (tint, baked AO, any four scalars); its meshes then need
   * withColors() geometry, and a layout mismatch throws at add().
   * `@solidrt/3d/glsl` exports a standard
   * vertex stage and lighting pieces built on exactly this contract.
   */
  vertex: string
  fragment: string
  /**
   * Per-instance attributes: the vertex stage reads these as `in` variables
   * beside the layout's own, and each drawn instance gets one record from
   * the mesh's instance buffer (interleaved floats in this order). A class
   * with instance attributes makes INSTANCED materials: attach their meshes
   * with createInstancedMesh, which carries the records - a createMesh mesh
   * is rejected at add(). A per-instance transform is data, not a matrix:
   * a position/yaw/scale record beats four vec4 columns for most fleets,
   * and the composed uModel still places the whole population.
   */
  instanceAttributes?: VertexAttribute[]
  /** Blend over what is behind, with the scene sorting this material's
   * meshes back-to-front after the opaque ones (see Material.transparent).
   * Sets the pipeline defaults blend "alpha" and depthWrite false; the
   * fragment must write premultiplied output (`vec4(rgb * a, a)`). Defaults
   * to true whenever `blend` is set to anything but "none": every blended
   * draw belongs after the opaques so it depth-tests against them, and
   * back-to-front is harmless for the order-independent modes. */
  transparent?: boolean
  /** Pipeline state; defaults match unlit: depth: true, cull: "back",
   * and for transparent materials blend "alpha", depthWrite: false. */
  depth?: boolean
  depthWrite?: boolean
  blend?: BlendMode
  cull?: CullMode
  topology?: Topology
  label?: string
}

/** The instance half of a shader material: uniform seeds and sampler
 * bindings for one parameterisation of a class's program. */
export type ShaderMaterialInstanceOptions = {
  /** Uniform seeds beyond the standard set; update per mesh later with
   * setMeshParams. */
  params?: ShaderParams
  textures?: Record<string, TextureId>
}

export type ShaderMaterialOptions = ShaderMaterialClassOptions & ShaderMaterialInstanceOptions

/**
 * One program and pipeline, many parameterisations: the class/instance
 * split unlit has internally, for your own GLSL. `instance()` returns a
 * Material sharing the class's pipeline with its own params/textures - the
 * class compiles once, and dispose() is on the class alone (instances hold
 * nothing of their own).
 */
export type ShaderMaterialClass = {
  instance(opts?: ShaderMaterialInstanceOptions): Material
  /** Destroy the shared program and pipeline. Instances still in use draw
   * nothing valid afterwards. */
  dispose(): void
}

/**
 * A material class from your own GLSL: sources without a `#version` line
 * get the standard pipeline preamble (`fragColor`, `iResolution`). Two
 * calls with identical sources compile two programs - there is no dedupe by
 * source value (a hidden cache keyed by content is the anti-pattern the GPU
 * layer avoids throughout); the class IS the app-owned split. Create one
 * per program at app scope, `instance()` per look, and `dispose()` the class
 * when the app is done with the look for good.
 */
export function shaderMaterialClass(opts: ShaderMaterialClassOptions): ShaderMaterialClass {
  // The standard-set contract, checked where the mistake is made: a vertex
  // stage that never mentions the matrices cannot place meshes, and with
  // shared params skipping undeclared names the omission would otherwise
  // surface as a silently untransformed render, not an error.
  for (let name of ["uModel", "uViewProj"]) {
    if (!new RegExp("\\b" + name + "\\b").test(opts.vertex)) {
      throw new Error(
        "shaderMaterial vertex stage must declare and use '" + name + "' (see the standard uniform set in AGENTS.md)",
      )
    }
  }
  let program: ProgramId | undefined
  let pipeline: RenderPipelineId | undefined
  // Attributes live in the vertex stage only, so unlike the uNormal scan
  // there is nothing to look for in the fragment source.
  let layout: VertexLayout = /\baColor\b/.test(opts.vertex) ? "colored" : "standard"
  let normalMatrix = /\buNormal\b/.test(opts.vertex) || /\buNormal\b/.test(opts.fragment)
  let transparent = opts.transparent ?? (opts.blend !== undefined && opts.blend !== "none")
  let depth = opts.depth ?? true
  // An empty list declares nothing - same as absent (the engine requires an
  // instance buffer exactly when attributes are declared).
  let instanceAttributes = opts.instanceAttributes?.length ? opts.instanceAttributes.map(a => ({ ...a })) : undefined
  let pipelineFor = (): RenderPipelineId => {
    if (pipeline === undefined) {
      let vs = compileShader("vertex", opts.vertex, { header: needsHeader(opts.vertex) })
      let fs = compileShader("fragment", opts.fragment, { header: needsHeader(opts.fragment) })
      program = linkProgram(vs, fs, { label: opts.label })
      destroyShader(vs)
      destroyShader(fs)
      pipeline = createRenderPipeline(program, {
        attributes: VERTEX_LAYOUTS[layout],
        instanceAttributes,
        depth,
        // depthWrite needs a depth buffer, so the transparent default
        // only applies when there is one.
        depthWrite: opts.depthWrite ?? (transparent && depth ? false : undefined),
        blend: opts.blend ?? (transparent ? "alpha" : undefined),
        cull: opts.cull ?? "back",
        topology: opts.topology,
        label: opts.label,
      })
    }
    return pipeline
  }
  return {
    instance(inst = {}) {
      return { normalMatrix, layout, transparent, instanceAttributes, pipeline: pipelineFor, params: inst.params ?? {}, textures: inst.textures }
    },
    dispose() {
      if (pipeline !== undefined) {
        destroyRenderPipeline(pipeline)
        pipeline = undefined
      }
      if (program !== undefined) {
        destroyProgram(program)
        program = undefined
      }
    },
  }
}

/**
 * A material from your own GLSL: the custom-look escape hatch, first-class
 * next to unlit. A class with a single instance - `shaderMaterialClass()`
 * is the form for one program with many parameterisations.
 *
 * The INSTANCE is the pipeline handle: two calls with identical sources
 * compile two pipelines - there is no dedupe by source value. Create one
 * per look at app scope, share it across meshes, and `dispose()` it if the
 * app is done with the look for good.
 */
export function shaderMaterial(opts: ShaderMaterialOptions): Material {
  let cls = shaderMaterialClass(opts)
  let material = cls.instance(opts)
  material.dispose = cls.dispose
  return material
}
