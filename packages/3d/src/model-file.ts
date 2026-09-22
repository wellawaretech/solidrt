// The baked model container (.srtm): ModelData as one file whose payload IS
// the GPU layout, so loading it is a header parse plus typed-array views -
// no per-vertex work. Written by tools/model.ts under bun (from parseGltf),
// read by loadModel on flux; both ends are this pure module.
//
// Layout, all little-endian:
//   "SRTM" u32 | version u32 | jsonLength u32 | json (padded to 4) | payload
// The JSON header describes each block's byte range into the payload; every
// block starts 4-aligned so Float32Array/Uint32Array views sit on it
// directly. Images travel as their encoded files (PNG/JPEG bytes), app
// data as the header's `extras` (JSON) and named `blobs` (aligned blocks).
// Geometry is a table the parts index, and a buffer view is written once
// however many geometries read it: parts sharing a Geometry object (a
// skinned or morphed mesh several nodes place) share one entry, and two
// geometries over one vertex buffer (a mirrored copy: same vertices, the
// flipped index order) share the vertex block - decodeModel restores the
// same identities, so the runtime uploads shared bytes once.

import type { ModelChannel, ModelData, ModelExtras, ModelMaterial, ModelNode, ModelSkin } from "./gltf.ts"
import { layoutStride, vertexView, VERTEX_FORMATS } from "./geometry.ts"
import type { VertexAttribute } from "@solidrt/core/gpu"
import type { Geometry, VertexLayout } from "./geometry.ts"

/** "SRTM" read as a little-endian u32. */
const MAGIC = 0x4d545253
// Version 3 retained the node hierarchy - a node table (name/parent/TRS)
// in the header, parts referencing their node, vertices NODE-LOCAL
// instead of world-baked - and carries skins (joints + inverse binds,
// parts in the "skinned" layout) and animation clips (channel
// times/values as payload blocks). Version-2 files world-baked and have
// none of it, so they are rejected - re-bake with `srt tool 3d/model`.
// Version 4 adds the metalness/roughness fields to the material records
// (metalness, roughness, metalnessRoughnessMap); a version-3 file lacks
// them, so it is rejected the same way rather than read as all-metal.
// Version 6 spells attribute formats in the WebGPU vocabulary
// (float32x3, unorm8x4, ...) and counts vertices in bytes, so a part may
// carry packed channels; a version-5 file's "vec3" words do not parse.
// Version 7 changes the named "skinned" layout's joints from float32x4 to
// uint8x4 (an integer shader input), so a version-6 skinned part would
// decode at the wrong stride; it is rejected. Re-bake with `srt tool
// 3d/model`.
// Version 8 carries morph targets: a part's packed targets block (names,
// the sparse-by-vertex texels, the extent - see packMorphTargets), the
// mesh weights on the node table, and clip channels on the "weights"
// path. A version-7 file has none, so it is rejected the same way.
// Version 9 names the plain layout "base" (position, normal, uv - the
// prefix "colored" and "skinned" extend) where version 8 wrote
// "standard"; a version-8 file's layout word does not parse.
// Version 10 carries app data and reuse: `extras` (JSON) on the root, the
// nodes, the parts and the materials, named binary `blobs` as payload
// blocks, a part's `placements` (the further nodes drawing the same
// geometry), and geometry as a table the parts index (shared entries and
// shared blocks, see the header). A version-9 file has none of these and is rejected like
// every earlier version rather than read as a file without them, so a
// stale bake never silently drops the data an app relies on. Re-bake
// with `srt tool 3d/model`.
const VERSION = 10

// The named layouts the container writes by name; a custom attribute-list
// layout (a skinned primitive with COLOR_0, a withAttribute channel) is
// written as its list.
const NAMED_LAYOUTS = ["base", "colored", "skinned"]

// A part's layout as written: a named preset, or a custom attribute list
// checked to the shape the stride and the pipeline are built from.
function decodeLayout(layout: string | VertexAttribute[], name: string): VertexLayout {
  if (typeof layout === "string") {
    if (!NAMED_LAYOUTS.includes(layout)) throw new Error("decodeModel: part '" + name + "' has an unsupported layout " + layout)
    return layout as VertexLayout
  }
  if (!Array.isArray(layout) || !layout.every((a) => typeof a.name === "string" && a.format in VERTEX_FORMATS)) {
    throw new Error("decodeModel: part '" + name + "' has a malformed attribute list")
  }
  return layout
}

type Block = { offset: number; bytes: number }

/** One geometry of the table: its interleaved vertex block, index block
 * and packed morph targets. Blocks may be shared between entries. */
type GeometryHeader = {
  vertices: Block
  layout: string | VertexAttribute[]
  vertexCount: number
  indexBits: 16 | 32
  index: Block
  /** The packed morph targets, when it has any. */
  morphs?: { names: string[]; texels: Block; extent: number[] }
  label?: string
}

type PartHeader = {
  name: string
  node: number
  skin: number | null
  material: number
  /** Index into the header's geometry table. */
  geometry: number
  /** The further nodes placing the part's geometry (ModelPart.placements). */
  placements?: number[]
  extras?: ModelExtras
}

type SkinHeader = { joints: number[]; inverseBind: Block; jointBounds: Block }

type ChannelHeader = {
  node: number
  path: ModelChannel["path"]
  interpolation: ModelChannel["interpolation"]
  times: Block
  values: Block
}

type ClipHeader = { name: string; duration: number; channels: ChannelHeader[] }

type Header = {
  nodes: ModelNode[]
  materials: ModelMaterial[]
  images: Block[]
  geometries: GeometryHeader[]
  parts: PartHeader[]
  skins: SkinHeader[]
  clips: ClipHeader[]
  bounds: number[]
  /** The root extras (ModelData.extras); node and material extras ride
   * inside their own records. */
  extras?: ModelExtras
  /** The named binary sections (ModelData.blobs), each its own block. */
  blobs?: Record<string, Block>
}

/** Serialize a model into the .srtm container. */
export function encodeModel(data: ModelData): Uint8Array {
  let blocks: Uint8Array[] = []
  let offset = 0
  let push = (bytes: Uint8Array): Block => {
    let block = { offset, bytes: bytes.byteLength }
    blocks.push(bytes)
    offset += bytes.byteLength
    let pad = (4 - (offset % 4)) % 4
    if (pad) {
      blocks.push(new Uint8Array(pad))
      offset += pad
    }
    return block
  }

  // A buffer view is written once however many geometries read it, and a
  // Geometry object gets one table entry however many parts draw it.
  let views = new Map<ArrayBufferView, Block>()
  let pushOnce = (view: ArrayBufferView): Block => {
    let have = views.get(view)
    if (have !== undefined) return have
    let block = push(new Uint8Array(view.buffer, view.byteOffset, view.byteLength))
    views.set(view, block)
    return block
  }
  let geometries: GeometryHeader[] = []
  let geometryIds = new Map<Geometry, number>()
  let geometryId = (g: Geometry, name: string): number => {
    let id = geometryIds.get(g)
    if (id !== undefined) return id
    if (g.streams !== undefined && g.streams.length > 0) {
      throw new Error("encodeModel: part '" + name + "' carries extra vertex streams; the container writes one interleaved buffer per geometry")
    }
    let layout = g.layout === undefined ? "base" : g.layout
    let header: GeometryHeader = {
      vertices: pushOnce(g.vertices),
      layout,
      vertexCount: g.vertices.byteLength / layoutStride(layout),
      indexBits: g.indices instanceof Uint32Array ? 32 : 16,
      index: pushOnce(g.indices),
    }
    if (g.morphs !== undefined) header.morphs = { names: g.morphs.names, texels: pushOnce(g.morphs.texels), extent: Array.from(g.morphs.extent) }
    if (g.label !== undefined) header.label = g.label
    id = geometries.length
    geometries.push(header)
    geometryIds.set(g, id)
    return id
  }
  let parts: PartHeader[] = data.parts.map((part) => {
    let header: PartHeader = {
      name: part.name,
      node: part.node,
      skin: part.skin,
      material: part.material,
      geometry: geometryId(part.geometry, part.name),
    }
    if (part.placements !== undefined && part.placements.length > 0) header.placements = part.placements
    if (part.extras !== undefined) header.extras = part.extras
    return header
  })
  let images = data.images.map((image) => push(image))
  let floats = (f: Float32Array): Block => push(new Uint8Array(f.buffer, f.byteOffset, f.byteLength))
  let skins: SkinHeader[] = data.skins.map((skin) => ({ joints: skin.joints, inverseBind: floats(skin.inverseBind), jointBounds: floats(skin.jointBounds) }))
  let clips: ClipHeader[] = data.clips.map((clip) => ({
    name: clip.name,
    duration: clip.duration,
    channels: clip.channels.map((c) => ({
      node: c.node,
      path: c.path,
      interpolation: c.interpolation,
      times: floats(c.times),
      values: floats(c.values),
    })),
  }))
  let header: Header = { nodes: data.nodes, materials: data.materials, images, geometries, parts, skins, clips, bounds: Array.from(data.bounds) }
  if (data.extras !== undefined) header.extras = data.extras
  if (data.blobs !== undefined) {
    header.blobs = {}
    for (let [name, bytes] of Object.entries(data.blobs)) header.blobs[name] = push(bytes)
  }

  let json = new TextEncoder().encode(JSON.stringify(header))
  let jsonPadded = json.byteLength + ((4 - (json.byteLength % 4)) % 4)
  let out = new Uint8Array(12 + jsonPadded + offset)
  let dv = new DataView(out.buffer)
  dv.setUint32(0, MAGIC, true)
  dv.setUint32(4, VERSION, true)
  dv.setUint32(8, json.byteLength, true)
  out.set(json, 12)
  let at = 12 + jsonPadded
  for (let block of blocks) {
    out.set(block, at)
    at += block.byteLength
  }
  return out
}

/**
 * Read a .srtm container back into ModelData. The geometry arrays are
 * VIEWS onto `bytes` (copied once only when the input is not 4-aligned),
 * so the bytes must outlive the model.
 */
export function decodeModel(bytes: Uint8Array): ModelData {
  if (bytes.byteOffset % 4 !== 0) bytes = new Uint8Array(bytes)
  let buffer = bytes.buffer as ArrayBuffer
  let base = bytes.byteOffset
  if (bytes.byteLength < 12) throw new Error("decodeModel: not a model file (too short)")
  let head = new DataView(buffer, base, 12)
  if (head.getUint32(0, true) !== MAGIC) throw new Error("decodeModel: not a model file (bad magic)")
  let version = head.getUint32(4, true)
  if (version !== VERSION) throw new Error("decodeModel: version " + version + ", expected " + VERSION)
  let jsonLength = head.getUint32(8, true)
  let header: Header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, base + 12, jsonLength)))
  let payload = base + 12 + jsonLength + ((4 - (jsonLength % 4)) % 4)

  // The geometry table first: one Geometry object per entry, so parts
  // naming the same entry share it (and the runtime shares its buffers).
  let geometries: Geometry[] = header.geometries.map((g, i) => {
    let name = g.label ?? "geometry " + i
    let layout = decodeLayout(g.layout, name)
    let indexCount = g.index.bytes / (g.indexBits / 8)
    let geometry: Geometry = {
      vertices: vertexView(layout, buffer, payload + g.vertices.offset, g.vertexCount * layoutStride(layout)),
      indices: g.indexBits === 32 ? new Uint32Array(buffer, payload + g.index.offset, indexCount) : new Uint16Array(buffer, payload + g.index.offset, indexCount),
    }
    if (g.label !== undefined) geometry.label = g.label
    if (g.layout !== "base") geometry.layout = layout
    if (g.morphs !== undefined) {
      let m = g.morphs
      geometry.morphs = { names: m.names, texels: new Float32Array(buffer, payload + m.texels.offset, m.texels.bytes / 4), extent: Float32Array.from(m.extent) }
    }
    return geometry
  })
  let parts = header.parts.map((part) => {
    let geometry = geometries[part.geometry]
    if (geometry === undefined) throw new Error("decodeModel: part '" + part.name + "' names a missing geometry " + part.geometry)
    let out: ModelData["parts"][0] = {
      name: part.name,
      node: part.node,
      skin: part.skin,
      material: part.material,
      geometry,
    }
    if (part.placements !== undefined) out.placements = part.placements
    if (part.extras !== undefined) out.extras = part.extras
    return out
  })
  let images = header.images.map((block) => new Uint8Array(buffer, payload + block.offset, block.bytes))
  let floats = (block: Block): Float32Array => new Float32Array(buffer, payload + block.offset, block.bytes / 4)
  let skins: ModelSkin[] = header.skins.map((skin) => ({ joints: skin.joints, inverseBind: floats(skin.inverseBind), jointBounds: floats(skin.jointBounds) }))
  let clips = header.clips.map((clip) => ({
    name: clip.name,
    duration: clip.duration,
    channels: clip.channels.map((c) => ({
      node: c.node,
      path: c.path,
      interpolation: c.interpolation,
      times: floats(c.times),
      values: floats(c.values),
    })),
  }))
  let data: ModelData = { nodes: header.nodes, parts, skins, clips, materials: header.materials, images, bounds: Float32Array.from(header.bounds) }
  if (header.extras !== undefined) data.extras = header.extras
  if (header.blobs !== undefined) {
    data.blobs = {}
    for (let [name, block] of Object.entries(header.blobs)) data.blobs[name] = new Uint8Array(buffer, payload + block.offset, block.bytes)
  }
  return data
}
