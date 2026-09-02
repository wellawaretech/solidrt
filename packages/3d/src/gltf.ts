// glTF 2.0, the subset an app needs to show authored models: the scene's
// node tree RETAINED - a node table of local TRS with parent links (matrix
// nodes decomposed; shear dropped), vertices in NODE-LOCAL space, one part
// per mesh primitive referencing its node - triangles with positions,
// normals (flat ones generated when absent, per the spec), one UV set and
// indices, and materials reduced to what lit()/unlit() draw - base color
// factor and texture, normal map (with scale), emissive factor and map
// (KHR_materials_emissive_strength folded in), double-sidedness, alpha
// blending and masking - plus the file's animations as baked clips
// (times/values per channel, the mixer's food). Nodes with no part below
// them and no animation channel targeting them (cameras, lights, unused
// empties) are pruned. Both containers: .gltf JSON with external or
// data: buffers and images, and single-file .glb.
//
// Pure module by design - a parse is JSON plus typed-array views plus one
// interleave loop per primitive, so it runs the same under bun (the bake
// tool in tools/model.ts, the check rig) and on flux (loadGltf in
// model.ts). It never decodes images: material.map indexes the encoded
// bytes in `images`, and uploading is the engine side's job.
//
// Outside the subset: Draco/meshopt-compressed meshes and any other
// required extension throw naming it; non-triangle primitives, sparse
// accessors and morph targets (the "weights" channel path) are skipped
// or ignored; vertex colors, tangents and further UV sets are dropped
// (the standard layout has no slot for them yet). Skins ARE parsed:
// joints, inverse binds, and the "skinned" vertex layout.

import { compose, decompose, det3, mat4, multiply } from "./math.ts"
import type { Mat4, Quat, Vec3 } from "./math.ts"
import { layoutStride, packGeometry, packIndices, STANDARD_FLOATS } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"

/** What lit()/unlit() take from a glTF material. */
export type ModelMaterial = {
  name: string
  /** Straight [r, g, b, a] 0..1 (glTF baseColorFactor). */
  color: [number, number, number, number]
  /** Index into ModelData.images (the base color texture), or null. */
  map: number | null
  /** glTF doubleSided; createModel's default material draws it with
   * `cull: "none"`. */
  doubleSided: boolean
  /** alphaMode BLEND; createModel's default material blends it. */
  transparent: boolean
  /** glTF alphaMode as written (default OPAQUE). MASK is a cutout:
   * createModel's default material draws it with `alphaTest: alphaCutoff`. */
  alphaMode: "OPAQUE" | "MASK" | "BLEND"
  /** glTF alphaCutoff (default 0.5); meaningful for MASK only. */
  alphaCutoff: number
  /** Index into ModelData.images (the tangent-space normal map), or null. */
  normalMap: number | null
  /** glTF normalTexture.scale (default 1); meaningful with normalMap. */
  normalScale: number
  /** glTF emissiveFactor (default [0, 0, 0] = off) with
   * KHR_materials_emissive_strength multiplied in. */
  emissive: [number, number, number]
  /** Index into ModelData.images (the emissive map), or null. */
  emissiveMap: number | null
}

/** One node of the model's retained hierarchy: a local TRS under `parent`
 * (an earlier index into ModelData.nodes, or null for a root). Parts and
 * - in rigged models - skin joints and animation tracks reference nodes
 * by index. */
export type ModelNode = {
  /** The glTF node's name (or `node<i>`). */
  name: string
  /** Index of the parent in ModelData.nodes (always lower - the table is
   * in pre-order), or null for a root node. */
  parent: number | null
  position: Vec3
  /** Unit quaternion [x, y, z, w]. */
  rotation: Quat
  scale: Vec3
}

/** One drawable: a mesh node's primitive. Vertices are LOCAL to its
 * node - except a skinned part, whose vertices are in MODEL space at the
 * bind pose (the spec ignores a skinned node's transform; the skin
 * matrices place them). */
export type ModelPart = {
  /** The owning node's name; a node whose mesh has several primitives
   * numbers them `name#<k>`. */
  name: string
  /** Index into ModelData.nodes - the node whose world transform places
   * this part (unused for placement when `skin` is set). */
  node: number
  /** Index into ModelData.skins, or null. A skinned part's geometry has
   * the "skinned" layout (aJoints/aWeights after the standard prefix). */
  skin: number | null
  geometry: Geometry
  /** Index into ModelData.materials. */
  material: number
}

/** One skin: the joints (as node-table indices) and each joint's inverse
 * bind matrix - model space to joint space at the bind pose. The palette
 * a skinned draw needs is jointWorld x inverseBind per joint. */
export type ModelSkin = {
  /** Indices into ModelData.nodes, one per joint. */
  joints: number[]
  /** 16 floats per joint, column-major, in joint order. */
  inverseBind: Float32Array
}

/** One animated property of one node: baked key times and values. */
export type ModelChannel = {
  /** Index into ModelData.nodes. */
  node: number
  path: "position" | "rotation" | "scale"
  /** glTF's three: "step" holds each key, "linear" lerps (a rotation
   * slerps), "cubic" is CUBICSPLINE - a Hermite with per-key tangents. */
  interpolation: "step" | "linear" | "cubic"
  /** Key times in seconds, ascending. */
  times: Float32Array
  /** One element per key: 3 floats for position/scale, 4 (a quaternion)
   * for rotation. "cubic" stores THREE elements per key - in-tangent,
   * value, out-tangent, in that order. */
  values: Float32Array
}

/** One named animation: baked channels over the model's nodes - what the
 * mixer plays and what animation-core will consume. */
export type ModelClip = {
  name: string
  /** Seconds: the largest key time over the channels. */
  duration: number
  channels: ModelChannel[]
  /** @internal The core clip id once a mixer registered it; freed by
   * model.dispose. */
  _core?: number
}

/** A parsed model: plain data, no GPU resources. What parseGltf and
 * decodeModel produce and createModel consumes. */
export type ModelData = {
  /** The retained node hierarchy, pre-order (parents before children),
   * pruned to nodes with a part somewhere below them or an animation
   * channel targeting them. */
  nodes: ModelNode[]
  parts: ModelPart[]
  /** The skins the parts reference (empty for an unrigged model). */
  skins: ModelSkin[]
  /** The file's animations (empty when it has none). */
  clips: ModelClip[]
  materials: ModelMaterial[]
  /** Encoded image files (PNG/JPEG bytes) the materials' `map` index. */
  images: Uint8Array[]
  /** World-space rest-pose [minX, minY, minZ, maxX, maxY, maxZ] over every
   * part - each part's local box through its node's composed transform, so
   * it is conservative (not vertex-tight) for parts under rotated nodes. */
  bounds: Float32Array
}

/** Resolves a relative uri of a .gltf (its .bin buffers, image files) to
 * bytes. Not needed for .glb or data: uris. */
export type UriResolver = (uri: string) => Uint8Array

const GLB_MAGIC = 0x46546c67
const CHUNK_JSON = 0x4e4f534a
const CHUNK_BIN = 0x004e4942
const MODE_TRIANGLES = 4

// The spec's alphaCutoff when a MASK material leaves it out.
const GLTF_ALPHA_CUTOFF = 0.5

// The "skinned" layout's stride (standard prefix + aJoints + aWeights).
const SKINNED_FLOATS = layoutStride("skinned")

const IDENTITY = mat4()

const COMPONENT_BYTES: Record<number, number> = { 5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4 }
const TYPE_ELEMENTS: Record<string, number> = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT2: 4, MAT3: 9, MAT4: 16 }

const DEFAULT_MATERIAL: ModelMaterial = {
  name: "default",
  color: [1, 1, 1, 1],
  map: null,
  doubleSided: false,
  transparent: false,
  alphaMode: "OPAQUE",
  alphaCutoff: GLTF_ALPHA_CUTOFF,
  normalMap: null,
  normalScale: 1,
  emissive: [0, 0, 0],
  emissiveMap: null,
}

/** True when the bytes are a .glb container (the "glTF" magic). */
export function isGlb(bytes: Uint8Array): boolean {
  return bytes.length >= 12 && new DataView(bytes.buffer, bytes.byteOffset, 12).getUint32(0, true) === GLB_MAGIC
}

/** The external uris a document references (buffers and images), so an
 * async caller can fetch them before parseGltf. Usually empty for .glb -
 * but a .glb MAY reference external files (some exporters write image
 * uris), so this reads its JSON chunk rather than assuming. data: uris
 * are never listed. */
export function gltfExternalUris(bytes: Uint8Array): string[] {
  let gltf = isGlb(bytes) ? readGlb(bytes).json : JSON.parse(new TextDecoder().decode(bytes))
  let uris: string[] = []
  for (let item of [...(gltf.buffers ?? []), ...(gltf.images ?? [])]) {
    if (typeof item.uri === "string" && !item.uri.startsWith("data:")) uris.push(item.uri)
  }
  return uris
}

/**
 * Parse a .glb or .gltf into ModelData. `resolve` supplies the bytes of
 * external files by their uri as written in the document (still
 * percent-encoded); a fully self-contained .glb needs none, but a .glb
 * with external image uris (spec-legal, and real exporters write them)
 * resolves the same way.
 */
export function parseGltf(bytes: Uint8Array, resolve?: UriResolver): ModelData {
  let gltf: any
  let bin: Uint8Array | null = null
  if (isGlb(bytes)) {
    let glb = readGlb(bytes)
    gltf = glb.json
    bin = glb.bin
  } else {
    gltf = JSON.parse(new TextDecoder().decode(bytes))
  }
  if (gltf.asset?.version !== undefined && !String(gltf.asset.version).startsWith("2")) {
    throw new Error("parseGltf: glTF version " + gltf.asset.version + " (only 2.x is supported)")
  }
  for (let ext of gltf.extensionsRequired ?? []) {
    if (ext === "KHR_draco_mesh_compression" || ext === "EXT_meshopt_compression") {
      throw new Error("parseGltf: the file's meshes are compressed (" + ext + "), which is not supported: re-export without mesh compression")
    }
    // Quantized attributes read through the normalized-integer path,
    // emissive strength folds into the emissive factor; every other
    // required extension changes what the file means.
    if (ext !== "KHR_mesh_quantization" && ext !== "KHR_materials_emissive_strength") {
      throw new Error("parseGltf: the file requires the " + ext + " extension, which is not supported")
    }
  }

  // Also reachable from a .glb: the container usually embeds everything,
  // but external uris are legal there too (real exporters use them for
  // images), which is why loadGltf resolves for both containers.
  let external = (uri: string, what: string): Uint8Array => {
    if (uri.startsWith("data:")) return decodeDataUri(uri)
    if (resolve === undefined) throw new Error("parseGltf: " + what + " references the external file " + uri + " and no resolver was given")
    return resolve(uri)
  }

  let buffers: Uint8Array[] = (gltf.buffers ?? []).map((b: any, i: number): Uint8Array => {
    if (b.uri === undefined) {
      if (bin === null) throw new Error("parseGltf: buffer " + i + " has no uri and the file has no binary chunk")
      return bin
    }
    return external(b.uri, "buffer " + i)
  })

  let bufferViewBytes = (index: number): Uint8Array => {
    let view = gltf.bufferViews[index]
    let buffer = buffers[view.buffer]
    if (buffer === undefined) throw new Error("parseGltf: bufferView " + index + " names a missing buffer")
    return buffer.subarray(view.byteOffset ?? 0, (view.byteOffset ?? 0) + view.byteLength)
  }

  // Images are pulled in only when a material samples them, in first-use
  // order, so `map` indexes a compact list.
  let images: Uint8Array[] = []
  let imageSlots = new Map<number, number>()
  let imageSlot = (index: number): number => {
    let slot = imageSlots.get(index)
    if (slot === undefined) {
      let image = gltf.images?.[index]
      if (image === undefined) throw new Error("parseGltf: texture names a missing image " + index)
      let bytes = image.uri !== undefined ? external(image.uri, "image " + index) : bufferViewBytes(image.bufferView)
      slot = images.length
      images.push(bytes)
      imageSlots.set(index, slot)
    }
    return slot
  }

  // A texture reference's image slot, or null (the reference absent, or
  // its texture imageless). Further UV sets are outside the subset, so a
  // non-zero texCoord is ignored and the map samples the one UV set.
  let textureSlot = (ref: any): number | null => {
    if (ref === undefined) return null
    let texture = gltf.textures?.[ref.index]
    return texture?.source !== undefined ? imageSlot(texture.source) : null
  }
  let materials: ModelMaterial[] = (gltf.materials ?? []).map((m: any, i: number): ModelMaterial => {
    let pbr = m.pbrMetallicRoughness ?? {}
    let factor = pbr.baseColorFactor ?? [1, 1, 1, 1]
    let emissiveFactor: number[] = m.emissiveFactor ?? [0, 0, 0]
    let strength = m.extensions?.KHR_materials_emissive_strength?.emissiveStrength ?? 1
    return {
      name: m.name ?? "material" + i,
      color: [factor[0], factor[1], factor[2], factor[3] ?? 1],
      map: textureSlot(pbr.baseColorTexture),
      doubleSided: m.doubleSided === true,
      transparent: m.alphaMode === "BLEND",
      alphaMode: m.alphaMode === "MASK" || m.alphaMode === "BLEND" ? m.alphaMode : "OPAQUE",
      alphaCutoff: typeof m.alphaCutoff === "number" ? m.alphaCutoff : GLTF_ALPHA_CUTOFF,
      normalMap: textureSlot(m.normalTexture),
      normalScale: typeof m.normalTexture?.scale === "number" ? m.normalTexture.scale : 1,
      emissive: [(emissiveFactor[0] ?? 0) * strength, (emissiveFactor[1] ?? 0) * strength, (emissiveFactor[2] ?? 0) * strength],
      emissiveMap: textureSlot(m.emissiveTexture),
    }
  })
  // Primitives without a material draw the spec's default; it is appended
  // only when something uses it.
  let defaultMaterial = -1

  let accessorFloats = (index: number, what: string): { data: Float32Array; elements: number; count: number } => {
    let acc = gltf.accessors[index]
    if (acc === undefined) throw new Error("parseGltf: " + what + " names a missing accessor " + index)
    if (acc.sparse !== undefined) throw new Error("parseGltf: " + what + " uses a sparse accessor, which is not supported")
    let elements = TYPE_ELEMENTS[acc.type]
    let compBytes = COMPONENT_BYTES[acc.componentType]
    if (elements === undefined || compBytes === undefined) throw new Error("parseGltf: " + what + " has an unknown accessor type")
    let out = new Float32Array(acc.count * elements)
    if (acc.bufferView === undefined) return { data: out, elements, count: acc.count }
    let view = gltf.bufferViews[acc.bufferView]
    let bytes = bufferViewBytes(acc.bufferView)
    let stride = view.byteStride ?? compBytes * elements
    let dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    let base = acc.byteOffset ?? 0
    let normalized = acc.normalized === true
    let read = readerFor(acc.componentType, normalized)
    for (let i = 0; i < acc.count; i++) {
      let at = base + i * stride
      for (let e = 0; e < elements; e++) out[i * elements + e] = read(dv, at + e * compBytes)
    }
    return { data: out, elements, count: acc.count }
  }

  // The node table is built lazily: a glTF node gets a slot only when a
  // part somewhere below it needs the chain (materialize walks ancestors
  // first, so the table stays in pre-order), which is what prunes cameras,
  // lights and unused empties.
  type PendingNode = { name: string; parent: PendingNode | null; position: Vec3; rotation: Quat; scale: Vec3; index: number | null }
  let nodes: ModelNode[] = []
  let materialize = (p: PendingNode): number => {
    if (p.index !== null) return p.index
    let parent = p.parent === null ? null : materialize(p.parent)
    p.index = nodes.length
    nodes.push({ name: p.name, parent, position: p.position, rotation: p.rotation, scale: p.scale })
    return p.index
  }

  let parts: ModelPart[] = []
  let bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])

  let emit = (prim: any, name: string, node: number, world: Mat4, skin: number | null): void => {
    if ((prim.mode ?? MODE_TRIANGLES) !== MODE_TRIANGLES) return
    if (prim.attributes?.POSITION === undefined) return
    let pos = accessorFloats(prim.attributes.POSITION, name + " POSITION")
    if (pos.elements !== 3) throw new Error("parseGltf: " + name + " POSITION is not VEC3")
    let count = pos.count
    let nrm = prim.attributes.NORMAL !== undefined ? accessorFloats(prim.attributes.NORMAL, name + " NORMAL").data : null
    let uv = prim.attributes.TEXCOORD_0 !== undefined ? accessorFloats(prim.attributes.TEXCOORD_0, name + " TEXCOORD_0").data : null
    // A skinned primitive: joints and weights become the "skinned"
    // layout's extra channels; its vertices are ALREADY model-space bind
    // pose (the node transform is ignored, per spec), so no flip and
    // identity bounds.
    let skinned = skin !== null
    let joints: Float32Array | null = null
    let weights: Float32Array | null = null
    if (skinned) {
      if (prim.attributes.JOINTS_0 === undefined || prim.attributes.WEIGHTS_0 === undefined) {
        throw new Error("parseGltf: " + name + " is skinned but lacks JOINTS_0/WEIGHTS_0")
      }
      let j = accessorFloats(prim.attributes.JOINTS_0, name + " JOINTS_0")
      let w = accessorFloats(prim.attributes.WEIGHTS_0, name + " WEIGHTS_0")
      if (j.elements !== 4 || w.elements !== 4) throw new Error("parseGltf: " + name + " JOINTS_0/WEIGHTS_0 are not VEC4")
      joints = j.data
      weights = w.data
    }
    let stride = skinned ? SKINNED_FLOATS : STANDARD_FLOATS
    let indices: ArrayLike<number> =
      prim.indices !== undefined ? accessorFloats(prim.indices, name + " indices").data : Array.from({ length: count }, (_, i) => i)
    if (indices.length % 3 !== 0) throw new Error("parseGltf: " + name + " index count is not a multiple of 3")

    // A mirroring chain (negative world determinant at the rest pose)
    // flips the displayed winding, so the index order is flipped here to
    // compensate - cull: "back" keeps the outside. Stored normals stay the
    // authored ones: the runtime's inverse-transpose maps them outward for
    // the flipped winding. Baked from the REST pose; a scale animated
    // across zero would unbake it, which is pathological.
    let flip = !skinned && det3(world) < 0

    let vertices: Float32Array
    let packedIndices: number[]
    if (nrm !== null) {
      vertices = new Float32Array(count * stride)
      for (let i = 0; i < count; i++) writeVertex(vertices, i, pos.data, nrm, uv, i, stride, joints, weights)
      packedIndices = new Array(indices.length)
      for (let i = 0; i < indices.length; i += 3) {
        packedIndices[i] = indices[i]!
        packedIndices[i + 1] = flip ? indices[i + 2]! : indices[i + 1]!
        packedIndices[i + 2] = flip ? indices[i + 1]! : indices[i + 2]!
      }
    } else {
      // No normals: the spec asks for flat shading, which needs one vertex
      // per triangle corner, so the primitive is un-indexed here and each
      // corner takes its face normal - of the AUTHORED winding (negated
      // after a flip), the direction authored normals would have, so the
      // runtime maps both the same way.
      let triangles = indices.length / 3
      vertices = new Float32Array(triangles * 3 * stride)
      packedIndices = new Array(triangles * 3)
      let face: Vec3 = [0, 0, 0]
      for (let t = 0; t < triangles; t++) {
        let a = indices[t * 3]!, b = indices[t * 3 + 1]!, c = indices[t * 3 + 2]!
        if (flip) [b, c] = [c, b]
        let out = t * 3
        writeVertex(vertices, out, pos.data, null, uv, a, stride, joints, weights)
        writeVertex(vertices, out + 1, pos.data, null, uv, b, stride, joints, weights)
        writeVertex(vertices, out + 2, pos.data, null, uv, c, stride, joints, weights)
        faceNormal(face, vertices, out, stride)
        if (flip) {
          face[0] = -face[0]
          face[1] = -face[1]
          face[2] = -face[2]
        }
        for (let k = 0; k < 3; k++) {
          let at = (out + k) * stride + 3
          vertices[at] = face[0]
          vertices[at + 1] = face[1]
          vertices[at + 2] = face[2]
          packedIndices[out + k] = out + k
        }
      }
      count = triangles * 3
    }

    // Model bounds: the part's local box through the node's rest-pose
    // world transform (8 corners - conservative under rotation, exact
    // under translation and axis-aligned scale). Skinned vertices are
    // already model-space, so their box goes through identity.
    let bw = skinned ? IDENTITY : world
    let lo: Vec3 = [Infinity, Infinity, Infinity]
    let hi: Vec3 = [-Infinity, -Infinity, -Infinity]
    for (let i = 0; i < count; i++) {
      let at = i * stride
      for (let k = 0; k < 3; k++) {
        let v = vertices[at + k]!
        if (v < lo[k]!) lo[k] = v
        if (v > hi[k]!) hi[k] = v
      }
    }
    for (let corner = 0; corner < 8; corner++) {
      let x = corner & 1 ? hi[0] : lo[0], y = corner & 2 ? hi[1] : lo[1], z = corner & 4 ? hi[2] : lo[2]
      let wx = bw[0] * x + bw[4] * y + bw[8] * z + bw[12]
      let wy = bw[1] * x + bw[5] * y + bw[9] * z + bw[13]
      let wz = bw[2] * x + bw[6] * y + bw[10] * z + bw[14]
      if (wx < bounds[0]!) bounds[0] = wx
      if (wy < bounds[1]!) bounds[1] = wy
      if (wz < bounds[2]!) bounds[2] = wz
      if (wx > bounds[3]!) bounds[3] = wx
      if (wy > bounds[4]!) bounds[4] = wy
      if (wz > bounds[5]!) bounds[5] = wz
    }

    let material = prim.material
    if (material === undefined) {
      if (defaultMaterial < 0) {
        defaultMaterial = materials.length
        materials.push({ ...DEFAULT_MATERIAL })
      }
      material = defaultMaterial
    }
    let geometry: Geometry = skinned
      ? { vertices, indices: packIndices(packedIndices, count), layout: "skinned", label: name }
      : packGeometry(vertices, packedIndices, { label: name })
    parts.push({ name, node, skin, geometry, material })
  }

  let local = mat4()
  let pendingByIndex = new Map<number, PendingNode>()
  let walk = (index: number, parent: PendingNode | null, parentWorld: Mat4): void => {
    let node = gltf.nodes[index]
    if (node === undefined) throw new Error("parseGltf: scene names a missing node " + index)
    let position: Vec3 = [0, 0, 0]
    let rotation: Quat = [0, 0, 0, 1]
    let scale: Vec3 = [1, 1, 1]
    if (node.matrix !== undefined) {
      for (let i = 0; i < 16; i++) local[i] = node.matrix[i]
      decompose(local, position, rotation, scale)
    } else {
      if (node.translation !== undefined) position = [node.translation[0], node.translation[1], node.translation[2]]
      if (node.rotation !== undefined) rotation = [node.rotation[0], node.rotation[1], node.rotation[2], node.rotation[3]]
      if (node.scale !== undefined) scale = [node.scale[0], node.scale[1], node.scale[2]]
    }
    // The world composes from the TRS (not a matrix node's raw matrix), so
    // winding and bounds match what the runtime will render.
    let world = mat4()
    multiply(world, parentWorld, compose(local, position, rotation, scale))
    let pending: PendingNode = { name: node.name ?? "node" + index, parent, position, rotation, scale, index: null }
    pendingByIndex.set(index, pending)
    if (node.mesh !== undefined) {
      let mesh = gltf.meshes[node.mesh]
      if (mesh === undefined) throw new Error("parseGltf: node " + index + " names a missing mesh " + node.mesh)
      if (node.name === undefined && mesh.name !== undefined) pending.name = mesh.name
      let slot = materialize(pending)
      let skin = typeof node.skin === "number" ? node.skin : null
      let prims: any[] = mesh.primitives ?? []
      for (let k = 0; k < prims.length; k++) emit(prims[k], prims.length > 1 ? pending.name + "#" + k : pending.name, slot, world, skin)
    }
    for (let child of node.children ?? []) walk(child, pending, world)
  }

  let scene = gltf.scenes?.[gltf.scene ?? 0]
  let roots: number[] = scene?.nodes ?? (gltf.nodes ?? []).map((_: unknown, i: number) => i)
  let root = mat4()
  for (let index of roots) walk(index, null, root)

  // Skins, after the walk: joints are ordinary nodes (usually meshless),
  // materialized here so the retained table carries them; part.skin
  // remaps from the file's skin index to the compact list.
  let skins: ModelSkin[] = []
  let skinSlots = new Map<number, number>()
  for (let part of parts) {
    if (part.skin === null) continue
    let slot = skinSlots.get(part.skin)
    if (slot === undefined) {
      let sk = gltf.skins?.[part.skin]
      if (sk === undefined) throw new Error("parseGltf: part '" + part.name + "' names a missing skin " + part.skin)
      let jointIndices: number[] = (sk.joints ?? []).map((j: number): number => {
        let pending = pendingByIndex.get(j)
        if (pending === undefined) throw new Error("parseGltf: skin joint node " + j + " is not in the scene")
        return materialize(pending)
      })
      if (jointIndices.length === 0) throw new Error("parseGltf: skin " + part.skin + " has no joints")
      let inverseBind: Float32Array
      if (sk.inverseBindMatrices !== undefined) {
        let acc = accessorFloats(sk.inverseBindMatrices, "skin inverseBindMatrices")
        if (acc.elements !== 16 || acc.count !== jointIndices.length) {
          throw new Error("parseGltf: skin inverseBindMatrices does not match the joint count")
        }
        inverseBind = acc.data
      } else {
        // The spec default: identity binds.
        inverseBind = new Float32Array(jointIndices.length * 16)
        for (let j = 0; j < jointIndices.length; j++) {
          inverseBind[j * 16] = 1
          inverseBind[j * 16 + 5] = 1
          inverseBind[j * 16 + 10] = 1
          inverseBind[j * 16 + 15] = 1
        }
      }
      slot = skins.length
      skins.push({ joints: jointIndices, inverseBind })
      skinSlots.set(part.skin, slot)
    }
    part.skin = slot
  }

  // Animations, after the walk so channels can materialize their target
  // nodes (a channel may target a meshless node - a joint, a rig pivot).
  const CHANNEL_PATHS: Record<string, ModelChannel["path"]> = { translation: "position", rotation: "rotation", scale: "scale" }
  let clips: ModelClip[] = (gltf.animations ?? []).map((anim: any, ai: number): ModelClip => {
    let clipName = anim.name ?? "clip" + ai
    let channels: ModelChannel[] = []
    let duration = 0
    for (let ch of anim.channels ?? []) {
      // Outside the subset: "weights" (morph targets) and extension paths.
      let path = CHANNEL_PATHS[ch.target?.path]
      if (path === undefined) continue
      // A target outside the walked scene has nothing to move.
      let pending = pendingByIndex.get(ch.target.node)
      if (pending === undefined) continue
      let sampler = anim.samplers?.[ch.sampler]
      if (sampler === undefined) throw new Error("parseGltf: animation " + clipName + " channel names a missing sampler " + ch.sampler)
      let what = "animation " + clipName
      let input = accessorFloats(sampler.input, what + " input")
      let output = accessorFloats(sampler.output, what + " output")
      let interpolation: ModelChannel["interpolation"] =
        sampler.interpolation === "STEP" ? "step" : sampler.interpolation === "CUBICSPLINE" ? "cubic" : "linear"
      let elements = path === "rotation" ? 4 : 3
      if (output.elements !== elements) throw new Error("parseGltf: " + what + " " + path + " output is not " + (elements === 4 ? "VEC4" : "VEC3"))
      let perKey = interpolation === "cubic" ? 3 : 1
      if (output.count !== input.count * perKey) {
        throw new Error("parseGltf: " + what + " " + path + " has " + output.count + " values for " + input.count + " keys")
      }
      if (input.count > 0) duration = Math.max(duration, input.data[input.count - 1]!)
      channels.push({ node: materialize(pending), path, interpolation, times: input.data, values: output.data })
    }
    return { name: clipName, duration, channels }
  })

  if (parts.length === 0) bounds.fill(0)
  return { nodes, parts, skins, clips, materials, images, bounds }
}

function readGlb(bytes: Uint8Array): { json: any; bin: Uint8Array | null } {
  let dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let version = dv.getUint32(4, true)
  if (version !== 2) throw new Error("parseGltf: glb version " + version + " (only 2 is supported)")
  let length = Math.min(dv.getUint32(8, true), bytes.byteLength)
  let json: any = null
  let bin: Uint8Array | null = null
  let at = 12
  while (at + 8 <= length) {
    let chunkLength = dv.getUint32(at, true)
    let chunkType = dv.getUint32(at + 4, true)
    let chunk = bytes.subarray(at + 8, at + 8 + chunkLength)
    if (chunkType === CHUNK_JSON) json = JSON.parse(new TextDecoder().decode(chunk))
    else if (chunkType === CHUNK_BIN && bin === null) bin = chunk
    at += 8 + chunkLength
  }
  if (json === null) throw new Error("parseGltf: glb has no JSON chunk")
  return { json, bin }
}

function decodeDataUri(uri: string): Uint8Array {
  let comma = uri.indexOf(",")
  if (comma < 0) throw new Error("parseGltf: malformed data: uri")
  let meta = uri.slice(0, comma)
  let payload = uri.slice(comma + 1)
  if (!meta.endsWith(";base64")) return new TextEncoder().encode(decodeURIComponent(payload))
  let text = atob(payload)
  let out = new Uint8Array(text.length)
  for (let i = 0; i < text.length; i++) out[i] = text.charCodeAt(i)
  return out
}

function readerFor(componentType: number, normalized: boolean): (dv: DataView, at: number) => number {
  switch (componentType) {
    case 5126:
      return (dv, at) => dv.getFloat32(at, true)
    case 5125:
      return (dv, at) => dv.getUint32(at, true)
    case 5123:
      return normalized ? (dv, at) => dv.getUint16(at, true) / 65535 : (dv, at) => dv.getUint16(at, true)
    case 5122:
      return normalized ? (dv, at) => Math.max(dv.getInt16(at, true) / 32767, -1) : (dv, at) => dv.getInt16(at, true)
    case 5121:
      return normalized ? (dv, at) => dv.getUint8(at) / 255 : (dv, at) => dv.getUint8(at)
    default:
      return normalized ? (dv, at) => Math.max(dv.getInt8(at) / 127, -1) : (dv, at) => dv.getInt8(at)
  }
}

// One interleaved vertex, node-local: position and normal copied as
// authored (the runtime's uModel/uNormal do the placing), uv copied or
// zero, and for the skinned layout the joint indices plus their weights
// RENORMALIZED to sum 1 (quantized exports drift a little; the spec asks
// for normalized weights).
function writeVertex(
  out: Float32Array,
  slot: number,
  pos: Float32Array,
  nrm: Float32Array | null,
  uv: Float32Array | null,
  src: number,
  stride: number,
  joints: Float32Array | null = null,
  weights: Float32Array | null = null,
): void {
  let at = slot * stride
  out[at] = pos[src * 3]!
  out[at + 1] = pos[src * 3 + 1]!
  out[at + 2] = pos[src * 3 + 2]!
  if (nrm !== null) {
    out[at + 3] = nrm[src * 3]!
    out[at + 4] = nrm[src * 3 + 1]!
    out[at + 5] = nrm[src * 3 + 2]!
  }
  if (uv !== null) {
    out[at + 6] = uv[src * 2]!
    out[at + 7] = uv[src * 2 + 1]!
  }
  if (joints !== null && weights !== null) {
    let j = src * 4
    out[at + 8] = joints[j]!
    out[at + 9] = joints[j + 1]!
    out[at + 10] = joints[j + 2]!
    out[at + 11] = joints[j + 3]!
    let sum = weights[j]! + weights[j + 1]! + weights[j + 2]! + weights[j + 3]!
    let inv = sum > 1e-8 ? 1 / sum : 0
    out[at + 12] = weights[j]! * inv
    out[at + 13] = weights[j + 1]! * inv
    out[at + 14] = weights[j + 2]! * inv
    out[at + 15] = weights[j + 3]! * inv
  }
}

// The unit normal of the triangle at three consecutive vertex slots.
function faceNormal(out: Vec3, v: Float32Array, first: number, stride: number): void {
  let a = first * stride, b = (first + 1) * stride, c = (first + 2) * stride
  let abx = v[b]! - v[a]!, aby = v[b + 1]! - v[a + 1]!, abz = v[b + 2]! - v[a + 2]!
  let acx = v[c]! - v[a]!, acy = v[c + 1]! - v[a + 1]!, acz = v[c + 2]! - v[a + 2]!
  let nx = aby * acz - abz * acy
  let ny = abz * acx - abx * acz
  let nz = abx * acy - aby * acx
  let len = Math.hypot(nx, ny, nz)
  if (len > 1e-12) {
    out[0] = nx / len
    out[1] = ny / len
    out[2] = nz / len
  } else {
    out[0] = 0
    out[1] = 1
    out[2] = 0
  }
}
