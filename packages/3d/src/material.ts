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

import { compileShader, createRenderPipeline, glsl, linkProgram } from "@solidrt/core/gpu"
import type { RenderPipelineId, ShaderParams, ShaderStageId, TextureId } from "@solidrt/core/gpu"
import { VERTEX_LAYOUT } from "./geometry.ts"

export type Material = {
  /** The shared pipeline for this material's class (lazily created). */
  pipeline(): RenderPipelineId
  /** Per-entry uniform values this material contributes at addDraw. */
  params: ShaderParams
  /** Per-entry sampler bindings, when the class samples textures. */
  textures?: Record<string, TextureId>
}

// One vertex stage serves every unlit class: MVP transform plus the UV
// varying. aNormal from the shared layout is deliberately not declared -
// inactive attributes are skipped and only the stride accounts for them.
const VERTEX_SRC = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  uniform mat4 uMVP;

  void main() {
    gl_Position = uMVP * vec4(aPos, 1.0);
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
