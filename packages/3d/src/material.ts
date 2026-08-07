// Materials pair GLSL with pipeline state, deduped hard: one program and
// one render pipeline per material CLASS (unlit color, unlit textured),
// created lazily at first use and kept for the app's lifetime. A material
// INSTANCE is just the per-entry uniform values (and sampler bindings) it
// contributes when a mesh becomes a draw entry - so a thousand meshes with
// a thousand colors still share one pipeline.
//
// Colors are straight [r, g, b, a?] 0..1 at the API and premultiplied here
// once, at the boundary (the engine's pixel contract). An alpha below 1
// does NOT blend yet: v1 pipelines draw opaque (blend "none"), so a
// translucent color overwrites what is behind it. Transparency arrives
// with the blend-factor vocabulary and back-to-front sorting (see
// okf/research/scene-graph-3d.md, staging step 4).
//
// Custom looks need no material system: the raw layer (compileShader /
// createRenderPipeline in @solidrt/core/gpu) is first-class, and a scene
// draws into an ordinary draw target - a custom-shaded mesh is a future
// material class here, or the app's own addDraw beside the scene's.

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
} from "@solidrt/core/gpu"
import { VERTEX_LAYOUT } from "./geometry.ts"

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
let pipelines: { color?: RenderPipelineId; map?: RenderPipelineId } = {}

function pipelineFor(kind: "color" | "map"): RenderPipelineId {
  let existing = pipelines[kind]
  if (existing !== undefined) return existing
  if (sharedVertex === undefined) sharedVertex = compileShader("vertex", VERTEX_SRC, { header: true })
  let fragment = compileShader("fragment", kind === "color" ? FRAGMENT_COLOR_SRC : FRAGMENT_MAP_SRC, {
    header: true,
  })
  let program = linkProgram(sharedVertex, fragment, { label: "scene-unlit-" + kind })
  let pipeline = createRenderPipeline(program, {
    attributes: VERTEX_LAYOUT,
    depth: true,
    cull: "back",
    label: "scene-unlit-" + kind,
  })
  pipelines[kind] = pipeline
  return pipeline
}

export type UnlitOptions = {
  /** Straight [r, g, b] or [r, g, b, a], 0..1. Default white. */
  color?: [number, number, number] | [number, number, number, number]
  /** A texture id to sample (tinted by `color` when both are given). */
  map?: TextureId
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
  if (opts.map !== undefined) {
    return { pipeline: () => pipelineFor("map"), params: { uColor }, textures: { uMap: opts.map } }
  }
  return { pipeline: () => pipelineFor("color"), params: { uColor } }
}

// Mirrors the engine's own preamble rule: a source carrying its own
// #version line is compiled exactly as written.
function needsHeader(source: string): boolean {
  return !source.trimStart().startsWith("#version")
}

export type ShaderMaterialOptions = {
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
   * fresnel view vector: `uCamPos - worldPos`). Declare any of the shared
   * layout's `in` attributes (aPos vec3, aNormal vec3, aUV vec2);
   * undeclared ones are skipped. `@solidrt/3d/glsl` exports a standard
   * vertex stage and lighting pieces built on exactly this contract.
   */
  vertex: string
  fragment: string
  /** Uniform seeds beyond the standard set; update per mesh later with
   * setMeshParams. */
  params?: ShaderParams
  textures?: Record<string, TextureId>
  /** Pipeline state; defaults match unlit: depth: true, cull: "back". */
  depth?: boolean
  depthWrite?: boolean
  blend?: BlendMode
  cull?: CullMode
  topology?: Topology
  label?: string
}

/**
 * A material from your own GLSL: the custom-look escape hatch, first-class
 * next to unlit. Sources without a `#version` line get the standard
 * pipeline preamble (`fragColor`, `iResolution`).
 *
 * The INSTANCE is the pipeline handle: two calls with identical sources
 * compile two pipelines - there is no dedupe by source value (a hidden
 * cache keyed by content is the anti-pattern the GPU layer avoids
 * throughout). Create one per look at app scope, share it across meshes,
 * and `dispose()` it if the app is done with the look for good.
 */
export function shaderMaterial(opts: ShaderMaterialOptions): Material {
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
  return {
    normalMatrix: /\buNormal\b/.test(opts.vertex) || /\buNormal\b/.test(opts.fragment),
    pipeline() {
      if (pipeline === undefined) {
        let vs = compileShader("vertex", opts.vertex, { header: needsHeader(opts.vertex) })
        let fs = compileShader("fragment", opts.fragment, { header: needsHeader(opts.fragment) })
        program = linkProgram(vs, fs, { label: opts.label })
        destroyShader(vs)
        destroyShader(fs)
        pipeline = createRenderPipeline(program, {
          attributes: VERTEX_LAYOUT,
          depth: opts.depth ?? true,
          depthWrite: opts.depthWrite,
          blend: opts.blend,
          cull: opts.cull ?? "back",
          topology: opts.topology,
          label: opts.label,
        })
      }
      return pipeline
    },
    params: opts.params ?? {},
    textures: opts.textures,
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
