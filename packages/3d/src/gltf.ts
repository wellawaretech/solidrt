// glTF 2.0, the subset an app needs to show authored models: the scene's
// node tree RETAINED - a node table of local TRS with parent links (matrix
// nodes decomposed; shear dropped), vertices in NODE-LOCAL space, one part
// per mesh primitive referencing its node - positions, normals (flat ones
// generated when absent, per the spec, for triangles), one UV set and
// indices in the primitive's topology (strips and fans unrolled to
// triangle lists, a line loop closed into a strip; lines and points keep
// theirs on the geometry), and materials reduced to what phong()/unlit()
// draw - base color
// factor and texture, normal map (with scale), emissive factor and map
// (KHR_materials_emissive_strength as the intensity), double-sidedness, alpha
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
// required extension throw naming it; tangents (the base channel and
// the morph target one) and further UV sets are dropped. Sparse
// accessors are read (what exporters write morph targets as), and
// morph targets land packed on the part's geometry with the mesh's
// weights on its node and the "weights" channel path kept. COLOR_0 IS
// parsed, linear and premultiplied as glTF stores it, into the aColor
// channel the stock materials read under vertexColors: the "colored"
// layout for a static primitive, the skinned list plus aColor for a
// rigged one. Skins ARE parsed: joints, inverse binds, and the "skinned"
// vertex layout.

import { compose, decompose, det3, mat4, multiply } from "./math.ts"
import { linearToSrgb } from "./color.ts"
import type { Mat4, Quat, Vec3 } from "./math.ts"
import { attributeAccess, layoutKey, layoutStride, packIndices, packMorphTargets, vertexView } from "./geometry.ts"
import type { AttributeAccess, MorphTarget, VertexLayout } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { Topology, VertexAttribute, VertexFormat } from "@solidrt/core/gpu"

/** What phong()/unlit() take from a glTF material. */
export type ModelMaterial = {
  name: string
  /** Straight [r, g, b, a] 0..1: glTF's LINEAR baseColorFactor encoded
   * to sRGB, what phong({ color }) takes. */
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
  /** glTF's linear emissiveFactor (default [0, 0, 0] = off) encoded to
   * sRGB, what phong({ emissive }) takes. */
  emissive: [number, number, number]
  /** KHR_materials_emissive_strength (default 1): phong's emissiveIntensity. */
  emissiveIntensity: number
  /** Index into ModelData.images (the emissive map), or null. */
  emissiveMap: number | null
  /** glTF metallicFactor (default 1): standard's metalness. */
  metalness: number
  /** glTF roughnessFactor (default 1): standard's roughness. */
  roughness: number
  /** Index into ModelData.images (glTF's ONE packed
   * metallicRoughnessTexture: green = roughness, blue = metalness), or
   * null; createModel hands it to `material` as both maps.metalnessMap
   * and maps.roughnessMap, standard's two channel-select options. */
  metalnessRoughnessMap: number | null
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
  /** Present on a node whose mesh carries morph targets: the initial
   * weight per target (the node's own `weights`, else the mesh's, else
   * zeros), what the node's parts draw with until a clip or
   * setMorphWeights writes them. */
  weights?: number[]
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
   * the "skinned" layout (aJoints/aWeights after the base prefix). */
  skin: number | null
  /** The part's vertices and indices. The layout is the base prefix,
   * aJoints/aWeights for a rigged primitive, aColor for one with COLOR_0
   * (premultiplied linear), each channel in the file's own format when
   * the vertex vocabulary has it (a quantized uv, a byte color, u8/u16
   * joints) and float otherwise - except joints, an integer channel:
   * an off-spec float JOINTS_0 lands in uint16x4. Positions and normals
   * are always float.
   * The preset name ("base", "colored", "skinned") is used when the
   * list equals it. */
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
  /** 6 floats per joint: the box, in the JOINT's space, of every vertex it
   * influences - the runtime culls a skinned part by the union of these
   * carried through the posed joints, so the box follows the animation
   * (Godot's and Unity's per-bone bounds). A joint influencing nothing
   * holds an inverted box (min > max). */
  jointBounds: Float32Array
}

/** One animated property of one node: baked key times and values. */
export type ModelChannel = {
  /** Index into ModelData.nodes. */
  node: number
  /** The node's local TRS, or "weights": the morph target weights of the
   * node's mesh (glTF's weights path). */
  path: "position" | "rotation" | "scale" | "weights"
  /** glTF's three: "step" holds each key, "linear" lerps (a rotation
   * slerps), "cubic" is CUBICSPLINE - a Hermite with per-key tangents. */
  interpolation: "step" | "linear" | "cubic"
  /** Key times in seconds, ascending. */
  times: Float32Array
  /** One element per key: 3 floats for position/scale, 4 (a quaternion)
   * for rotation, one per morph target for weights (`channelElements`
   * reads it off the data). "cubic" stores THREE elements per key -
   * in-tangent, value, out-tangent, in that order. */
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
  /** @internal Root-corrected core clips (travel pinned and/or height
   * rebased) by variant key, registered on first such play; freed by
   * model.dispose. */
  _coreVariants?: Map<string, number>
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
   * part - each part's local box through its node's composed transform, and
   * a skinned part's per-joint boxes through the joints' rest transforms
   * (where the skin places its vertices, armature scale included) - so it
   * is conservative (not vertex-tight) for parts under rotated nodes and
   * for skinned parts. */
  bounds: Float32Array
}

/** Resolves a relative uri of a .gltf (its .bin buffers, image files) to
 * bytes. Not needed for .glb or data: uris. */
export type UriResolver = (uri: string) => Uint8Array

const GLB_MAGIC = 0x46546c67
const CHUNK_JSON = 0x4e4f534a
const CHUNK_BIN = 0x004e4942
// glTF primitive modes (the GL enum values).
const MODE_POINTS = 0
const MODE_LINES = 1
const MODE_LINE_LOOP = 2
const MODE_LINE_STRIP = 3
const MODE_TRIANGLES = 4
const MODE_TRIANGLE_STRIP = 5
const MODE_TRIANGLE_FAN = 6

/** A triangle strip unrolled to a list: triangle t is (t, t+1, t+2),
 * every odd one with its last two swapped so the winding stays put. */
function stripTriangles(strip: ArrayLike<number>): number[] {
  let out: number[] = []
  for (let t = 0; t + 2 < strip.length; t++) {
    if (t % 2 === 0) out.push(strip[t]!, strip[t + 1]!, strip[t + 2]!)
    else out.push(strip[t]!, strip[t + 2]!, strip[t + 1]!)
  }
  return out
}

/** A triangle fan unrolled to a list: every triangle shares the hub. */
function fanTriangles(fan: ArrayLike<number>): number[] {
  let out: number[] = []
  for (let t = 1; t + 1 < fan.length; t++) out.push(fan[0]!, fan[t]!, fan[t + 1]!)
  return out
}

/** A line loop as a strip that returns to its first vertex. */
function closeLoop(loop: ArrayLike<number>): number[] {
  let out = Array.from(loop)
  if (loop.length > 0) out.push(loop[0]!)
  return out
}

// The spec's alphaCutoff when a MASK material leaves it out.
const GLTF_ALPHA_CUTOFF = 0.5

// The vertex format an accessor's (componentType, normalized, element
// count) spells when the table has that row, keyed "<type>[n]x<count>".
// These are exactly the quantized forms KHR_mesh_quantization allows for
// uvs, colors, joints and weights, so a quantized export keeps its bytes
// in the vertex buffer; a channel with no row (a byte-pair uv, a short
// normal, any position) widens to float32xN.
const ACCESSOR_FORMATS: Record<string, VertexFormat> = {
  "5126x1": "float32",
  "5126x2": "float32x2",
  "5126x3": "float32x3",
  "5126x4": "float32x4",
  "5121nx4": "unorm8x4",
  "5120nx4": "snorm8x4",
  "5123nx2": "unorm16x2",
  "5123nx4": "unorm16x4",
  "5122nx2": "snorm16x2",
  "5122nx4": "snorm16x4",
  "5121x4": "uint8x4",
  "5123x2": "uint16x2",
  "5123x4": "uint16x4",
}
const FLOAT_FORMATS: Record<number, VertexFormat> = { 1: "float32", 2: "float32x2", 3: "float32x3", 4: "float32x4" }

// The vertex format a channel of `elements` components lands in: the
// accessor's own when it has a row, float otherwise.
function channelFormat(format: VertexFormat | null, elements: number): VertexFormat {
  return format ?? FLOAT_FORMATS[elements]!
}

// A part's layout as the preset it equals, else its list: what the
// container writes and what the presets' docs promise ("base",
// "colored", "skinned" when the file carried floats).
function presetOrList(attrs: VertexAttribute[]): VertexLayout | undefined {
  let key = layoutKey(attrs)
  if (key === layoutKey("base")) return undefined
  if (key === layoutKey("colored")) return "colored"
  if (key === layoutKey("skinned")) return "skinned"
  return attrs
}

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
  emissiveIntensity: 1,
  emissiveMap: null,
  metalness: 1,
  roughness: 1,
  metalnessRoughnessMap: null,
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
    // emissive strength is the emissive intensity; every other required
    // extension changes what the file means.
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
    // glTF factors are linear; the material options are sRGB.
    return {
      name: m.name ?? "material" + i,
      color: [linearToSrgb(factor[0]), linearToSrgb(factor[1]), linearToSrgb(factor[2]), factor[3] ?? 1],
      map: textureSlot(pbr.baseColorTexture),
      doubleSided: m.doubleSided === true,
      transparent: m.alphaMode === "BLEND",
      alphaMode: m.alphaMode === "MASK" || m.alphaMode === "BLEND" ? m.alphaMode : "OPAQUE",
      alphaCutoff: typeof m.alphaCutoff === "number" ? m.alphaCutoff : GLTF_ALPHA_CUTOFF,
      normalMap: textureSlot(m.normalTexture),
      normalScale: typeof m.normalTexture?.scale === "number" ? m.normalTexture.scale : 1,
      emissive: [linearToSrgb(emissiveFactor[0] ?? 0), linearToSrgb(emissiveFactor[1] ?? 0), linearToSrgb(emissiveFactor[2] ?? 0)],
      emissiveIntensity: strength,
      emissiveMap: textureSlot(m.emissiveTexture),
      metalness: typeof pbr.metallicFactor === "number" ? pbr.metallicFactor : 1,
      roughness: typeof pbr.roughnessFactor === "number" ? pbr.roughnessFactor : 1,
      metalnessRoughnessMap: textureSlot(pbr.metallicRoughnessTexture),
    }
  })
  // Primitives without a material draw the spec's default; it is appended
  // only when something uses it.
  let defaultMaterial = -1

  let accessorFloats = (index: number, what: string): { data: Float32Array; elements: number; count: number; format: VertexFormat | null } => {
    let acc = gltf.accessors[index]
    if (acc === undefined) throw new Error("parseGltf: " + what + " names a missing accessor " + index)
    let elements = TYPE_ELEMENTS[acc.type]
    let compBytes = COMPONENT_BYTES[acc.componentType]
    if (elements === undefined || compBytes === undefined) throw new Error("parseGltf: " + what + " has an unknown accessor type")
    let out = new Float32Array(acc.count * elements)
    let normalized = acc.normalized === true
    let format = ACCESSOR_FORMATS[acc.componentType + (normalized ? "n" : "") + "x" + elements] ?? null
    let read = readerFor(acc.componentType, normalized)
    if (acc.bufferView !== undefined) {
      let view = gltf.bufferViews[acc.bufferView]
      let bytes = bufferViewBytes(acc.bufferView)
      let stride = view.byteStride ?? compBytes * elements
      let dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
      let base = acc.byteOffset ?? 0
      for (let i = 0; i < acc.count; i++) {
        let at = base + i * stride
        for (let e = 0; e < elements; e++) out[i * elements + e] = read(dv, at + e * compBytes)
      }
    }
    // A sparse accessor overrides `sparse.count` elements of the base
    // (the bufferView, or zeros without one): tightly packed indices in
    // their own component type and values in the accessor's - the form
    // exporters write morph targets in.
    let sparse = acc.sparse
    if (sparse !== undefined) {
      let indexBytes = COMPONENT_BYTES[sparse.indices?.componentType]
      if (indexBytes === undefined || sparse.indices.bufferView === undefined || sparse.values?.bufferView === undefined) {
        throw new Error("parseGltf: " + what + " has a malformed sparse accessor")
      }
      let ib = bufferViewBytes(sparse.indices.bufferView)
      let idv = new DataView(ib.buffer, ib.byteOffset, ib.byteLength)
      let ibase = sparse.indices.byteOffset ?? 0
      let readIndex = readerFor(sparse.indices.componentType, false)
      let vb = bufferViewBytes(sparse.values.bufferView)
      let vdv = new DataView(vb.buffer, vb.byteOffset, vb.byteLength)
      let vbase = sparse.values.byteOffset ?? 0
      for (let i = 0; i < sparse.count; i++) {
        let target = readIndex(idv, ibase + i * indexBytes)
        if (target >= acc.count) throw new Error("parseGltf: " + what + " sparse index " + target + " is past the accessor's " + acc.count + " elements")
        for (let e = 0; e < elements; e++) out[target * elements + e] = read(vdv, vbase + (i * elements + e) * compBytes)
      }
    }
    return { data: out, elements, count: acc.count, format }
  }

  // The node table is built lazily: a glTF node gets a slot only when a
  // part somewhere below it needs the chain (materialize walks ancestors
  // first, so the table stays in pre-order), which is what prunes cameras,
  // lights and unused empties.
  type PendingNode = { name: string; parent: PendingNode | null; position: Vec3; rotation: Quat; scale: Vec3; world: Mat4; index: number | null }
  let nodes: ModelNode[] = []
  let materialize = (p: PendingNode): number => {
    if (p.index !== null) return p.index
    let parent = p.parent === null ? null : materialize(p.parent)
    p.index = nodes.length
    nodes.push({ name: p.name, parent, position: p.position, rotation: p.rotation, scale: p.scale })
    return p.index
  }

  let parts: ModelPart[] = []
  let pendingJointBounds: { skin: number; positions: Float32Array; count: number; joints: Float32Array; weights: Float32Array; slack: number }[] = []
  let bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
  // Grow a bounds box by a local box through a transform: its 8 corners,
  // conservative under rotation, exact under translation and axis-aligned
  // scale. An empty box (no vertex grew it) contributes nothing.
  let growBounds = (into: Float32Array, box: ArrayLike<number>, m: Mat4): void => {
    if (!(box[0]! <= box[3]!)) return
    for (let corner = 0; corner < 8; corner++) {
      let x = corner & 1 ? box[3]! : box[0]!, y = corner & 2 ? box[4]! : box[1]!, z = corner & 4 ? box[5]! : box[2]!
      let wx = m[0] * x + m[4] * y + m[8] * z + m[12]
      let wy = m[1] * x + m[5] * y + m[9] * z + m[13]
      let wz = m[2] * x + m[6] * y + m[10] * z + m[14]
      if (wx < into[0]!) into[0] = wx
      if (wy < into[1]!) into[1] = wy
      if (wz < into[2]!) into[2] = wz
      if (wx > into[3]!) into[3] = wx
      if (wy > into[4]!) into[4] = wy
      if (wz > into[5]!) into[5] = wz
    }
  }

  let emit = (prim: any, name: string, node: number, world: Mat4, skin: number | null, targetNames: string[] | null): void => {
    if (prim.attributes?.POSITION === undefined) return
    let mode: number = prim.mode ?? MODE_TRIANGLES
    let pos = accessorFloats(prim.attributes.POSITION, name + " POSITION")
    if (pos.elements !== 3) throw new Error("parseGltf: " + name + " POSITION is not VEC3")
    let count = pos.count
    let nrm = prim.attributes.NORMAL !== undefined ? accessorFloats(prim.attributes.NORMAL, name + " NORMAL").data : null
    let uv = prim.attributes.TEXCOORD_0 !== undefined ? accessorFloats(prim.attributes.TEXCOORD_0, name + " TEXCOORD_0") : null
    let color = prim.attributes.COLOR_0 !== undefined ? accessorFloats(prim.attributes.COLOR_0, name + " COLOR_0") : null
    if (color !== null && color.elements !== 3 && color.elements !== 4) throw new Error("parseGltf: " + name + " COLOR_0 is not VEC3 or VEC4")
    // A skinned primitive: joints and weights become the "skinned"
    // layout's extra channels; its vertices are ALREADY model-space bind
    // pose (the node transform is ignored, per spec), so no flip and
    // identity bounds.
    let skinned = skin !== null
    let joints: { data: Float32Array; format: VertexFormat | null } | null = null
    let weights: { data: Float32Array; format: VertexFormat | null } | null = null
    if (skinned) {
      if (prim.attributes.JOINTS_0 === undefined || prim.attributes.WEIGHTS_0 === undefined) {
        throw new Error("parseGltf: " + name + " is skinned but lacks JOINTS_0/WEIGHTS_0")
      }
      let j = accessorFloats(prim.attributes.JOINTS_0, name + " JOINTS_0")
      let w = accessorFloats(prim.attributes.WEIGHTS_0, name + " WEIGHTS_0")
      if (j.elements !== 4 || w.elements !== 4) throw new Error("parseGltf: " + name + " JOINTS_0/WEIGHTS_0 are not VEC4")
      joints = j
      weights = w
    }
    // The layout the vertices are written in: the base prefix (a
    // position is always float; a uv keeps a quantized form), the skin
    // channels for a skinned primitive in the file's own integer forms
    // (joints feed an integer `in`, so a float JOINTS_0 narrows to u16),
    // aColor after them for one with COLOR_0 (a VEC3 color widens to four
    // floats: it gains an alpha).
    let attrs: VertexAttribute[] = [
      { name: "aPos", format: "float32x3" },
      { name: "aNormal", format: "float32x3" },
      { name: "aUV", format: channelFormat(uv?.format ?? null, 2) },
    ]
    if (joints !== null && weights !== null) {
      attrs.push({ name: "aJoints", format: joints.format ?? "uint16x4" }, { name: "aWeights", format: channelFormat(weights.format, 4) })
    }
    if (color !== null) attrs.push({ name: "aColor", format: channelFormat(color.elements === 4 ? color.format : null, 4) })
    let layout = presetOrList(attrs)
    let raw: ArrayLike<number> =
      prim.indices !== undefined ? accessorFloats(prim.indices, name + " indices").data : Array.from({ length: count }, (_, i) => i)
    // The primitive's topology, carried on the geometry. Triangle strips
    // and fans are unrolled to lists (what the winding flip, the
    // flat-shading un-index and the picking shape expect) and a line
    // loop, which has no GPU topology, is closed into a strip.
    let topology: Topology
    let indices: ArrayLike<number>
    switch (mode) {
      case MODE_TRIANGLES:
        topology = "triangles"
        indices = raw
        break
      case MODE_TRIANGLE_STRIP:
        topology = "triangles"
        indices = stripTriangles(raw)
        break
      case MODE_TRIANGLE_FAN:
        topology = "triangles"
        indices = fanTriangles(raw)
        break
      case MODE_LINES:
        topology = "lines"
        indices = raw
        break
      case MODE_LINE_STRIP:
        topology = "line-strip"
        indices = raw
        break
      case MODE_LINE_LOOP:
        topology = "line-strip"
        indices = closeLoop(raw)
        break
      case MODE_POINTS:
        topology = "points"
        indices = raw
        break
      default:
        throw new Error("parseGltf: " + name + " has an unknown primitive mode " + mode)
    }
    let triangles = topology === "triangles"
    if (triangles && indices.length % 3 !== 0) throw new Error("parseGltf: " + name + " index count is not a multiple of 3")

    // A mirroring chain (negative world determinant at the rest pose)
    // flips the displayed winding, so the index order is flipped here to
    // compensate - cull: "back" keeps the outside. Stored normals stay the
    // authored ones: the runtime's inverse-transpose maps them outward for
    // the flipped winding. Baked from the REST pose; a scale animated
    // across zero would unbake it, which is pathological. Lines and
    // points have no winding.
    let flip = triangles && !skinned && det3(world) < 0

    // The channels of the part's buffer, written through the accessors:
    // whatever a channel's format, the writer takes floats.
    let source: VertexSource = { pos: pos.data, nrm, uv: uv?.data ?? null, color: color === null ? null : { data: color.data, elements: color.elements }, joints: joints?.data ?? null, weights: weights?.data ?? null }
    let vertices: ArrayBufferView
    let writer: VertexWriter
    let packedIndices: number[]
    // Output slot -> source vertex, for the channels written after the
    // interleave (the morph targets): null while the slots are the
    // source order, the un-index's corner table otherwise.
    let remap: number[] | null = null
    if (nrm !== null || !triangles) {
      // Indexed as authored. Lines and points without normals get zero
      // ones (there is no face to take one from; nothing lights them).
      vertices = vertexView(layout, new ArrayBuffer(count * layoutStride(layout)))
      writer = vertexWriter(vertices, layout)
      for (let i = 0; i < count; i++) writeVertex(writer, i, source, i)
      packedIndices = Array.from(indices)
      if (flip) {
        for (let i = 0; i < packedIndices.length; i += 3) {
          let b = packedIndices[i + 1]!
          packedIndices[i + 1] = packedIndices[i + 2]!
          packedIndices[i + 2] = b
        }
      }
    } else {
      // No normals: the spec asks for flat shading, which needs one vertex
      // per triangle corner, so the primitive is un-indexed here and each
      // corner takes its face normal - of the AUTHORED winding (negated
      // after a flip), the direction authored normals would have, so the
      // runtime maps both the same way.
      let triangles = indices.length / 3
      vertices = vertexView(layout, new ArrayBuffer(triangles * 3 * layoutStride(layout)))
      writer = vertexWriter(vertices, layout)
      packedIndices = new Array(triangles * 3)
      remap = new Array(triangles * 3)
      let face: Vec3 = [0, 0, 0]
      for (let t = 0; t < triangles; t++) {
        let a = indices[t * 3]!, b = indices[t * 3 + 1]!, c = indices[t * 3 + 2]!
        if (flip) [b, c] = [c, b]
        let out = t * 3
        writeVertex(writer, out, source, a)
        writeVertex(writer, out + 1, source, b)
        writeVertex(writer, out + 2, source, c)
        remap[out] = a
        remap[out + 1] = b
        remap[out + 2] = c
        faceNormal(face, writer.pos, out)
        if (flip) {
          face[0] = -face[0]
          face[1] = -face[1]
          face[2] = -face[2]
        }
        for (let k = 0; k < 3; k++) {
          for (let c = 0; c < 3; c++) writer.nrm.set(out + k, c, face[c]!)
          packedIndices[out + k] = out + k
        }
      }
      count = triangles * 3
    }

    // Morph targets: each target's POSITION and NORMAL deltas (a target
    // may carry either; TANGENT is dropped with the base tangents), read
    // dense through the accessors (sparse ones expand there), remapped
    // through the un-index when the primitive was flattened - normal
    // deltas are dropped then, as flat normals are generated - and packed
    // sparse by vertex onto the geometry. Names come from the mesh's
    // extras.targetNames (what Blender writes), else "target<t>".
    let targets: any[] = prim.targets ?? []
    let morphs = targets.length === 0 ? undefined : packMorphTargets(count, targets.map((target: any, t: number): MorphTarget => {
      let what = name + " target " + t
      let deltas = (attribute: string): Float32Array | null => {
        if (target?.[attribute] === undefined) return null
        let acc = accessorFloats(target[attribute], what + " " + attribute)
        if (acc.elements !== 3 || acc.count !== pos.count) throw new Error("parseGltf: " + what + " " + attribute + " is not VEC3 over the primitive's vertices")
        if (remap === null) return acc.data
        let out = new Float32Array(count * 3)
        for (let slot = 0; slot < count; slot++) {
          let from = remap[slot]! * 3
          out[slot * 3] = acc.data[from]!
          out[slot * 3 + 1] = acc.data[from + 1]!
          out[slot * 3 + 2] = acc.data[from + 2]!
        }
        return out
      }
      return { name: targetNames?.[t] ?? "target" + t, position: deltas("POSITION") ?? new Float32Array(count * 3), normal: remap === null ? deltas("NORMAL") : null }
    }))

    // Model bounds: the part's local box through the node's rest-pose
    // world transform (8 corners - conservative under rotation, exact
    // under translation and axis-aligned scale), grown by the morph
    // extent first so a morphed shape stays inside. A skinned part's
    // vertices are placed by its joints, not its node, so its box is
    // folded in after the skins exist (growBounds over the joint boxes).
    if (!skinned) {
      let lo: Vec3 = [Infinity, Infinity, Infinity]
      let hi: Vec3 = [-Infinity, -Infinity, -Infinity]
      for (let i = 0; i < count; i++) {
        for (let k = 0; k < 3; k++) {
          let v = writer.pos.get(i, k)
          if (v < lo[k]!) lo[k] = v
          if (v > hi[k]!) hi[k] = v
        }
      }
      if (morphs !== undefined) {
        for (let k = 0; k < 3; k++) {
          lo[k] = lo[k]! + morphs.extent[k]!
          hi[k] = hi[k]! + morphs.extent[3 + k]!
        }
      }
      growBounds(bounds, [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]], world)
    } else {
      // Joint boxes need the skin's inverse binds, and skins are built
      // after the walk (their joints are ordinary nodes the walk
      // registers), so the arrays are parked under the FILE's skin index
      // and grown then - each box padded by the largest morph delta, as
      // a morph moves a vertex in whichever joint's space.
      let slack = morphs === undefined ? 0 : Math.max(...Array.from(morphs.extent, Math.abs))
      pendingJointBounds.push({ skin: skin!, positions: pos.data, count: pos.count, joints: joints!.data, weights: weights!.data, slack })
    }

    let material = prim.material
    if (material === undefined) {
      if (defaultMaterial < 0) {
        defaultMaterial = materials.length
        materials.push({ ...DEFAULT_MATERIAL })
      }
      material = defaultMaterial
    }
    let geometry: Geometry = { vertices, indices: packIndices(packedIndices, count), label: name }
    if (layout !== undefined) geometry.layout = layout
    if (morphs !== undefined) geometry.morphs = morphs
    if (!triangles) geometry.topology = topology
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
    let pending: PendingNode = { name: node.name ?? "node" + index, parent, position, rotation, scale, world, index: null }
    pendingByIndex.set(index, pending)
    if (node.mesh !== undefined) {
      let mesh = gltf.meshes[node.mesh]
      if (mesh === undefined) throw new Error("parseGltf: node " + index + " names a missing mesh " + node.mesh)
      if (node.name === undefined && mesh.name !== undefined) pending.name = mesh.name
      let slot = materialize(pending)
      let skin = typeof node.skin === "number" ? node.skin : null
      let prims: any[] = mesh.primitives ?? []
      // Morph target names and the initial weights are the MESH's (a
      // node may override the weights); the weights land on the node,
      // which is what a weights channel and setMorphWeights address.
      let targetCount = Math.max(0, ...prims.map((p: any): number => p?.targets?.length ?? 0))
      let targetNames: string[] | null = Array.isArray(mesh.extras?.targetNames) ? mesh.extras.targetNames.map(String) : null
      if (targetCount > 0) {
        let weights: number[] = Array.isArray(node.weights) ? node.weights : Array.isArray(mesh.weights) ? mesh.weights : []
        if (weights.length !== targetCount) {
          if (weights.length !== 0) throw new Error("parseGltf: node " + pending.name + " has " + weights.length + " weights for " + targetCount + " morph targets")
          weights = new Array(targetCount).fill(0)
        }
        nodes[slot]!.weights = weights.map(Number)
      }
      for (let k = 0; k < prims.length; k++) emit(prims[k], prims.length > 1 ? pending.name + "#" + k : pending.name, slot, world, skin, targetNames)
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
  // Per-joint bounds, grown from the parked skinned primitives once the
  // compact skins exist: each influenced vertex (weight above zero) goes
  // through the joint's inverse bind into joint space and grows that
  // joint's box.
  let growJointBounds = (skin: ModelSkin, positions: Float32Array, count: number, joints: Float32Array, weights: Float32Array, slack: number): void => {
    let jb = skin.jointBounds
    let ib = skin.inverseBind
    for (let i = 0; i < count; i++) {
      let x = positions[i * 3]!, y = positions[i * 3 + 1]!, z = positions[i * 3 + 2]!
      for (let k = 0; k < 4; k++) {
        if (weights[i * 4 + k]! <= 0) continue
        let j = joints[i * 4 + k]!
        if (j >= skin.joints.length) continue
        let m = j * 16
        let px = ib[m]! * x + ib[m + 4]! * y + ib[m + 8]! * z + ib[m + 12]!
        let py = ib[m + 1]! * x + ib[m + 5]! * y + ib[m + 9]! * z + ib[m + 13]!
        let pz = ib[m + 2]! * x + ib[m + 6]! * y + ib[m + 10]! * z + ib[m + 14]!
        let b = j * 6
        if (px - slack < jb[b]!) jb[b] = px - slack
        if (py - slack < jb[b + 1]!) jb[b + 1] = py - slack
        if (pz - slack < jb[b + 2]!) jb[b + 2] = pz - slack
        if (px + slack > jb[b + 3]!) jb[b + 3] = px + slack
        if (py + slack > jb[b + 4]!) jb[b + 4] = py + slack
        if (pz + slack > jb[b + 5]!) jb[b + 5] = pz + slack
      }
    }
  }
  let skinSlots = new Map<number, number>()
  // Each compact skin's joints' rest-pose world transforms, for the bounds.
  let skinJointWorlds: Mat4[][] = []
  for (let part of parts) {
    if (part.skin === null) continue
    let slot = skinSlots.get(part.skin)
    if (slot === undefined) {
      let sk = gltf.skins?.[part.skin]
      if (sk === undefined) throw new Error("parseGltf: part '" + part.name + "' names a missing skin " + part.skin)
      let jointWorlds: Mat4[] = []
      let jointIndices: number[] = (sk.joints ?? []).map((j: number): number => {
        let pending = pendingByIndex.get(j)
        if (pending === undefined) throw new Error("parseGltf: skin joint node " + j + " is not in the scene")
        jointWorlds.push(pending.world)
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
      let jointBounds = new Float32Array(jointIndices.length * 6)
      for (let j = 0; j < jointIndices.length; j++) jointBounds.set([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity], j * 6)
      skins.push({ joints: jointIndices, inverseBind, jointBounds })
      skinJointWorlds.push(jointWorlds)
      skinSlots.set(part.skin, slot)
    }
    part.skin = slot
  }
  for (let parked of pendingJointBounds) {
    let slot = skinSlots.get(parked.skin)
    if (slot !== undefined) growJointBounds(skins[slot]!, parked.positions, parked.count, parked.joints, parked.weights, parked.slack)
  }
  // Skinned parts' share of the model bounds: each joint's box (joint
  // space) through that joint's rest-pose world transform, which is where
  // the skin places the vertices it influences at rest (a vertex blended
  // between joints lands inside the union's box). The bind-pose vertex
  // box itself is not where the part renders once an armature carries a
  // rotation or a scale.
  skins.forEach((skin, s) => {
    let worlds = skinJointWorlds[s]!
    for (let j = 0; j < skin.joints.length; j++) growBounds(bounds, skin.jointBounds.subarray(j * 6, j * 6 + 6), worlds[j]!)
  })

  // Animations, after the walk so channels can materialize their target
  // nodes (a channel may target a meshless node - a joint, a rig pivot).
  const CHANNEL_PATHS: Record<string, ModelChannel["path"]> = { translation: "position", rotation: "rotation", scale: "scale", weights: "weights" }
  let clips: ModelClip[] = (gltf.animations ?? []).map((anim: any, ai: number): ModelClip => {
    let clipName = anim.name ?? "clip" + ai
    let channels: ModelChannel[] = []
    let duration = 0
    for (let ch of anim.channels ?? []) {
      // Extension paths (KHR_animation_pointer) are outside the subset.
      let path = CHANNEL_PATHS[ch.target?.path]
      if (path === undefined) continue
      // A target outside the walked scene has nothing to move, and a
      // weights channel on a node without morph targets nothing to weigh.
      let pending = pendingByIndex.get(ch.target.node)
      if (pending === undefined) continue
      let targetCount = pending.index === null ? undefined : nodes[pending.index]!.weights?.length
      if (path === "weights" && targetCount === undefined) continue
      let sampler = anim.samplers?.[ch.sampler]
      if (sampler === undefined) throw new Error("parseGltf: animation " + clipName + " channel names a missing sampler " + ch.sampler)
      let what = "animation " + clipName
      let input = accessorFloats(sampler.input, what + " input")
      let output = accessorFloats(sampler.output, what + " output")
      let interpolation: ModelChannel["interpolation"] =
        sampler.interpolation === "STEP" ? "step" : sampler.interpolation === "CUBICSPLINE" ? "cubic" : "linear"
      let perKey = interpolation === "cubic" ? 3 : 1
      if (path === "weights") {
        // Scalars, one per target per key: the output is as long as the
        // key count times the node's target count.
        if (output.elements !== 1) throw new Error("parseGltf: " + what + " weights output is not SCALAR")
        if (output.count !== input.count * perKey * targetCount!) {
          throw new Error("parseGltf: " + what + " weights has " + output.count + " values for " + input.count + " keys of " + targetCount + " targets")
        }
      } else {
        let elements = path === "rotation" ? 4 : 3
        if (output.elements !== elements) throw new Error("parseGltf: " + what + " " + path + " output is not " + (elements === 4 ? "VEC4" : "VEC3"))
        if (output.count !== input.count * perKey) {
          throw new Error("parseGltf: " + what + " " + path + " has " + output.count + " values for " + input.count + " keys")
        }
      }
      if (input.count > 0) duration = Math.max(duration, input.data[input.count - 1]!)
      channels.push({ node: materialize(pending), path, interpolation, times: input.data, values: output.data })
    }
    return { name: clipName, duration, channels }
  })

  // Nothing grew the box (no parts, or skinned parts whose weights reach
  // no joint): an empty model sits at the origin.
  if (!(bounds[0]! <= bounds[3]!)) bounds.fill(0)
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

// The source channels of a primitive as the accessors decoded them
// (floats, whatever the file's component type) and the writers over the
// part's buffer, one per channel the layout carries.
type VertexSource = {
  pos: Float32Array
  nrm: Float32Array | null
  uv: Float32Array | null
  color: { data: Float32Array; elements: number } | null
  joints: Float32Array | null
  weights: Float32Array | null
}
type VertexWriter = {
  pos: AttributeAccess
  nrm: AttributeAccess
  uv: AttributeAccess
  color: AttributeAccess | null
  joints: AttributeAccess | null
  weights: AttributeAccess | null
}

function vertexWriter(vertices: ArrayBufferView, layout: VertexLayout | undefined): VertexWriter {
  return {
    pos: attributeAccess(vertices, layout, "aPos")!,
    nrm: attributeAccess(vertices, layout, "aNormal")!,
    uv: attributeAccess(vertices, layout, "aUV")!,
    color: attributeAccess(vertices, layout, "aColor"),
    joints: attributeAccess(vertices, layout, "aJoints"),
    weights: attributeAccess(vertices, layout, "aWeights"),
  }
}

// One interleaved vertex, node-local: position and normal copied as
// authored (the runtime's uModel/uNormal do the placing), uv copied or
// zero, and for a skinned layout the joint indices plus their weights
// RENORMALIZED to sum 1 (quantized exports drift a little; the spec asks
// for normalized weights). A COLOR_0 (VEC3 or VEC4, already 0..1 - the
// normalized-integer path unpacks quantized exports) lands at the
// layout's aColor slot premultiplied, alpha 1 for VEC3. Every write goes
// through the channel's accessor, which re-encodes a packed format.
function writeVertex(w: VertexWriter, slot: number, src: VertexSource, i: number): void {
  for (let k = 0; k < 3; k++) w.pos.set(slot, k, src.pos[i * 3 + k]!)
  if (src.nrm !== null) {
    for (let k = 0; k < 3; k++) w.nrm.set(slot, k, src.nrm[i * 3 + k]!)
  }
  if (src.uv !== null) {
    w.uv.set(slot, 0, src.uv[i * 2]!)
    w.uv.set(slot, 1, src.uv[i * 2 + 1]!)
  }
  if (src.color !== null && w.color !== null) {
    let c = i * src.color.elements
    let a = src.color.elements === 4 ? src.color.data[c + 3]! : 1
    for (let k = 0; k < 3; k++) w.color.set(slot, k, src.color.data[c + k]! * a)
    w.color.set(slot, 3, a)
  }
  if (src.joints !== null && src.weights !== null && w.joints !== null && w.weights !== null) {
    let j = i * 4
    let sum = src.weights[j]! + src.weights[j + 1]! + src.weights[j + 2]! + src.weights[j + 3]!
    let inv = sum > 1e-8 ? 1 / sum : 0
    for (let k = 0; k < 4; k++) {
      w.joints.set(slot, k, src.joints[j + k]!)
      w.weights.set(slot, k, src.weights[j + k]! * inv)
    }
  }
}

// The unit normal of the triangle at three consecutive vertex slots.
function faceNormal(out: Vec3, pos: AttributeAccess, first: number): void {
  let a = first, b = first + 1, c = first + 2
  let abx = pos.get(b, 0) - pos.get(a, 0), aby = pos.get(b, 1) - pos.get(a, 1), abz = pos.get(b, 2) - pos.get(a, 2)
  let acx = pos.get(c, 0) - pos.get(a, 0), acy = pos.get(c, 1) - pos.get(a, 1), acz = pos.get(c, 2) - pos.get(a, 2)
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
