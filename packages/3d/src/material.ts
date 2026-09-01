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
  TextureBindings,
  TextureId,
  Topology,
  VertexAttribute,
} from "@solidrt/core/gpu"
import { layoutAttributes, layoutKey, layoutSlot } from "./geometry.ts"
import type { VertexLayout } from "./geometry.ts"
import { litFragment, litShadowFragment, litVertex, SKIN_DECLS, SKIN_MATRIX, UNLIT_VERTEX, unlitFragment, unlitShadowFragment, unlitVertex } from "./glsl.ts"

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
  /** What a shadow view draws this material's meshes with instead of its
   * default depth override: the depth pass culling the side this material
   * culls (Three's shadowSide rule, Godot's shadow pass), so a `cull:
   * "none"` caster casts from both faces, for a cutout (lit
   * alphaTest with a map) the same discard, so a plant casts leaves and
   * not rectangles, and for a skinned material the same skinning, so a
   * posed caster casts its pose. Absent = the default (a back-culling
   * material casts from its back faces). A shaderMaterial supplies its
   * own through the instance option of the same name. */
  shadow?: Material
  /** True when the vertex stage declares `uBones` (read from the source,
   * like normalMatrix): the program skins by the mesh's palette texture,
   * so a shadow or override entry for a skinned mesh merges the mesh's
   * own uBones binding even though this material is not the mesh's. */
  skinned?: boolean
  /** Present on materials that own their pipeline (shaderMaterial). */
  dispose?(): void
}

// One shaderMaterialClass per unlit option combination (map x transparent
// x cull x alphaTest), cached for the app's lifetime like lit's; one
// pipeline per vertex layout inside each.
let unlitClasses = new Map<string, ShaderMaterialClass>()

export type UnlitOptions = {
  /** Straight [r, g, b] or [r, g, b, a], 0..1. Default white. */
  color?: [number, number, number] | [number, number, number, number]
  /** A texture id to sample (tinted by `color` when both are given). */
  map?: TextureId
  /** Blend over what is behind (color alpha and map alpha both count).
   * Without it an alpha below 1 still draws opaque. See Material.transparent. */
  transparent?: boolean
  /** Which faces to drop; default "back". "none" draws both sides of
   * single-layer geometry (foliage cards, glass, a mirrored part), and
   * lit materials then light a back face with its normal flipped, as
   * Three's DoubleSide and Godot's CULL_DISABLED do. */
  cull?: CullMode
  /** Cutout: drop a fragment whose final alpha (color x map, and for lit
   * the vertex color too) is below this, 0..1 (Three's alphaTest, glTF
   * alphaMode MASK with its alphaCutoff). Opaque otherwise:
   * depth-written, not sorted, unlike `transparent`. Foliage cards and
   * fences want it with `cull: "none"`; a mapped cutout casts its cutout
   * (Material.shadow). */
  alphaTest?: number
  /** Take the scene's fog (default true, Three's `material.fog`): the
   * fragment fades toward the fog color with its distance from the
   * camera once `scene.setFog` is set. `false` drops the fog code from
   * the program - a sky sphere or a far backdrop that must keep its
   * color, an emissive marker. */
  fog?: boolean
  /** Offset and repeat for the uv every map of this material samples:
   * `uv * repeat + offset` (defaults `[1, 1]` / `[0, 0]`). ONE transform
   * per MATERIAL (Godot's uv1_offset/uv1_scale, Unity's Tiling/Offset) -
   * deliberately not Three's per-texture transform, since a TextureId is
   * a shared value whose sampling is creation-time state. A cutout's
   * shadow transforms the same way; lit's lightMap (aUV2) is exempt.
   * Scrolling surfaces (water, a conveyor) drive it per frame with
   * `setMeshParams(mesh, { uMapTransform: [ru, rv, ou, ov] })`. Needs a
   * map; not with triplanar (whose repeat is uTriplanar). */
  mapTransform?: { offset?: [number, number]; repeat?: [number, number] }
  /** Skin positions (and lit normals) by the "skinned" layout's aJoints/
   * aWeights against the `uBones` palette texture - the rigged-model
   * variant createModel picks for skinned parts. The material then
   * requires "skinned" geometry, and something must bind `uBones`
   * (createModel binds each skin's palette texture to its meshes;
   * updateSkins writes it from the model's joints). The shadow variants
   * (depth and cutout) skin the same way, so a caster casts its pose. */
  skinned?: boolean
}

/** The uMapTransform vec4 for a mapTransform option: [repeatU, repeatV,
 * offsetU, offsetV]. */
function mapTransformParam(t: { offset?: [number, number]; repeat?: [number, number] }): number[] {
  let repeat = t.repeat ?? [1, 1]
  let offset = t.offset ?? [0, 0]
  return [repeat[0], repeat[1], offset[0], offset[1]]
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
  let map = opts.map !== undefined
  let transparent = opts.transparent === true
  let cull = opts.cull ?? "back"
  let alphaTest = opts.alphaTest !== undefined
  let fog = opts.fog !== false
  let mapTransform = opts.mapTransform !== undefined
  let skinned = opts.skinned === true
  if (mapTransform && !map) throw new Error("unlit: mapTransform without a map to transform")
  let key = [map, transparent, cull, alphaTest, fog, mapTransform, skinned].join("|")
  let cls = unlitClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: unlitVertex({ skinned }),
      fragment: unlitFragment({ map, alphaTest, transparent, fog, mapTransform }),
      transparent,
      cull,
      label: "scene-unlit-" + key,
    })
    unlitClasses.set(key, cls)
  }
  let params: ShaderParams = { uColor }
  if (alphaTest) params.uAlphaTest = opts.alphaTest!
  if (mapTransform) params.uMapTransform = mapTransformParam(opts.mapTransform!)
  return cls.instance({
    params,
    textures: map ? { uMap: opts.map! } : undefined,
    shadow:
      alphaTest && map
        ? unlitShadowMaterial(shadowCull(cull), skinned, uColor, opts.alphaTest!, opts.map!, mapTransform ? params.uMapTransform as number[] : undefined)
        : undefined,
  })
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
  /** A tangent-space normal map (OpenGL-style +Y, as glTF mandates),
   * sampled at the same uv as `map` and bending the lit normal - track
   * relief, kart panel lines, without triangles. The tangent frame is
   * built per fragment from screen-space derivatives (NORMAL_MAP in
   * `@solidrt/3d/glsl` - Three's untangented path), so ANY UV-mapped
   * geometry works with no tangent channel; the trade is mild seams on
   * mirrored UVs. Not with `triplanar` (which samples by world position).
   * Wants `mipmap: true` at creation like any map seen at distance. */
  normalMap?: TextureId
  /** How strongly the normal map bends the surface, default 1 (0 flattens
   * it). One float, as in Unity (_BumpScale) and Godot (normal_scale) -
   * not Three's Vector2, whose second component exists to flip
   * DirectX-style green channels. */
  normalScale?: number
  /** Light the surface emits, [r, g, b] 0..1 with any intensity folded in
   * (the uLightColor convention) - added after the lighting terms,
   * unaffected by lights and shadows, fogged like everything else.
   * Defaults to WHITE when `emissiveMap` is given (the map is the
   * emission - fixing Three's black-default gotcha, where an emissiveMap
   * alone shows nothing) and to off otherwise. */
  emissive?: [number, number, number]
  /** A texture multiplying `emissive` per fragment - lamps, screens,
   * nitro glow baked into one map. Sampled at the same uv as `map`. */
  emissiveMap?: TextureId
  /** A texture whose RED channel scales `specular` per fragment (Three's
   * specularMap) - chrome and rubber on one mesh. With it, `specular`
   * defaults to 1 (the map is the strength). */
  specularMap?: TextureId
  /** A baked-light texture (an offline render, an AO+GI bake), sampled by
   * the geometry's aUV2 channel and ADDED to the light sum like the
   * hemisphere term - a fully baked scene runs with no lights at all
   * (Three's lightMap; Unity and Godot bake at scene level, but here the
   * material picks the program). The geometry must carry aUV2
   * (withAttribute) or add() throws. */
  lightMap?: TextureId
  /** Scales the lightMap, default 1. */
  lightMapIntensity?: number
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

// The lit program is built by litFragment in ./glsl - the same builder an
// app calls to get a lit material with its own GLSL in it, composed from
// the same exported constants. What varies per flag: map x vertexColors x
// triplanar x receiveShadow x transparent x cull (a class that shows back
// faces lights them with the normal flipped, else a double-sided leaf's
// back is black) x alphaTest (the cutoff itself is a per-entry uniform,
// one class for every value) x fog (the scene's fog composed last, or
// left out of the program) x the surface maps (normalMap, emissive /
// emissiveMap, specularMap, lightMap) x mapTransform. The key's
// dimensionality costs nothing by itself: classes are created lazily per
// combination USED, so the program count is the app's distinct material
// configurations, bounded by its material count, never by this tuple's
// width. An opaque class writes alpha 1 (see
// unlitFragment). Lights arrive through the scene's shared params (light
// nodes); the base color, map and highlight are per entry. The shadow set
// is shared too and indexed like the lights: one atlas sampler,
// directional light i's maps (one, or its cascades) as map slots
// uShadowFirst[i] .. + uShadowCount[i] with a tile rect and a matrix
// each, and its biases (target-level, bound by the scene); uShadowCount
// 0 means it does not cast; SHADOW_LOOKUP turns the index into the factor.
// The option combination that picks a lit class: the class-cache key is
// its values in this order, and the same object builds the program, so
// the two cannot drift apart. `lit` fills no slot - an app that does
// reaches for litFragment directly, and owns the class it builds.
type LitClass = {
  map: boolean
  vertexColors: boolean
  triplanar: boolean
  transparent: boolean
  receiveShadow: boolean
  cull: CullMode
  alphaTest: boolean
  fog: boolean
  normalMap: boolean
  emissive: boolean
  emissiveMap: boolean
  specularMap: boolean
  lightMap: boolean
  mapTransform: boolean
  skinned: boolean
}

function litClassKey(c: LitClass): string {
  return Object.values(c).join("|")
}
let litClasses = new Map<string, ShaderMaterialClass>()

/**
 * A lit material: hemisphere ambient plus the scene's directional lights
 * (DirectionalLight nodes), Lambert diffuse, optional
 * Blinn-Phong highlight. Same options as unlit (color, map, transparent,
 * cull, alphaTest) plus vertexColors, specular/shininess and triplanar mapping. One program
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
  let triplanar = map && opts.triplanar !== undefined
  let alphaTest = opts.alphaTest !== undefined
  let cull = opts.cull ?? "back"
  let normalMap = opts.normalMap !== undefined
  let emissiveMap = opts.emissiveMap !== undefined
  let emissive = opts.emissive !== undefined || emissiveMap
  let specularMap = opts.specularMap !== undefined
  let lightMap = opts.lightMap !== undefined
  let mapTransform = opts.mapTransform !== undefined
  if (triplanar && normalMap) throw new Error("lit: normalMap cannot combine with triplanar (normal maps sample by uv)")
  if (triplanar && mapTransform) throw new Error("lit: mapTransform cannot combine with triplanar (its repeat is the triplanar value)")
  if (mapTransform && !map && !normalMap && !emissiveMap && !specularMap) {
    throw new Error("lit: mapTransform without a map to transform")
  }
  let flags: LitClass = {
    map,
    vertexColors: opts.vertexColors === true,
    triplanar,
    transparent: opts.transparent === true,
    receiveShadow: opts.receiveShadow !== false,
    cull,
    alphaTest,
    fog: opts.fog !== false,
    normalMap,
    emissive,
    emissiveMap,
    specularMap,
    lightMap,
    mapTransform,
    skinned: opts.skinned === true,
  }
  let key = litClassKey(flags)
  let cls = litClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: litVertex(flags),
      fragment: litFragment(flags),
      transparent: flags.transparent,
      cull,
      label: "scene-lit-" + key,
    })
    litClasses.set(key, cls)
  }
  let params: ShaderParams = {
    uColor,
    uSpecular: opts.specular ?? (specularMap ? 1 : 0),
    uShininess: opts.shininess ?? 30,
  }
  if (triplanar) params.uTriplanar = opts.triplanar!
  if (alphaTest) params.uAlphaTest = opts.alphaTest!
  if (normalMap) params.uNormalScale = opts.normalScale ?? 1
  if (emissive) params.uEmissive = opts.emissive ?? [1, 1, 1]
  if (lightMap) params.uLightMapIntensity = opts.lightMapIntensity ?? 1
  if (mapTransform) params.uMapTransform = mapTransformParam(opts.mapTransform!)
  let textures: TextureBindings | undefined
  if (map || normalMap || emissiveMap || specularMap || lightMap) {
    textures = {}
    if (map) textures.uMap = opts.map!
    if (normalMap) textures.uNormalMap = opts.normalMap!
    if (emissiveMap) textures.uEmissiveMap = opts.emissiveMap!
    if (specularMap) textures.uSpecularMap = opts.specularMap!
    if (lightMap) textures.uLightMap = opts.lightMap!
  }
  // A mapped cutout casts its cutout, triplanar included (the shadow
  // source resolves the base exactly as the main one does). A color-only
  // alphaTest is a constant over the mesh - all or nothing, and nothing
  // needs no program - so it keeps the plain cull-only variant.
  let material = cls.instance({
    params,
    textures,
    shadow:
      alphaTest && map
        ? litShadowMaterial(flags, uColor, opts.alphaTest!, opts.triplanar, opts.map!, mapTransform ? params.uMapTransform as number[] : undefined)
        : undefined,
  })
  return material
}

let litShadowClasses = new Map<string, ShaderMaterialClass>()

/** The shadow variant of a discarding lit material: litShadowFragment on
 * the same vertex stage, culling the side the material's own pipeline
 * keeps (Three's shadowSide rule), instanced with the same values so the
 * cutout it casts is the one it draws. Its params carry only the uniforms
 * its program declares - per-entry params reject unknown names. One class
 * per lit class, an instance per material. */
function litShadowMaterial(
  flags: LitClass,
  uColor: number[],
  uAlphaTest: number,
  triplanar: number | undefined,
  uMap: TextureId,
  uMapTransform?: number[],
): Material {
  // A skinned cutout keeps its skinning (flags.skinned rides into the
  // key and the vertex stage), so the shadow it casts is posed: the
  // shadow entry merges the mesh's uBones binding (Material.skinned).
  let key = litClassKey(flags)
  let cls = litShadowClasses.get(key)
  if (cls === undefined) {
    let fragment = litShadowFragment(flags)
    if (fragment === undefined) throw new Error("litShadowMaterial: " + key + " cannot discard")
    cls = shaderMaterialClass({
      vertex: litVertex(flags),
      fragment,
      cull: shadowCull(flags.cull),
      label: "scene-lit-shadow-" + key,
    })
    litShadowClasses.set(key, cls)
  }
  let params: ShaderParams = { uColor, uAlphaTest }
  if (flags.triplanar) params.uTriplanar = triplanar!
  if (uMapTransform !== undefined) params.uMapTransform = uMapTransform
  let material = cls.instance({ params, textures: { uMap } })
  // Only triplanar sampling reads the normal here: everywhere else the
  // linker drops vNormal and uNormal reflects inactive, so writing it per
  // move would warn every time. The value cannot matter, skip the write.
  if (!flags.triplanar) material.normalMatrix = false
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

// Its skinned twin: the same position-only pass with the skin matrix
// applied first, so a posed caster's depth is its pose (the entry's
// uBones binding comes from the mesh, see Material.skinned).
const SHADOW_DEPTH_VERTEX_SKINNED = glsl`
  in vec3 aPos;
  ${SKIN_DECLS}
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  void main() {
    ${SKIN_MATRIX}
    gl_Position = uViewProj * uModel * (skin * vec4(aPos, 1.0));
  }
`

const SHADOW_DEPTH_FRAGMENT = glsl`
  void main() {
    fragColor = vec4(1.0);
  }
`

let shadowDepth = new Map<string, Material>()

/** The override material of a scene's shadow view (internal): one class
 * per cull mode x skinned for the app, built on first use. The default,
 * "front", is the caster's back surface (see above); a material culling
 * or skinning otherwise carries its own variant as Material.shadow. */
export function shadowDepthMaterial(cull: CullMode = "front", skinned = false): Material {
  let key = cull + (skinned ? "|skinned" : "")
  let material = shadowDepth.get(key)
  if (material === undefined) {
    material = shaderMaterialClass({
      vertex: skinned ? SHADOW_DEPTH_VERTEX_SKINNED : SHADOW_DEPTH_VERTEX,
      fragment: SHADOW_DEPTH_FRAGMENT,
      cull,
      label: "scene-shadow-depth-" + key,
    }).instance()
    shadowDepth.set(key, material)
  }
  return material
}

/** The shadow pass's cull for a material's cull: the opposite side
 * (Three's shadowSide default), none stays none. */
function shadowCull(cull: CullMode): CullMode {
  return cull === "none" ? "none" : cull === "back" ? "front" : "back"
}

/** "back" unskinned maps to the default depth material, so its variant
 * is undefined; a skinned class always needs its own (the default does
 * not skin). */
function shadowVariant(cull: CullMode, skinned: boolean): Material | undefined {
  if (cull === "back" && !skinned) return undefined
  return shadowDepthMaterial(shadowCull(cull), skinned)
}

let unlitShadowClasses = new Map<string, ShaderMaterialClass>()

/** The shadow variant of a discarding unlit material: unlitShadowFragment
 * on the same vertex stage (skinned when the material skins, so the
 * cutout casts its pose), litShadowMaterial's unlit twin. unlit()'s
 * only discard is the mapped cutout, so what varies is the shadow cull,
 * skinning and whether the cutout's uv is transformed - one class per
 * combination. An instance per material (its map, color, cutoff,
 * transform). */
function unlitShadowMaterial(cull: CullMode, skinned: boolean, uColor: number[], uAlphaTest: number, uMap: TextureId, uMapTransform?: number[]): Material {
  let key = cull + "|" + (uMapTransform !== undefined) + (skinned ? "|skinned" : "")
  let cls = unlitShadowClasses.get(key)
  if (cls === undefined) {
    let fragment = unlitShadowFragment({ map: true, alphaTest: true, mapTransform: uMapTransform !== undefined })
    if (fragment === undefined) throw new Error("unlitShadowMaterial: the cutout options cannot discard")
    cls = shaderMaterialClass({
      vertex: unlitVertex({ skinned }),
      fragment,
      cull,
      label: "scene-unlit-shadow-" + key,
    })
    unlitShadowClasses.set(key, cls)
  }
  let params: ShaderParams = { uColor, uAlphaTest }
  if (uMapTransform !== undefined) params.uMapTransform = uMapTransform
  return cls.instance({ params, textures: { uMap } })
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
  out vec3 vWorldPos;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec3 uCamRight;
  uniform vec3 uCamUp;

  void main() {
    vec3 center = uModel[3].xyz;
    vec2 size = vec2(length(uModel[0].xyz), length(uModel[1].xyz));
    vec3 world = center + uCamRight * (aPos.x * size.x) + uCamUp * (aPos.y * size.y);
    vWorldPos = world;
    gl_Position = uViewProj * vec4(world, 1.0);
    vUv = aUV;
  }
`

const SPRITE_FIXED_Y_VERTEX_SRC = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  out vec3 vWorldPos;
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
    vWorldPos = world;
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
  let fog = opts.fog !== false
  let key = [map, transparent, fixedY, fog].join("|")
  let cls = spriteClasses.get(key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: fixedY ? SPRITE_FIXED_Y_VERTEX_SRC : SPRITE_VERTEX_SRC,
      fragment: unlitFragment({ map, transparent, fog }),
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
  /** The depth variant a shadow view draws this instance with (see
   * Material.shadow): a cutout's discard, an instanced class's vertex
   * placement. Default: the depth pass with this class's cull side,
   * skinned like the class when its vertex skins by uBones. */
  shadow?: Material
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
  // Skinning is a vertex-stage affair like the attributes; a source that
  // mentions uBones skins by the mesh's palette texture (Material.skinned).
  let skinned = /\buBones\b/.test(opts.vertex)
  let transparent = opts.transparent ?? (opts.blend !== undefined && opts.blend !== "none")
  let depth = opts.depth ?? true
  let cull = opts.cull ?? "back"
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
        cull,
        topology: opts.topology,
        label: opts.label,
      })
      pipelines.set(key, pipeline)
    }
    return pipeline
  }
  return {
    instance(inst = {}) {
      return {
        normalMatrix,
        skinned,
        attributes,
        transparent,
        instanceAttributes,
        pipeline: pipelineFor,
        params: inst.params ?? {},
        textures: inst.textures,
        // Lazy: the depth materials are shaderMaterialClass instances
        // themselves, so an eager variant would recurse into its own cache.
        get shadow() {
          return inst.shadow ?? shadowVariant(cull, skinned)
        },
      }
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
