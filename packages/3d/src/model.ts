// Models in a scene: ModelData (parsed glTF or a decoded .srtm) becomes a
// Group of meshes, one per part, with the images uploaded as textures and
// a material per glTF material - Three's `gltf.scene`, an object you add
// to the scene and place with setTransform. The model owns what it
// created (geometry buffers, textures): dispose() frees them and detaches
// the group. loadGltf / loadModel are the read-then-create conveniences
// over flux:fs; parseGltf / decodeModel + createModel are the primitives
// under them, for bytes obtained any other way (a binary import, a fetch).

import { file } from "flux:fs"
import { decodeImage } from "@solidrt/core"
import { createTexture, destroyTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { gltfExternalUris, isGlb, parseGltf } from "./gltf.ts"
import type { ModelData, ModelMaterial } from "./gltf.ts"
import { decodeModel } from "./model-file.ts"
import { disposeGeometry } from "./geometry-gpu.ts"
import { lit } from "./material.ts"
import type { Material } from "./material.ts"
import { add, createGroup, createMesh, remove } from "./scene.ts"
import type { Mesh, SceneNode } from "./scene.ts"

/** Anisotropic filtering level for a model's textures: the engines' usual
 * default (Godot ships 2x, Unity's quality presets 2-8x) - enough to keep a
 * tiled surface legible at a grazing angle, cheap on every GPU. Clamped to
 * the device by the runtime. */
const MODEL_ANISOTROPY = 4

export type ModelOptions = {
  /** The material for each glTF material (default: `lit` with its color,
   * map and transparency). `map` is the uploaded base color texture, or
   * null. Called once per material, shared by every part using it. */
  material?: (material: ModelMaterial, map: TextureId | null) => Material
  /** Debug name for the textures. */
  label?: string
}

/** A model in the scene: a Group whose children are the parts' meshes. */
export type Model = SceneNode & {
  kind: "group"
  /** The parts by name, in file order; each `mesh` is a child of the model. */
  parts: { name: string; mesh: Mesh }[]
  /** One per glTF material, in file order. */
  materials: Material[]
  /** Local [minX, minY, minZ, maxX, maxY, maxZ] over every part. */
  bounds: Float32Array
  /** Detach the model and free its geometry buffers and textures. */
  dispose(): void
}

/**
 * Build the scene object for parsed model data: upload its images (repeat
 * wrap, mipmapped, MODEL_ANISOTROPY), make a material per glTF material, a
 * mesh per part, all under one Group. Synchronous - the data is already in
 * memory.
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
  let make = opts.material ?? ((m: ModelMaterial, map: TextureId | null): Material => lit({
        color: m.color,
        map: map ?? undefined,
        transparent: m.transparent,
        cull: m.doubleSided ? "none" : "back",
        alphaTest: m.alphaMode === "MASK" ? m.alphaCutoff : undefined,
      }))
  let materials = data.materials.map((m) => make(m, m.map === null ? null : textures[m.map]!))

  let model = createGroup() as Model
  model.parts = data.parts.map((part) => {
    let material = materials[part.material]
    if (material === undefined) throw new Error("createModel: part '" + part.name + "' names a missing material " + part.material)
    let mesh = createMesh(part.geometry, material)
    add(model, mesh)
    return { name: part.name, mesh }
  })
  model.materials = materials
  model.bounds = data.bounds
  model.dispose = () => {
    if (model.parent !== null) remove(model)
    for (let part of model.parts) disposeGeometry(part.mesh.geometry)
    for (let id of textures) destroyTexture(id)
    textures.length = 0
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
  let files = new Map<string, Uint8Array>()
  if (!isGlb(bytes)) {
    let dir = path.slice(0, path.lastIndexOf("/") + 1)
    for (let uri of gltfExternalUris(bytes)) {
      if (!files.has(uri)) files.set(uri, await file(dir + decodeURIComponent(uri)).bytes())
    }
  }
  return createModel(parseGltf(bytes, (uri) => files.get(uri)!), opts)
}

/** Read a baked .srtm model (`srt tool 3d/model`) and build it: no parsing,
 * the geometry views the file's bytes directly. */
export async function loadModel(path: string, opts?: ModelOptions): Promise<Model> {
  return createModel(decodeModel(await file(path).bytes()), opts)
}
