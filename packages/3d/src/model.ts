// Models in a scene: ModelData (parsed glTF or a decoded .sol3m) becomes a
// Group carrying the file's node hierarchy - nested Groups with the file's
// local TRS, each part's mesh under its node - with the images uploaded as
// textures and a material per glTF material: Three's `gltf.scene`, an
// object you add to the scene and place with setTransform, whose named
// nodes (`model.nodes`) can be moved individually. A part several nodes
// place (ModelPart.placements: a reused mesh, EXT_mesh_gpu_instancing) is
// ONE InstancedMesh at identity under the model root with an instance
// under every placement node, so the copies are one draw entry and still
// ride, animate, pick and cull as nodes of the hierarchy. The file's
// extras and blobs come along as plain data. The model owns what it
// created (geometry buffers, record buffers, textures): dispose() frees
// them and detaches the group. loadGltf / loadModel are the
// read-then-create conveniences over flux:fs; parseGltf / decodeModel +
// createModel are the primitives under them, for bytes obtained any other
// way (a binary import, a fetch). Compressed images (KTX2, what `sol tool
// 3d/model` bakes) are turned into this device's block format by
// transcodeModelImages, the one asynchronous step between the two.

import { file } from "flux:fs"
import * as spatial from "flux:spatial"
import { createEffect, createRoot, decodeImage, getOwner, isDisposed, isKtx2, onCleanup, quality as appQuality, runWithOwner, transcodeTexture, untrack } from "@solidrt/core"
import type { Owner } from "@solidrt/core"
import { createMutableTexture, createTexture, destroyTexture, setTextureSampling, uploadTexture } from "@solidrt/core/gpu"
import type { TextureFormat, TextureId } from "@solidrt/core/gpu"
import { resourcePool, textureClass } from "./resource-pool.ts"
import { mipLevels } from "./environment-bake.ts"
import { QUALITY_TIERS, textureSampling } from "./quality.ts"
import { gltfExternalUris, modelImageUses, parseGltf } from "./gltf.ts"
import type { ModelClip, ModelData, ModelExtras, ModelMaterial } from "./gltf.ts"
import { decodeModel } from "./model-file.ts"
import { disposeGeometry } from "./geometry-gpu.ts"
import { layoutSlot } from "./geometry.ts"
import { standard } from "./material.ts"
import type { Material } from "./material.ts"
import { add, afterFree, createGroup, createMorphState, remove, setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"
import { addInstance, createInstancedMesh, createMesh, disposeInstances } from "./mesh.ts"
import type { InstancedMesh, InstanceNode, Mesh } from "./mesh.ts"
import { refreshJointBounds, unbindSkeleton } from "./skeleton.ts"
import type { WornPiece } from "./skeleton.ts"

// A model's textures sample as the APP tier's row says (QualityRow's
// anisotropy and mipLimit): a texture is id state shared by every scene
// that draws the model, so no scene's own row can set it - the reason the
// tier is app-wide, as Unity's texture quality is. Every live model texture
// is registered here with its chain length, and one app-lifetime effect
// re-samples them all when the tier changes (setTextureSampling, in
// place). Created under no owner, like core's device facts: the first
// model is usually made inside a component, and the follower belongs to
// the app, not to whoever loaded first.
let liveTextures = new Map<TextureId, number>()
let following = false
function followTier(): void {
  if (following) return
  following = true
  runWithOwner(null, () =>
    createRoot(() =>
      createEffect(
        () => appQuality(),
        tier => {
          let row = QUALITY_TIERS[tier]
          for (let [id, levels] of liveTextures) setTextureSampling(id, textureSampling(row, levels))
        },
        { defer: true },
      ),
    ),
  )
}

/** A glTF material's uploaded textures, by phong()/standard() option name;
 * null where the material has none. */
export type ModelMaps = {
  map: TextureId | null
  normalMap: TextureId | null
  emissiveMap: TextureId | null
  /** glTF's ONE packed metallicRoughnessTexture under both of standard's
   * names (blue = metalness, green = roughness): pass both through. */
  metalnessMap: TextureId | null
  roughnessMap: TextureId | null
}

export type ModelOptions = {
  /** The material for each glTF material (default: `standard` with its
   * color, maps, normal scale, metalness/roughness, emissive and
   * transparency - the glTF material model; a scene with no `environment`
   * renders its metals near black, so set one, or return `phong` here for
   * the Blinn-Phong look). `maps` holds the uploaded textures by
   * phong()/standard() option name. Called once per material -
   * or once per (material, skinned, vertexColors, morphed) combination
   * when parts that differ in any share a material - and shared by every
   * part using it. `skinned` is true when the material must skin (pass it
   * through to `phong`/`unlit`, or read aJoints/aWeights + uBones yourself);
   * `vertexColors` is true when the part carries COLOR_0 in aColor (pass
   * it through, or read aColor yourself); `morphed` is true when the part
   * carries morph targets (pass it through as `morph`, or splice
   * MORPH_DECLS/MORPH_APPLY yourself); `instanced` is true when the part
   * is a population (ModelPart.placements: drawn as one InstancedMesh),
   * so the material must be instanced (pass it through as `instanced`,
   * or declare INSTANCE_MATRIX_ATTRIBUTES and splice INSTANCE_MATRIX
   * yourself). `data.materials` is in file order, so the calls arrive in
   * file order too. */
  material?: (material: ModelMaterial, maps: ModelMaps, skinned: boolean, vertexColors: boolean, morphed: boolean, instanced: boolean) => Material
  /** Debug name for the textures. */
  label?: string
  /** Free the model with the owning reactive scope (default true, like
   * createScene: `dispose()` runs at the scope's cleanup); `false` opts
   * out, then call dispose yourself. For loadGltf/loadModel the scope is
   * the caller's at the call, before the await. */
  autoFree?: boolean
}

/** A model in the scene: a Group carrying the file's node hierarchy as
 * nested Groups, with each part's mesh a child of its node. */
export type Model = SceneNode & {
  kind: "group"
  /** The parts by name, in file order; each `mesh` sits under its node.
   * A part with placements (a mesh several nodes place) is instead ONE
   * InstancedMesh at identity under the model root, with `instances` in
   * placement order (the part's node first, then its placements), each
   * an instance node at identity under its placement node in `nodes`:
   * move the placement node and its copy follows; a pick on a copy
   * reports `hit.mesh` = this mesh and `hit.instance` one of these
   * (its `parent` is the placement node). A material swap on the mesh
   * applies to every copy. `extras` is the glTF mesh's, when it has
   * any. */
  parts: { name: string; mesh: Mesh; instances?: InstanceNode[]; extras?: ModelExtras }[]
  /** The file's retained nodes in table order (parents first), each an
   * ordinary Group under the model: `setTransform` on one moves its
   * subtree - a wheel spins relative to the axle it hangs from. Names
   * repeat when the file's do; find yours with `.find()`/`.filter()`.
   * `extras` is the glTF node's, when it has any. */
  nodes: { name: string; node: SceneNode; extras?: ModelExtras }[]
  /** One per glTF material, in file order. */
  materials: Material[]
  /** The file's root extras (glTF `extras`, or what a bake wrote), when
   * it has any. */
  extras?: ModelExtras
  /** The file's named binary sections (a bake's), when it has any: views
   * onto the loaded bytes. */
  blobs?: Record<string, Uint8Array>
  /** The file's animation clips (empty when it has none); createMixer
   * plays them. Channel node indices resolve through `nodes`. */
  clips: ModelClip[]
  /** Local rest-pose [minX, minY, minZ, maxX, maxY, maxZ] over every part
   * (conservative for parts under rotated nodes). */
  bounds: Float32Array
  /** Detach the model and free its geometry buffers and textures. A
   * worn piece (bindSkeleton) comes off its body; a body takes the
   * pieces it wears with it. */
  dispose(): void
  /** @internal The skins the parts draw, in file order: the palette
   * texture (shared by identical skins), the joint nodes in row order,
   * their inverse binds and joint-space influence boxes. */
  _skins: ModelSkinNodes[]
  /** @internal The pieces bound onto this model's skeleton. */
  _worn: WornPiece[]
  /** @internal The body this model is bound onto, or null. */
  _body: Model | null
  /** @internal Subtrees bindSkeleton moved under the body's joints (no
   * longer under this model); detached again by dispose. */
  _grafts: SceneNode[]
  /** @internal The textures the model owns (the images, the skin
   * palettes), each with the pool class it is given back under. */
  _textures: ModelTexture[]
  /** @internal The GPU bytes of those textures: what a region weighs the
   * model's textures by. */
  _textureBytes: number
}

/** @internal One skin of a built model: ModelSkin with its joints
 * resolved to the model's nodes and its palette texture. */
export type ModelSkinNodes = {
  texture: TextureId
  joints: SceneNode[]
  inverseBind: Float32Array
  jointBounds: Float32Array
}

/**
 * Transcode the model's compressed images (KTX2) into this device's block
 * format, off the JS thread (all are started at once; the runtime runs a
 * few at a time), and return the data with them as `textures`; the images that are PNG or JPEG are left for
 * createModel to decode. The step between a parse (or decodeModel) and
 * createModel for data that may hold KTX2 images; loadModel and loadGltf
 * run it. Data with no KTX2 image comes back as it is.
 */
export async function transcodeModelImages(data: ModelData): Promise<ModelData> {
  if (!data.images.some((image) => isKtx2(image.bytes))) return data
  let textures = await Promise.all(data.images.map((image) => (isKtx2(image.bytes) ? transcodeTexture(image.bytes) : undefined)))
  return { ...data, textures }
}

/** @internal A built model's texture: its id and its pool class (null
 * for a create-once compressed texture, which is destroyed, not given). */
type ModelTexture = { id: TextureId; cls: string | null }

// A model texture from the package's pool (a write into a retired id of
// the class, re-sampled; no create) or created mutable so it can be
// pooled later. Compressed storage is create-once: created immutable,
// never pooled.
function takeTexture(
  data: Uint8Array | Float32Array,
  width: number,
  height: number,
  format: TextureFormat,
  mipmap: boolean,
  sampling: { wrap?: "repeat"; anisotropy: number; baseLevel: number },
  label: string | undefined,
): ModelTexture {
  if (format.startsWith("etc2") || format.startsWith("bc7")) {
    return { id: createTexture(data, width, height, { ...sampling, format, mipmap, autoFree: false, label }), cls: null }
  }
  let cls = textureClass(format, width, height, mipmap)
  let pool = resourcePool()
  if (pool !== null) {
    let id = pool.take(cls) as TextureId | undefined
    if (id !== undefined) {
      uploadTexture(id, data)
      setTextureSampling(id, sampling)
      return { id, cls }
    }
  }
  return { id: createMutableTexture(data, width, height, { ...sampling, format, mipmap, autoFree: false, label }), cls }
}

function freeTexture(texture: ModelTexture): void {
  liveTextures.delete(texture.id)
  let pool = resourcePool()
  if (texture.cls !== null && pool !== null) pool.give(texture.cls, texture.id)
  else destroyTexture(texture.id)
}

// The GPU bytes of a texture: its texels at the format's size, the mip
// chain a third more.
const MIP_CHAIN_FACTOR = 4 / 3
function textureBytes(width: number, height: number, format: TextureFormat, mipmap: boolean, data: ArrayBufferView): number {
  if (format.startsWith("etc2") || format.startsWith("bc7")) return data.byteLength
  let texel = format === "rgba32f" ? 16 : format === "rgba16f" ? 8 : format === "r32f" ? 4 : format === "r8" ? 1 : 4
  return Math.ceil(width * height * texel * (mipmap ? MIP_CHAIN_FACTOR : 1))
}

/**
 * Build the scene object for parsed model data: upload its images (repeat
 * wrap, mipmapped, the app quality tier's anisotropy and mip limit - a
 * tier change re-samples them in place; the base color and emissive
 * images as "rgba8-srgb" since glTF stores those sRGB-encoded, the data
 * maps as rgba8), make a material per glTF material, the file's node hierarchy as
 * nested Groups, and a mesh per part under its node. Synchronous - the
 * data is already in memory: every step of `modelSteps` run at once. A
 * region's build runs the same steps one per slice of the frame budget
 * (`buildModel` in region.ts), which is why this is the drained form
 * and not the other way round.
 *
 * A compressed image uploads as transcodeModelImages left it: at the
 * format and color space its file names, with the mip chain the file
 * carries (none can be generated for compressed storage). Data holding a
 * KTX2 image that was not transcoded throws.
 */
export function createModel(data: ModelData, opts: ModelOptions = {}): Model {
  let steps = modelSteps(data, opts)
  let result = steps.next()
  while (!result.done) result = steps.next()
  return result.value
}

/**
 * createModel as steps: a generator that yields after every GPU create
 * (each image upload, the skin palettes, each part) and returns the
 * Model - what a region's build delegates to (`yield* buildModel(...)`)
 * so a model's creates spread over frames under the slice budget. Closed
 * early (a cancelled build), it frees what it made so far.
 */
export function* modelSteps(data: ModelData, opts: ModelOptions = {}): Generator<void, Model> {
  let label = opts.label
  let uses = modelImageUses(data)
  followTier()
  let row = QUALITY_TIERS[untrack(appQuality)]
  let textures: ModelTexture[] = []
  let bytes = 0
  // Everything freed if the build is closed before it completes: the
  // textures so far, the morph states and record buffers of the parts
  // so far.
  let complete = false
  let built: Model | null = null
  try {
    for (let i = 0; i < data.images.length; i++) {
      let image = data.images[i]!
      let name = label ? label + "-" + image.name : image.name
      let ready = data.textures?.[i]
      let texture: ModelTexture
      let levels: number
      if (ready !== undefined) {
        levels = ready.mipmap ? mipLevels(Math.max(ready.width, ready.height)) : 1
        texture = takeTexture(ready.data, ready.width, ready.height, ready.format, ready.mipmap, { wrap: "repeat", ...textureSampling(row, levels) }, name)
        bytes += textureBytes(ready.width, ready.height, ready.format, ready.mipmap, ready.data)
      } else {
        if (isKtx2(image.bytes)) {
          throw new Error("createModel: image '" + image.name + "' is a KTX2 compressed texture, which is transcoded before the model is created: await transcodeModelImages(data) first (loadModel and loadGltf do)")
        }
        let decoded = decodeImage(image.bytes)
        let format: TextureFormat = uses[i]!.srgb ? "rgba8-srgb" : "rgba8"
        levels = mipLevels(Math.max(decoded.width, decoded.height))
        texture = takeTexture(decoded.data, decoded.width, decoded.height, format, true, { wrap: "repeat", ...textureSampling(row, levels) }, name)
        bytes += textureBytes(decoded.width, decoded.height, format, true, decoded.data)
      }
      liveTextures.set(texture.id, levels)
      textures.push(texture)
      yield
    }
    let make = opts.material ?? ((m: ModelMaterial, maps: ModelMaps, skinned: boolean, vertexColors: boolean, morphed: boolean, instanced: boolean): Material => {
      // An emissive factor of zero is emission OFF (the glTF product rule:
      // factor times texture), so the map is skipped too - no sampler for
      // a term that cannot show.
      let emissive = m.emissive[0] > 0 || m.emissive[1] > 0 || m.emissive[2] > 0
      return standard({
        color: m.color,
        map: maps.map ?? undefined,
        normalMap: maps.normalMap ?? undefined,
        normalScale: m.normalScale,
        metalness: m.metalness,
        roughness: m.roughness,
        metalnessMap: maps.metalnessMap ?? undefined,
        roughnessMap: maps.roughnessMap ?? undefined,
        emissive: emissive ? m.emissive : undefined,
        emissiveIntensity: emissive ? m.emissiveIntensity : undefined,
        emissiveMap: emissive ? maps.emissiveMap ?? undefined : undefined,
        transparent: m.transparent,
        cull: m.doubleSided ? "none" : "back",
        alphaTest: m.alphaMode === "MASK" ? m.alphaCutoff : undefined,
        skinned: skinned || undefined,
        vertexColors: vertexColors || undefined,
        morph: morphed || undefined,
        instanced: instanced || undefined,
      })
    })
    let slot = (index: number | null): TextureId | null => (index === null ? null : textures[index]!.id)
    // One instance per glTF material as today, plus a skinned variant per
    // material the skinned parts bring, a vertex-colored one per material
    // the COLOR_0 parts bring, a morphed and an instanced one likewise (a
    // material shared by parts that differ in any needs two programs -
    // different vertex stages).
    let variants = new Map<string, Material>()
    let materialFor = (index: number, skinned: boolean, vertexColors: boolean, morphed: boolean, instanced: boolean): Material => {
      let m = data.materials[index]
      if (m === undefined) throw new Error("createModel: a part names a missing material " + index)
      let key = index + (skinned ? "|skinned" : "") + (vertexColors ? "|colored" : "") + (morphed ? "|morphed" : "") + (instanced ? "|instanced" : "")
      let made = variants.get(key)
      if (made === undefined) {
        made = make(
          m,
          {
            map: slot(m.map),
            normalMap: slot(m.normalMap),
            emissiveMap: slot(m.emissiveMap),
            metalnessMap: slot(m.metalnessRoughnessMap),
            roughnessMap: slot(m.metalnessRoughnessMap),
          },
          skinned,
          vertexColors,
          morphed,
          instanced,
        )
        variants.set(key, made)
      }
      return made
    }
    let materials = data.materials.map((_, i) => materialFor(i, false, false, false, false))

    let model = createGroup() as Model
    built = model
    model._skins = []
    model._worn = []
    model._body = null
    model._grafts = []
    model.parts = []
    model._textures = textures
    // The node table is pre-order, so every parent group exists before its
    // children reference it.
    let groups: SceneNode[] = data.nodes.map((n) => {
      let group = createGroup()
      setTransform(group, { position: n.position, quaternion: n.rotation, scale: n.scale })
      // The file's transform is the node's rest pose: what a mixer's
      // layers compose over and a spring chain settles to.
      group._rest = Float64Array.from([...n.position, ...n.rotation, ...n.scale])
      return group
    })
    data.nodes.forEach((n, i) => add(n.parent === null ? model : groups[n.parent]!, groups[i]!))
    model.nodes = data.nodes.map((n, i) => (n.extras === undefined ? { name: n.name, node: groups[i]! } : { name: n.name, node: groups[i]!, extras: n.extras }))
    // A glTF node with morph targets owns its parts' weights (the file's
    // node.weights / mesh.weights seed them): one register, one weights
    // texture, every part of the mesh reads it - so setMorphWeights on
    // the node (and a clip's weights track) moves the whole mesh.
    data.nodes.forEach((n, i) => {
      if (n.weights === undefined) return
      let part = data.parts.find((p) => p.node === i && p.geometry.morphs !== undefined)
      if (part === undefined) return
      groups[i]!._morph = createMorphState(part.geometry.morphs!.names, n.weights, label ? label + "-" + n.name + "-weights" : n.name + "-weights")
    })
    // Each skin's palette lives in an rgba32f texture, 4 texels wide, one
    // row per joint (the four columns of that joint's mat4), sized to the
    // RIG: rig size is bounded by texture height (>= 2048 everywhere), not
    // the vertex uniform budget, so there is no joint cap. The texture id
    // is bound as uBones on every mesh drawing the skin and freed with the
    // model's other textures. The bone matrices themselves are the spatial
    // core's job: each joint node carries a palette-row binding (texture,
    // row, inverse bind, the model root as anchor) that enterScene attaches,
    // so the flush writes model-local jointWorld x inverseBind rows whenever
    // joints move - there is no JS palette walk, and posing joints with
    // setTransform is always enough. Identical skins (same joints, same
    // inverse binds - the body/legs and LOD splits exporters produce) share
    // one texture, so their palette is computed and uploaded once. A worn
    // piece's rows move onto another model's joints (bindSkeleton).
    data.skins.forEach((skin, i) => {
      let joints = skin.joints.map((n) => {
        let joint = groups[n]
        if (joint === undefined) throw new Error("createModel: skin " + i + " names a missing node " + n)
        return joint
      })
      let key = skin.joints.join(",")
      let same = model._skins.find(
        (other, j) =>
          data.skins[j]!.joints.join(",") === key &&
          other.inverseBind.length === skin.inverseBind.length &&
          other.inverseBind.every((v, k) => v === skin.inverseBind[k]),
      )
      let texture: TextureId
      if (same !== undefined) texture = same.texture
      else {
        let palette = takeTexture(new Float32Array(skin.joints.length * 16), 4, skin.joints.length, "rgba32f", false, { anisotropy: 1, baseLevel: 0 }, label ? label + "-skin" + i : "skin" + i)
        bytes += textureBytes(4, skin.joints.length, "rgba32f", false, new Float32Array(0))
        textures.push(palette)
        texture = palette.id
      }
      model._skins.push({ texture, joints, inverseBind: skin.inverseBind, jointBounds: skin.jointBounds })
      if (same !== undefined) return
      joints.forEach((joint, j) => {
        ;(joint._palettes ??= []).push({ texture, row: j, post: skin.inverseBind.slice(j * 16, j * 16 + 16), anchor: model })
      })
    })
    if (data.skins.length > 0) yield
    // Each joint's influence box, in joint space: culling-only bounds the
    // flush carries through the pose, united over every skin reaching it.
    refreshJointBounds(model)
    let groupOf = (part: ModelData["parts"][0], index: number): SceneNode => {
      let node = groups[index]
      if (node === undefined) throw new Error("createModel: part '" + part.name + "' names a missing node " + index)
      return node
    }
    for (let part of data.parts) {
      let skinned = part.skin !== null
      let morphed = part.geometry.morphs !== undefined
      let colored = layoutSlot(part.geometry.layout, "aColor") !== null
      let extras = part.extras
      let placements = part.placements ?? []
      if (placements.length > 0) {
        // A population: the geometry once, drawn per placement. The mesh
        // sits at identity under the model root, which anchors its records,
        // and each placement node - the first is `part.node` - carries an
        // instance at identity, so the record the core writes is that
        // node's model-relative world: moving the node (setTransform, a
        // clip, a transition) moves the copy, hiding it hides the copy,
        // and a pick lands on the instance. Skins place by palette and
        // morph weights are per node, so neither can share one geometry;
        // parseGltf never folds them, and a hand-built ModelData that does
        // is refused rather than drawn wrong.
        if (skinned) throw new Error("createModel: part '" + part.name + "' is skinned and has placements; a skinned part cannot be shared")
        if (morphed) throw new Error("createModel: part '" + part.name + "' has morph targets and placements; a morphed part cannot be shared")
        let material = materialFor(part.material, false, colored, false, true)
        let mesh = createInstancedMesh(part.geometry, material, { capacity: placements.length + 1, anchor: model, label: label ? label + "-" + part.name : part.name })
        add(model, mesh)
        let instances = [part.node, ...placements].map((index) => addInstance(mesh, undefined, groupOf(part, index)))
        model.parts.push(extras === undefined ? { name: part.name, mesh, instances } : { name: part.name, mesh, instances, extras })
        yield
        continue
      }
      let material = materialFor(part.material, skinned, colored, morphed, false)
      let mesh = createMesh(part.geometry, material)
      // The part's weights are its glTF node's (above), not its own.
      if (morphed) mesh._morphOwner = groups[part.node] ?? mesh
      if (skinned) {
        // A skinned part's vertices are model-space bind pose and the skin
        // matrices place them, so its mesh hangs off the model root (the
        // spec ignores the node's transform for skinned meshes) and uModel
        // stays the model's own placement.
        let skin = model._skins[part.skin!]
        if (skin === undefined) throw new Error("createModel: part '" + part.name + "' names a missing skin " + part.skin)
        mesh._textures = { uBones: skin.texture }
        // Culled by its joints' boxes, so the box follows the animation.
        mesh._cullJoints = skin.joints.slice()
        add(model, mesh)
      } else {
        add(groupOf(part, part.node), mesh)
      }
      model.parts.push(extras === undefined ? { name: part.name, mesh } : { name: part.name, mesh, extras })
      yield
    }
    model.materials = materials
    model.clips = data.clips
    model.bounds = data.bounds
    model._textureBytes = bytes
    if (data.extras !== undefined) model.extras = data.extras
    if (data.blobs !== undefined) model.blobs = data.blobs
    let disposed = false
    model.dispose = () => {
      if (disposed) return
      // A worn piece comes off first (its rows sit on the body's joints);
      // a body takes its pieces with it (their skins read joints that are
      // about to go).
      if (model._body !== null) unbindSkeleton(model)
      for (let worn of model._worn.slice()) worn.piece.dispose()
      if (model.parent !== null) remove(model)
      // A destroyed model still animating out (its root waits for every
      // part's exit) keeps its textures until it is gone: destroy then
      // dispose is the unmount shape, and a corpse must not sample a
      // destroyed texture.
      if (afterFree(model, () => model.dispose())) return
      disposed = true
      freeModel(model)
      // Core clips a mixer registered; their players drop at the next
      // advance (the leave above already killed the target nodes).
      for (let clip of model.clips) {
        if (clip._core !== undefined) {
          spatial.destroyClip(clip._core)
          clip._core = undefined
        }
        if (clip._coreVariants !== undefined) {
          for (let id of clip._coreVariants.values()) spatial.destroyClip(id)
          clip._coreVariants = undefined
        }
      }
    }
    if (opts.autoFree !== false && getOwner()) onCleanup(() => model.dispose())
    complete = true
    return model
  } finally {
    if (!complete) {
      if (built !== null) freeModel(built)
      else for (let texture of textures) freeTexture(texture)
    }
  }
}

// What a model holds on the GPU, freed: a population's record buffers
// (mesh-owned, the one explicit free), the geometry buffers (shared like
// any part's), the textures (to the pool where it has a class), the
// nodes' weights textures (the parts' entries are off the scene by now).
function freeModel(model: Model): void {
  for (let part of model.parts) {
    if (part.instances !== undefined) disposeInstances(part.mesh as InstancedMesh)
    disposeGeometry(part.mesh.geometry)
  }
  for (let texture of model._textures) freeTexture(texture)
  model._textures = []
  for (let n of model.nodes ?? []) {
    if (n.node._morph !== null) {
      destroyTexture(n.node._morph.texture)
      n.node._morph = null
    }
  }
}

// Build under the reactive scope a loader was called from, captured
// before its first await (an await leaves the reactive context; whatever
// scope happens to be current when the bytes arrive is not the caller's).
// autoFree then frees the model with that scope. A scope gone by the time
// the load settles - a component unmounted mid-load - can own nothing
// any more, so the model is freed on arrival and the promise resolves
// with it disposed: the caller nobody listens to gets a valid, inert
// object rather than a rejection or a leak.
function buildOwned(owner: Owner | null, opts: ModelOptions | undefined, build: () => Model): Model {
  if (opts?.autoFree !== false && owner !== null && isDisposed(owner)) {
    let model = build()
    model.dispose()
    return model
  }
  return runWithOwner(owner, build)
}

/**
 * Read a .glb or .gltf (with its external .bin and image files, resolved
 * next to it) and build the model. The parse runs on the runtime - fine
 * for models of tens of thousands of vertices; bake bigger ones with
 * `sol tool 3d/model` and use loadModel.
 */
export async function loadGltf(path: string, opts?: ModelOptions): Promise<Model> {
  let owner = getOwner()
  let bytes = await file(path).bytes()
  // A .glb usually embeds everything, but external uris are legal there
  // too (some exporters keep images as files), so both containers get
  // the same prefetch; gltfExternalUris is empty for a self-contained one.
  let files = new Map<string, Uint8Array>()
  let dir = path.slice(0, path.lastIndexOf("/") + 1)
  for (let uri of gltfExternalUris(bytes)) {
    if (!files.has(uri)) files.set(uri, await file(dir + decodeURIComponent(uri)).bytes())
  }
  let data = await transcodeModelImages(parseGltf(bytes, (uri) => files.get(uri)!))
  return buildOwned(owner, opts, () => createModel(data, opts))
}

/** Read a baked .sol3m model (`sol tool 3d/model`) and build it: no parsing,
 * the geometry views the file's bytes directly. */
export async function loadModel(path: string, opts?: ModelOptions): Promise<Model> {
  let owner = getOwner()
  let bytes = await file(path).bytes()
  let data = await transcodeModelImages(decodeModel(bytes))
  return buildOwned(owner, opts, () => createModel(data, opts))
}
