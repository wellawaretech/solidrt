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
import { lit } from "./material.ts"
import type { Material } from "./material.ts"
import { add, createGroup, remove, setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"
import { createMesh } from "./mesh.ts"
import type { Mesh } from "./mesh.ts"

/** Anisotropic filtering level for a model's textures: the engines' usual
 * default (Godot ships 2x, Unity's quality presets 2-8x) - enough to keep a
 * tiled surface legible at a grazing angle, cheap on every GPU. Clamped to
 * the device by the runtime. */
const MODEL_ANISOTROPY = 4

/** A glTF material's uploaded textures, by lit() option name; null where
 * the material has none. */
export type ModelMaps = {
  map: TextureId | null
  normalMap: TextureId | null
  emissiveMap: TextureId | null
}

export type ModelOptions = {
  /** The material for each glTF material (default: `lit` with its color,
   * maps, normal scale, emissive and transparency). `maps` holds the
   * uploaded textures by lit() option name. Called once per material -
   * or once per (material, skinned) combination when skinned parts share
   * a material with static ones - and shared by every part using it.
   * `skinned` is true when the material must skin (pass it through to
   * `lit`/`unlit`, or read aJoints/aWeights + uBones yourself).
   * `data.materials` is in file order, so the calls arrive in file order
   * too. */
  material?: (material: ModelMaterial, maps: ModelMaps, skinned: boolean) => Material
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
  /** Detach the model and free its geometry buffers and textures. */
  dispose(): void
}

/**
 * Build the scene object for parsed model data: upload its images (repeat
 * wrap, mipmapped, MODEL_ANISOTROPY), make a material per glTF material,
 * the file's node hierarchy as nested Groups, and a mesh per part under
 * its node. Synchronous - the data is already in memory.
 */
export function createModel(data: ModelData, opts: ModelOptions = {}): Model {
  let label = opts.label
  let textures: TextureId[] = data.images.map((bytes, i) => {
    let image = decodeImage(bytes)
    return createTexture(image.data, image.width, image.height, {
      wrap: "repeat",
      mipmap: true,
      anisotropy: MODEL_ANISOTROPY,
      autoFree: false,
      label: label ? label + "-image" + i : undefined,
    })
  })
  let make = opts.material ?? ((m: ModelMaterial, maps: ModelMaps, skinned: boolean): Material => {
    // An emissive factor of zero is emission OFF (the glTF product rule:
    // factor times texture), so the map is skipped too - no sampler for
    // a term that cannot show.
    let emissive = m.emissive[0] > 0 || m.emissive[1] > 0 || m.emissive[2] > 0
    return lit({
      color: m.color,
      map: maps.map ?? undefined,
      normalMap: maps.normalMap ?? undefined,
      normalScale: m.normalScale,
      emissive: emissive ? m.emissive : undefined,
      emissiveMap: emissive ? maps.emissiveMap ?? undefined : undefined,
      transparent: m.transparent,
      cull: m.doubleSided ? "none" : "back",
      alphaTest: m.alphaMode === "MASK" ? m.alphaCutoff : undefined,
      skinned: skinned || undefined,
    })
  })
  let slot = (index: number | null): TextureId | null => (index === null ? null : textures[index]!)
  // One instance per glTF material as today, plus a skinned variant per
  // material the skinned parts bring (a material shared by a static and
  // a skinned part needs two programs - different vertex stages).
  let variants = new Map<string, Material>()
  let materialFor = (index: number, skinned: boolean): Material => {
    let m = data.materials[index]
    if (m === undefined) throw new Error("createModel: a part names a missing material " + index)
    let key = index + (skinned ? "|skinned" : "")
    let made = variants.get(key)
    if (made === undefined) {
      made = make(m, { map: slot(m.map), normalMap: slot(m.normalMap), emissiveMap: slot(m.emissiveMap) }, skinned)
      variants.set(key, made)
    }
    return made
  }
  let materials = data.materials.map((_, i) => materialFor(i, false))

  let model = createGroup() as Model
  // The node table is pre-order, so every parent group exists before its
  // children reference it.
  let groups: SceneNode[] = data.nodes.map((n) => {
    let group = createGroup()
    setTransform(group, { position: n.position, quaternion: n.rotation, scale: n.scale })
    return group
  })
  data.nodes.forEach((n, i) => add(n.parent === null ? model : groups[n.parent]!, groups[i]!))
  model.nodes = data.nodes.map((n, i) => ({ name: n.name, node: groups[i]! }))
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
  // one texture, so their palette is computed and uploaded once.
  let skinTextures: TextureId[] = []
  data.skins.forEach((skin, i) => {
    let joints = skin.joints.join(",")
    for (let j = 0; j < i; j++) {
      let other = data.skins[j]!
      if (
        other.joints.join(",") === joints &&
        other.inverseBind.length === skin.inverseBind.length &&
        other.inverseBind.every((v, k) => v === skin.inverseBind[k])
      ) {
        skinTextures.push(skinTextures[j]!)
        return
      }
    }
    let texture = createMutableTexture(new Float32Array(skin.joints.length * 16), 4, skin.joints.length, {
      format: "rgba32f",
      autoFree: false,
      label: label ? label + "-skin" + i : "skin" + i,
    })
    textures.push(texture)
    skinTextures.push(texture)
    for (let j = 0; j < skin.joints.length; j++) {
      let joint = groups[skin.joints[j]!]
      if (joint === undefined) throw new Error("createModel: skin " + i + " names a missing node " + skin.joints[j])
      ;(joint._palettes ??= []).push({ texture, row: j, post: skin.inverseBind.slice(j * 16, j * 16 + 16), anchor: model })
    }
  })
  model.parts = data.parts.map((part) => {
    let skinned = part.skin !== null
    let material = materialFor(part.material, skinned)
    let mesh = createMesh(part.geometry, material)
    if (skinned) {
      // A skinned part's vertices are model-space bind pose and the skin
      // matrices place them, so its mesh hangs off the model root (the
      // spec ignores the node's transform for skinned meshes) and uModel
      // stays the model's own placement.
      let texture = skinTextures[part.skin!]
      if (texture === undefined) throw new Error("createModel: part '" + part.name + "' names a missing skin " + part.skin)
      mesh._textures = { uBones: texture }
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
    if (model.parent !== null) remove(model)
    for (let part of model.parts) disposeGeometry(part.mesh.geometry)
    for (let id of textures) destroyTexture(id)
    textures.length = 0
    // Core clips a mixer registered; their players drop at the next
    // advance (the leave above already killed the target nodes).
    for (let clip of model.clips) {
      if (clip._core !== undefined) {
        spatial.destroyClip(clip._core)
        clip._core = undefined
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
