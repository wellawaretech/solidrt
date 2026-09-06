// Check rig for the glTF parser (src/gltf.ts) and the .srtm container
// (src/model-file.ts): a glb built in memory from the box generator, split
// back into planar accessors the way exporters write them, under a node
// tree - a translated mesh under a translated parent (hierarchy retained,
// vertices left local), a mirrored one (winding must flip), one without
// normals on a matrix-form node (flat normals generated, TRS decomposed)
// and a meshless empty (pruned). Then the container round trip and the
// .gltf + external file path. Pure-module inputs only, so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/gltf-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { gltfExternalUris, isGlb, parseGltf } from "../src/gltf.ts"
import type { ModelData } from "../src/gltf.ts"
import { decodeModel, encodeModel } from "../src/model-file.ts"
import { sampleChannel } from "../src/clip.ts"
import { box, validateGeometry, STANDARD_FLOATS } from "../src/geometry.ts"
import { linearToSrgb } from "../src/color.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
let nearAll = (a: ArrayLike<number>, b: number[]): boolean => a.length === b.length && b.every((v, i) => near(a[i]!, v))
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

// Animation accessors: three keys at 0/1/2 s, a linear translation, a
// linear rotation (identity -> 90 -> 180 degrees about z), a step scale,
// and a two-key CUBICSPLINE translation with zero tangents (a smoothstep).
let animTimes = new Float32Array([0, 1, 2])
let animPos = new Float32Array([0, 0, 0, 1, 0, 0, 1, 1, 0])
let animRot = new Float32Array([0, 0, 0, 1, 0, 0, Math.SQRT1_2, Math.SQRT1_2, 0, 0, 1, 0])
let animScale = new Float32Array([1, 1, 1, 2, 2, 2, 3, 3, 3])
let cubicTimes = new Float32Array([0, 2])
let cubicPos = new Float32Array([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0])
let animTimesView = pushView(asBytes(animTimes))
let animPosView = pushView(asBytes(animPos))
let animRotView = pushView(asBytes(animRot))
let animScaleView = pushView(asBytes(animScale))
let cubicTimesView = pushView(asBytes(cubicTimes))
let cubicPosView = pushView(asBytes(cubicPos))

// Skin accessors: every vertex weighted between joints 0 and 1 with
// weights that sum to 2 (the parser must renormalize to 0.6/0.4), and
// two inverse binds - identity, and a translate(-1, 0, 0).
let jointsData = new Float32Array(vertexCount * 4)
let weightsData = new Float32Array(vertexCount * 4)
for (let i = 0; i < vertexCount; i++) {
  jointsData[i * 4 + 1] = 1
  weightsData[i * 4] = 1.2
  weightsData[i * 4 + 1] = 0.8
}
let bindData = new Float32Array(32)
bindData[0] = 1; bindData[5] = 1; bindData[10] = 1; bindData[15] = 1
bindData[16] = 1; bindData[21] = 1; bindData[26] = 1; bindData[31] = 1
bindData[28] = -1
let jointsView = pushView(asBytes(jointsData))
let weightsView = pushView(asBytes(weightsData))
let bindView = pushView(asBytes(bindData))

let bounds = (data: Float32Array, n: number): { min: number[]; max: number[] } => {
  let min = new Array(n).fill(Infinity)
  let max = new Array(n).fill(-Infinity)
  for (let i = 0; i < data.length; i++) {
    min[i % n] = Math.min(min[i % n], data[i]!)
    max[i % n] = Math.max(max[i % n], data[i]!)
  }
  return { min, max }
}

// "flat" is a matrix-form node: a 90 degree y rotation, column-major, so
// the parser must decompose it to the quaternion [0, sqrt(.5), 0, sqrt(.5)].
const ROT_Y_90 = [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1]

let document = {
  asset: { version: "2.0" },
  scene: 0,
  scenes: [{ nodes: [3, 1, 2, 6] }],
  nodes: [
    { name: "shifted", mesh: 0, translation: [1, 0, 0] },
    { name: "mirrored", mesh: 0, scale: [-1, 1, 1] },
    { name: "flat", mesh: 1, matrix: ROT_Y_90 },
    { name: "rig", translation: [1, 0, 0], children: [0, 4, 5] },
    { name: "empty", translation: [9, 9, 9] },
    { name: "tail", translation: [0, 0, 2] },
    // The node transform of a skinned mesh must be IGNORED (its
    // vertices are model-space bind pose): [5, 5, 5] must not shift the
    // part or the bounds.
    { name: "skinny", mesh: 2, skin: 0, translation: [5, 5, 5] },
  ],
  skins: [{ joints: [4, 5], inverseBindMatrices: 12 }],
  meshes: [
    { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2 }, indices: 3, material: 0 }] },
    { primitives: [{ attributes: { POSITION: 0 }, indices: 3, material: 1 }] },
    { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2, JOINTS_0: 10, WEIGHTS_0: 11 }, indices: 3, material: 0 }] },
  ],
  accessors: [
    { bufferView: posView, componentType: 5126, count: vertexCount, type: "VEC3", ...bounds(positions, 3) },
    { bufferView: nrmView, componentType: 5126, count: vertexCount, type: "VEC3" },
    { bufferView: uvView, componentType: 5126, count: vertexCount, type: "VEC2" },
    { bufferView: idxView, componentType: 5123, count: indices.length, type: "SCALAR" },
    { bufferView: animTimesView, componentType: 5126, count: 3, type: "SCALAR" },
    { bufferView: animPosView, componentType: 5126, count: 3, type: "VEC3" },
    { bufferView: animRotView, componentType: 5126, count: 3, type: "VEC4" },
    { bufferView: animScaleView, componentType: 5126, count: 3, type: "VEC3" },
    { bufferView: cubicTimesView, componentType: 5126, count: 2, type: "SCALAR" },
    { bufferView: cubicPosView, componentType: 5126, count: 6, type: "VEC3" },
    { bufferView: jointsView, componentType: 5126, count: vertexCount, type: "VEC4" },
    { bufferView: weightsView, componentType: 5126, count: vertexCount, type: "VEC4" },
    { bufferView: bindView, componentType: 5126, count: 2, type: "MAT4" },
  ],
  animations: [
    {
      name: "move",
      channels: [
        { sampler: 0, target: { node: 0, path: "translation" } },
        { sampler: 1, target: { node: 4, path: "rotation" } },
        { sampler: 2, target: { node: 1, path: "scale" } },
        { sampler: 0, target: { node: 0, path: "weights" } },
      ],
      samplers: [
        { input: 4, output: 5, interpolation: "LINEAR" },
        { input: 4, output: 6 },
        { input: 4, output: 7, interpolation: "STEP" },
      ],
    },
    {
      name: "bounce",
      channels: [{ sampler: 0, target: { node: 0, path: "translation" } }],
      samplers: [{ input: 8, output: 9, interpolation: "CUBICSPLINE" }],
    },
  ],
  bufferViews,
  buffers: [{ byteLength: binLength }],
  materials: [
    { name: "red", pbrMetallicRoughness: { baseColorFactor: [1, 0, 0, 1] } },
    {
      name: "glass",
      alphaMode: "BLEND",
      doubleSided: true,
      pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 0.5], baseColorTexture: { index: 0 } },
      normalTexture: { index: 0, scale: 0.5 },
      emissiveFactor: [1, 0.5, 0],
      emissiveTexture: { index: 0 },
      extensions: { KHR_materials_emissive_strength: { emissiveStrength: 2 } },
    },
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
if (gltfExternalUris(file).length !== 0) fail("gltfExternalUris: a self-contained glb lists external uris")

// A .glb MAY reference external files (spec-legal; real exporters write
// image uris): they must be listed and resolved like a .gltf's.
{
  let externalImageGlb = glb({ ...document, images: [{ uri: "textures/base.png" }] }, binBlocks, binLength)
  if (gltfExternalUris(externalImageGlb).join() !== "textures/base.png") {
    fail(`gltfExternalUris on a glb with an external image: ${gltfExternalUris(externalImageGlb).join()}`)
  }
  throws("glb external image without resolver", () => parseGltf(externalImageGlb), "no resolver")
  let resolvedGlb = parseGltf(externalImageGlb, (uri) => {
    if (uri === "textures/base.png") return fakePng
    throw new Error("unexpected uri " + uri)
  })
  if (resolvedGlb.images.length !== 1 || resolvedGlb.images[0]!.join() !== fakePng.join()) {
    fail("glb external image: the png bytes did not come through the resolver")
  }
}

// --- parse ----------------------------------------------------------------

let model = parseGltf(file)
for (let part of model.parts) validateGeometry(part.geometry)
if (model.parts.length !== 4) fail(`parts: ${model.parts.length}, expected 4`)
if (model.parts.map((p) => p.name).join() !== "shifted,mirrored,flat,skinny") fail(`part names: ${model.parts.map((p) => p.name).join()}`)
if (model.materials.length !== 4) fail(`materials: ${model.materials.length}, expected 4`)
if (model.images.length !== 1 || model.images[0]!.join() !== fakePng.join()) fail("images: the png bytes did not come through")

// The node table: pre-order (rig materialized by its descendant part),
// "flat"'s matrix decomposed to TRS, and the meshless "empty" and
// "tail" retained ONLY as a skin's joints / an animation target
// (materialized after the walk, so they come last).
if (model.nodes.map((n) => n.name).join() !== "rig,shifted,mirrored,flat,skinny,empty,tail") fail(`node names: ${model.nodes.map((n) => n.name).join()}`)
if (model.nodes.map((n) => (n.parent === null ? "-" : n.parent)).join() !== "-,0,-,-,-,0,0") fail(`node parents: ${model.nodes.map((n) => n.parent).join()}`)
if (model.parts.map((p) => p.node).join() !== "1,2,3,4") fail(`part nodes: ${model.parts.map((p) => p.node).join()}`)
if (model.parts.map((p) => (p.skin === null ? "-" : p.skin)).join() !== "-,-,-,0") fail(`part skins: ${model.parts.map((p) => p.skin).join()}`)

// The skin: joints remapped to the compact table, binds through, the
// skinned layout with renormalized weights, node transform ignored.
if (model.skins.length !== 1) fail(`skins: ${model.skins.length}, expected 1`)
let skin = model.skins[0]!
if (skin.joints.join() !== "5,6") fail(`skin joints: ${skin.joints.join()}`)
if (skin.inverseBind.length !== 32 || !near(skin.inverseBind[28]!, -1)) fail(`skin inverse binds: ${skin.inverseBind.length} floats, [28] = ${skin.inverseBind[28]}`)
let skinny = model.parts[3]!.geometry
if (skinny.layout !== "skinned") fail(`skinny layout: ${String(skinny.layout)}`)
if (skinny.vertices.length !== vertexCount * 16) fail(`skinny stride: ${skinny.vertices.length / vertexCount} floats per vertex`)
for (let i = 0; i < vertexCount; i++) {
  let at = i * 16
  for (let k = 0; k < 8; k++) {
    if (!near(skinny.vertices[at + k]!, cube.vertices[i * STANDARD_FLOATS + k]!)) {
      fail(`skinny: vertex ${i} standard float ${k} differs (node transform baked?)`)
      break
    }
  }
  if (skinny.vertices[at + 8] !== 0 || skinny.vertices[at + 9] !== 1) fail(`skinny: vertex ${i} joints ${skinny.vertices[at + 8]},${skinny.vertices[at + 9]}`)
  if (!near(skinny.vertices[at + 12]!, 0.6) || !near(skinny.vertices[at + 13]!, 0.4)) {
    fail(`skinny: vertex ${i} weights not renormalized: ${skinny.vertices[at + 12]},${skinny.vertices[at + 13]}`)
  }
  if (i > 2) break
}
// Joint boxes: every vertex is weighted to both joints, so joint 0 (an
// identity bind) boxes the whole cube and joint 1 (bound at x -1) the same
// cube shifted by its inverse bind.
let cubeBox = bounds(positions, 3)
let jointBox = (j: number): number[] => Array.from(skin.jointBounds.subarray(j * 6, j * 6 + 6))
if (!nearAll(jointBox(0), [...cubeBox.min, ...cubeBox.max])) fail(`joint 0 bounds: ${jointBox(0).join()}`)
if (!nearAll(jointBox(1), [cubeBox.min[0]! - 1, cubeBox.min[1]!, cubeBox.min[2]!, cubeBox.max[0]! - 1, cubeBox.max[1]!, cubeBox.max[2]!])) {
  fail(`joint 1 bounds: ${jointBox(1).join()}`)
}
let rig = model.nodes[0]!
let shiftedNode = model.nodes[1]!
let mirroredNode = model.nodes[2]!
let flatNode = model.nodes[3]!
if (rig.position.join() !== "1,0,0" || shiftedNode.position.join() !== "1,0,0") fail("node positions: rig/shifted")
if (mirroredNode.scale.join() !== "-1,1,1") fail(`mirrored node scale: ${mirroredNode.scale.join()}`)
let q = flatNode.rotation
if (!(near(q[0], 0) && near(q[1], Math.SQRT1_2) && near(q[2], 0) && near(q[3], Math.SQRT1_2))) fail(`flat node rotation (matrix decompose): ${q.join()}`)
if (!near(flatNode.scale[0], 1) || !near(flatNode.scale[1], 1) || !near(flatNode.scale[2], 1)) fail(`flat node scale: ${flatNode.scale.join()}`)

// Vertices stay node-local: the shifted part's geometry is the cube's,
// untranslated - the node carries the placement.
let shifted = model.parts[0]!.geometry
if (shifted.vertices.length !== cube.vertices.length) fail("shifted: vertex count changed")
for (let i = 0; i < vertexCount * STANDARD_FLOATS; i++) {
  if (!near(shifted.vertices[i]!, cube.vertices[i]!)) {
    fail(`shifted: vertex float ${i} = ${shifted.vertices[i]}, expected ${cube.vertices[i]} (local, unbaked)`)
    break
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
// The mirrored part keeps LOCAL vertices and normals unmirrored; the
// winding flip is baked into the index order (each triangle's b/c
// swapped), so the LOCAL winding deliberately disagrees with the stored
// normals - the node's mirroring transform turns both right at render.
if (windingAgrees(mirrored)) fail("mirrored: winding was not flipped under the mirroring transform")
for (let i = 0; i < vertexCount; i++) {
  let at = i * STANDARD_FLOATS
  if (!near(mirrored.vertices[at]!, cube.vertices[at]!) || !near(mirrored.vertices[at + 3]!, cube.vertices[at + 3]!)) {
    fail(`mirrored: vertex ${i} x/nx not local (baked?)`)
    break
  }
}
let flippedIndices: number[] = []
for (let t = 0; t < cube.indices.length; t += 3) flippedIndices.push(cube.indices[t]!, cube.indices[t + 2]!, cube.indices[t + 1]!)
if (mirrored.indices.join() !== flippedIndices.join()) fail("mirrored: indices are not the flipped cube indices")

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
// Local flat normals of an axis-aligned cube are axis-aligned - the
// node's rotation is NOT baked in.
for (let t = 0; t < triangles; t++) {
  let a = t * 3 * STANDARD_FLOATS
  let axisAligned = [3, 4, 5].filter((k) => near(Math.abs(flat.vertices[a + k]!), 1)).length === 1
  if (!axisAligned) fail(`flat: triangle ${t} normal is not axis aligned in local space`)
}

let red = model.materials[0]!
let glass = model.materials[1]!
// Factors are linear in the file and sRGB on the material: 0 and 1 survive
// the encode up to float rounding, 0.5 does not (it lands on ~0.735).
if (!nearAll(red.color, [1, 0, 0, 1]) || red.map !== null || red.transparent || red.doubleSided) fail(`red material: ${JSON.stringify(red)}`)
if (red.normalMap !== null || red.normalScale !== 1 || red.emissive.join() !== "0,0,0" || red.emissiveMap !== null) fail(`red material surface maps: ${JSON.stringify(red)}`)
if (!nearAll(glass.color, [1, 1, 1, 0.5]) || glass.map !== 0 || !glass.transparent || !glass.doubleSided) fail(`glass material: ${JSON.stringify(glass)}`)
if (glass.normalMap !== 0 || glass.normalScale !== 0.5) fail(`glass normal map: ${JSON.stringify(glass)}`)
// KHR_materials_emissive_strength stays a separate intensity, not folded
// into the (sRGB-encoded) emissive color.
if (!nearAll(glass.emissive, [1, linearToSrgb(0.5), 0]) || glass.emissiveIntensity !== 2 || glass.emissiveMap !== 0) {
  fail(`glass emissive: ${JSON.stringify(glass)}`)
}
if (red.alphaMode !== "OPAQUE" || glass.alphaMode !== "BLEND") fail("alphaMode: OPAQUE/BLEND")
let leaf = model.materials[2]!
let cutout = model.materials[3]!
if (leaf.alphaMode !== "MASK" || leaf.alphaCutoff !== 0.3 || leaf.transparent) fail(`leaf material: ${JSON.stringify(leaf)}`)
if (cutout.alphaMode !== "MASK" || cutout.alphaCutoff !== 0.5) fail(`cutout default: ${JSON.stringify(cutout)}`)
if (model.parts[0]!.material !== 0 || model.parts[2]!.material !== 1) fail("parts: material indices")

let b = model.bounds
if (!(near(b[0]!, -0.5) && near(b[3]!, 2.5) && near(b[1]!, -0.5) && near(b[4]!, 0.5) && near(b[2]!, -0.5) && near(b[5]!, 0.5))) fail(`bounds: ${Array.from(b).join()}`)

// --- clips and sampling ---------------------------------------------------

if (model.clips.map((c) => c.name).join() !== "move,bounce") fail(`clips: ${model.clips.map((c) => c.name).join()}`)
let move = model.clips[0]!
let bounce = model.clips[1]!
if (!near(move.duration, 2) || !near(bounce.duration, 2)) fail(`clip durations: ${move.duration}, ${bounce.duration}`)
// The weights channel (morph targets) is skipped, the other three kept;
// targets remap to the compact node table.
if (move.channels.length !== 3) fail(`move channels: ${move.channels.length}, expected 3`)
if (move.channels.map((c) => c.node + ":" + c.path + ":" + c.interpolation).join() !== "1:position:linear,5:rotation:linear,2:scale:step") {
  fail(`move channel headers: ${move.channels.map((c) => c.node + ":" + c.path + ":" + c.interpolation).join()}`)
}
if (bounce.channels.length !== 1 || bounce.channels[0]!.interpolation !== "cubic") fail("bounce: one cubic channel expected")
if (bounce.channels[0]!.values.length !== 18) fail(`bounce cubic values: ${bounce.channels[0]!.values.length} floats, expected 18`)

let sample: number[] = [0, 0, 0, 0]
let expectSample = (label: string, channel: (typeof move.channels)[0], t: number, expected: number[]): void => {
  sampleChannel(channel, t, sample)
  for (let e = 0; e < expected.length; e++) {
    if (!near(sample[e]!, expected[e]!)) {
      fail(`${label} at ${t}: [${sample.slice(0, expected.length).join()}], expected [${expected.join()}]`)
      return
    }
  }
}
let movePos = move.channels[0]!
expectSample("linear position", movePos, -1, [0, 0, 0])
expectSample("linear position", movePos, 0.5, [0.5, 0, 0])
expectSample("linear position", movePos, 1.5, [1, 0.5, 0])
expectSample("linear position", movePos, 5, [1, 1, 0])
let moveRot = move.channels[1]!
let h = Math.sin(Math.PI / 8)
expectSample("slerp rotation", moveRot, 0.5, [0, 0, h, Math.cos(Math.PI / 8)])
expectSample("slerp rotation", moveRot, 2, [0, 0, 1, 0])
let moveScale = move.channels[2]!
expectSample("step scale", moveScale, 1.9, [2, 2, 2])
expectSample("step scale", moveScale, 2, [3, 3, 3])
let cubic = bounce.channels[0]!
expectSample("cubic position", cubic, 0, [0, 0, 0])
// Zero tangents make the Hermite a smoothstep: halfway = half the value.
expectSample("cubic position", cubic, 1, [2, 0, 0])
expectSample("cubic position", cubic, 2, [4, 0, 0])

// --- container round trip -------------------------------------------------

let sameModel = (a: ModelData, b: ModelData, label: string): void => {
  if (JSON.stringify(a.nodes) !== JSON.stringify(b.nodes)) fail(`${label}: nodes`)
  if (a.parts.length !== b.parts.length) return fail(`${label}: part count`)
  for (let i = 0; i < a.parts.length; i++) {
    let p = a.parts[i]!, q = b.parts[i]!
    if (p.name !== q.name || p.material !== q.material || p.node !== q.node || p.skin !== q.skin) fail(`${label}: part ${i} header`)
    if ((p.geometry.layout ?? "standard") !== (q.geometry.layout ?? "standard")) fail(`${label}: part ${i} layout`)
    if (p.geometry.vertices.join() !== q.geometry.vertices.join()) fail(`${label}: part ${i} vertices`)
    if (p.geometry.indices.join() !== q.geometry.indices.join()) fail(`${label}: part ${i} indices`)
    if (p.geometry.indices.constructor !== q.geometry.indices.constructor) fail(`${label}: part ${i} index type`)
  }
  if (JSON.stringify(a.materials) !== JSON.stringify(b.materials)) fail(`${label}: materials`)
  if (a.skins.length !== b.skins.length) fail(`${label}: skin count`)
  for (let i = 0; i < a.skins.length; i++) {
    if (a.skins[i]!.joints.join() !== b.skins[i]!.joints.join()) fail(`${label}: skin ${i} joints`)
    if (a.skins[i]!.inverseBind.join() !== b.skins[i]!.inverseBind.join()) fail(`${label}: skin ${i} binds`)
  }
  if (a.clips.length !== b.clips.length) fail(`${label}: clip count`)
  for (let i = 0; i < a.clips.length; i++) {
    let c = a.clips[i]!, d = b.clips[i]!
    if (c.name !== d.name || !near(c.duration, d.duration) || c.channels.length !== d.channels.length) fail(`${label}: clip ${i} header`)
    for (let k = 0; k < c.channels.length; k++) {
      let x = c.channels[k]!, y = d.channels[k]!
      if (x.node !== y.node || x.path !== y.path || x.interpolation !== y.interpolation) fail(`${label}: clip ${i} channel ${k} header`)
      if (x.times.join() !== y.times.join() || x.values.join() !== y.values.join()) fail(`${label}: clip ${i} channel ${k} data`)
    }
  }
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
if (plain.nodes.length !== 1 || plain.nodes[0]!.name !== "node0" || plain.nodes[0]!.parent !== null) fail(`nameless node table: ${JSON.stringify(plain.nodes)}`)

console.log(failures === 0 ? "gltf-check: all checks passed" : `gltf-check: ${failures} failure(s)`)
if (failures > 0) throw new Error("gltf-check failed")
