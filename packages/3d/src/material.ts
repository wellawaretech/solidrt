// Materials pair GLSL with pipeline state, deduped hard: one program per
// material CLASS (unlit color, unlit textured), and one render pipeline
// per vertex layout the class meets (a pipeline is program + attribute
// list, so the program never recompiles for a wider geometry),
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
  programAttributes,
} from "@solidrt/core/gpu"
import type {
  BlendMode,
  CullMode,
  ProgramId,
  RenderPipelineId,
  ShaderParams,
  ShaderStageId,
  TextureBindings,
  TextureId,
  Topology,
  VertexAttribute,
} from "@solidrt/core/gpu"
import { layoutAttributes, layoutKey, layoutSlot } from "./geometry.ts"
import type { VertexLayout } from "./geometry.ts"
import { BLINN_SPECULAR, HEMISPHERE, LAMBERT, LIT_VERTEX, LIT_VERTEX_COLORED, MAX_LIGHTS, SHADOW, SHADOW_LOOKUP, SHADOW_SLOTS } from "./glsl.ts"

export type Material = {
  /** The pipeline this material draws with for geometry of `layout`
   * (lazily created, one per layout met). */
  pipeline(layout: VertexLayout | undefined): RenderPipelineId
  /** Per-entry uniform values this material contributes at addDraw. */
  params: ShaderParams
  /** Per-entry sampler bindings, when the material samples textures. */
  textures?: TextureBindings
  /** True when the vertex stage declares `uNormal`: the scene then writes
   * the world matrix's inverse-transpose alongside uModel for meshes using
   * this material (set automatically by shaderMaterial). */
  normalMatrix?: boolean
  /** The vertex attributes the linked program reads from the geometry
   * (name and format, per the engine's reflection of the compiled program,
   * instance attributes excluded). Links the program on first call. A mesh
   * whose geometry layout lacks any of them is rejected at add(); extra
   * channels in the geometry are fine (inactive attributes keep the
   * stride). */
  attributes(): VertexAttribute[]
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
let programs: Partial<Record<UnlitClass, ProgramId>> = {}
let pipelines = new Map<string, RenderPipelineId>()

// One program per unlit CLASS: fragment kind x transparency. Blend state is
// pipeline state and so is the attribute list, so the pipeline is keyed by
// class and vertex layout.
type UnlitClass = "color" | "map" | "color-transparent" | "map-transparent"

function programFor(cls: UnlitClass): ProgramId {
  let program = programs[cls]
  if (program === undefined) {
    if (sharedVertex === undefined) sharedVertex = compileShader("vertex", VERTEX_SRC, { header: true })
    let fragment = compileShader("fragment", cls.startsWith("color") ? FRAGMENT_COLOR_SRC : FRAGMENT_MAP_SRC, {
      header: true,
    })
    program = linkProgram(sharedVertex, fragment, { label: "scene-unlit-" + cls })
    programs[cls] = program
  }
  return program
}

function pipelineFor(cls: UnlitClass, layout: VertexLayout | undefined): RenderPipelineId {
  let key = cls + "|" + layoutKey(layout)
  let existing = pipelines.get(key)
  if (existing !== undefined) return existing
  let program = programFor(cls)
  let transparent = cls.endsWith("-transparent")
  let pipeline = createRenderPipeline(program, {
    attributes: layoutAttributes(layout),
    depth: true,
    depthWrite: transparent ? false : undefined,
    blend: transparent ? "alpha" : undefined,
    cull: "back",
    label: "scene-unlit-" + cls,
  })
  pipelines.set(key, pipeline)
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
  let cls: UnlitClass = opts.map !== undefined ? (transparent ? "map-transparent" : "map") : transparent ? "color-transparent" : "color"
  return {
    pipeline: layout => pipelineFor(cls, layout),
    attributes: () => programAttributes(programFor(cls)),
    params: { uColor },
    textures: opts.map !== undefined ? { uMap: opts.map } : undefined,
    transparent,
  }
}

export type LitOptions = UnlitOptions & {
  /** Multiply the base by the geometry's per-vertex aColor (withColors
   * geometry; add() throws without it). */
  vertexColors?: boolean
  /** Blinn-Phong highlight strength, 0..1 (default 0: pure diffuse). */
  specular?: number
  /** Highlight tightness, wide sheen (~8) to mirror dot (~150); default 30. */
  shininess?: number
  /** Sample `map` by WORLD position instead of UV - the value is the
   * texture repeats per world unit - blended across the three axis planes
   * by the normal. Tiles generated geometry at one density regardless of
   * each part's size or UVs; the map must be created with
   * `wrap: "repeat"`. */
  triplanar?: number
  /**
   * Receive the scene's directional shadows (default true, like Godot and
   * Three): each casting light's term is multiplied by its shadow-map
   * factor (SHADOW in `@solidrt/3d/glsl`). `false` opts out - a material
   * that must never darken (an emissive surface, a far skybox) - and
   * drops the map sample from its program. A material option, not a
   * node flag as in Three, because the material picks the program (like
   * vertexColors and triplanar; Godot's `disable_receive_shadows`); in a
   * scene with no `castShadow` light the receiving variant draws exactly
   * like the opted-out one. Custom materials receive by declaring the
   * scene's shadow set (see SHADOW's doc) and composing `shadow` per light.
   */
  receiveShadow?: boolean
}

// The lit fragment is composed from the same exported pieces an app
// composes by hand, per flag: map x vertexColors x triplanar x shadow x
// transparent. Lights arrive through the scene's shared params
// (light nodes); the base color, map and highlight are per entry. The
// shadow set is shared too and indexed like the lights: one atlas sampler,
// directional light i's maps (one, or its cascades) as map slots
// uShadowFirst[i] .. + uShadowCount[i] with a tile rect and a matrix
// each, and its biases (target-level, bound by the scene); uShadowCount
// 0 means it does not cast; SHADOW_LOOKUP turns the index into the factor.
function litFragment(map: boolean, vertexColors: boolean, triplanar: boolean, shadow: boolean): string {
  return glsl`
    in vec3 vWorldPos;
    in vec3 vNormal;
    in vec2 vUv;
    ${vertexColors ? "in vec4 vColor;" : ""}
    uniform vec4 uColor;
    ${map ? "uniform sampler2D uMap;" : ""}
    uniform float uSpecular;
    uniform float uShininess;
    ${triplanar ? "uniform float uTriplanar;" : ""}
    uniform vec3 uCamPos;
    uniform vec3 uHemiSky;
    uniform vec3 uHemiGround;
    uniform int uLightCount;
    uniform vec3 uLightDir[${MAX_LIGHTS}];
    uniform vec3 uLightColor[${MAX_LIGHTS}];
    ${
      shadow
        ? `${SHADOW_SLOTS}
    ${SHADOW}
    ${SHADOW_LOOKUP}`
        : ""
    }
    ${HEMISPHERE}
    ${LAMBERT}
    ${BLINN_SPECULAR}

    void main() {
      vec3 n = normalize(vNormal);
      vec4 base = uColor;
      ${
        map
          ? triplanar
            ? `vec3 w = pow(abs(n), vec3(4.0));
      w /= w.x + w.y + w.z;
      vec3 p = vWorldPos * uTriplanar;
      base *= texture(uMap, p.yz) * w.x + texture(uMap, p.xz) * w.y + texture(uMap, p.xy) * w.z;`
            : "base *= texture(uMap, vUv);"
          : ""
      }
      ${vertexColors ? "base *= vColor;" : ""}
      vec3 v = normalize(uCamPos - vWorldPos);
      vec3 light = hemisphere(n, uHemiSky, uHemiGround);
      vec3 spec = vec3(0.0);
      for (int i = 0; i < ${MAX_LIGHTS}; i++) {
        if (i >= uLightCount) break;
        vec3 l = uLightDir[i];
        ${
          shadow ? "float s = lightShadow(i, vWorldPos, n);" : "float s = 1.0;"
        }
        light += uLightColor[i] * lambert(n, l) * s;
        spec += uLightColor[i] * blinnSpecular(n, v, l, uShininess) * s;
      }
      fragColor = vec4(base.rgb * light + spec * uSpecular * base.a, base.a);
    }
  `
}

let litClasses = new Map<string, ShaderMaterialClass>()

/**
 * A lit material: hemisphere ambient plus the scene's directional lights
 * (DirectionalLight nodes), Lambert diffuse, optional
 * Blinn-Phong highlight. Same options as unlit (color, map, transparent)
 * plus vertexColors, specular/shininess and triplanar mapping. One program
 * per option combination, one pipeline per vertex layout met, shared by
 * every instance - a thousand lit meshes still share one pipeline. No
 * lights set means black except for the hemisphere term, which also
 * starts at zero: set at least one of the two.
 */
export function lit(opts: LitOptions = {}): Material {
  let color = opts.color ?? [1, 1, 1]
  let a = color.length === 4 ? color[3] : 1
  let uColor = [color[0] * a, color[1] * a, color[2] * a, a]
  let map = opts.map !== undefined
  let vertexColors = opts.vertexColors === true
  let triplanar = map && opts.triplanar !== undefined
  let transparent = opts.transparent === true
  let shadow = opts.receiveShadow !== false
  let key = [map, vertexColors, triplanar, transparent, shadow].join("|")
  let cls = litClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: vertexColors ? LIT_VERTEX_COLORED : LIT_VERTEX,
      fragment: litFragment(map, vertexColors, triplanar, shadow),
      transparent,
      label: "scene-lit-" + key,
    })
    litClasses.set(key, cls)
  }
  let material = cls.instance({
    params: triplanar
      ? { uColor, uSpecular: opts.specular ?? 0, uShininess: opts.shininess ?? 30, uTriplanar: opts.triplanar! }
      : { uColor, uSpecular: opts.specular ?? 0, uShininess: opts.shininess ?? 30 },
    textures: map ? { uMap: opts.map! } : undefined,
  })
  return material
}

// The shadow depth pass: position only, no color of interest (the target's
// depth texture is the output; the color write is the pipeline's minimum).
// Front faces culled, Three's shadowSide default: the map holds each
// caster's BACK surface, so a receiving front face at the same depth
// compares lit without a bias and acne needs no fighting on closed meshes.
const SHADOW_DEPTH_VERTEX = glsl`
  in vec3 aPos;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  void main() {
    gl_Position = uViewProj * uModel * vec4(aPos, 1.0);
  }
`

const SHADOW_DEPTH_FRAGMENT = glsl`
  void main() {
    fragColor = vec4(1.0);
  }
`

let shadowDepth: Material | undefined

/** The override material of a scene's shadow view (internal): one class
 * for the app, built on first use. */
export function shadowDepthMaterial(): Material {
  if (shadowDepth === undefined) {
    shadowDepth = shaderMaterialClass({
      vertex: SHADOW_DEPTH_VERTEX,
      fragment: SHADOW_DEPTH_FRAGMENT,
      cull: "front",
      label: "scene-shadow-depth",
    }).instance()
  }
  return shadowDepth
}

export type SpriteOptions = UnlitOptions & {
  /** Which way the quad turns to face the camera. `"full"` (default,
   * Three's Sprite): both axes follow the view, the quad is always flat
   * to the screen. `"fixed-y"` (Godot's BILLBOARD_FIXED_Y): only the yaw
   * follows the camera, the quad stays upright on world y - trees and
   * standing characters, the classic sprite. */
  billboard?: "full" | "fixed-y"
}

// The billboard vertex stages: the unit quad's corners placed along the
// camera axes at the mesh's world position, with the quad's size read
// off uModel's column lengths so `scale` sizes the sprite like any mesh.
// The rotation part of uModel is otherwise ignored (the camera decides
// the facing). Fixed-y takes the yaw from the camera-to-center direction
// flattened onto XZ; straight above or below there is no yaw to take, so
// the quad falls back to facing +z rather than dividing by zero.
const SPRITE_VERTEX_SRC = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec3 uCamRight;
  uniform vec3 uCamUp;

  void main() {
    vec3 center = uModel[3].xyz;
    vec2 size = vec2(length(uModel[0].xyz), length(uModel[1].xyz));
    vec3 world = center + uCamRight * (aPos.x * size.x) + uCamUp * (aPos.y * size.y);
    gl_Position = uViewProj * vec4(world, 1.0);
    vUv = aUV;
  }
`

const SPRITE_FIXED_Y_VERTEX_SRC = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec3 uCamPos;

  void main() {
    vec3 center = uModel[3].xyz;
    vec2 size = vec2(length(uModel[0].xyz), length(uModel[1].xyz));
    vec3 toCam = uCamPos - center;
    toCam.y = 0.0;
    float len = length(toCam);
    vec3 right = len > 1e-6 ? vec3(toCam.z, 0.0, -toCam.x) / len : vec3(1.0, 0.0, 0.0);
    vec3 world = center + right * (aPos.x * size.x) + vec3(0.0, aPos.y * size.y, 0.0);
    gl_Position = uViewProj * vec4(world, 1.0);
    vUv = aUV;
  }
`

let spriteClasses = new Map<string, ShaderMaterialClass>()

/**
 * A sprite material: unlit color/map on a quad that turns to face the
 * camera in the vertex stage (the shared uCamRight/uCamUp basis, or
 * uCamPos for fixed-y), so a thousand sprites cost no per-frame JS. Draw
 * it with createSprite / `<Sprite>`, which supply the unit quad; on other
 * geometry the vertex stage still flattens every vertex onto the camera
 * plane. Unlike unlit, `transparent` defaults to TRUE - sprites are cutouts
 * far more often than not (Three's SpriteMaterial default) - pass false
 * for an opaque one. Culling is off: a camera-facing quad has no back.
 */
export function sprite(opts: SpriteOptions = {}): Material {
  let color = opts.color ?? [1, 1, 1]
  let a = color.length === 4 ? color[3] : 1
  let uColor = [color[0] * a, color[1] * a, color[2] * a, a]
  let map = opts.map !== undefined
  let transparent = opts.transparent !== false
  let fixedY = opts.billboard === "fixed-y"
  let key = [map, transparent, fixedY].join("|")
  let cls = spriteClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: fixedY ? SPRITE_FIXED_Y_VERTEX_SRC : SPRITE_VERTEX_SRC,
      fragment: map ? FRAGMENT_MAP_SRC : FRAGMENT_COLOR_SRC,
      transparent,
      cull: "none",
      label: "scene-sprite-" + key,
    })
    spriteClasses.set(key, cls)
  }
  return cls.instance({ params: { uColor }, textures: map ? { uMap: opts.map! } : undefined })
}

/** The attributes `material` reads that `layout` does not carry (name and
 * format) - empty when the pair is drawable. */
export function missingAttributes(material: Material, layout: VertexLayout | undefined): VertexAttribute[] {
  let missing: VertexAttribute[] = []
  for (let attr of material.attributes()) {
    let slot = layoutSlot(layout, attr.name)
    if (slot === null || slot.format !== attr.format) missing.push(attr)
  }
  return missing
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
   * geometry's `in` attributes by name (the standard aPos vec3, aNormal
   * vec3, aUV vec2, or any channel appended with withAttribute);
   * undeclared ones are skipped. What the program READS is the engine's
   * word (reflected from the linked program, so an `in` the compiler
   * dropped does not count); one the mesh's geometry layout does not
   * carry (name and format) throws at add() - so `in vec4 aColor` needs
   * withColors() geometry. The class builds one pipeline per layout its
   * meshes bring, the program compiles once.
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
  textures?: TextureBindings
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
  let pipelines = new Map<string, RenderPipelineId>()
  // Attributes live in the vertex stage only, so unlike the uNormal scan
  // there is nothing to look for in the fragment source.
  let normalMatrix = /\buNormal\b/.test(opts.vertex) || /\buNormal\b/.test(opts.fragment)
  let transparent = opts.transparent ?? (opts.blend !== undefined && opts.blend !== "none")
  let depth = opts.depth ?? true
  // An empty list declares nothing - same as absent (the engine requires an
  // instance buffer exactly when attributes are declared).
  let instanceAttributes = opts.instanceAttributes?.length ? opts.instanceAttributes.map(a => ({ ...a })) : undefined
  let programFor = (): ProgramId => {
    if (program === undefined) {
      let vs = compileShader("vertex", opts.vertex, { header: needsHeader(opts.vertex) })
      let fs = compileShader("fragment", opts.fragment, { header: needsHeader(opts.fragment) })
      program = linkProgram(vs, fs, { label: opts.label })
      destroyShader(vs)
      destroyShader(fs)
    }
    return program
  }
  // What the program reads from the GEOMETRY: the engine's reflection of
  // the linked program minus the per-instance names (those come from the
  // record buffer, declared on the pipeline beside the layout).
  let attributes = (): VertexAttribute[] =>
    programAttributes(programFor()).filter(a => !instanceAttributes?.some(i => i.name === a.name))
  let pipelineFor = (layout: VertexLayout | undefined): RenderPipelineId => {
    let key = layoutKey(layout)
    let pipeline = pipelines.get(key)
    if (pipeline === undefined) {
      pipeline = createRenderPipeline(programFor(), {
        attributes: layoutAttributes(layout),
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
      pipelines.set(key, pipeline)
    }
    return pipeline
  }
  return {
    instance(inst = {}) {
      return { normalMatrix, attributes, transparent, instanceAttributes, pipeline: pipelineFor, params: inst.params ?? {}, textures: inst.textures }
    },
    dispose() {
      for (let pipeline of pipelines.values()) destroyRenderPipeline(pipeline)
      pipelines.clear()
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
