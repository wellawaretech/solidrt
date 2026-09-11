// Models in a scene: ModelData (parsed glTF or a decoded .srtm) becomes a
// Group carrying the file's node hierarchy - nested Groups with the file's
// local TRS, each part's mesh under its node - with the images uploaded as
// textures and a material per glTF material: Three's `gltf.scene`, an
// object you add to the scene and place with setTransform, whose named
// nodes (`model.nodes`) can be moved individually. The model owns what it
// created (geometry buffers, textures): dispose() frees them and detaches
// the group. loadGltf / loadModel are the read-then-create conveniences
// over flux:fs; parseGltf / decodeModel + createModel are the primitives
// under them, for bytes obtained any other way (a binary import, a fetch).

import { file } from "flux:fs"
import * as spatial from "flux:spatial"
import { decodeImage } from "@solidrt/core"
import { createMutableTexture, createTexture, destroyTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { gltfExternalUris, parseGltf } from "./gltf.ts"
import type { ModelClip, ModelData, ModelMaterial } from "./gltf.ts"
import { decodeModel } from "./model-file.ts"
import { disposeGeometry } from "./geometry-gpu.ts"
import { layoutSlot } from "./geometry.ts"
import { standard } from "./material.ts"
import type { Material } from "./material.ts"
import { add, afterFree, createGroup, createMorphState, remove, setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"
import { createMesh } from "./mesh.ts"
import type { Mesh } from "./mesh.ts"
import { refreshJointBounds, unbindSkeleton } from "./skeleton.ts"
import type { WornPiece } from "./skeleton.ts"

/** Anisotropic filtering level for a model's textures: the engines' usual
 * default (Godot ships 2x, Unity's quality presets 2-8x) - enough to keep a
 * tiled surface legible at a grazing angle, cheap on every GPU. Clamped to
 * the device by the runtime. */
const MODEL_ANISOTROPY = 4

/** A glTF material's uploaded textures, by lit()/standard() option name;
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
   * renders its metals near black, so set one, or return `lit` here for
   * the Blinn-Phong look). `maps` holds the uploaded textures by
   * lit()/standard() option name. Called once per material -
   * or once per (material, skinned, vertexColors, morphed) combination
   * when parts that differ in any share a material - and shared by every
   * part using it. `skinned` is true when the material must skin (pass it
   * through to `lit`/`unlit`, or read aJoints/aWeights + uBones yourself);
   * `vertexColors` is true when the part carries COLOR_0 in aColor (pass
   * it through, or read aColor yourself); `morphed` is true when the part
   * carries morph targets (pass it through as `morph`, or splice
   * MORPH_DECLS/MORPH_APPLY yourself). `data.materials` is in file order,
   * so the calls arrive in file order too. */
  material?: (material: ModelMaterial, maps: ModelMaps, skinned: boolean, vertexColors: boolean, morphed: boolean) => Material
  /** Debug name for the textures. */
  label?: string
}

/** A model in the scene: a Group carrying the file's node hierarchy as
 * nested Groups, with each part's mesh a child of its node. */
export type Model = SceneNode & {
  kind: "group"
  /** The parts by name, in file order; each `mesh` sits under its node. */
  parts: { name: string; mesh: Mesh }[]
  /** The file's retained nodes in table order (parents first), each an
   * ordinary Group under the model: `setTransform` on one moves its
   * subtree - a wheel spins relative to the axle it hangs from. Names
   * repeat when the file's do; find yours with `.find()`/`.filter()`. */
  nodes: { name: string; node: SceneNode }[]
  /** One per glTF material, in file order. */
  materials: Material[]
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
 * Build the scene object for parsed model data: upload its images (repeat
 * wrap, mipmapped, MODEL_ANISOTROPY; the base color and emissive images
 * as "rgba8-srgb" since glTF stores those sRGB-encoded, the data maps as
 * rgba8), make a material per glTF material, the file's node hierarchy as
 * nested Groups, and a mesh per part under its node. Synchronous - the
 * data is already in memory.
 */
export function createModel(data: ModelData, opts: ModelOptions = {}): Model {
  let label = opts.label
  let colorImages = new Set<number>()
  for (let m of data.materials) {
    if (m.map !== null) colorImages.add(m.map)
    if (m.emissiveMap !== null) colorImages.add(m.emissiveMap)
  }
  let textures: TextureId[] = data.images.map((bytes, i) => {
    let image = decodeImage(bytes)
    return createTexture(image.data, image.width, image.height, {
      format: colorImages.has(i) ? "rgba8-srgb" : "rgba8",
      wrap: "repeat",
      mipmap: true,
      anisotropy: MODEL_ANISOTROPY,
      autoFree: false,
      label: label ? label + "-image" + i : undefined,
    })
  })
  let make = opts.material ?? ((m: ModelMaterial, maps: ModelMaps, skinned: boolean, vertexColors: boolean, morphed: boolean): Material => {
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
    })
  })
  let slot = (index: number | null): TextureId | null => (index === null ? null : textures[index]!)
  // One instance per glTF material as today, plus a skinned variant per
  // material the skinned parts bring and a vertex-colored one per material
  // the COLOR_0 parts bring (a material shared by parts that differ in
  // either needs two programs - different vertex stages).
  let variants = new Map<string, Material>()
  let materialFor = (index: number, skinned: boolean, vertexColors: boolean, morphed: boolean): Material => {
    let m = data.materials[index]
    if (m === undefined) throw new Error("createModel: a part names a missing material " + index)
    let key = index + (skinned ? "|skinned" : "") + (vertexColors ? "|colored" : "") + (morphed ? "|morphed" : "")
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
      )
      variants.set(key, made)
    }
    return made
  }
  let materials = data.materials.map((_, i) => materialFor(i, false, false, false))

  let model = createGroup() as Model
  model._skins = []
  model._worn = []
  model._body = null
  model._grafts = []
  // The node table is pre-order, so every parent group exists before its
  // children reference it.
  let groups: SceneNode[] = data.nodes.map((n) => {
    let group = createGroup()
    setTransform(group, { position: n.position, quaternion: n.rotation, scale: n.scale })
    return group
  })
  data.nodes.forEach((n, i) => add(n.parent === null ? model : groups[n.parent]!, groups[i]!))
  model.nodes = data.nodes.map((n, i) => ({ name: n.name, node: groups[i]! }))
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
    let texture =
      same?.texture ??
      createMutableTexture(new Float32Array(skin.joints.length * 16), 4, skin.joints.length, {
        format: "rgba32f",
        autoFree: false,
        label: label ? label + "-skin" + i : "skin" + i,
      })
    model._skins.push({ texture, joints, inverseBind: skin.inverseBind, jointBounds: skin.jointBounds })
    if (same !== undefined) return
    textures.push(texture)
    joints.forEach((joint, j) => {
      ;(joint._palettes ??= []).push({ texture, row: j, post: skin.inverseBind.slice(j * 16, j * 16 + 16), anchor: model })
    })
  })
  // Each joint's influence box, in joint space: culling-only bounds the
  // flush carries through the pose, united over every skin reaching it.
  refreshJointBounds(model)
  model.parts = data.parts.map((part) => {
    let skinned = part.skin !== null
    let morphed = part.geometry.morphs !== undefined
    let material = materialFor(part.material, skinned, layoutSlot(part.geometry.layout, "aColor") !== null, morphed)
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
      let node = groups[part.node]
      if (node === undefined) throw new Error("createModel: part '" + part.name + "' names a missing node " + part.node)
      add(node, mesh)
    }
    return { name: part.name, mesh }
  })
  model.materials = materials
  model.clips = data.clips
  model.bounds = data.bounds
  model.dispose = () => {
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
    for (let part of model.parts) disposeGeometry(part.mesh.geometry)
    for (let id of textures) destroyTexture(id)
    textures.length = 0
    // The nodes' weights textures (the parts' entries are off the scene
    // with the remove above).
    for (let n of model.nodes) {
      if (n.node._morph !== null) {
        destroyTexture(n.node._morph.texture)
        n.node._morph = null
      }
    }
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
  return model
}

/**
 * Read a .glb or .gltf (with its external .bin and image files, resolved
 * next to it) and build the model. The parse runs on the runtime - fine
 * for models of tens of thousands of vertices; bake bigger ones with
 * `srt tool 3d/model` and use loadModel.
 */
export async function loadGltf(path: string, opts?: ModelOptions): Promise<Model> {
  let bytes = await file(path).bytes()
  // A .glb usually embeds everything, but external uris are legal there
  // too (some exporters keep images as files), so both containers get
  // the same prefetch; gltfExternalUris is empty for a self-contained one.
  let files = new Map<string, Uint8Array>()
  let dir = path.slice(0, path.lastIndexOf("/") + 1)
  for (let uri of gltfExternalUris(bytes)) {
    if (!files.has(uri)) files.set(uri, await file(dir + decodeURIComponent(uri)).bytes())
  }
  return createModel(parseGltf(bytes, (uri) => files.get(uri)!), opts)
}

/** Read a baked .srtm model (`srt tool 3d/model`) and build it: no parsing,
 * the geometry views the file's bytes directly. */
export async function loadModel(path: string, opts?: ModelOptions): Promise<Model> {
  return createModel(decodeModel(await file(path).bytes()), opts)
}
