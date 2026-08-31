// glTF 2.0, the subset an app needs to show authored models: the scene's
// node tree with world transforms BAKED into the vertices (one part per
// mesh node, its name kept), triangles with positions, normals (flat ones
// generated when absent, per the spec), one UV set and indices, and
// materials reduced to what lit()/unlit() draw - base color factor and
// texture, normal map (with scale), emissive factor and map
// (KHR_materials_emissive_strength folded in), double-sidedness, alpha
// blending and masking. Both containers: .gltf JSON with external or
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
// accessors and morph/skin data are skipped or ignored; vertex colors,
// tangents and further UV sets are dropped (the standard layout has no
// slot for them yet).

import { compose, mat4, multiply, normalMatrix } from "./math.ts"
import type { Mat4, Quat, Vec3 } from "./math.ts"
import { packGeometry, STANDARD_FLOATS } from "./geometry.ts"
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

/** One drawable: a mesh node's primitive, vertices in WORLD space. */
export type ModelPart = {
  /** The glTF node's name (or the mesh's, or `node<i>`); a node with
   * several primitives numbers them `name#<k>`. */
  name: string
  geometry: Geometry
  /** Index into ModelData.materials. */
  material: number
}

/** A parsed model: plain data, no GPU resources. What parseGltf and
 * decodeModel produce and createModel consumes. */
export type ModelData = {
  parts: ModelPart[]
  materials: ModelMaterial[]
  /** Encoded image files (PNG/JPEG bytes) the materials' `map` index. */
  images: Uint8Array[]
  /** World-space [minX, minY, minZ, maxX, maxY, maxZ] over every part. */
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

/** The external uris a .gltf document references (buffers and images), so
 * an async caller can fetch them before parseGltf. Empty for .glb and
 * data: uris. */
export function gltfExternalUris(bytes: Uint8Array): string[] {
  if (isGlb(bytes)) return []
  let gltf = JSON.parse(new TextDecoder().decode(bytes))
  let uris: string[] = []
  for (let item of [...(gltf.buffers ?? []), ...(gltf.images ?? [])]) {
    if (typeof item.uri === "string" && !item.uri.startsWith("data:")) uris.push(item.uri)
  }
  return uris
}

/**
 * Parse a .glb or .gltf into ModelData. `resolve` supplies the bytes of a
 * .gltf's external files by their uri as written in the document (still
 * percent-encoded); omit it for .glb.
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

  let parts: ModelPart[] = []
  let bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
  let normal = mat4()

  let emit = (prim: any, name: string, world: Mat4): void => {
    if ((prim.mode ?? MODE_TRIANGLES) !== MODE_TRIANGLES) return
    if (prim.attributes?.POSITION === undefined) return
    let pos = accessorFloats(prim.attributes.POSITION, name + " POSITION")
    if (pos.elements !== 3) throw new Error("parseGltf: " + name + " POSITION is not VEC3")
    let count = pos.count
    let nrm = prim.attributes.NORMAL !== undefined ? accessorFloats(prim.attributes.NORMAL, name + " NORMAL").data : null
    let uv = prim.attributes.TEXCOORD_0 !== undefined ? accessorFloats(prim.attributes.TEXCOORD_0, name + " TEXCOORD_0").data : null
    let indices: ArrayLike<number> =
      prim.indices !== undefined ? accessorFloats(prim.indices, name + " indices").data : Array.from({ length: count }, (_, i) => i)
    if (indices.length % 3 !== 0) throw new Error("parseGltf: " + name + " index count is not a multiple of 3")

    normalMatrix(normal, world)
    let flip = det3(world) < 0

    let vertices: Float32Array
    let packedIndices: number[]
    if (nrm !== null) {
      vertices = new Float32Array(count * STANDARD_FLOATS)
      for (let i = 0; i < count; i++) writeVertex(vertices, i, pos.data, nrm, uv, i, world, normal)
      packedIndices = new Array(indices.length)
      for (let i = 0; i < indices.length; i += 3) {
        packedIndices[i] = indices[i]!
        packedIndices[i + 1] = flip ? indices[i + 2]! : indices[i + 1]!
        packedIndices[i + 2] = flip ? indices[i + 1]! : indices[i + 2]!
      }
    } else {
      // No normals: the spec asks for flat shading, which needs one vertex
      // per triangle corner, so the primitive is un-indexed here and each
      // corner takes its face normal (computed in world space, after the
      // bake, so a mirroring transform is already accounted for).
      let triangles = indices.length / 3
      vertices = new Float32Array(triangles * 3 * STANDARD_FLOATS)
      packedIndices = new Array(triangles * 3)
      let face: Vec3 = [0, 0, 0]
      for (let t = 0; t < triangles; t++) {
        let a = indices[t * 3]!, b = indices[t * 3 + 1]!, c = indices[t * 3 + 2]!
        if (flip) [b, c] = [c, b]
        let out = t * 3
        writeVertex(vertices, out, pos.data, null, uv, a, world, normal)
        writeVertex(vertices, out + 1, pos.data, null, uv, b, world, normal)
        writeVertex(vertices, out + 2, pos.data, null, uv, c, world, normal)
        faceNormal(face, vertices, out)
        for (let k = 0; k < 3; k++) {
          let at = (out + k) * STANDARD_FLOATS + 3
          vertices[at] = face[0]
          vertices[at + 1] = face[1]
          vertices[at + 2] = face[2]
          packedIndices[out + k] = out + k
        }
      }
      count = triangles * 3
    }

    for (let i = 0; i < count; i++) {
      let at = i * STANDARD_FLOATS
      let x = vertices[at]!, y = vertices[at + 1]!, z = vertices[at + 2]!
      if (x < bounds[0]!) bounds[0] = x
      if (y < bounds[1]!) bounds[1] = y
      if (z < bounds[2]!) bounds[2] = z
      if (x > bounds[3]!) bounds[3] = x
      if (y > bounds[4]!) bounds[4] = y
      if (z > bounds[5]!) bounds[5] = z
    }

    let material = prim.material
    if (material === undefined) {
      if (defaultMaterial < 0) {
        defaultMaterial = materials.length
        materials.push({ ...DEFAULT_MATERIAL })
      }
      material = defaultMaterial
    }
    parts.push({ name, geometry: packGeometry(vertices, packedIndices, { label: name }), material })
  }

  let local = mat4()
  let walk = (index: number, parent: Mat4): void => {
    let node = gltf.nodes[index]
    if (node === undefined) throw new Error("parseGltf: scene names a missing node " + index)
    let world = mat4()
    multiply(world, parent, nodeMatrix(local, node))
    if (node.mesh !== undefined) {
      let mesh = gltf.meshes[node.mesh]
      if (mesh === undefined) throw new Error("parseGltf: node " + index + " names a missing mesh " + node.mesh)
      let name = node.name ?? mesh.name ?? "node" + index
      let prims: any[] = mesh.primitives ?? []
      for (let k = 0; k < prims.length; k++) emit(prims[k], prims.length > 1 ? name + "#" + k : name, world)
    }
    for (let child of node.children ?? []) walk(child, world)
  }

  let scene = gltf.scenes?.[gltf.scene ?? 0]
  let roots: number[] = scene?.nodes ?? (gltf.nodes ?? []).map((_: unknown, i: number) => i)
  let root = mat4()
  for (let index of roots) walk(index, root)

  if (parts.length === 0) bounds.fill(0)
  return { parts, materials, images, bounds }
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

const NO_TRANSLATION: Vec3 = [0, 0, 0]
const NO_ROTATION: Quat = [0, 0, 0, 1]
const NO_SCALE: Vec3 = [1, 1, 1]

function nodeMatrix(out: Mat4, node: any): Mat4 {
  if (node.matrix !== undefined) {
    for (let i = 0; i < 16; i++) out[i] = node.matrix[i]
    return out
  }
  return compose(out, node.translation ?? NO_TRANSLATION, node.rotation ?? NO_ROTATION, node.scale ?? NO_SCALE)
}

function det3(m: Mat4): number {
  return (
    m[0] * (m[5] * m[10] - m[6] * m[9]) -
    m[4] * (m[1] * m[10] - m[2] * m[9]) +
    m[8] * (m[1] * m[6] - m[2] * m[5])
  )
}

// One interleaved vertex: position through the world matrix, normal (when
// given) through the normal matrix and re-normalized, uv copied or zero.
function writeVertex(
  out: Float32Array,
  slot: number,
  pos: Float32Array,
  nrm: Float32Array | null,
  uv: Float32Array | null,
  src: number,
  world: Mat4,
  normal: Mat4,
): void {
  let at = slot * STANDARD_FLOATS
  let x = pos[src * 3]!, y = pos[src * 3 + 1]!, z = pos[src * 3 + 2]!
  out[at] = world[0] * x + world[4] * y + world[8] * z + world[12]
  out[at + 1] = world[1] * x + world[5] * y + world[9] * z + world[13]
  out[at + 2] = world[2] * x + world[6] * y + world[10] * z + world[14]
  if (nrm !== null) {
    let nx = nrm[src * 3]!, ny = nrm[src * 3 + 1]!, nz = nrm[src * 3 + 2]!
    let wx = normal[0] * nx + normal[4] * ny + normal[8] * nz
    let wy = normal[1] * nx + normal[5] * ny + normal[9] * nz
    let wz = normal[2] * nx + normal[6] * ny + normal[10] * nz
    let len = Math.hypot(wx, wy, wz)
    if (len > 1e-12) {
      wx /= len
      wy /= len
      wz /= len
    } else {
      wx = 0
      wy = 1
      wz = 0
    }
    out[at + 3] = wx
    out[at + 4] = wy
    out[at + 5] = wz
  }
  if (uv !== null) {
    out[at + 6] = uv[src * 2]!
    out[at + 7] = uv[src * 2 + 1]!
  }
}

// The unit normal of the triangle at three consecutive vertex slots.
function faceNormal(out: Vec3, v: Float32Array, first: number): void {
  let a = first * STANDARD_FLOATS, b = (first + 1) * STANDARD_FLOATS, c = (first + 2) * STANDARD_FLOATS
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
