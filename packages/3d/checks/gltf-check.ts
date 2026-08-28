// Check rig for the glTF parser (src/gltf.ts) and the .srtm container
// (src/model-file.ts): a glb built in memory from the box generator, split
// back into planar accessors the way exporters write them, under three
// nodes - translated, mirrored (winding must flip), and one without
// normals (flat normals must be generated). Then the container round trip
// and the .gltf + external file path. Pure-module inputs only, so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/gltf-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { gltfExternalUris, isGlb, parseGltf } from "../src/gltf.ts"
import type { ModelData } from "../src/gltf.ts"
import { decodeModel, encodeModel } from "../src/model-file.ts"
import { box, validateGeometry, STANDARD_FLOATS } from "../src/geometry.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
let throws = (label: string, fn: () => unknown, needle?: string): void => {
  try {
    fn()
    fail(`${label}: did not throw`)
  } catch (e) {
    if (needle !== undefined && !String(e).includes(needle)) fail(`${label}: threw "${e}", expected it to mention "${needle}"`)
  }
}

// --- a glb from the box generator ---------------------------------------

let cube = box({ width: 1, height: 1, depth: 1 })
let vertexCount = cube.vertices.length / STANDARD_FLOATS
let positions = new Float32Array(vertexCount * 3)
let normals = new Float32Array(vertexCount * 3)
let uvs = new Float32Array(vertexCount * 2)
for (let i = 0; i < vertexCount; i++) {
  let at = i * STANDARD_FLOATS
  positions.set(cube.vertices.subarray(at, at + 3), i * 3)
  normals.set(cube.vertices.subarray(at + 3, at + 6), i * 3)
  uvs.set(cube.vertices.subarray(at + 6, at + 8), i * 2)
}
let indices = new Uint16Array(cube.indices)
let fakePng = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5])

type Chunk = { bytes: Uint8Array; view: { buffer: number; byteOffset: number; byteLength: number } }
let binBlocks: Uint8Array[] = []
let binLength = 0
let bufferViews: Chunk["view"][] = []
let pushView = (bytes: Uint8Array): number => {
  let view = { buffer: 0, byteOffset: binLength, byteLength: bytes.byteLength }
  binBlocks.push(bytes)
  binLength += bytes.byteLength
  let pad = (4 - (binLength % 4)) % 4
  if (pad) {
    binBlocks.push(new Uint8Array(pad))
    binLength += pad
  }
  bufferViews.push(view)
  return bufferViews.length - 1
}
let asBytes = (a: Float32Array | Uint16Array): Uint8Array => new Uint8Array(a.buffer, a.byteOffset, a.byteLength)
let posView = pushView(asBytes(positions))
let nrmView = pushView(asBytes(normals))
let uvView = pushView(asBytes(uvs))
let idxView = pushView(asBytes(indices))
let pngView = pushView(fakePng)

let bounds = (data: Float32Array, n: number): { min: number[]; max: number[] } => {
  let min = new Array(n).fill(Infinity)
  let max = new Array(n).fill(-Infinity)
  for (let i = 0; i < data.length; i++) {
    min[i % n] = Math.min(min[i % n], data[i]!)
    max[i % n] = Math.max(max[i % n], data[i]!)
  }
  return { min, max }
}

let document = {
  asset: { version: "2.0" },
  scene: 0,
  scenes: [{ nodes: [0, 1, 2] }],
  nodes: [
    { name: "shifted", mesh: 0, translation: [2, 0, 0] },
    { name: "mirrored", mesh: 0, scale: [-1, 1, 1] },
    { name: "flat", mesh: 1, rotation: [0, Math.SQRT1_2, 0, Math.SQRT1_2] },
  ],
  meshes: [
    { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2 }, indices: 3, material: 0 }] },
    { primitives: [{ attributes: { POSITION: 0 }, indices: 3, material: 1 }] },
  ],
  accessors: [
    { bufferView: posView, componentType: 5126, count: vertexCount, type: "VEC3", ...bounds(positions, 3) },
    { bufferView: nrmView, componentType: 5126, count: vertexCount, type: "VEC3" },
    { bufferView: uvView, componentType: 5126, count: vertexCount, type: "VEC2" },
    { bufferView: idxView, componentType: 5123, count: indices.length, type: "SCALAR" },
  ],
  bufferViews,
  buffers: [{ byteLength: binLength }],
  materials: [
    { name: "red", pbrMetallicRoughness: { baseColorFactor: [1, 0, 0, 1] } },
    { name: "glass", alphaMode: "BLEND", doubleSided: true, pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 0.5], baseColorTexture: { index: 0 } } },
    { name: "leaf", alphaMode: "MASK", alphaCutoff: 0.3, doubleSided: true, pbrMetallicRoughness: { baseColorTexture: { index: 0 } } },
    { name: "cutout-default", alphaMode: "MASK" },
  ],
  textures: [{ source: 0 }],
  images: [{ bufferView: pngView, mimeType: "image/png" }],
}

function glb(json: unknown, bin: Uint8Array[], binBytes: number): Uint8Array {
  let text = new TextEncoder().encode(JSON.stringify(json))
  let jsonPadded = text.byteLength + ((4 - (text.byteLength % 4)) % 4)
  let out = new Uint8Array(12 + 8 + jsonPadded + 8 + binBytes)
  let dv = new DataView(out.buffer)
  dv.setUint32(0, 0x46546c67, true)
  dv.setUint32(4, 2, true)
  dv.setUint32(8, out.byteLength, true)
  dv.setUint32(12, jsonPadded, true)
  dv.setUint32(16, 0x4e4f534a, true)
  out.set(text, 20)
  out.fill(0x20, 20 + text.byteLength, 20 + jsonPadded)
  let at = 20 + jsonPadded
  dv.setUint32(at, binBytes, true)
  dv.setUint32(at + 4, 0x004e4942, true)
  at += 8
  for (let block of bin) {
    out.set(block, at)
    at += block.byteLength
  }
  return out
}

let file = glb(document, binBlocks, binLength)
if (!isGlb(file)) fail("isGlb: the built glb is not recognized")
if (gltfExternalUris(file).length !== 0) fail("gltfExternalUris: a glb lists external uris")

// --- parse ----------------------------------------------------------------

let model = parseGltf(file)
for (let part of model.parts) validateGeometry(part.geometry)
if (model.parts.length !== 3) fail(`parts: ${model.parts.length}, expected 3`)
if (model.parts.map((p) => p.name).join() !== "shifted,mirrored,flat") fail(`part names: ${model.parts.map((p) => p.name).join()}`)
if (model.materials.length !== 4) fail(`materials: ${model.materials.length}, expected 4`)
if (model.images.length !== 1 || model.images[0]!.join() !== fakePng.join()) fail("images: the png bytes did not come through")

let shifted = model.parts[0]!.geometry
if (shifted.vertices.length !== cube.vertices.length) fail("shifted: vertex count changed")
for (let i = 0; i < vertexCount; i++) {
  let at = i * STANDARD_FLOATS
  for (let k = 0; k < STANDARD_FLOATS; k++) {
    let expect = cube.vertices[at + k]! + (k === 0 ? 2 : 0)
    if (!near(shifted.vertices[at + k]!, expect)) {
      fail(`shifted: vertex ${i} float ${k} = ${shifted.vertices[at + k]}, expected ${expect}`)
      break
    }
  }
}
if (shifted.indices.join() !== cube.indices.join()) fail("shifted: indices changed")

// Every triangle's geometric normal must agree with its vertex normals -
// which is exactly what a mirroring node breaks unless the winding flips.
let windingAgrees = (g: { vertices: Float32Array; indices: Uint16Array | Uint32Array }): boolean => {
  let v = g.vertices
  for (let t = 0; t < g.indices.length; t += 3) {
    let a = g.indices[t]! * STANDARD_FLOATS, b = g.indices[t + 1]! * STANDARD_FLOATS, c = g.indices[t + 2]! * STANDARD_FLOATS
    let abx = v[b]! - v[a]!, aby = v[b + 1]! - v[a + 1]!, abz = v[b + 2]! - v[a + 2]!
    let acx = v[c]! - v[a]!, acy = v[c + 1]! - v[a + 1]!, acz = v[c + 2]! - v[a + 2]!
    let nx = aby * acz - abz * acy, ny = abz * acx - abx * acz, nz = abx * acy - aby * acx
    let dot = nx * v[a + 3]! + ny * v[a + 4]! + nz * v[a + 5]!
    if (dot <= 0) return false
  }
  return true
}
let mirrored = model.parts[1]!.geometry
if (!windingAgrees(shifted)) fail("shifted: winding disagrees with normals")
if (!windingAgrees(mirrored)) fail("mirrored: winding was not flipped under the mirroring transform")
for (let i = 0; i < vertexCount; i++) {
  let at = i * STANDARD_FLOATS
  if (!near(mirrored.vertices[at]!, -cube.vertices[at]!) || !near(mirrored.vertices[at + 3]!, -cube.vertices[at + 3]!)) {
    fail(`mirrored: vertex ${i} x/nx not mirrored`)
    break
  }
}

let flat = model.parts[2]!.geometry
let triangles = cube.indices.length / 3
if (flat.vertices.length !== triangles * 3 * STANDARD_FLOATS) fail(`flat: ${flat.vertices.length / STANDARD_FLOATS} vertices, expected ${triangles * 3} (un-indexed)`)
if (!windingAgrees(flat)) fail("flat: generated normals disagree with winding")
for (let t = 0; t < triangles; t++) {
  let a = t * 3 * STANDARD_FLOATS
  let len = Math.hypot(flat.vertices[a + 3]!, flat.vertices[a + 4]!, flat.vertices[a + 5]!)
  if (!near(len, 1)) fail(`flat: triangle ${t} normal length ${len}`)
  for (let k = 1; k < 3; k++) {
    let b = a + k * STANDARD_FLOATS
    if (!near(flat.vertices[a + 3]!, flat.vertices[b + 3]!) || !near(flat.vertices[a + 4]!, flat.vertices[b + 4]!) || !near(flat.vertices[a + 5]!, flat.vertices[b + 5]!)) {
      fail(`flat: triangle ${t} corners disagree on the normal`)
      break
    }
  }
}
// A cube's flat normals are axis-aligned whatever its rotation by 90 degrees.
for (let t = 0; t < triangles; t++) {
  let a = t * 3 * STANDARD_FLOATS
  let axisAligned = [3, 4, 5].filter((k) => near(Math.abs(flat.vertices[a + k]!), 1)).length === 1
  if (!axisAligned) fail(`flat: triangle ${t} normal is not axis aligned after the 90 degree rotation`)
}

let red = model.materials[0]!
let glass = model.materials[1]!
if (red.color.join() !== "1,0,0,1" || red.map !== null || red.transparent || red.doubleSided) fail(`red material: ${JSON.stringify(red)}`)
if (glass.color.join() !== "1,1,1,0.5" || glass.map !== 0 || !glass.transparent || !glass.doubleSided) fail(`glass material: ${JSON.stringify(glass)}`)
if (red.alphaMode !== "OPAQUE" || glass.alphaMode !== "BLEND") fail("alphaMode: OPAQUE/BLEND")
let leaf = model.materials[2]!
let cutout = model.materials[3]!
if (leaf.alphaMode !== "MASK" || leaf.alphaCutoff !== 0.3 || leaf.transparent) fail(`leaf material: ${JSON.stringify(leaf)}`)
if (cutout.alphaMode !== "MASK" || cutout.alphaCutoff !== 0.5) fail(`cutout default: ${JSON.stringify(cutout)}`)
if (model.parts[0]!.material !== 0 || model.parts[2]!.material !== 1) fail("parts: material indices")

let b = model.bounds
if (!(near(b[0]!, -0.5) && near(b[3]!, 2.5) && near(b[1]!, -0.5) && near(b[4]!, 0.5) && near(b[2]!, -0.5) && near(b[5]!, 0.5))) fail(`bounds: ${Array.from(b).join()}`)

// --- container round trip -------------------------------------------------

let sameModel = (a: ModelData, b: ModelData, label: string): void => {
  if (a.parts.length !== b.parts.length) return fail(`${label}: part count`)
  for (let i = 0; i < a.parts.length; i++) {
    let p = a.parts[i]!, q = b.parts[i]!
    if (p.name !== q.name || p.material !== q.material) fail(`${label}: part ${i} header`)
    if (p.geometry.vertices.join() !== q.geometry.vertices.join()) fail(`${label}: part ${i} vertices`)
    if (p.geometry.indices.join() !== q.geometry.indices.join()) fail(`${label}: part ${i} indices`)
    if (p.geometry.indices.constructor !== q.geometry.indices.constructor) fail(`${label}: part ${i} index type`)
  }
  if (JSON.stringify(a.materials) !== JSON.stringify(b.materials)) fail(`${label}: materials`)
  if (a.images.length !== b.images.length || a.images.some((img, i) => img.join() !== b.images[i]!.join())) fail(`${label}: images`)
  if (a.bounds.join() !== b.bounds.join()) fail(`${label}: bounds`)
}
let encoded = encodeModel(model)
sameModel(model, decodeModel(encoded), "round trip")
// A ragged offset must not break the views (decodeModel copies once).
let ragged = new Uint8Array(encoded.byteLength + 1)
ragged.set(encoded, 1)
sameModel(model, decodeModel(ragged.subarray(1)), "round trip (unaligned)")
throws("decodeModel garbage", () => decodeModel(new Uint8Array(32)), "bad magic")

// --- .gltf with external files, and the refusals --------------------------

let external = { ...document, buffers: [{ byteLength: binLength, uri: "scene%20data.bin" }], images: [{ uri: "textures/base.png" }] }
let externalBytes = new TextEncoder().encode(JSON.stringify(external))
if (gltfExternalUris(externalBytes).join() !== "scene%20data.bin,textures/base.png") fail(`gltfExternalUris: ${gltfExternalUris(externalBytes).join()}`)
let bin = new Uint8Array(binLength)
{
  let at = 0
  for (let block of binBlocks) {
    bin.set(block, at)
    at += block.byteLength
  }
}
let resolved = parseGltf(externalBytes, (uri) => {
  if (uri === "scene%20data.bin") return bin
  if (uri === "textures/base.png") return fakePng
  throw new Error("unexpected uri " + uri)
})
sameModel(model, resolved, ".gltf + external files")
throws("external without resolver", () => parseGltf(externalBytes), "no resolver")

let dataUri = { ...external, buffers: [{ byteLength: binLength, uri: "data:application/octet-stream;base64," + btoa(String.fromCharCode(...bin)) }], images: [{ bufferView: pngView, mimeType: "image/png" }] }
sameModel(model, parseGltf(new TextEncoder().encode(JSON.stringify(dataUri))), ".gltf + data: uri")

let draco = { ...document, extensionsRequired: ["KHR_draco_mesh_compression"] }
throws("draco", () => parseGltf(glb(draco, binBlocks, binLength)), "compressed")
let unknownExt = { ...document, extensionsRequired: ["KHR_lights_punctual"] }
throws("unknown required extension", () => parseGltf(glb(unknownExt, binBlocks, binLength)), "KHR_lights_punctual")
let noMaterial = { ...document, meshes: [{ primitives: [{ attributes: { POSITION: 0, NORMAL: 1 }, indices: 3 }] }], nodes: [{ mesh: 0 }], scenes: [{ nodes: [0] }], materials: [] }
let plain = parseGltf(glb(noMaterial, binBlocks, binLength))
if (plain.materials.length !== 1 || plain.materials[0]!.name !== "default" || plain.parts[0]!.name !== "node0") fail(`default material: ${JSON.stringify(plain.materials)} / ${plain.parts[0]?.name}`)

console.log(failures === 0 ? "gltf-check: all checks passed" : `gltf-check: ${failures} failure(s)`)
if (failures > 0) throw new Error("gltf-check failed")
