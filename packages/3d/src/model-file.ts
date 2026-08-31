// The baked model container (.srtm): ModelData as one file whose payload IS
// the GPU layout, so loading it is a header parse plus typed-array views -
// no per-vertex work. Written by tools/model.ts under bun (from parseGltf),
// read by loadModel on flux; both ends are this pure module.
//
// Layout, all little-endian:
//   "SRTM" u32 | version u32 | jsonLength u32 | json (padded to 4) | payload
// The JSON header describes each block's byte range into the payload; every
// block starts 4-aligned so Float32Array/Uint32Array views sit on it
// directly. Images travel as their encoded files (PNG/JPEG bytes).

import type { ModelChannel, ModelData, ModelMaterial, ModelNode, ModelSkin } from "./gltf.ts"
import { layoutStride } from "./geometry.ts"
import type { VertexLayout } from "./geometry.ts"

/** "SRTM" read as a little-endian u32. */
const MAGIC = 0x4d545253
// Version 3 retained the node hierarchy - a node table (name/parent/TRS)
// in the header, parts referencing their node, vertices NODE-LOCAL
// instead of world-baked - and carries skins (joints + inverse binds,
// parts in the "skinned" layout) and animation clips (channel
// times/values as payload blocks). Version-2 files world-baked and have
// none of it, so they are rejected - re-bake with `srt tool 3d/model`.
const VERSION = 3

// The named layouts the container writes; a custom attribute-list layout
// has no name to store, so encodeModel rejects it.
const NAMED_LAYOUTS = ["standard", "colored", "skinned"]

type Block = { offset: number; bytes: number }

type PartHeader = Block & {
  name: string
  node: number
  skin: number | null
  material: number
  layout: string
  vertexCount: number
  indexBits: 16 | 32
  index: Block
}

type SkinHeader = { joints: number[]; inverseBind: Block }

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
  parts: PartHeader[]
  skins: SkinHeader[]
  clips: ClipHeader[]
  bounds: number[]
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

  let parts: PartHeader[] = data.parts.map((part) => {
    let g = part.geometry
    let layout = g.layout === undefined ? "standard" : g.layout
    if (typeof layout !== "string") {
      throw new Error("encodeModel: part '" + part.name + "' has a custom attribute-list layout; only the named layouts are written")
    }
    let vertices = push(new Uint8Array(g.vertices.buffer, g.vertices.byteOffset, g.vertices.byteLength))
    let index = push(new Uint8Array(g.indices.buffer, g.indices.byteOffset, g.indices.byteLength))
    return {
      ...vertices,
      name: part.name,
      node: part.node,
      skin: part.skin,
      material: part.material,
      layout,
      vertexCount: g.vertices.length / layoutStride(layout),
      indexBits: g.indices instanceof Uint32Array ? 32 : 16,
      index,
    }
  })
  let images = data.images.map((image) => push(image))
  let floats = (f: Float32Array): Block => push(new Uint8Array(f.buffer, f.byteOffset, f.byteLength))
  let skins: SkinHeader[] = data.skins.map((skin) => ({ joints: skin.joints, inverseBind: floats(skin.inverseBind) }))
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
  let header: Header = { nodes: data.nodes, materials: data.materials, images, parts, skins, clips, bounds: Array.from(data.bounds) }

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

  let parts = header.parts.map((part) => {
    if (!NAMED_LAYOUTS.includes(part.layout)) throw new Error("decodeModel: part '" + part.name + "' has an unsupported layout " + part.layout)
    let layout = part.layout as VertexLayout
    let indexCount = part.index.bytes / (part.indexBits / 8)
    let geometry: ModelData["parts"][0]["geometry"] = {
      vertices: new Float32Array(buffer, payload + part.offset, part.vertexCount * layoutStride(layout)),
      indices: part.indexBits === 32 ? new Uint32Array(buffer, payload + part.index.offset, indexCount) : new Uint16Array(buffer, payload + part.index.offset, indexCount),
      label: part.name,
    }
    if (part.layout !== "standard") geometry.layout = layout
    return {
      name: part.name,
      node: part.node,
      skin: part.skin,
      material: part.material,
      geometry,
    }
  })
  let images = header.images.map((block) => new Uint8Array(buffer, payload + block.offset, block.bytes))
  let floats = (block: Block): Float32Array => new Float32Array(buffer, payload + block.offset, block.bytes / 4)
  let skins: ModelSkin[] = header.skins.map((skin) => ({ joints: skin.joints, inverseBind: floats(skin.inverseBind) }))
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
  return { nodes: header.nodes, parts, skins, clips, materials: header.materials, images, bounds: Float32Array.from(header.bounds) }
}
