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
import { channelElements, sampleChannel } from "../src/clip.ts"
import { box, geometryAttribute, geometryBounds, layoutKey, layoutStride, validateGeometry, vertexBytes, withAttribute, MORPH_ENTRY_TEXELS, MORPH_TEXEL_FLOATS, BASE_FLOATS, VERTEX_LAYOUTS } from "../src/geometry.ts"
import type { Geometry } from "../src/geometry.ts"
import { linearToSrgb } from "../src/color.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
// The rigs here build base all-float layouts, so the vertex bytes read
// back as floats; the view type is not the geometry contract.
let floats = (g: Geometry): Float32Array => new Float32Array(g.vertices.buffer, g.vertices.byteOffset, g.vertices.byteLength / Float32Array.BYTES_PER_ELEMENT)
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
let vertexCount = floats(cube).length / BASE_FLOATS
let positions = new Float32Array(vertexCount * 3)
let normals = new Float32Array(vertexCount * 3)
let uvs = new Float32Array(vertexCount * 2)
for (let i = 0; i < vertexCount; i++) {
  let at = i * BASE_FLOATS
  positions.set(floats(cube).subarray(at, at + 3), i * 3)
  normals.set(floats(cube).subarray(at + 3, at + 6), i * 3)
  uvs.set(floats(cube).subarray(at + 6, at + 8), i * 2)
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
let asBytes = (a: Float32Array | Uint16Array | Uint8Array): Uint8Array => new Uint8Array(a.buffer, a.byteOffset, a.byteLength)
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
// two inverse binds - identity, and a translate(-1, 0, 0). The float
// joints accessor is off-spec (JOINTS_0 is u8 or u16) and is what the
// narrowing check below feeds; "skinny" itself uses the u8 accessor.
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

// A COLOR_0 accessor the way exporters quantize it: normalized u8 VEC4,
// every vertex [255, 0, 0, 128] - red at half alpha.
let colorData = new Uint8Array(vertexCount * 4)
for (let i = 0; i < vertexCount; i++) colorData.set([255, 0, 0, 128], i * 4)
let colorView = pushView(asBytes(colorData))

// Quantized skin channels the way exporters write them: u8 joints and
// normalized u8 weights [255, 170] (sum above 1, so the parser
// renormalizes to 0.6/0.4).
let jointsU8 = new Uint8Array(vertexCount * 4)
let weightsU8 = new Uint8Array(vertexCount * 4)
for (let i = 0; i < vertexCount; i++) {
  jointsU8[i * 4 + 1] = 1
  weightsU8[i * 4] = 255
  weightsU8[i * 4 + 1] = 170
}
let jointsU8View = pushView(asBytes(jointsU8))
let weightsU8View = pushView(asBytes(weightsU8))

// Morph target accessors: "puff" as a SPARSE accessor the way exporters
// write targets (no bufferView - a zero base - with two overrides: vertex
// 0 up by 1, vertex 5 right by 0.5), "lift" dense (every vertex +z by 1,
// its normal delta +z by 0.5), and a weights track: three keys of two
// weights each, [0, 0] -> [1, 0] -> [0.5, 1].
let sparseIndices = new Uint16Array([0, 5])
let sparseValues = new Float32Array([0, 1, 0, 0.5, 0, 0])
let liftPositions = new Float32Array(vertexCount * 3)
let liftNormals = new Float32Array(vertexCount * 3)
for (let i = 0; i < vertexCount; i++) {
  liftPositions[i * 3 + 2] = 1
  liftNormals[i * 3 + 2] = 0.5
}
let morphWeights = new Float32Array([0, 0, 1, 0, 0.5, 1])
let sparseIndicesView = pushView(asBytes(sparseIndices))
let sparseValuesView = pushView(asBytes(sparseValues))
let liftPositionsView = pushView(asBytes(liftPositions))
let liftNormalsView = pushView(asBytes(liftNormals))
let morphWeightsView = pushView(asBytes(morphWeights))

// Vertex i of channel `name` as floats, through the accessor.
let read = (g: Geometry, name: string, i: number): number[] => {
  let a = geometryAttribute(g, name)
  if (a === null) {
    fail("read: no " + name + " channel")
    return []
  }
  let out: number[] = []
  for (let k = 0; k < a.components; k++) out.push(a.get(i, k))
  return out
}

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
    { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2, JOINTS_0: 14, WEIGHTS_0: 11 }, indices: 3, material: 0 }] },
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
    { bufferView: colorView, componentType: 5121, normalized: true, count: vertexCount, type: "VEC4" },
    { bufferView: jointsU8View, componentType: 5121, count: vertexCount, type: "VEC4" },
    { bufferView: weightsU8View, componentType: 5121, normalized: true, count: vertexCount, type: "VEC4" },
    {
      componentType: 5126,
      count: vertexCount,
      type: "VEC3",
      sparse: { count: 2, indices: { bufferView: sparseIndicesView, componentType: 5123 }, values: { bufferView: sparseValuesView } },
    },
    { bufferView: liftPositionsView, componentType: 5126, count: vertexCount, type: "VEC3" },
    { bufferView: liftNormalsView, componentType: 5126, count: vertexCount, type: "VEC3" },
    { bufferView: morphWeightsView, componentType: 5126, count: 6, type: "SCALAR" },
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

// --- primitive modes ------------------------------------------------------
// Lines and points keep their topology on the geometry (still indexed,
// zero normals when the file has none: the flat-shading un-index is for
// triangles), a triangle strip unrolls to a list that the mirror flip then
// applies to, a line loop closes into a strip, an unknown mode throws.
{
  let modes = parseGltf(
    glb(
      {
        ...document,
        scenes: [{ nodes: [0, 1, 2, 3] }],
        nodes: [
          { name: "wires", mesh: 0 },
          { name: "dots", mesh: 1 },
          { name: "strip", mesh: 2, scale: [-1, 1, 1] },
          { name: "loop", mesh: 3 },
        ],
        meshes: [
          { primitives: [{ attributes: { POSITION: 0 }, indices: 3, mode: 1 }] },
          { primitives: [{ attributes: { POSITION: 0 }, indices: 3, mode: 0 }] },
          { primitives: [{ attributes: { POSITION: 0, NORMAL: 1 }, indices: 3, mode: 5 }] },
          { primitives: [{ attributes: { POSITION: 0 }, indices: 3, mode: 2 }] },
        ],
        skins: [],
        animations: [],
      },
      binBlocks,
      binLength,
    ),
  )
  for (let part of modes.parts) validateGeometry(part.geometry)
  if (modes.parts.map((p) => p.name).join() !== "wires,dots,strip,loop") fail(`mode part names: ${modes.parts.map((p) => p.name).join()}`)
  let geometryOf = (name: string) => modes.parts.find((p) => p.name === name)!.geometry
  let wires = geometryOf("wires")
  if (wires.topology !== "lines") fail(`wires topology: ${String(wires.topology)}`)
  if (wires.indices.join() !== cube.indices.join()) fail("wires: indices changed")
  if (floats(wires).length !== vertexCount * BASE_FLOATS) fail("wires: un-indexed, the flat-shading path ran on lines")
  if (floats(wires)[3] !== 0 || floats(wires)[4] !== 0 || floats(wires)[5] !== 0) fail("wires: normals are not zero")
  if (geometryOf("dots").topology !== "points") fail("dots topology")
  let strip = geometryOf("strip")
  if (strip.topology !== undefined) fail(`strip topology: ${String(strip.topology)}, expected a triangle list`)
  if (strip.indices.length !== (cube.indices.length - 2) * 3) fail(`strip: ${strip.indices.length} indices, expected ${(cube.indices.length - 2) * 3}`)
  // Triangle 0 of the strip is (s0, s1, s2) and triangle 1 is (s1, s3, s2)
  // (the odd swap); under the mirror both flip their last two.
  let s = cube.indices
  if (Array.from(strip.indices.subarray(0, 6)).join() !== [s[0], s[2], s[1], s[1], s[2], s[3]].join()) {
    fail(`strip under a mirror: first triangles ${Array.from(strip.indices.subarray(0, 6)).join()}`)
  }
  let loop = geometryOf("loop")
  if (loop.topology !== "line-strip") fail(`loop topology: ${String(loop.topology)}`)
  if (loop.indices.length !== cube.indices.length + 1 || loop.indices[loop.indices.length - 1] !== loop.indices[0]) fail("loop: not closed into a strip")
  throws(
    "unknown primitive mode",
    () =>
      parseGltf(
        glb({ ...document, scenes: [{ nodes: [0] }], nodes: [{ name: "odd", mesh: 0 }], meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 3, mode: 7 }] }], skins: [], animations: [] }, binBlocks, binLength),
      ),
    "primitive mode",
  )
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
let skinnedStride = layoutStride("skinned")
if (vertexBytes(skinny.vertices).byteLength !== vertexCount * skinnedStride) {
  fail(`skinny stride: ${vertexBytes(skinny.vertices).byteLength / vertexCount} bytes per vertex, expected ${skinnedStride}`)
}
for (let i = 0; i < vertexCount; i++) {
  let base = [...read(skinny, "aPos", i), ...read(skinny, "aNormal", i), ...read(skinny, "aUV", i)]
  if (!nearAll(base, Array.from(floats(cube).subarray(i * BASE_FLOATS, (i + 1) * BASE_FLOATS)))) {
    fail(`skinny: vertex ${i} base prefix differs (node transform baked?)`)
  }
  if (!nearAll(read(skinny, "aJoints", i), [0, 1, 0, 0])) fail(`skinny: vertex ${i} joints ${read(skinny, "aJoints", i)}`)
  if (!nearAll(read(skinny, "aWeights", i), [0.6, 0.4, 0, 0])) fail(`skinny: vertex ${i} weights not renormalized: ${read(skinny, "aWeights", i)}`)
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
if (floats(shifted).length !== floats(cube).length) fail("shifted: vertex count changed")
for (let i = 0; i < vertexCount * BASE_FLOATS; i++) {
  if (!near(floats(shifted)[i]!, floats(cube)[i]!)) {
    fail(`shifted: vertex float ${i} = ${floats(shifted)[i]}, expected ${floats(cube)[i]} (local, unbaked)`)
    break
  }
}
if (shifted.indices.join() !== cube.indices.join()) fail("shifted: indices changed")

// Every triangle's geometric normal must agree with its vertex normals -
// which is exactly what a mirroring node breaks unless the winding flips.
let windingAgrees = (g: Geometry): boolean => {
  let v = floats(g)
  for (let t = 0; t < g.indices.length; t += 3) {
    let a = g.indices[t]! * BASE_FLOATS, b = g.indices[t + 1]! * BASE_FLOATS, c = g.indices[t + 2]! * BASE_FLOATS
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
  let at = i * BASE_FLOATS
  if (!near(floats(mirrored)[at]!, floats(cube)[at]!) || !near(floats(mirrored)[at + 3]!, floats(cube)[at + 3]!)) {
    fail(`mirrored: vertex ${i} x/nx not local (baked?)`)
    break
  }
}
let flippedIndices: number[] = []
for (let t = 0; t < cube.indices.length; t += 3) flippedIndices.push(cube.indices[t]!, cube.indices[t + 2]!, cube.indices[t + 1]!)
if (mirrored.indices.join() !== flippedIndices.join()) fail("mirrored: indices are not the flipped cube indices")

let flat = model.parts[2]!.geometry
let triangles = cube.indices.length / 3
if (floats(flat).length !== triangles * 3 * BASE_FLOATS) fail(`flat: ${floats(flat).length / BASE_FLOATS} vertices, expected ${triangles * 3} (un-indexed)`)
if (!windingAgrees(flat)) fail("flat: generated normals disagree with winding")
for (let t = 0; t < triangles; t++) {
  let a = t * 3 * BASE_FLOATS
  let len = Math.hypot(floats(flat)[a + 3]!, floats(flat)[a + 4]!, floats(flat)[a + 5]!)
  if (!near(len, 1)) fail(`flat: triangle ${t} normal length ${len}`)
  for (let k = 1; k < 3; k++) {
    let b = a + k * BASE_FLOATS
    if (!near(floats(flat)[a + 3]!, floats(flat)[b + 3]!) || !near(floats(flat)[a + 4]!, floats(flat)[b + 4]!) || !near(floats(flat)[a + 5]!, floats(flat)[b + 5]!)) {
      fail(`flat: triangle ${t} corners disagree on the normal`)
      break
    }
  }
}
// Local flat normals of an axis-aligned cube are axis-aligned - the
// node's rotation is NOT baked in.
for (let t = 0; t < triangles; t++) {
  let a = t * 3 * BASE_FLOATS
  let axisAligned = [3, 4, 5].filter((k) => near(Math.abs(floats(flat)[a + k]!), 1)).length === 1
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

// The unskinned parts span x -0.5..2.5 (the cube under "shifted" at
// x 2, "mirrored", "flat"); the skinned part is placed by its joints at
// rest, not by its node or its bind-pose box: joint 0 ("empty", world
// (10, 9, 9), identity bind) puts the cube at 9.5..10.5 / 8.5..9.5 /
// 8.5..9.5 and joint 1 ("tail", world (1, 0, 2), bind translate(-1, 0, 0))
// at -0.5..0.5 / -0.5..0.5 / 1.5..2.5.
let b = model.bounds
if (!(near(b[0]!, -0.5) && near(b[3]!, 10.5) && near(b[1]!, -0.5) && near(b[4]!, 9.5) && near(b[2]!, -0.5) && near(b[5]!, 9.5))) fail(`bounds: ${Array.from(b).join()}`)

// --- clips and sampling ---------------------------------------------------

if (model.clips.map((c) => c.name).join() !== "move,bounce") fail(`clips: ${model.clips.map((c) => c.name).join()}`)
let move = model.clips[0]!
let bounce = model.clips[1]!
if (!near(move.duration, 2) || !near(bounce.duration, 2)) fail(`clip durations: ${move.duration}, ${bounce.duration}`)
// The weights channel targets a node WITHOUT morph targets, so it has
// nothing to weigh and is skipped; the other three are kept, their
// targets remapped to the compact node table.
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
    if ((p.placements ?? []).join() !== (q.placements ?? []).join()) fail(`${label}: part ${i} placements`)
    if (JSON.stringify(p.extras) !== JSON.stringify(q.extras)) fail(`${label}: part ${i} extras`)
    if (layoutKey(p.geometry.layout) !== layoutKey(q.geometry.layout)) fail(`${label}: part ${i} layout`)
    if (floats(p.geometry).join() !== floats(q.geometry).join()) fail(`${label}: part ${i} vertices`)
    if (p.geometry.indices.join() !== q.geometry.indices.join()) fail(`${label}: part ${i} indices`)
    if (p.geometry.indices.constructor !== q.geometry.indices.constructor) fail(`${label}: part ${i} index type`)
    let pm = p.geometry.morphs, qm = q.geometry.morphs
    if ((pm === undefined) !== (qm === undefined)) fail(`${label}: part ${i} morphs presence`)
    else if (pm !== undefined && qm !== undefined) {
      if (pm.names.join() !== qm.names.join()) fail(`${label}: part ${i} morph names`)
      if (pm.texels.join() !== qm.texels.join()) fail(`${label}: part ${i} morph texels`)
      if (pm.extent.join() !== qm.extent.join()) fail(`${label}: part ${i} morph extent`)
    }
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
  if (JSON.stringify(a.extras) !== JSON.stringify(b.extras)) fail(`${label}: extras`)
  let blobs = (m: ModelData): [string, Uint8Array][] => Object.entries(m.blobs ?? {})
  if (blobs(a).map(([k]) => k).join() !== blobs(b).map(([k]) => k).join() || blobs(a).some(([k, v]) => v.join() !== b.blobs![k]!.join())) fail(`${label}: blobs`)
}
let encoded = encodeModel(model)
sameModel(model, decodeModel(encoded), "round trip")
// A ragged offset must not break the views (decodeModel copies once).
let ragged = new Uint8Array(encoded.byteLength + 1)
ragged.set(encoded, 1)
sameModel(model, decodeModel(ragged.subarray(1)), "round trip (unaligned)")
throws("decodeModel garbage", () => decodeModel(new Uint8Array(32)), "bad magic")
// The container writes one interleaved buffer per part: extra streams are
// run-time data and refused.
{
  let part = model.parts[0]!
  let streamed = withAttribute(part.geometry, { name: "aWave", format: "float32" }, () => [0], { stream: 1 })
  throws("encodeModel refuses streams", () => encodeModel({ ...model, parts: [{ ...part, geometry: streamed }] }), "streams")
}

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
// COLOR_0: the quantized u8 VEC4 lands in aColor premultiplied and
// linear (no sRGB decode: glTF vertex colors are linear) IN ITS OWN
// FORMAT (unorm8x4, 4 bytes a vertex), and a rigged primitive with u8
// joints and normalized u8 weights keeps those too (uint8x4, unorm8x4,
// the weights renormalized before encoding); both round-trip through
// the container as attribute lists with their packed bytes intact.
{
  let half = 128 / 255
  let painted = parseGltf(
    glb(
      {
        ...document,
        scenes: [{ nodes: [0, 1, 2, 3] }],
        nodes: [{ name: "painted", mesh: 0 }, { name: "paintedskin", mesh: 1, skin: 0 }, { name: "j0" }, { name: "j1" }],
        skins: [{ joints: [2, 3], inverseBindMatrices: 12 }],
        meshes: [
          { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2, COLOR_0: 13 }, indices: 3, material: 0 }] },
          { primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2, JOINTS_0: 14, WEIGHTS_0: 15, COLOR_0: 13 }, indices: 3, material: 0 }] },
        ],
        animations: [],
      },
      binBlocks,
      binLength,
    ),
  )
  for (let part of painted.parts) validateGeometry(part.geometry)
  if (painted.parts.length !== 2) fail(`painted parts: ${painted.parts.length}`)
  let flat = painted.parts[0]!.geometry
  let paintedKey = layoutKey([...VERTEX_LAYOUTS.base, { name: "aColor", format: "unorm8x4" }])
  if (layoutKey(flat.layout) !== paintedKey) fail(`painted layout: ${layoutKey(flat.layout)}`)
  if (!(flat.vertices instanceof Uint8Array)) fail("painted: a packed layout is bytes")
  if (!nearAll(read(flat, "aColor", 0), [half, 0, 0, half])) fail(`painted color: ${read(flat, "aColor", 0)}`)
  if (flat.vertices.byteLength !== vertexCount * layoutStride(flat.layout) || layoutStride(flat.layout) !== 36) fail(`painted stride: ${layoutStride(flat.layout)}`)
  if (!near(read(flat, "aUV", 1)[0]!, floats(cube)[BASE_FLOATS + 6]!)) fail("painted: the base prefix shifted")
  let rig = painted.parts[1]!.geometry
  let want = layoutKey([...VERTEX_LAYOUTS.base, { name: "aJoints", format: "uint8x4" }, { name: "aWeights", format: "unorm8x4" }, { name: "aColor", format: "unorm8x4" }])
  if (layoutKey(rig.layout) !== want) fail(`paintedskin layout: ${layoutKey(rig.layout)}`)
  if (!nearAll(read(rig, "aColor", 0), [half, 0, 0, half])) fail("paintedskin color")
  if (!nearAll(read(rig, "aJoints", 0), [0, 1, 0, 0])) fail(`paintedskin joints: ${read(rig, "aJoints", 0)}`)
  if (!nearAll(read(rig, "aWeights", 0), [0.6, 0.4, 0, 0])) fail(`paintedskin weights not renormalized: ${read(rig, "aWeights", 0)}`)
  let back = decodeModel(encodeModel(painted))
  if (layoutKey(back.parts[0]!.geometry.layout) !== paintedKey) fail("container: packed color layout round trip")
  if (layoutKey(back.parts[1]!.geometry.layout) !== want) fail(`container: packed skin layout round trip: ${layoutKey(back.parts[1]!.geometry.layout)}`)
  if (vertexBytes(back.parts[1]!.geometry.vertices).join() !== vertexBytes(rig.vertices).join()) fail("container: packed bytes changed")
  if (!nearAll(read(back.parts[1]!.geometry, "aWeights", 0), [0.6, 0.4, 0, 0])) fail("container: packed weights")
  throws("COLOR_0 not VEC3/VEC4", () => parseGltf(glb({ ...document, scenes: [{ nodes: [0] }], nodes: [{ mesh: 0 }], meshes: [{ primitives: [{ attributes: { POSITION: 0, COLOR_0: 2 }, indices: 3 }] }], skins: [], animations: [] }, binBlocks, binLength)), "COLOR_0")
}

// --- morph targets ----------------------------------------------------------
// A mesh with two targets ("puff" sparse, "lift" dense with normal
// deltas) and mesh weights, named through extras.targetNames; a second
// node of the same mesh overriding the weights; a flat-shaded mesh whose
// targets must follow the un-index; a weights clip on the first node.
{
  let morphMesh = {
    weights: [0.25, 0.5],
    extras: { targetNames: ["puff", "lift"] },
    primitives: [{ attributes: { POSITION: 0, NORMAL: 1, TEXCOORD_0: 2 }, indices: 3, material: 0, targets: [{ POSITION: 16 }, { POSITION: 17, NORMAL: 18 }] }],
  }
  let flatMorphMesh = { primitives: [{ attributes: { POSITION: 0 }, indices: 3, material: 0, targets: [{ POSITION: 16 }] }] }
  let morphDocument = {
    ...document,
    scenes: [{ nodes: [0, 1, 2] }],
    nodes: [
      { name: "face", mesh: 0 },
      { name: "face2", mesh: 0, translation: [3, 0, 0], weights: [0.75, 0.25] },
      { name: "flatface", mesh: 1, translation: [0, 3, 0] },
    ],
    meshes: [morphMesh, flatMorphMesh],
    skins: [],
    animations: [
      {
        name: "smile",
        channels: [{ sampler: 0, target: { node: 0, path: "weights" } }],
        samplers: [{ input: 4, output: 19, interpolation: "LINEAR" }],
      },
    ],
  }
  let morphed = parseGltf(glb(morphDocument, binBlocks, binLength))
  for (let part of morphed.parts) validateGeometry(part.geometry)
  if (morphed.parts.map((p) => p.name).join() !== "face,face2,flatface") fail(`morph parts: ${morphed.parts.map((p) => p.name).join()}`)
  if (morphed.nodes[0]!.weights?.join() !== "0.25,0.5") fail(`face weights (the mesh's): ${morphed.nodes[0]!.weights?.join()}`)
  if (morphed.nodes[1]!.weights?.join() !== "0.75,0.25") fail(`face2 weights (the node's override): ${morphed.nodes[1]!.weights?.join()}`)
  if (morphed.nodes[2]!.weights?.join() !== "0") fail(`flatface weights (zeros by default): ${morphed.nodes[2]!.weights?.join()}`)
  let face = morphed.parts[0]!.geometry
  let morphs = face.morphs
  if (morphs === undefined) fail("face: no morphs on the geometry")
  else {
    if (morphs.names.join() !== "puff,lift") fail(`morph names: ${morphs.names.join()}`)
    // Every vertex is lifted, vertices 0 and 5 are puffed too: headers
    // count 2 there and 1 elsewhere, entries laid out in vertex order and
    // in target order within a vertex, two texels each.
    let entries = vertexCount + 2
    if (morphs.texels.length !== (vertexCount + entries * MORPH_ENTRY_TEXELS) * MORPH_TEXEL_FLOATS) fail(`morph texels: ${morphs.texels.length} floats`)
    let header = (v: number): number[] => [morphs!.texels[v * MORPH_TEXEL_FLOATS]!, morphs!.texels[v * MORPH_TEXEL_FLOATS + 1]!]
    let entry = (texel: number): number[] => Array.from(morphs!.texels.subarray(texel * MORPH_TEXEL_FLOATS, (texel + MORPH_ENTRY_TEXELS) * MORPH_TEXEL_FLOATS))
    if (header(0).join() !== `${vertexCount},2`) fail(`vertex 0 header: ${header(0).join()}`)
    if (header(1).join() !== `${vertexCount + 2 * MORPH_ENTRY_TEXELS},1`) fail(`vertex 1 header: ${header(1).join()}`)
    if (header(5)[1] !== 2) fail(`vertex 5 header: ${header(5).join()}`)
    if (!nearAll(entry(header(0)[0]!), [0, 0, 1, 0, 0, 0, 0, 0])) fail(`vertex 0 puff entry: ${entry(header(0)[0]!).join()}`)
    if (!nearAll(entry(header(0)[0]! + MORPH_ENTRY_TEXELS), [1, 0, 0, 1, 0, 0, 0.5, 0])) fail(`vertex 0 lift entry: ${entry(header(0)[0]! + MORPH_ENTRY_TEXELS).join()}`)
    if (!nearAll(entry(header(5)[0]!), [0, 0.5, 0, 0, 0, 0, 0, 0])) fail(`vertex 5 puff entry (sparse): ${entry(header(5)[0]!).join()}`)
    if (!nearAll(entry(header(1)[0]!), [1, 0, 0, 1, 0, 0, 0.5, 0])) fail(`vertex 1 lift entry: ${entry(header(1)[0]!).join()}`)
    if (!nearAll(morphs.extent, [0, 0, 0, 0.5, 1, 1])) fail(`morph extent: ${morphs.extent.join()}`)
    // The extent grows the geometry's box and the model's rest bounds.
    if (!nearAll(geometryBounds(face), [-0.5, -0.5, -0.5, 1, 1.5, 1.5])) fail(`morphed geometry bounds: ${geometryBounds(face).join()}`)
  }
  if (!nearAll(morphed.bounds, [-0.5, -0.5, -0.5, 4, 4.5, 1.5])) fail(`morphed model bounds: ${morphed.bounds.join()}`)
  // The un-indexed part: a corner whose source vertex is 0 or 5 carries
  // the puff delta of that vertex, every other corner none.
  let flatFace = morphed.parts[2]!.geometry
  if (flatFace.morphs === undefined) fail("flatface: no morphs")
  else {
    let corners = cube.indices.length
    for (let slot = 0; slot < corners; slot++) {
      let source = cube.indices[slot]!
      let count = flatFace.morphs.texels[slot * MORPH_TEXEL_FLOATS + 1]
      if (count !== (source === 0 || source === 5 ? 1 : 0)) {
        fail(`flatface corner ${slot} (source ${source}): ${count} entries`)
        break
      }
    }
  }
  // The weights clip: one channel of two elements per key, sampled by
  // the JS core like any other.
  let smile = morphed.clips[0]!
  if (morphed.clips.length !== 1 || smile.channels.length !== 1) fail(`smile clip: ${morphed.clips.length} clips, ${smile?.channels.length} channels`)
  let weightsChannel = smile.channels[0]!
  if (weightsChannel.node !== 0 || weightsChannel.path !== "weights" || weightsChannel.values.length !== 6) fail(`weights channel: ${JSON.stringify({ node: weightsChannel.node, path: weightsChannel.path, values: weightsChannel.values.length })}`)
  if (channelElements(weightsChannel) !== 2) fail(`weights channel elements: ${channelElements(weightsChannel)}`)
  expectSample("weights", weightsChannel, 0.5, [0.5, 0])
  expectSample("weights", weightsChannel, 1.5, [0.75, 0.5])
  // The container carries it all through.
  let back = decodeModel(encodeModel(morphed))
  sameModel(morphed, back, "morph round trip")
  if (back.nodes[1]!.weights?.join() !== "0.75,0.25") fail("container: node weights")
  // A weights track sized for the wrong target count, and node weights
  // sized for the wrong target count, both throw.
  throws(
    "weights channel count",
    () => parseGltf(glb({ ...morphDocument, animations: [{ name: "bad", channels: [{ sampler: 0, target: { node: 0, path: "weights" } }], samplers: [{ input: 4, output: 5 }] }] }, binBlocks, binLength)),
    "weights",
  )
  throws("node weights count", () => parseGltf(glb({ ...morphDocument, nodes: [{ name: "face", mesh: 0, weights: [1] }] }, binBlocks, binLength)), "weights")
}

// --- reuse and app data -------------------------------------------------------
// One mesh under four nodes: two plain placements, a mirrored one (its own
// part: the baked winding cannot serve both signs) and a node carrying
// EXT_mesh_gpu_instancing (three instances, TRANSLATION + ROTATION from the
// animation accessors, synthesized as children "swarm[i]"); a second mesh
// once. Extras on the root, a node, the mesh and a material ride through;
// the container carries placements, extras and named blobs.
{
  let reuseDocument = {
    ...document,
    extras: { level: "hangar", spawn: [1, 2, 3] },
    extensionsUsed: ["EXT_mesh_gpu_instancing"],
    extensionsRequired: ["EXT_mesh_gpu_instancing"],
    scenes: [{ nodes: [0, 1, 2, 3, 4] }],
    nodes: [
      { name: "crateA", mesh: 0, translation: [1, 0, 0], extras: { collider: "box" } },
      { name: "crateB", mesh: 0, translation: [4, 0, 0], extras: {} },
      { name: "crateMirror", mesh: 0, scale: [-1, 1, 1] },
      { name: "swarm", mesh: 0, translation: [0, 5, 0], extensions: { EXT_mesh_gpu_instancing: { attributes: { TRANSLATION: 5, ROTATION: 6 } } } },
      { name: "solo", mesh: 1 },
    ],
    meshes: [{ ...document.meshes[0]!, extras: { kind: "crate" } }, document.meshes[1]!],
    materials: [{ ...document.materials[0]!, extras: { surface: "metal" } }, ...document.materials.slice(1)],
    skins: [],
    animations: [],
  }
  let reused = parseGltf(glb(reuseDocument, binBlocks, binLength))
  for (let part of reused.parts) validateGeometry(part.geometry)
  if (reused.nodes.map((n) => n.name).join() !== "crateA,crateB,crateMirror,swarm,swarm[0],swarm[1],swarm[2],solo") fail(`reuse node names: ${reused.nodes.map((n) => n.name).join()}`)
  if (reused.nodes.map((n) => (n.parent === null ? "-" : n.parent)).join() !== "-,-,-,-,3,3,3,-") fail(`reuse node parents: ${reused.nodes.map((n) => n.parent).join()}`)
  if (reused.parts.map((p) => p.name).join() !== "crateA,crateMirror,solo") fail(`reuse parts: ${reused.parts.map((p) => p.name).join()}`)
  let crate = reused.parts[0]!
  if (crate.node !== 0 || crate.placements?.join() !== "1,4,5,6") fail(`crate placements: node ${crate.node}, placements ${crate.placements?.join()}`)
  if (reused.parts[1]!.placements !== undefined || reused.parts[2]!.placements !== undefined) fail("reuse: the mirrored and the single part must carry no placements")
  if (!windingAgrees(crate.geometry)) fail("crate: the shared part's winding follows its first (unmirrored) placement")
  if (windingAgrees(reused.parts[1]!.geometry)) fail("crateMirror: the mirrored copy keeps its own flipped part")
  // The instances: the node's local TRS composed with each instance's,
  // the rotation renormalized, and the extension's rotation about z.
  let swarm1 = reused.nodes[5]!
  if (swarm1.position.join() !== "1,0,0" || !nearAll(swarm1.rotation, [0, 0, Math.SQRT1_2, Math.SQRT1_2]) || swarm1.scale.join() !== "1,1,1") fail(`swarm[1]: ${JSON.stringify(swarm1)}`)
  // Extras: the root's, the node's (an empty object is dropped), the
  // mesh's on the part and the material's.
  if (JSON.stringify(reused.extras) !== JSON.stringify({ level: "hangar", spawn: [1, 2, 3] })) fail(`root extras: ${JSON.stringify(reused.extras)}`)
  if (JSON.stringify(reused.nodes[0]!.extras) !== JSON.stringify({ collider: "box" }) || reused.nodes[1]!.extras !== undefined) fail("node extras")
  if (JSON.stringify(crate.extras) !== JSON.stringify({ kind: "crate" }) || reused.parts[2]!.extras !== undefined) fail("part extras")
  if (JSON.stringify(reused.materials[0]!.extras) !== JSON.stringify({ surface: "metal" }) || reused.materials[1]!.extras !== undefined) fail("material extras")
  // Bounds grow through every placement: crateA at x 0.5..1.5, crateB at
  // 3.5..4.5, the mirror at -0.5..0.5, the swarm at (0..1, 5..6) plus the
  // half cube, solo at the origin.
  if (!nearAll(reused.bounds, [-0.5, -0.5, -0.5, 4.5, 6.5, 0.5])) fail(`reuse bounds: ${reused.bounds.join()}`)
  // The container: placements and extras in the header, blobs as aligned
  // blocks a typed view sits on directly.
  let heights = new Float32Array([0.5, 1.5, 2.5])
  let baked: ModelData = { ...reused, blobs: { grid: new Uint8Array([1, 2, 3, 4, 5]), heights: new Uint8Array(heights.buffer) } }
  let back = decodeModel(encodeModel(baked))
  sameModel(baked, back, "reuse round trip")
  let backHeights = back.blobs?.heights
  if (backHeights === undefined || backHeights.byteOffset % 4 !== 0) fail("blob alignment")
  else if (new Float32Array(backHeights.buffer, backHeights.byteOffset, 3).join() !== heights.join()) fail("blob heights")
  // The refusals: an instancing node with no attribute, and attributes
  // that disagree on the count.
  let swarmNode = reuseDocument.nodes[3]!
  throws("instancing without attributes", () => parseGltf(glb({ ...reuseDocument, nodes: [{ ...swarmNode, extensions: { EXT_mesh_gpu_instancing: {} } }], scenes: [{ nodes: [0] }] }, binBlocks, binLength)), "no TRANSLATION")
  throws(
    "instancing count mismatch",
    () => parseGltf(glb({ ...reuseDocument, nodes: [{ ...swarmNode, extensions: { EXT_mesh_gpu_instancing: { attributes: { TRANSLATION: 5, SCALE: 9 } } } }], scenes: [{ nodes: [0] }] }, binBlocks, binLength)),
    "disagree",
  )
}

// --- part order and shared geometry ------------------------------------------
// Parts come out in WALK order however the meshes interleave (crate,
// slab, mirrored crate - not grouped by mesh), the mirrored twin shares
// the crate's vertex buffer with its own index order (one block in the
// container), and a skinned mesh two nodes place gives two parts over ONE
// geometry object, which the container keeps as one table entry.
{
  let interleaved = {
    ...document,
    scenes: [{ nodes: [0, 1, 2] }],
    nodes: [
      { name: "crate1", mesh: 0 },
      { name: "slab", mesh: 1, translation: [0, -1, 0] },
      { name: "crateM", mesh: 0, scale: [-1, 1, 1], translation: [3, 0, 0] },
    ],
    skins: [],
    animations: [],
  }
  let ordered = parseGltf(glb(interleaved, binBlocks, binLength))
  if (ordered.parts.map((p) => p.name).join() !== "crate1,slab,crateM") fail(`walk-order parts: ${ordered.parts.map((p) => p.name).join()}`)
  let crate1 = ordered.parts[0]!.geometry
  let crateM = ordered.parts[2]!.geometry
  if (crateM.vertices !== crate1.vertices) fail("mirrored twin: the vertex buffer is not shared")
  if (crateM === crate1 || crateM.indices === crate1.indices) fail("mirrored twin: needs its own geometry and index order")
  if (windingAgrees(crateM)) fail("mirrored twin: index order not flipped")
  let orderedBack = decodeModel(encodeModel(ordered))
  sameModel(ordered, orderedBack, "shared vertices round trip")
  if (orderedBack.parts[2]!.geometry.vertices.byteOffset !== orderedBack.parts[0]!.geometry.vertices.byteOffset) fail("container: the shared vertex block was written twice")
  if (orderedBack.parts[2]!.geometry.indices.byteOffset === orderedBack.parts[0]!.geometry.indices.byteOffset) fail("container: the twin's index block must be its own")

  let twoSkinned = {
    ...document,
    scenes: [{ nodes: [0, 1, 2, 3] }],
    nodes: [{ name: "skinA", mesh: 2, skin: 0 }, { name: "skinB", mesh: 2, skin: 0 }, { name: "j0" }, { name: "j1" }],
    skins: [{ joints: [2, 3], inverseBindMatrices: 12 }],
    animations: [],
  }
  let rigged = parseGltf(glb(twoSkinned, binBlocks, binLength))
  if (rigged.parts.length !== 2 || rigged.parts[0]!.placements !== undefined) fail("skinned duplicates: two parts, no placements")
  if (rigged.parts[0]!.geometry !== rigged.parts[1]!.geometry) fail("skinned duplicates: one geometry object expected")
  let riggedBack = decodeModel(encodeModel(rigged))
  sameModel(rigged, riggedBack, "shared geometry round trip")
  if (riggedBack.parts[0]!.geometry !== riggedBack.parts[1]!.geometry) fail("container: shared geometry identity lost")
}

// --- no scene -----------------------------------------------------------------
// A document with neither `scene` nor `scenes` draws everything from its
// TRUE roots (a nested node is not a second root against the identity),
// and a hierarchy that reaches a node twice (a cycle) throws instead of
// recursing until the stack goes.
{
  let sceneless = {
    ...document,
    scene: undefined,
    scenes: undefined,
    nodes: [
      { name: "parent", translation: [1, 0, 0], children: [1] },
      { name: "child", mesh: 0, translation: [0, 2, 0] },
    ],
    skins: [],
    animations: [],
  }
  let parsed = parseGltf(glb(sceneless, binBlocks, binLength))
  if (parsed.parts.length !== 1) fail(`sceneless parts: ${parsed.parts.length}, expected 1`)
  if (parsed.nodes.map((n) => n.name).join() !== "parent,child") fail(`sceneless nodes: ${parsed.nodes.map((n) => n.name).join()}`)
  if (!nearAll(parsed.bounds, [0.5, 1.5, -0.5, 1.5, 2.5, 0.5])) fail(`sceneless bounds (child under parent, once): ${parsed.bounds.join()}`)
  throws(
    "cyclic hierarchy",
    () => parseGltf(glb({ ...document, scenes: [{ nodes: [0] }], nodes: [{ name: "a", mesh: 0, children: [1] }, { name: "b", children: [0] }], skins: [], animations: [] }, binBlocks, binLength)),
    "twice",
  )
}

let noMaterial = { ...document, meshes: [{ primitives: [{ attributes: { POSITION: 0, NORMAL: 1 }, indices: 3 }] }], nodes: [{ mesh: 0 }], scenes: [{ nodes: [0] }], materials: [] }
let plain = parseGltf(glb(noMaterial, binBlocks, binLength))
if (plain.materials.length !== 1 || plain.materials[0]!.name !== "default" || plain.parts[0]!.name !== "node0") fail(`default material: ${JSON.stringify(plain.materials)} / ${plain.parts[0]?.name}`)
if (plain.nodes.length !== 1 || plain.nodes[0]!.name !== "node0" || plain.nodes[0]!.parent !== null) fail(`nameless node table: ${JSON.stringify(plain.nodes)}`)

console.log(failures === 0 ? "gltf-check: all checks passed" : `gltf-check: ${failures} failure(s)`)
if (failures > 0) throw new Error("gltf-check failed")
