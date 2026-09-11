// Geometry: one interleaved vertex buffer described by a layout - an
// ordered attribute list that starts with the position (float32x3) and
// may carry any named channels after it, each in one of the vertex formats
// (the float32 family, halves, normalized integers, and unsigned and
// signed integers up to 32 bits; see VERTEX_FORMATS). The standard prefix (position, normal, uv: 8 floats)
// is what every generator emits and what the stock materials read; a
// hand-built layout may leave the normal and uv out when nothing reads
// them, and may pack a channel (a color in unorm8x4 is 4 bytes, not 16).
// The layout is open data: `withAttribute` appends any named channel
// (Three's `setAttribute`), and "colored" names the one common case, the
// prefix plus an aColor float32x4 as the per-vertex data channel for
// custom materials (tint, baked AO, any four scalars). Materials read
// attributes by name and adapt to whatever layout their geometry carries
// (one pipeline per layout met), so a geometry may carry more channels
// than a material reads, and a packed channel feeds the same float `in`
// as a float one; an integer channel (uint*/sint*) feeds an integer `in`
// (uvec*/ivec*) and nothing else, WebGPU's rule. Indices are uint16 or uint32 (the generators here emit
// uint16; hand-built geometry past 64k vertices uses a Uint32Array and
// the draw entry follows the array type). Winding is counter-clockwise
// seen from outside in the y-up world, which the standard camera rig
// (perspective() with its baked y flip) presents as the engine's
// displayed-CCW front faces: every generator here culls correctly with
// cull: "back". Normals ride along unused by the unlit materials so the
// layout is ready for lights without a geometry change (inactive
// attributes are skipped but keep the stride).
//
// A geometry may carry more than one vertex buffer: `vertices` under
// `layout` is stream 0 (where aPos lives), and `streams` holds any number
// of extra buffers with layouts of their own, every one holding the same
// vertex count - Unity's vertex streams, Three's one-buffer-per-channel
// when each stream carries one attribute. A stream is what an app rewrites
// per frame without touching the rest (updateVertices in geometry-gpu.ts).
//
// No function here knows an offset or a stride: vertex data is read and
// written through `geometryAttribute`, an accessor bound to one attribute
// that finds the stream carrying it and decodes and encodes through the
// format's codec.
//
// Pure module by design - geometry is data, and every function here is
// array math (the check rig checks/geometry-check.ts runs it headless on
// flux). The GPU buffer step lives in geometry-gpu.ts.

import type { Topology, VertexAttribute, VertexFormat } from "@solidrt/core/gpu"
import { premultipliedColor } from "./color.ts"
import { add, compose, cross, mat4, normalize, normalMatrix, sub, updateRotation, updateScale } from "./math.ts"
import type { Quat, TransformUpdate, Vec2, Vec3 } from "./math.ts"
import type { Capsule } from "./scene.ts"

/** A vertex layout: the named presets, or an explicit attribute list
 * that begins with `aPos` float32x3 (placement is universal) and carries
 * any named channels after it. The standard prefix (aPos float32x3,
 * aNormal float32x3, aUV float32x2) is what every generator emits and
 * what the stock materials read, not a rule a layout must follow: a
 * material reads channels by name and a missing one throws at add(), so
 * a point carrying a position and one packed channel is 16 bytes, not
 * 48. Absent on a Geometry means "standard". */
export type VertexLayout = "standard" | "colored" | "skinned" | VertexAttribute[]

const STANDARD_ATTRIBUTES: VertexAttribute[] = [
  { name: "aPos", format: "float32x3" },
  { name: "aNormal", format: "float32x3" },
  { name: "aUV", format: "float32x2" },
]

/** The attribute lists behind the named layouts. Every layout shares the
 * standard prefix, so one shader vocabulary serves all of them. */
export const VERTEX_LAYOUTS: Record<"standard" | "colored" | "skinned", VertexAttribute[]> = {
  standard: STANDARD_ATTRIBUTES,
  colored: [...STANDARD_ATTRIBUTES, { name: "aColor", format: "float32x4" }],
  // The rigged-model layout: 4 joint indices (unsigned bytes, what an
  // exporter writes for a rig under 256 joints; a wider rig lands in list
  // form with uint16x4) and their weights per vertex, what a skinned
  // vertex stage reads (`in uvec4 aJoints`).
  skinned: [...STANDARD_ATTRIBUTES, { name: "aJoints", format: "uint8x4" }, { name: "aWeights", format: "float32x4" }],
}

/** Floats per vertex in the "standard" layout (the generators' own write
 * format before packing). */
export const STANDARD_FLOATS = 8

/** The shader `in` family a vertex format feeds (WebGPU's rule: the
 * format decides): "float" for the float and normalized formats (`in
 * vec*`), "uint" for uint* (`in uvec*`), "sint" for sint* (`in ivec*`). */
export type FormatKind = "float" | "uint" | "sint"

/** One vertex format's codec: its size, its component count, its kind,
 * and the read and write of component `k` of an attribute at byte `at`
 * of a DataView, in the value the shader sees (a normalized integer
 * decodes to 0..1 / -1..1, an unnormalized one to its exact value).
 * Every format is a multiple of 4 bytes (WebGPU's alignment rule), which
 * is what keeps every offset and stride 4-aligned with no padding
 * arithmetic anywhere. */
export type FormatCodec = {
  bytes: number
  components: number
  kind: FormatKind
  get(dv: DataView, at: number, k: number): number
  set(dv: DataView, at: number, k: number, v: number): void
}

// The integer component kinds behind the packed formats: width, the
// value that maps to 1 (GL ES 3.0's normalization rule, -max mapping to
// -1 and the one value below it clamped there), and the raw accessors.
type IntKind = { bytes: number; max: number; get(dv: DataView, at: number): number; set(dv: DataView, at: number, v: number): void }
const U8: IntKind = { bytes: 1, max: 255, get: (dv, at) => dv.getUint8(at), set: (dv, at, v) => dv.setUint8(at, v) }
const S8: IntKind = { bytes: 1, max: 127, get: (dv, at) => dv.getInt8(at), set: (dv, at, v) => dv.setInt8(at, v) }
const U16: IntKind = { bytes: 2, max: 65535, get: (dv, at) => dv.getUint16(at, true), set: (dv, at, v) => dv.setUint16(at, v, true) }
const S16: IntKind = { bytes: 2, max: 32767, get: (dv, at) => dv.getInt16(at, true), set: (dv, at, v) => dv.setInt16(at, v, true) }
const U32: IntKind = { bytes: 4, max: 4294967295, get: (dv, at) => dv.getUint32(at, true), set: (dv, at, v) => dv.setUint32(at, v, true) }
const S32: IntKind = { bytes: 4, max: 2147483647, get: (dv, at) => dv.getInt32(at, true), set: (dv, at, v) => dv.setInt32(at, v, true) }

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v
}

function floatCodec(components: number, bytes: number, get: (dv: DataView, at: number) => number, set: (dv: DataView, at: number, v: number) => void): FormatCodec {
  return {
    bytes: components * bytes,
    components,
    kind: "float",
    get: (dv, at, k) => get(dv, at + k * bytes),
    set: (dv, at, k, v) => set(dv, at + k * bytes, v),
  }
}

function unormCodec(components: number, int: IntKind): FormatCodec {
  return {
    bytes: components * int.bytes,
    components,
    kind: "float",
    get: (dv, at, k) => int.get(dv, at + k * int.bytes) / int.max,
    set: (dv, at, k, v) => int.set(dv, at + k * int.bytes, Math.round(clamp(v, 0, 1) * int.max)),
  }
}

function snormCodec(components: number, int: IntKind): FormatCodec {
  return {
    bytes: components * int.bytes,
    components,
    kind: "float",
    get: (dv, at, k) => Math.max(int.get(dv, at + k * int.bytes) / int.max, -1),
    set: (dv, at, k, v) => int.set(dv, at + k * int.bytes, Math.round(clamp(v, -1, 1) * int.max)),
  }
}

function uintCodec(components: number, int: IntKind): FormatCodec {
  return {
    bytes: components * int.bytes,
    components,
    kind: "uint",
    get: (dv, at, k) => int.get(dv, at + k * int.bytes),
    set: (dv, at, k, v) => int.set(dv, at + k * int.bytes, Math.round(clamp(v, 0, int.max))),
  }
}

function sintCodec(components: number, int: IntKind): FormatCodec {
  return {
    bytes: components * int.bytes,
    components,
    kind: "sint",
    get: (dv, at, k) => int.get(dv, at + k * int.bytes),
    set: (dv, at, k, v) => int.set(dv, at + k * int.bytes, Math.round(clamp(v, -int.max - 1, int.max))),
  }
}

let getF32 = (dv: DataView, at: number) => dv.getFloat32(at, true)
let setF32 = (dv: DataView, at: number, v: number) => dv.setFloat32(at, v, true)
let getF16 = (dv: DataView, at: number) => dv.getFloat16(at, true)
let setF16 = (dv: DataView, at: number, v: number) => dv.setFloat16(at, v, true)

/** The vertex vocabulary: one codec per format (the engine's table in
 * the same spelling), so it is also the list a declared format must be
 * one of. A format feeds the shader `in` of its kind and component
 * count (`formatFeeds`). */
export const VERTEX_FORMATS: Record<VertexFormat, FormatCodec> = {
  float32: floatCodec(1, Float32Array.BYTES_PER_ELEMENT, getF32, setF32),
  float32x2: floatCodec(2, Float32Array.BYTES_PER_ELEMENT, getF32, setF32),
  float32x3: floatCodec(3, Float32Array.BYTES_PER_ELEMENT, getF32, setF32),
  float32x4: floatCodec(4, Float32Array.BYTES_PER_ELEMENT, getF32, setF32),
  float16x2: floatCodec(2, Float16Array.BYTES_PER_ELEMENT, getF16, setF16),
  float16x4: floatCodec(4, Float16Array.BYTES_PER_ELEMENT, getF16, setF16),
  unorm8x4: unormCodec(4, U8),
  snorm8x4: snormCodec(4, S8),
  unorm16x2: unormCodec(2, U16),
  unorm16x4: unormCodec(4, U16),
  snorm16x2: snormCodec(2, S16),
  snorm16x4: snormCodec(4, S16),
  uint8x4: uintCodec(4, U8),
  uint16x2: uintCodec(2, U16),
  uint16x4: uintCodec(4, U16),
  uint32: uintCodec(1, U32),
  uint32x2: uintCodec(2, U32),
  uint32x3: uintCodec(3, U32),
  uint32x4: uintCodec(4, U32),
  sint8x4: sintCodec(4, S8),
  sint16x2: sintCodec(2, S16),
  sint16x4: sintCodec(4, S16),
  sint32: sintCodec(1, S32),
  sint32x2: sintCodec(2, S32),
  sint32x3: sintCodec(3, S32),
  sint32x4: sintCodec(4, S32),
}

/** Whether a layout declaring `declared` may feed a program input the
 * engine reflects as `reflected` (float32xN for `vec*`, uint32xN for
 * `uvec*`, sint32xN for `ivec*`): same component count and same kind,
 * the engine's own pipeline rule. */
export function formatFeeds(declared: VertexFormat, reflected: VertexFormat): boolean {
  let d = VERTEX_FORMATS[declared]
  let r = VERTEX_FORMATS[reflected]
  return d.components === r.components && d.kind === r.kind
}

/** Whether a format is one of the float32 family: what a Float32Array
 * holds directly, and what the generators write. */
export function isFloatFormat(format: VertexFormat): boolean {
  return format.startsWith("float32")
}

/** The attribute list of a layout (a preset name resolves to its list). */
export function layoutAttributes(layout?: VertexLayout): VertexAttribute[] {
  if (layout === undefined || layout === "standard") return VERTEX_LAYOUTS.standard
  if (layout === "colored") return VERTEX_LAYOUTS.colored
  if (layout === "skinned") return VERTEX_LAYOUTS.skinned
  return layout
}

/** Bytes per vertex of a layout - its interleave stride. */
export function layoutStride(layout?: VertexLayout): number {
  let stride = 0
  for (let attr of layoutAttributes(layout)) stride += VERTEX_FORMATS[attr.format].bytes
  return stride
}

/** A layout's identity as a string (name:format per attribute, in order):
 * two layouts with equal keys interleave identically. */
export function layoutKey(layout?: VertexLayout): string {
  return layoutAttributes(layout)
    .map(a => a.name + ":" + a.format)
    .join(",")
}

/** Where an attribute sits in the interleave: its byte offset, format
 * and component count. Null when the layout does not carry that name. */
export function layoutSlot(layout: VertexLayout | undefined, name: string): { offset: number; format: VertexFormat; components: number } | null {
  let offset = 0
  for (let attr of layoutAttributes(layout)) {
    let codec = VERTEX_FORMATS[attr.format]
    if (attr.name === name) return { offset, format: attr.format, components: codec.components }
    offset += codec.bytes
  }
  return null
}

/** Whether every attribute of a layout is float32-family: such a buffer
 * is a Float32Array of `layoutStride / 4` floats per vertex, the
 * generators' write form. */
export function isFloatLayout(layout?: VertexLayout): boolean {
  return layoutAttributes(layout).every(a => isFloatFormat(a.format))
}

/** The check every layout must pass: aPos float32x3 first and no
 * duplicate names. */
function checkLayout(layout: VertexAttribute[], where: string): void {
  let first = layout[0]
  if (first === undefined || first.name !== "aPos" || first.format !== "float32x3") {
    throw new Error(where + ": a layout must start with aPos float32x3")
  }
  let seen = new Set<string>()
  for (let attr of layout) {
    if (seen.has(attr.name)) throw new Error(where + ": duplicate attribute '" + attr.name + "'")
    seen.add(attr.name)
  }
}

/** The bytes of a vertex buffer, whatever view holds them. */
export function vertexBytes(vertices: ArrayBufferView): Uint8Array {
  return new Uint8Array(vertices.buffer, vertices.byteOffset, vertices.byteLength)
}

/** The view a vertex buffer of `layout` is handed out as: a Float32Array
 * over an all-float layout (the generators' form, indexable as floats),
 * a Uint8Array over a layout with a packed channel. */
export function vertexView(layout: VertexLayout | undefined, buffer: ArrayBuffer, byteOffset = 0, byteLength = buffer.byteLength - byteOffset): ArrayBufferView {
  return isFloatLayout(layout)
    ? new Float32Array(buffer, byteOffset, byteLength / Float32Array.BYTES_PER_ELEMENT)
    : new Uint8Array(buffer, byteOffset, byteLength)
}

/** Vertices in a buffer of `layout`. Throws naming `where` when the byte
 * length is not a whole number of them, or the view is not 4-aligned
 * (every stride is, and the GPU upload and the picking shape count on
 * it). */
export function vertexCount(vertices: ArrayBufferView, layout: VertexLayout | undefined, where: string): number {
  let stride = layoutStride(layout)
  if (vertices.byteOffset % Float32Array.BYTES_PER_ELEMENT !== 0) {
    throw new Error(where + ": vertex data must start on a 4-byte boundary")
  }
  if (vertices.byteLength % stride !== 0) {
    throw new Error(where + ": " + vertices.byteLength + " vertex bytes is not a whole number of " + stride + "-byte (" + layoutKey(layout) + ") vertices")
  }
  return vertices.byteLength / stride
}

/** A view on one attribute of a vertex buffer: component `k` of vertex
 * `i`, read and written as the float the shader sees, through the
 * format's codec. The one way anything touches vertex data, so no
 * caller knows an offset, a stride or a format. */
export type AttributeAccess = {
  format: VertexFormat
  components: number
  get(i: number, k: number): number
  set(i: number, k: number, v: number): void
}

/** The accessor for `name` over a bare vertex buffer of `layout`; null
 * when the layout lacks the name. */
export function attributeAccess(vertices: ArrayBufferView, layout: VertexLayout | undefined, name: string): AttributeAccess | null {
  let slot = layoutSlot(layout, name)
  if (slot === null) return null
  let stride = layoutStride(layout)
  let codec = VERTEX_FORMATS[slot.format]
  let offset = slot.offset
  let dv = new DataView(vertices.buffer, vertices.byteOffset, vertices.byteLength)
  return {
    format: slot.format,
    components: codec.components,
    get: (i, k) => codec.get(dv, i * stride + offset, k),
    set: (i, k, v) => codec.set(dv, i * stride + offset, k, v),
  }
}

/** One extra vertex buffer of a geometry (see `Geometry.streams`): its
 * own attribute list and bytes, holding the geometry's vertex count. */
export type VertexStream = { layout: VertexAttribute[]; vertices: ArrayBufferView }

/** Every vertex buffer of a geometry in pipeline order: stream 0
 * (`vertices` under `layout`), then `streams`. */
export function geometryStreams(geometry: Geometry): { layout: VertexLayout | undefined; vertices: ArrayBufferView }[] {
  let out: { layout: VertexLayout | undefined; vertices: ArrayBufferView }[] = [{ layout: geometry.layout, vertices: geometry.vertices }]
  for (let stream of geometry.streams ?? []) out.push(stream)
  return out
}

/** The attribute lists of a geometry's streams, in order: what a pipeline
 * declares as its vertex-step buffer layouts. */
export function geometryLayouts(geometry: Geometry): VertexAttribute[][] {
  return geometryStreams(geometry).map(s => layoutAttributes(s.layout))
}

/** A geometry's layout identity across every stream (`layoutKey` per
 * stream, `|` between them): two geometries with equal keys bind the same
 * pipeline. */
export function geometryKey(geometry: Geometry): string {
  return geometryStreams(geometry)
    .map(s => layoutKey(s.layout))
    .join("|")
}

/** Where an attribute sits in a geometry: its stream index and its slot
 * there. Null when no stream carries the name. */
export function geometrySlot(geometry: Geometry, name: string): { stream: number; offset: number; format: VertexFormat; components: number } | null {
  let streams = geometryStreams(geometry)
  for (let i = 0; i < streams.length; i++) {
    let slot = layoutSlot(streams[i]!.layout, name)
    if (slot !== null) return { stream: i, ...slot }
  }
  return null
}

/** The geometry's vertex count: stream 0's, which every extra stream must
 * hold too. Throws naming `where` on a stream of another count. */
export function geometryVertexCount(geometry: Geometry, where: string): number {
  let streams = geometryStreams(geometry)
  let count = vertexCount(streams[0]!.vertices, streams[0]!.layout, where)
  for (let i = 1; i < streams.length; i++) {
    let n = vertexCount(streams[i]!.vertices, streams[i]!.layout, where + " stream " + i)
    if (n !== count) throw new Error(where + ": stream " + i + " holds " + n + " vertices, stream 0 holds " + count)
  }
  return count
}

/** `attributeAccess` over whichever of a geometry's streams carries
 * `name`; null when none does. */
export function geometryAttribute(geometry: Geometry, name: string): AttributeAccess | null {
  for (let stream of geometryStreams(geometry)) {
    let access = attributeAccess(stream.vertices, stream.layout, name)
    if (access !== null) return access
  }
  return null
}

// The accessor a reader of the standard channels falls back on when the
// layout lacks the channel: zeros, so a point cloud without a normal or
// uv reads (0, 0, 0) and (0, 0).
const ZERO_ACCESS: AttributeAccess = { format: "float32", components: 0, get: () => 0, set: () => {} }

/** The stream rules: every extra stream declares at least one attribute
 * and no name appears in two streams (each is one shader `in`). */
function checkStreams(geometry: Geometry, where: string): void {
  let seen = new Set<string>()
  let layouts = geometryLayouts(geometry)
  for (let i = 0; i < layouts.length; i++) {
    let attrs = layouts[i]!
    if (i > 0 && attrs.length === 0) throw new Error(where + ": stream " + i + " declares no attributes")
    for (let attr of attrs) {
      if (seen.has(attr.name)) throw new Error(where + ": attribute '" + attr.name + "' appears in two streams")
      seen.add(attr.name)
    }
  }
}

/**
 * The structural check for geometry about to draw: the layout starts
 * with aPos, every stream holds a whole number of the geometry's
 * vertices under its stride, no attribute name repeats across streams,
 * and indices are present. Throws naming the geometry. The scene runs it at
 * add() so hand-built geometry (a bare `layout: "colored"` over a
 * miscounted array) fails there instead of drawing garbage triangles.
 * Deliberately no max-index scan: that is O(indices) per add, and the
 * generators and merge/transform keep indices in range by construction.
 */
export function validateGeometry(geometry: Geometry): void {
  let name = geometry.label ? "geometry '" + geometry.label + "'" : "geometry"
  let layout = geometry.layout
  if (layout !== undefined && typeof layout !== "string") checkLayout(layout, name)
  checkStreams(geometry, name)
  geometryVertexCount(geometry, name)
  let topology = geometryTopology(geometry)
  let count = geometry.indices.length
  // Lines and points may be empty: a feature-edge pass over a smooth
  // closed shape has nothing to draw, and that is a result, not a bug.
  switch (topology) {
    case "triangles":
      if (count === 0) throw new Error(name + ": no indices")
      if (count % 3 !== 0) throw new Error(name + ": " + count + " indices is not a whole number of triangles")
      break
    case "lines":
      if (count % 2 !== 0) throw new Error(name + ": " + count + " indices is not a whole number of lines")
      break
    case "line-strip":
      if (count < 2) throw new Error(name + ": a line strip needs at least 2 indices")
      break
    case "triangle-strip":
      if (count < 3) throw new Error(name + ": a triangle strip needs at least 3 indices")
      break
    case "points":
      break
    default:
      throw new Error(name + ": unknown topology '" + String(topology) + "'")
  }
}

/** The geometry's topology with the default applied: what its indices
 * list, "triangles" unless it says otherwise. */
export function geometryTopology(geometry: Geometry): Topology {
  return geometry.topology ?? "triangles"
}

/** The options every generator shares (each generator's own options type
 * extends this with its dimensions, all optional with defaults): `label`
 * for the GPU buffers, and `layout` to emit vertices in a wider layout
 * directly (the standard channels written, the extra slots zeroed for
 * fillAttribute/fillColors to write in place), so colored or custom
 * channel geometry is built in one pass instead of generate-then-repack. */
export type GeometryOptions = { label?: string; layout?: VertexLayout }

/** The generator path writes the standard channels in their standard
 * order into a Float32Array, so a layout a generator is asked to emit
 * must start with the prefix and stay float32-family throughout; past
 * the prefix the names are open. Packing is a pass over the result
 * (`withAttribute` with a packed format), not a generator option. */
function checkGeneratorLayout(layout: VertexAttribute[], where: string): void {
  checkLayout(layout, where)
  for (let i = 0; i < STANDARD_ATTRIBUTES.length; i++) {
    let want = STANDARD_ATTRIBUTES[i]!
    let got = layout[i]
    if (got === undefined || got.name !== want.name || got.format !== want.format) {
      throw new Error(where + ": a generator layout must start with the standard prefix (aPos float32x3, aNormal float32x3, aUV float32x2)")
    }
  }
  for (let attr of layout) {
    if (!isFloatFormat(attr.format)) {
      throw new Error(where + ": a generator layout is float32-family throughout ('" + attr.name + "' is " + attr.format + "); pack a channel with withAttribute over the result")
    }
  }
}

/** The generator tail: pack standard-layout vertices (number[] of 8 per
 * vertex, or an already-written Float32Array) and indices into a Geometry
 * of the requested layout. A wider layout spreads the standard channels
 * to its stride, leaving the extra slots zero. */
export function packGeometry(
  verts: ArrayLike<number>,
  indices: number[] | Uint16Array | Uint32Array,
  options: GeometryOptions = {},
): Geometry {
  let { label, layout } = options
  if (verts.length % STANDARD_FLOATS !== 0) {
    throw new Error("packGeometry: vertex data is not a whole number of standard-layout vertices")
  }
  let count = verts.length / STANDARD_FLOATS
  let packedIndices = indices instanceof Uint16Array || indices instanceof Uint32Array ? indices : packIndices(indices, count)
  let stride = generatorStride(options, "packGeometry")
  if (stride === STANDARD_FLOATS) {
    let vertices = verts instanceof Float32Array ? verts : new Float32Array(verts)
    return layout === undefined ? { vertices, indices: packedIndices, label } : { vertices, indices: packedIndices, layout, label }
  }
  let vertices = new Float32Array(count * stride)
  for (let i = 0; i < count; i++) {
    let s = i * STANDARD_FLOATS
    let d = i * stride
    for (let k = 0; k < STANDARD_FLOATS; k++) vertices[d + k] = verts[s + k]!
  }
  return { vertices, indices: packedIndices, layout, label }
}

// The stride in FLOATS a generator writing a Float32Array directly must
// use for the requested layout (float32-family by the generator rule, so
// the byte stride is a whole number of floats), and the matching finish
// (no repack: the data is already laid out).
function generatorStride(options: GeometryOptions, where = "generator layout"): number {
  let { layout } = options
  let attrs = layoutAttributes(layout)
  if (layout !== undefined && typeof layout !== "string") checkGeneratorLayout(attrs, where)
  return layoutStride(attrs) / Float32Array.BYTES_PER_ELEMENT
}

function finishGeometry(vertices: Float32Array, indices: number[], options: GeometryOptions): Geometry {
  let { label, layout } = options
  let packed = packIndices(indices, vertexCount(vertices, layout, "generator"))
  return layout === undefined ? { vertices, indices: packed, label } : { vertices, indices: packed, layout, label }
}

/** Uint16 indices when they fit, Uint32Array past 64k vertices - the draw
 * entry follows the array type. The tail of every unbounded generator. */
export function packIndices(indices: number[], vertexCount: number): Uint16Array | Uint32Array {
  return vertexCount > 65535 ? new Uint32Array(indices) : new Uint16Array(indices)
}

export type Geometry = {
  /** The interleaved vertex bytes in the layout's order: a Float32Array
   * for an all-float layout (the generators' form: [pos.xyz, normal.xyz,
   * uv.xy] per vertex, plus color.rgba in "colored"), any 4-aligned view
   * otherwise. Read and write channels through `geometryAttribute`; the
   * view type is never the contract. */
  vertices: ArrayBufferView
  /** The array type picks the draw's index format: Uint32Array past 64k
   * vertices. */
  indices: Uint16Array | Uint32Array
  /** Extra vertex buffers beside `vertices` (which is stream 0, where
   * aPos lives): each its own attribute list over its own bytes, holding
   * the same vertex count. A pipeline reads every stream; a material
   * finds a channel by name whichever stream carries it. The shape for
   * data that changes on its own schedule: the per-frame channel in a
   * stream of its own, rewritten in place with updateVertices while the
   * static channels stay put. `withAttribute` with a `stream` option
   * appends to or opens one. Absent means one buffer. */
  streams?: VertexStream[]
  /** What the indices describe; absent means "triangles". The index
   * buffer and the primitive it lists travel together (Godot's surface
   * primitive, Unity's Mesh.SetIndices topology, Three's Line/Points
   * object types over one BufferGeometry): the material draws whatever
   * topology its geometry carries, one pipeline per (layout, topology)
   * pair. Only a triangle list gets the picking shape - a lines, points
   * or strip geometry picks (and collides) by its bounds box, the tier a
   * sprite or an instanced mesh is at, so hits carry no face/uv. Lines
   * are one pixel wide on GL ES whatever the display density. A lines
   * or points geometry may have no indices at all (it then draws
   * nothing; the other topologies reject that at add()).
   * wireframeGeometry/edgesGeometry build "lines" over a triangle
   * geometry's own vertices. */
  topology?: Topology
  /** Vertex layout; absent means "standard". Must match the material's
   * layout - the scene rejects a mismatched pair at add(). */
  layout?: VertexLayout
  /** Debug name for the lazily-created GPU buffers. */
  label?: string
  _bounds?: Float32Array
}

/**
 * The geometry's LOCAL axis-aligned bounds as [minX, minY, minZ, maxX,
 * maxY, maxZ], computed from the vertices on first use and cached; a
 * stream-0 updateVertices drops the cache. What the transparent sort and
 * a model's bounds read (the picking index keeps its own copy of the
 * positions and derives its box there); a flat geometry legitimately has
 * zero extent on an axis. An empty geometry yields a zero box at the
 * origin.
 */
export function geometryBounds(geometry: Geometry): Float32Array {
  let bounds = geometry._bounds
  if (bounds === undefined) {
    bounds = new Float32Array([Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity])
    let count = geometryVertexCount(geometry, "geometryBounds")
    let pos = geometryAttribute(geometry, "aPos")!
    for (let i = 0; i < count; i++) {
      let x = pos.get(i, 0), y = pos.get(i, 1), z = pos.get(i, 2)
      if (x < bounds[0]!) bounds[0] = x
      if (y < bounds[1]!) bounds[1] = y
      if (z < bounds[2]!) bounds[2] = z
      if (x > bounds[3]!) bounds[3] = x
      if (y > bounds[4]!) bounds[4] = y
      if (z > bounds[5]!) bounds[5] = z
    }
    if (bounds[0]! > bounds[3]!) bounds.fill(0)
    geometry._bounds = bounds
  }
  return bounds
}

/** Per-vertex values for withAttribute/fillAttribute: a flat array of the
 * attribute's size per vertex, or a callback deriving each vertex's value
 * from the standard channels (what a baker wants). A channel the layout
 * lacks (a point cloud without a normal or uv) arrives as zeros. */
export type AttributeFill = ArrayLike<number> | ((index: number, pos: Vec3, normal: Vec3, uv: Vec2) => ArrayLike<number>)
/** AttributeFill for the aColor vec4 channel (4 per vertex). */
export type ColorFill = AttributeFill

/** Options of `withAttribute`: the result's label, and which stream the
 * channel lands in - 0 (the default) interleaves it into the main buffer,
 * 1..n appends it to that extra stream, n + 1 opens a new stream holding
 * it alone (Three's one buffer per channel; the shape for a channel
 * rewritten per frame). */
export type WithAttributeOptions = { label?: string; stream?: number }

/**
 * Append a named channel to a geometry: a new geometry (the source is
 * untouched, its GPU buffers stay independent) whose layout is the
 * source's plus `attr`, every existing channel copied through and the new
 * slots written from `fill`. This is Three's `geometry.setAttribute` for
 * an interleaved buffer - the one generic primitive; `withColors` is its
 * aColor spelling. A material reads the channel by declaring an `in` of
 * the matching name and component count in its vertex stage. A packed
 * format (`unorm8x4` for a color, `snorm16x2`, `float16x4`, ...) is how a
 * channel is compressed: the fill is given in float and encoded on write.
 * `options.stream` picks the stream the channel lands in (see
 * `WithAttributeOptions`); the other streams are shared with the source.
 */
export function withAttribute(geometry: Geometry, attr: VertexAttribute, fill: AttributeFill, options: WithAttributeOptions = {}): Geometry {
  if (geometrySlot(geometry, attr.name) !== null) {
    throw new Error("withAttribute: geometry already carries '" + attr.name + "'")
  }
  if (!(attr.format in VERTEX_FORMATS)) {
    throw new Error("withAttribute: unknown format '" + String(attr.format) + "' for " + attr.name + " (expected " + Object.keys(VERTEX_FORMATS).join(", ") + ")")
  }
  let count = geometryVertexCount(geometry, "withAttribute")
  let streams = geometryStreams(geometry)
  let target = options.stream ?? 0
  if (!Number.isInteger(target) || target < 0 || target > streams.length) {
    throw new Error("withAttribute: stream " + target + " is out of range; the geometry has streams 0.." + (streams.length - 1) + " and " + streams.length + " opens a new one")
  }
  let added: VertexAttribute = { name: attr.name, format: attr.format }
  let layout: VertexAttribute[]
  let vertices: ArrayBufferView
  if (target === streams.length) {
    layout = [added]
    vertices = vertexView(layout, new ArrayBuffer(count * layoutStride(layout)))
  } else {
    let source = streams[target]!
    let srcLayout = layoutAttributes(source.layout)
    let srcStride = layoutStride(srcLayout)
    layout = [...srcLayout, added]
    if (target === 0) checkLayout(layout, "withAttribute")
    let stride = layoutStride(layout)
    let src = vertexBytes(source.vertices)
    let buffer = new ArrayBuffer(count * stride)
    let dst = new Uint8Array(buffer)
    for (let i = 0; i < count; i++) dst.set(src.subarray(i * srcStride, (i + 1) * srcStride), i * stride)
    vertices = vertexView(layout, buffer)
  }
  // The callback reads the standard channels from the SOURCE geometry,
  // whichever stream carries them; the new slot is written on the fresh
  // buffer.
  fillWith(attributeAccess(vertices, layout, attr.name)!, geometry, count, attr.name, fill, 0)
  let extra = (geometry.streams ?? []).slice()
  if (target === 0) {
    // The main buffer was rebuilt; the extra streams ride along.
  } else if (target === streams.length) {
    extra.push({ layout, vertices })
  } else {
    extra[target - 1] = { layout, vertices }
  }
  let out: Geometry = {
    vertices: target === 0 ? vertices : geometry.vertices,
    indices: geometry.indices,
    topology: geometry.topology,
    layout: target === 0 ? layout : geometry.layout,
    label: options.label ?? (geometry.label ? geometry.label + "-" + attr.name : undefined),
  }
  if (extra.length > 0) out.streams = extra
  return out
}

/**
 * Derive a "colored"-layout geometry from a standard one: the same
 * positions, normals, uvs and indices, plus an aColor float32x4 per
 * vertex - the data channel for materials whose vertex stage reads `in
 * vec4 aColor` (a tint, baked ambient occlusion, any four scalars; the
 * name is the standard vocabulary, the contents are yours - raw floats,
 * so a tint the stock materials read under `vertexColors` is
 * premultiplied linear: encode an sRGB pick with premultipliedColor).
 * `withAttribute` with the aColor channel; the "colored" preset name is
 * kept on the result. For a 4-byte color use `withAttribute` with
 * `unorm8x4` directly.
 */
export function withColors(geometry: Geometry, fill: ColorFill, label?: string): Geometry {
  if (geometrySlot(geometry, "aColor") !== null) {
    throw new Error("withColors: geometry already carries an aColor channel")
  }
  let out = withAttribute(geometry, { name: "aColor", format: "float32x4" }, fill, { label: label ?? (geometry.label ? geometry.label + "-colored" : undefined) })
  if (layoutKey(out.layout) === layoutKey("colored")) out.layout = "colored"
  return out
}

/**
 * The in-place primitive under withAttribute: write one channel the
 * geometry already carries, in whichever stream (withAttribute ADDS a
 * channel; this overwrites an existing one). The pos/normal/uv the
 * callback receives are read from the geometry itself, so a builder
 * baking transforms while writing hands the baker world-space vertices.
 * Fills vertices [first, first + count) - count defaults to the rest of
 * the buffer - and `fill` indexes relative to `first`, so a per-part
 * callback works unchanged for both APIs. Returns the vertices of the
 * stream holding the channel (what updateVertices re-uploads).
 */
export function fillAttribute(geometry: Geometry, name: string, fill: AttributeFill, first = 0, count?: number): ArrayBufferView {
  let slot = geometrySlot(geometry, name)
  if (slot === null) throw new Error("fillAttribute: geometry has no '" + name + "' attribute")
  let total = geometryVertexCount(geometry, "fillAttribute")
  fillWith(geometryAttribute(geometry, name)!, geometry, total, name, fill, first, count)
  return geometryStreams(geometry)[slot.stream]!.vertices
}

/** The write loop behind withAttribute and fillAttribute: `slot` is the
 * accessor written, `source` the geometry the callback's standard
 * channels are read from (zeros for a channel it lacks), `total` its
 * vertex count. */
function fillWith(slot: AttributeAccess, source: Geometry, total: number, name: string, fill: AttributeFill, first: number, count?: number): void {
  let n = count ?? total - first
  if (!Number.isInteger(first) || !Number.isInteger(n) || first < 0 || n < 0 || first + n > total) {
    throw new Error("fillAttribute: range [" + first + ", " + (first + n) + ") is outside the buffer's " + total + " vertices")
  }
  let size = slot.components
  let fn = typeof fill === "function" ? fill : null
  let flat = typeof fill === "function" ? null : fill
  if (flat !== null && flat.length !== n * size) {
    throw new Error("fillAttribute: fill has " + flat.length + " values, expected " + size + " per vertex (" + n * size + ")")
  }
  let pos = geometryAttribute(source, "aPos")!
  let nrm = geometryAttribute(source, "aNormal") ?? ZERO_ACCESS
  let uv = geometryAttribute(source, "aUV") ?? ZERO_ACCESS
  for (let i = 0; i < n; i++) {
    let v = first + i
    let value: ArrayLike<number>
    let s: number
    if (fn !== null) {
      value = fn(i, [pos.get(v, 0), pos.get(v, 1), pos.get(v, 2)], [nrm.get(v, 0), nrm.get(v, 1), nrm.get(v, 2)], [uv.get(v, 0), uv.get(v, 1)])
      s = 0
      if (value.length !== size) {
        throw new Error("fillAttribute: fill callback returned " + value.length + " values for '" + name + "', expected " + size)
      }
    } else {
      value = flat!
      s = i * size
    }
    for (let k = 0; k < size; k++) slot.set(v, k, value[s + k]!)
  }
}

/** `fillAttribute` for the aColor channel of a color-carrying geometry
 * (fill is 4 per vertex). */
export function fillColors(geometry: Geometry, fill: ColorFill, first = 0, count?: number): ArrayBufferView {
  return fillAttribute(geometry, "aColor", fill, first, count)
}

/**
 * Bake a placement (the setTransform shape: Euler XYZ radians or a
 * quaternion, not both; number = uniform scale; absent = identity) into a
 * geometry: a new geometry (the source is
 * untouched, its GPU buffers stay independent) whose positions are moved
 * by the transform and whose normals (when the layout carries aNormal)
 * follow through the inverse-transpose, renormalized - correct under
 * non-uniform scale. UVs, colors, indices and layout copy through; extra
 * streams are shared with the source (positions and normals live in
 * stream 0). This is Three's `geometry.applyMatrix4`, the first
 * half of authoring a static scene as data: transform each part into place,
 * mergeGeometries the parts, draw one mesh.
 */
export function transformGeometry(geometry: Geometry, transform: TransformUpdate, label?: string): Geometry {
  let rot: Quat = [0, 0, 0, 1]
  updateRotation(rot, transform, "transformGeometry")
  let scl: Vec3 = [1, 1, 1]
  if (transform.scale !== undefined) updateScale(scl, transform.scale)
  let m = compose(mat4(), transform.position ?? [0, 0, 0], rot, scl)
  let n = normalMatrix(mat4(), m)
  let count = geometryVertexCount(geometry, "transformGeometry")
  let out = vertexView(geometry.layout, vertexBytes(geometry.vertices).slice().buffer)
  let pos = attributeAccess(out, geometry.layout, "aPos")!
  let nrm = attributeAccess(out, geometry.layout, "aNormal")
  for (let i = 0; i < count; i++) {
    let x = pos.get(i, 0), y = pos.get(i, 1), z = pos.get(i, 2)
    pos.set(i, 0, m[0] * x + m[4] * y + m[8] * z + m[12])
    pos.set(i, 1, m[1] * x + m[5] * y + m[9] * z + m[13])
    pos.set(i, 2, m[2] * x + m[6] * y + m[10] * z + m[14])
    if (nrm === null) continue
    let nx = nrm.get(i, 0), ny = nrm.get(i, 1), nz = nrm.get(i, 2)
    let tx = n[0] * nx + n[4] * ny + n[8] * nz
    let ty = n[1] * nx + n[5] * ny + n[9] * nz
    let tz = n[2] * nx + n[6] * ny + n[10] * nz
    let len = Math.hypot(tx, ty, tz) || 1
    nrm.set(i, 0, tx / len)
    nrm.set(i, 1, ty / len)
    nrm.set(i, 2, tz / len)
  }
  let result: Geometry = {
    vertices: out,
    indices: geometry.indices,
    topology: geometry.topology,
    layout: geometry.layout,
    label: label ?? (geometry.label ? geometry.label + "-transformed" : undefined),
  }
  if (geometry.streams !== undefined) result.streams = geometry.streams
  return result
}

/**
 * Concatenate geometries into one: vertices appended in order, indices
 * offset to match, uint32 indices past 64k vertices. Every part must share
 * one layout - a mixed list throws, because the strides differ and a merge
 * that picked one would draw garbage, not a mesh missing a channel - and
 * one list topology (a mixed list throws the same way; strips cannot be
 * concatenated at all, the seam would join them). The
 * second half of authoring a static scene as data (Three's
 * `BufferGeometryUtils.mergeGeometries`): the result is one draw entry and
 * one uModel write however many parts went in, so only what actually moves
 * keeps a node of its own.
 */
export function mergeGeometries(parts: Geometry[], label?: string): Geometry {
  if (parts.length === 0) throw new Error("mergeGeometries: no parts")
  let first = parts[0]!
  let key = geometryKey(first)
  let topology = geometryTopology(first)
  if (topology === "line-strip" || topology === "triangle-strip") throw new Error("mergeGeometries: cannot merge " + topology + " geometry")
  let vertexTotal = 0
  let indexCount = 0
  for (let part of parts) {
    if (geometryKey(part) !== key) {
      throw new Error("mergeGeometries: mixed layouts (" + key + " and " + geometryKey(part) + ")")
    }
    if (geometryTopology(part) !== topology) {
      throw new Error("mergeGeometries: mixed topologies (" + topology + " and " + geometryTopology(part) + ")")
    }
    vertexTotal += geometryVertexCount(part, "mergeGeometries")
    indexCount += part.indices.length
  }
  // Every stream concatenates the same way: part after part, at the
  // stream's own stride.
  let layouts = geometryStreams(first).map(s => s.layout)
  let merged = layouts.map(layout => {
    let stride = layoutStride(layout)
    let bytes = new Uint8Array(vertexTotal * stride)
    return { layout, stride, bytes }
  })
  let indices = vertexTotal > 65535 ? new Uint32Array(indexCount) : new Uint16Array(indexCount)
  let base = 0
  let iOffset = 0
  for (let part of parts) {
    let streams = geometryStreams(part)
    for (let k = 0; k < merged.length; k++) merged[k]!.bytes.set(vertexBytes(streams[k]!.vertices), base * merged[k]!.stride)
    let src = part.indices
    for (let i = 0; i < src.length; i++) indices[iOffset + i] = src[i]! + base
    base += geometryVertexCount(part, "mergeGeometries")
    iOffset += src.length
  }
  let out: Geometry = { vertices: vertexView(first.layout, merged[0]!.bytes.buffer), indices, topology: first.topology, layout: first.layout, label }
  if (merged.length > 1) out.streams = merged.slice(1).map(m => ({ layout: layoutAttributes(m.layout), vertices: vertexView(m.layout, m.bytes.buffer) }))
  return out
}

/** Positions closer than this (model units) are one vertex to the edge
 * builders: a uv seam or a per-face normal split duplicates a position,
 * and an edge is the same edge whichever copy a triangle names. */
const WELD_PRECISION = 1e-4

/** Degrees between two faces' normals from which the edge between them
 * counts as a feature edge in edgesGeometry; under it the faces read as
 * one smooth surface and the edge stays hidden. Three's default. */
const EDGES_THRESHOLD_ANGLE = 1

/** One id per vertex, shared by every vertex at the same position (to
 * WELD_PRECISION) whatever its normal and uv; plus the id count. */
function weldPositions(geometry: Geometry): { ids: Uint32Array; count: number } {
  let count = geometryVertexCount(geometry, "weldPositions")
  let pos = geometryAttribute(geometry, "aPos")!
  let ids = new Uint32Array(count)
  let seen = new Map<string, number>()
  let scale = 1 / WELD_PRECISION
  for (let i = 0; i < count; i++) {
    let key = Math.round(pos.get(i, 0) * scale) + "," + Math.round(pos.get(i, 1) * scale) + "," + Math.round(pos.get(i, 2) * scale)
    let id = seen.get(key)
    if (id === undefined) {
      id = seen.size
      seen.set(key, id)
    }
    ids[i] = id
  }
  return { ids, count: seen.size }
}

/** The edge table of a triangle geometry, welded by position, as a lines
 * index list: every edge once for `cosThreshold` null, else the feature
 * edges - those with one face (a border) or whose two faces' normals
 * dot at or below the threshold. Each edge names the vertex pair of the
 * first triangle that had it. */
function edgeIndices(geometry: Geometry, name: string, cosThreshold: number | null): Uint16Array | Uint32Array {
  if (geometryTopology(geometry) !== "triangles") {
    throw new Error(name + ": needs a triangle geometry, got " + geometryTopology(geometry))
  }
  let pos = geometryAttribute(geometry, "aPos")!
  let weld = weldPositions(geometry)
  let src = geometry.indices
  type Edge = { a: number; b: number; nx: number; ny: number; nz: number; faces: number; sharp: boolean }
  let edges = new Map<number, Edge>()
  for (let t = 0; t + 2 < src.length; t += 3) {
    let i0 = src[t]!, i1 = src[t + 1]!, i2 = src[t + 2]!
    // The face normal, for the feature test.
    let ux = pos.get(i1, 0) - pos.get(i0, 0), uy = pos.get(i1, 1) - pos.get(i0, 1), uz = pos.get(i1, 2) - pos.get(i0, 2)
    let wx = pos.get(i2, 0) - pos.get(i0, 0), wy = pos.get(i2, 1) - pos.get(i0, 1), wz = pos.get(i2, 2) - pos.get(i0, 2)
    let nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx
    let len = Math.hypot(nx, ny, nz) || 1
    nx /= len
    ny /= len
    nz /= len
    for (let k = 0; k < 3; k++) {
      let ia = k === 0 ? i0 : k === 1 ? i1 : i2
      let ib = k === 0 ? i1 : k === 1 ? i2 : i0
      let wa = weld.ids[ia]!, wb = weld.ids[ib]!
      if (wa === wb) continue
      let key = Math.min(wa, wb) * weld.count + Math.max(wa, wb)
      let e = edges.get(key)
      if (e === undefined) {
        edges.set(key, { a: ia, b: ib, nx, ny, nz, faces: 1, sharp: false })
      } else {
        e.faces++
        if (cosThreshold !== null && !e.sharp && e.nx * nx + e.ny * ny + e.nz * nz <= cosThreshold) e.sharp = true
      }
    }
  }
  let out: number[] = []
  for (let e of edges.values()) {
    if (cosThreshold === null || e.faces === 1 || e.sharp) out.push(e.a, e.b)
  }
  return packIndices(out, weld.ids.length)
}

/**
 * Every edge of a triangle geometry as a "lines" geometry over the SAME
 * vertices (Three's WireframeGeometry): each triangle's three sides, once
 * each - edges are matched by position, so a uv seam or a per-face
 * normal split draws one line, not two on top of each other. The
 * source's vertex array and layout are shared by reference, and so is
 * their GPU upload, which makes the wireframe a second index buffer over
 * the same vertices: a "skinned" part's wireframe draws in its pose
 * under `unlit({ skinned: true })` with the mesh's own palette, and any
 * material draws it as lines because the topology rides on the
 * geometry. Lines are one pixel wide.
 */
export function wireframeGeometry(geometry: Geometry, label?: string): Geometry {
  return overVertices(geometry, edgeIndices(geometry, "wireframeGeometry", null), label ?? (geometry.label ? geometry.label + "-wireframe" : undefined))
}

// A "lines" geometry over the same vertices (every stream shared) as its
// source, with its own indices: the edge builders' tail.
function overVertices(geometry: Geometry, indices: Uint16Array | Uint32Array, label: string | undefined): Geometry {
  let out: Geometry = { vertices: geometry.vertices, indices, topology: "lines", layout: geometry.layout, label }
  if (geometry.streams !== undefined) out.streams = geometry.streams
  return out
}

/**
 * The feature edges of a triangle geometry as a "lines" geometry over the
 * same vertices (Three's EdgesGeometry): an edge draws when its two faces
 * meet at `thresholdAngle` degrees or more, or when only one face has it
 * (an open border): the outline that a dense mesh's wireframe, a solid
 * blob, cannot give. A box draws its twelve edges and no face diagonals
 * at any threshold; a generated round shape is faceted, so its facet
 * lines show until the threshold passes its facet angle (360 divided by
 * the radial segments: a default cylinder loses its side seams and keeps
 * its two rims from 16 degrees, a default sphere draws nothing from 16).
 * A smooth closed shape above its facet angle has no edges at all, and
 * the empty lines geometry that comes back attaches and draws nothing.
 * Shares vertices, layout and GPU upload with the source like
 * wireframeGeometry.
 */
export function edgesGeometry(geometry: Geometry, thresholdAngle = EDGES_THRESHOLD_ANGLE, label?: string): Geometry {
  return overVertices(geometry, edgeIndices(geometry, "edgesGeometry", Math.cos((thresholdAngle * Math.PI) / 180)), label ?? (geometry.label ? geometry.label + "-edges" : undefined))
}

// The debug helpers: Three's GridHelper, AxesHelper, Box3Helper and
// PlaneHelper as "lines" builders (Godot and Unity keep the equivalents in
// the editor; with no editor and topology on the geometry they are plain
// data here). A helper is a Geometry like any other: a node places it, a
// material draws it. Normals and uvs fill the standard prefix and mean
// nothing on a line.

// Three's GridHelper defaults, sRGB: the subdivision lines (0x888888) and
// the two through the origin (0x444444).
const GRID_COLOR: [number, number, number] = [0.53, 0.53, 0.53]
const GRID_CENTER_COLOR: [number, number, number] = [0.27, 0.27, 0.27]
// Three's defaults: a 10 by 10 grid of unit cells.
const GRID_SIZE = 10
const GRID_DIVISIONS = 10
// X red, Y green, Z blue: the axis colors of every engine.
const AXIS_COLORS: [number, number, number][] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]

// The layout a colored helper emits: "colored" unless the caller asked for
// a wider one, which must still carry the aColor channel the helper writes.
function helperLayout(options: GeometryOptions, name: string): VertexLayout {
  let layout = options.layout ?? "colored"
  if (layoutSlot(layout, "aColor") === null) throw new Error(name + ": layout has no aColor channel to write")
  return layout
}

// The helper tail: pack, mark as lines, write the colors when there are any.
function packLines(verts: number[], indices: number[], options: GeometryOptions, colors?: number[]): Geometry {
  let geometry = packGeometry(verts, indices, options)
  geometry.topology = "lines"
  if (colors !== undefined) fillColors(geometry, colors)
  return geometry
}

/**
 * A square grid of lines in the XZ plane at y 0, centered on the origin
 * (Three's GridHelper with its defaults): `divisions` cells of
 * `size / divisions` along each side, the subdivision lines in `color`
 * and the two lines through the origin in `centerColor`, which exist only
 * when `divisions` is even. Colors are sRGB 0..1 like a material's,
 * written premultiplied linear into the "colored" layout's aColor:
 * `unlit({ vertexColors: true })` draws them, and a material that reads no
 * aColor (a plain `unlit({ color })`) draws the grid in its own color.
 * (divisions + 1) * 4 vertices, one per line end. Lines are one pixel
 * wide.
 */
export type GridHelperOptions = GeometryOptions & {
  /** Side length of the square, default 10. */
  size?: number
  /** Cells along each side, default 10 (unit cells). */
  divisions?: number
  /** sRGB color of the subdivision lines, default Three's 0x888888. */
  color?: [number, number, number] | [number, number, number, number]
  /** sRGB color of the two lines through the origin (present when
   * `divisions` is even), default Three's 0x444444. */
  centerColor?: [number, number, number] | [number, number, number, number]
}

export function gridHelper(options: GridHelperOptions = {}): Geometry {
  let { size = GRID_SIZE, divisions = GRID_DIVISIONS, color = GRID_COLOR, centerColor = GRID_CENTER_COLOR } = options
  if (!(Number.isInteger(divisions) && divisions >= 1)) throw new Error("gridHelper: divisions must be a whole number of 1 or more, got " + divisions)
  let half = size / 2
  let step = size / divisions
  let line = premultipliedColor(color)
  let center = premultipliedColor(centerColor)
  let verts: number[] = []
  let colors: number[] = []
  for (let i = 0; i <= divisions; i++) {
    let k = -half + i * step
    let t = i / divisions
    // The line across x at z = k, then the line across z at x = k.
    verts.push(-half, 0, k, 0, 1, 0, 0, t, half, 0, k, 0, 1, 0, 1, t)
    verts.push(k, 0, -half, 0, 1, 0, t, 0, k, 0, half, 0, 1, 0, t, 1)
    let c = i * 2 === divisions ? center : line
    for (let end = 0; end < 4; end++) colors.push(c[0]!, c[1]!, c[2]!, c[3]!)
  }
  let indices: number[] = []
  for (let i = 0; i < verts.length / STANDARD_FLOATS; i++) indices.push(i)
  return packLines(verts, indices, { label: options.label, layout: helperLayout(options, "gridHelper") }, colors)
}

/** Three segments from the origin along +x, +y and +z, X red, Y green,
 * Z blue (the axis colors of every engine; Three's AxesHelper), in the
 * "colored" layout like gridHelper: 6 vertices. */
export type AxesHelperOptions = GeometryOptions & {
  /** Length of each axis segment, default 1. */
  size?: number
}

export function axesHelper(options: AxesHelperOptions = {}): Geometry {
  let { size = 1 } = options
  let verts: number[] = []
  let colors: number[] = []
  AXIS_COLORS.forEach((axis, i) => {
    let d = [0, 0, 0]
    d[i] = 1
    verts.push(0, 0, 0, d[0]!, d[1]!, d[2]!, 0, 0)
    verts.push(d[0]! * size, d[1]! * size, d[2]! * size, d[0]!, d[1]!, d[2]!, 1, 0)
    let c = premultipliedColor(axis)
    colors.push(c[0]!, c[1]!, c[2]!, c[3]!, c[0]!, c[1]!, c[2]!, c[3]!)
  })
  return packLines(verts, [0, 1, 2, 3, 4, 5], { label: options.label, layout: helperLayout(options, "axesHelper") }, colors)
}

/**
 * The twelve edges of an axis-aligned box given as `[minX, minY, minZ,
 * maxX, maxY, maxZ]` - what geometryBounds, a model's `bounds` and the
 * spatial queries return - as "lines" geometry (Three's Box3Helper): 8
 * vertices, 24 indices, standard layout, so a plain `unlit({ color })`
 * draws it. The bounds are baked in local space: put the helper under the
 * node whose bounds they are. For a box that moves every frame do what
 * Three does - draw `edgesGeometry(box())`, a unit cube's edges, on a
 * node whose position is the box center and whose scale is its size, and
 * update the transform instead of rebuilding geometry.
 */
export function box3Helper(bounds: ArrayLike<number>, options: GeometryOptions = {}): Geometry {
  if (bounds.length !== 6) throw new Error("box3Helper: bounds must be [minX, minY, minZ, maxX, maxY, maxZ], got " + bounds.length + " values")
  let verts: number[] = []
  let indices: number[] = []
  // Corner i has bit 0 for x, bit 1 for y, bit 2 for z (set = the max
  // side); an edge joins two corners that differ in exactly one bit. The
  // normal is the corner's outward diagonal.
  let n = 1 / Math.sqrt(3)
  for (let i = 0; i < 8; i++) {
    let x = (i & 1) !== 0, y = (i & 2) !== 0, z = (i & 4) !== 0
    verts.push(bounds[x ? 3 : 0]!, bounds[y ? 4 : 1]!, bounds[z ? 5 : 2]!, x ? n : -n, y ? n : -n, z ? n : -n, 0, 0)
    for (let bit = 1; bit < 8; bit <<= 1) if ((i & bit) === 0) indices.push(i, i | bit)
  }
  return packLines(verts, indices, options)
}

/**
 * A collision capsule's outline as "lines" geometry (Unity's capsule
 * collider gizmo, Godot's CapsuleShape3D gizmo; Three has no capsule
 * helper): the `{ a, b, radius }` volume overlap()/sweep()/moveAndSlide
 * take, drawn in the space it was given, so a scene-space volume goes
 * under a node with the identity transform like a box3Helper of query
 * bounds. A ring around each end of the segment, four lines between the
 * rings, and two half circles over each cap in perpendicular planes; a
 * == b is a sphere and draws its three great circles. `segments` is the
 * ring resolution, a multiple of 4 so the lines and arcs land on ring
 * points. Standard layout, normals radial from the segment.
 */
export type CapsuleHelperOptions = GeometryOptions & {
  /** Points per ring, a positive multiple of 4, default 32. */
  segments?: number
}

export function capsuleHelper(volume: Capsule, options: CapsuleHelperOptions = {}): Geometry {
  let { segments = 32 } = options
  if (!Number.isInteger(segments) || segments < 4 || segments % 4 !== 0) throw new Error("capsuleHelper: segments must be a positive multiple of 4")
  let { a, b, radius } = volume
  let span = sub(b, a)
  let single = Math.hypot(span[0], span[1], span[2]) === 0
  let axis = single ? [0, 1, 0] as Vec3 : normalize(span)
  // A frame around the axis: u from the world axis least aligned with it.
  let ax = Math.abs(axis[0]), ay = Math.abs(axis[1]), az = Math.abs(axis[2])
  let pick: Vec3 = ax <= ay && ax <= az ? [1, 0, 0] : ay <= az ? [0, 1, 0] : [0, 0, 1]
  let u = normalize(cross(axis, pick))
  let v = cross(axis, u)
  let verts: number[] = []
  let indices: number[] = []
  // A point on the cap sphere at `center` in direction c*p + s*q.
  let push = (center: Vec3, p: Vec3, q: Vec3, c: number, sn: number): void => {
    let nx = c * p[0] + sn * q[0]
    let ny = c * p[1] + sn * q[1]
    let nz = c * p[2] + sn * q[2]
    verts.push(center[0] + radius * nx, center[1] + radius * ny, center[2] + radius * nz, nx, ny, nz, 0, 0)
  }
  let ring = (center: Vec3): number => {
    let base = verts.length / STANDARD_FLOATS
    for (let i = 0; i < segments; i++) {
      let t = (i / segments) * Math.PI * 2
      push(center, u, v, Math.cos(t), Math.sin(t))
      indices.push(base + i, base + ((i + 1) % segments))
    }
    return base
  }
  // Half circle from +p over `out` (the cap's pole direction) to -p.
  let arc = (center: Vec3, p: Vec3, out: Vec3): void => {
    let base = verts.length / STANDARD_FLOATS
    let cells = segments / 2
    for (let i = 0; i <= cells; i++) {
      let t = (i / cells) * Math.PI
      push(center, p, out, Math.cos(t), Math.sin(t))
      if (i > 0) indices.push(base + i - 1, base + i)
    }
  }
  let ringA = ring(a)
  if (!single) {
    let ringB = ring(b)
    let quarter = segments / 4
    for (let k = 0; k < 4; k++) indices.push(ringA + k * quarter, ringB + k * quarter)
  }
  let down: Vec3 = [-axis[0], -axis[1], -axis[2]]
  arc(a, u, down)
  arc(a, v, down)
  arc(b, u, axis)
  arc(b, v, axis)
  return packLines(verts, indices, options)
}

/**
 * A plane marker as "lines" geometry (Three's PlaneHelper without its
 * translucent fill): a `size` square in the XY plane at the origin facing
 * +z exactly like plane(), its two diagonals, and a unit segment from the
 * center along the normal so the facing reads. Place it with the node
 * transform the way plane() is placed (Three's takes a Plane and does the
 * lookAt itself; there is no Plane type here). 6 vertices, 14 indices,
 * standard layout.
 */
export type PlaneHelperOptions = GeometryOptions & {
  /** Side length of the square, default 1. */
  size?: number
}

export function planeHelper(options: PlaneHelperOptions = {}): Geometry {
  let { size = 1 } = options
  let h = size / 2
  // prettier-ignore
  let verts = [
    -h, -h, 0, 0, 0, 1, 0, 1,
    h, -h, 0, 0, 0, 1, 1, 1,
    h, h, 0, 0, 0, 1, 1, 0,
    -h, h, 0, 0, 0, 1, 0, 0,
    0, 0, 0, 0, 0, 1, 0.5, 0.5,
    0, 0, 1, 0, 0, 1, 0.5, 0.5,
  ]
  // The outline, the two diagonals, the normal tick.
  return packLines(verts, [0, 1, 1, 2, 2, 3, 3, 0, 0, 2, 1, 3, 4, 5], options)
}

// Three's ArrowHelper proportions: the head is a fifth of the length and
// a fifth as wide as it is long.
const ARROW_HEAD_LENGTH = 0.2
const ARROW_HEAD_WIDTH = 0.2

/**
 * An arrow from the origin along +y as "lines" geometry (Three's
 * ArrowHelper, whose local arrow points up +y too, with its solid cone
 * replaced by a pyramid outline): the shaft, four lines from the tip to a
 * diamond base, and the base itself. Aim it with the node transform -
 * `quatFromTo(out, [0, 1, 0], direction)` is the rotation - and put the
 * origin in `position`; Three's constructor takes both, here they are the
 * node's. 7 vertices, 18 indices, standard layout.
 */
export type ArrowHelperOptions = GeometryOptions & {
  /** Tip distance from the origin, default 1. */
  length?: number
  /** Head length from base to tip, default a fifth of `length`. */
  headLength?: number
  /** Head width across the base, default a fifth of `headLength`. */
  headWidth?: number
}

export function arrowHelper(options: ArrowHelperOptions = {}): Geometry {
  let { length = 1 } = options
  let headLength = options.headLength ?? length * ARROW_HEAD_LENGTH
  let headWidth = options.headWidth ?? headLength * ARROW_HEAD_WIDTH
  let base = length - headLength
  let w = headWidth / 2
  // prettier-ignore
  let verts = [
    0, 0, 0, 0, 1, 0, 0, 0,
    0, base, 0, 0, 1, 0, 0, 1,
    0, length, 0, 0, 1, 0, 0, 1,
    w, base, 0, 0, 1, 0, 1, 1,
    0, base, w, 0, 1, 0, 1, 1,
    -w, base, 0, 0, 1, 0, 1, 1,
    0, base, -w, 0, 1, 0, 1, 1,
  ]
  // The shaft, the four tip lines, the base diamond.
  return packLines(verts, [0, 1, 2, 3, 2, 4, 2, 5, 2, 6, 3, 4, 4, 5, 5, 6, 6, 3], options)
}

// Indices for a row-major (cellRows + 1) x (cellCols + 1) vertex grid: two
// CCW triangles per cell, split across the row0col0-row1col1 diagonal -
// the one quad pattern every grid generator here shares (rows run along
// the surface, columns around, same handedness everywhere). A collapsed
// first/last vertex row (sphere pole, cone apex) skips its zero-area
// triangle per cell.
function gridIndices(cellRows: number, cellCols: number, skipFirst = false, skipLast = false): number[] {
  let cols = cellCols + 1
  let out: number[] = []
  for (let r = 0; r < cellRows; r++) {
    for (let c = 0; c < cellCols; c++) {
      let r0 = r * cols + c
      let r1 = r0 + cols
      if (!skipFirst || r > 0) out.push(r0 + 1, r0, r1 + 1)
      if (!skipLast || r < cellRows - 1) out.push(r0, r1, r1 + 1)
    }
  }
  return out
}

/** An axis-aligned box centered on the origin: 24 vertices, 36 indices. */
export type BoxOptions = GeometryOptions & { width?: number; height?: number; depth?: number }

export function box(options: BoxOptions = {}): Geometry {
  let { width = 1, height = 1, depth = 1 } = options
  let x = width / 2
  let y = height / 2
  let z = depth / 2
  let verts: number[] = []
  let indices: number[] = []
  type P = [number, number, number]
  // Corners a (bottom-left) through d (top-left), CCW seen from outside.
  let quad = (a: P, b: P, c: P, d: P, n: P) => {
    let base = verts.length / STANDARD_FLOATS
    let uv = [[0, 1], [1, 1], [1, 0], [0, 0]]
    let corners = [a, b, c, d]
    for (let i = 0; i < 4; i++) {
      let p = corners[i]!
      let t = uv[i]!
      verts.push(p[0], p[1], p[2], n[0], n[1], n[2], t[0]!, t[1]!)
    }
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3)
  }
  quad([-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z], [0, 0, 1]) // front
  quad([x, -y, -z], [-x, -y, -z], [-x, y, -z], [x, y, -z], [0, 0, -1]) // back
  quad([x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z], [1, 0, 0]) // right
  quad([-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z], [-1, 0, 0]) // left
  quad([-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z], [0, 1, 0]) // top
  quad([-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z], [0, -1, 0]) // bottom
  return packGeometry(verts, indices, options)
}

/**
 * A rectangle in the XY plane facing +z, centered on the origin. For a
 * ground plane, rotate it flat: `rotation={[-Math.PI / 2, 0, 0]}`.
 */
export type PlaneOptions = GeometryOptions & { width?: number; height?: number }

export function plane(options: PlaneOptions = {}): Geometry {
  let { width = 1, height = 1 } = options
  let x = width / 2
  let y = height / 2
  // prettier-ignore
  let vertices = [
    -x, -y, 0, 0, 0, 1, 0, 1,
    x, -y, 0, 0, 0, 1, 1, 1,
    x, y, 0, 0, 0, 1, 1, 0,
    -x, y, 0, 0, 0, 1, 0, 0,
  ]
  return packGeometry(vertices, [0, 1, 2, 0, 2, 3], options)
}

/**
 * A (p,q) torus knot swept into a tube, centered on the origin and standing
 * y-up: the knot's disc lies in the XZ plane with the weave running
 * vertically - the orientation a y-up world with XZ floors wants (the
 * standard-vocabulary divergence: Three's equivalent stands on z). The
 * tube's (tubularSegments x radialSegments) grid stores each vertex once;
 * the seam row/column duplicate the first with u/v = 1 (distinct texture
 * coordinates, so genuinely distinct vertices). UVs: u 0..1 along the knot,
 * v 0..1 around the tube.
 */
export type TorusKnotOptions = GeometryOptions & {
  radius?: number
  tube?: number
  tubularSegments?: number
  radialSegments?: number
  p?: number
  q?: number
}

export function torusKnot(options: TorusKnotOptions = {}): Geometry {
  let { radius = 1, tube = 0.4, tubularSegments = 64, radialSegments = 8, p = 2, q = 3 } = options
  // A point on the knot curve at parameter t (0..2*PI*p).
  let point = (t: number): Vec3 => {
    let qp = (q / p) * t
    let r = radius * (2 + Math.cos(qp)) * 0.5
    return [r * Math.cos(t), radius * Math.sin(qp) * 0.5, r * Math.sin(t)]
  }

  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let stride = generatorStride(options)
  let vertices = new Float32Array(rows * cols * stride)
  let at = 0

  for (let i = 0; i < rows; i++) {
    let t = (i / tubularSegments) * Math.PI * 2 * p
    // A stable frame along the curve: tangent from a finite difference, and
    // a normal biased away from the axis (P1 + P2), which is well-defined
    // everywhere on a torus knot and needs no parallel transport.
    let p1 = point(t)
    let p2 = point(t + 0.01)
    let tangent = sub(p2, p1)
    let bitangent = normalize(cross(tangent, add(p2, p1)))
    let normal = normalize(cross(bitangent, tangent))

    for (let j = 0; j < cols; j++) {
      let v = (j / radialSegments) * Math.PI * 2
      let cv = -Math.cos(v) * tube
      let sv = Math.sin(v) * tube
      let x = p1[0] + cv * normal[0] + sv * bitangent[0]
      let y = p1[1] + cv * normal[1] + sv * bitangent[1]
      let z = p1[2] + cv * normal[2] + sv * bitangent[2]
      let n = normalize([x - p1[0], y - p1[1], z - p1[2]])
      vertices[at] = x
      vertices[at + 1] = y
      vertices[at + 2] = z
      vertices[at + 3] = n[0]
      vertices[at + 4] = n[1]
      vertices[at + 5] = n[2]
      vertices[at + 6] = i / tubularSegments
      vertices[at + 7] = j / radialSegments
      at += stride
    }
  }

  return finishGeometry(vertices, gridIndices(tubularSegments, radialSegments), options)
}

/**
 * A capped cylinder on the y axis, centered on the origin. Different top
 * and bottom radii make it a truncated cone (`cone()` is the zero-top
 * case); side normals tilt with the taper. Side UVs: u around the
 * circumference, v 0 at the top to 1 at the bottom; caps get a planar
 * disc map. A zero radius skips that cap and the degenerate side
 * triangles at the apex. heightSegments subdivides the side top to
 * bottom (Three's, default 1) for deformation and per-vertex gradients.
 */
export type CylinderOptions = GeometryOptions & {
  radiusTop?: number
  radiusBottom?: number
  height?: number
  radialSegments?: number
  heightSegments?: number
}

export function cylinder(options: CylinderOptions = {}): Geometry {
  let { radiusTop = 0.5, radiusBottom = 0.5, height = 1, radialSegments = 24, heightSegments = 1 } = options
  if (!Number.isInteger(heightSegments) || heightSegments < 1) throw new Error("cylinder: heightSegments must be a positive integer")
  let h = height / 2
  let cols = radialSegments + 1
  let verts: number[] = []
  // Side normal: perpendicular to the slant line in the (radial, y) plane.
  let slant = Math.hypot(height, radiusBottom - radiusTop) || 1
  let nr = height / slant
  let ny = (radiusBottom - radiusTop) / slant
  for (let iy = 0; iy <= heightSegments; iy++) {
    let v = iy / heightSegments
    let r = radiusTop + (radiusBottom - radiusTop) * v
    let y = h - height * v
    for (let ix = 0; ix < cols; ix++) {
      let u = ix / radialSegments
      let phi = u * Math.PI * 2
      let dx = -Math.cos(phi)
      let dz = Math.sin(phi)
      verts.push(r * dx, y, r * dz, nr * dx, ny, nr * dz, u, v)
    }
  }
  let indices = gridIndices(heightSegments, radialSegments, radiusTop <= 0, radiusBottom <= 0)
  // Caps fan around a center vertex; the planar UV map has no seam, so the
  // ring wraps with modulo instead of duplicating a column.
  let cap = (r: number, y: number, up: number) => {
    let base = verts.length / STANDARD_FLOATS
    verts.push(0, y, 0, 0, up, 0, 0.5, 0.5)
    for (let i = 0; i < radialSegments; i++) {
      let phi = (i / radialSegments) * Math.PI * 2
      let x = -Math.cos(phi) * r
      let z = Math.sin(phi) * r
      verts.push(x, y, z, 0, up, 0, 0.5 + x / (2 * r), 0.5 + (up > 0 ? z : -z) / (2 * r))
    }
    for (let i = 0; i < radialSegments; i++) {
      let j = (i + 1) % radialSegments
      if (up > 0) indices.push(base, base + 1 + i, base + 1 + j)
      else indices.push(base, base + 1 + j, base + 1 + i)
    }
  }
  if (radiusTop > 0) cap(radiusTop, h, 1)
  if (radiusBottom > 0) cap(radiusBottom, -h, -1)
  return packGeometry(verts, indices, options)
}

/** A capped cone on the y axis, centered on the origin: `cylinder()` with
 * a zero top radius (each apex vertex carries its column's side normal, so
 * the surface shades smoothly around). */
export type ConeOptions = GeometryOptions & { radius?: number; height?: number; radialSegments?: number; heightSegments?: number }

export function cone(options: ConeOptions = {}): Geometry {
  let { radius = 0.5, height = 1, radialSegments = 24, ...rest } = options
  return cylinder({ ...rest, radiusTop: 0, radiusBottom: radius, height, radialSegments })
}

/**
 * A capsule on the y axis, centered on the origin: a cylinder of `radius`
 * closed by two hemispheres. `height` is the TOTAL extent pole to pole
 * like cylinder()'s (Godot's CapsuleMesh and Unity's capsule agree;
 * Three's `height` is the middle section only, ours minus 2 * radius),
 * so `height: 2 * radius` is a sphere and anything less throws.
 * capSegments subdivides each hemisphere pole to equator, radialSegments
 * the circumference, heightSegments the band (Three's, default 1).
 * Normals are radial from each cap's center, so the band shades as a
 * cylinder. UVs: u around, v 0 at the top pole to 1 at the bottom by arc
 * length. The matching collision volume is
 * `{ a: [0, -(height / 2 - radius), 0], b: [0, height / 2 - radius, 0], radius }`.
 */
export type CapsuleOptions = GeometryOptions & {
  radius?: number
  height?: number
  capSegments?: number
  radialSegments?: number
  heightSegments?: number
}

export function capsule(options: CapsuleOptions = {}): Geometry {
  let { radius = 0.5, height = 1, capSegments = 8, radialSegments = 24, heightSegments = 1 } = options
  if (!Number.isInteger(heightSegments) || heightSegments < 1) throw new Error("capsule: heightSegments must be a positive integer")
  // Half the middle section: each hemisphere's center sits at +-half.
  let half = height / 2 - radius
  if (half < 0) throw new Error("capsule: height must be at least 2 * radius")
  // Arc length pole to pole: two quarter circles and the band.
  let arc = Math.PI * radius + 2 * half
  let verts: number[] = []
  // A sphere() row at polar angle theta, its center shifted along y.
  let row = (theta: number, offset: number, v: number): void => {
    let sinT = Math.sin(theta)
    let cosT = Math.cos(theta)
    for (let ix = 0; ix <= radialSegments; ix++) {
      let u = ix / radialSegments
      let phi = u * Math.PI * 2
      let nx = -Math.cos(phi) * sinT
      let ny = cosT
      let nz = Math.sin(phi) * sinT
      verts.push(radius * nx, radius * ny + offset, radius * nz, nx, ny, nz, u, v)
    }
  }
  // capSegments + 1 rows per hemisphere; the two equator rows sit at the
  // ends of the band, with heightSegments - 1 equator rows between them.
  for (let i = 0; i <= capSegments; i++) {
    let theta = (i / capSegments) * (Math.PI / 2)
    row(theta, half, (radius * theta) / arc)
  }
  for (let i = 1; i < heightSegments; i++) {
    let t = i / heightSegments
    row(Math.PI / 2, half - 2 * half * t, (radius * (Math.PI / 2) + 2 * half * t) / arc)
  }
  for (let i = 0; i <= capSegments; i++) {
    let theta = Math.PI / 2 + (i / capSegments) * (Math.PI / 2)
    row(theta, -half, (radius * theta + 2 * half) / arc)
  }
  // Both pole rows are collapsed to the pole point, as in sphere().
  let indices = gridIndices(2 * capSegments + heightSegments, radialSegments, true, true)
  return packGeometry(verts, indices, options)
}

/**
 * A torus lying flat, centered on the origin: the ring lies in the XZ
 * plane with the hole on the y axis - the y-up orientation torusKnot also
 * uses (Three's equivalent stands in XY). Option names are Three's:
 * radialSegments subdivides the tube cross-section, tubularSegments the
 * ring. UVs: u 0..1 around the ring, v 0..1 around the tube, seam
 * row/column duplicated like torusKnot.
 */
export type TorusOptions = GeometryOptions & {
  radius?: number
  tube?: number
  radialSegments?: number
  tubularSegments?: number
}

export function torus(options: TorusOptions = {}): Geometry {
  let { radius = 0.5, tube = 0.2, radialSegments = 12, tubularSegments = 32 } = options
  let rows = tubularSegments + 1
  let cols = radialSegments + 1
  let stride = generatorStride(options)
  let vertices = new Float32Array(rows * cols * stride)
  let at = 0
  for (let i = 0; i < rows; i++) {
    let phi = (i / tubularSegments) * Math.PI * 2
    let dx = -Math.cos(phi)
    let dz = Math.sin(phi)
    for (let j = 0; j < cols; j++) {
      let psi = (j / radialSegments) * Math.PI * 2
      let cp = Math.cos(psi)
      let sp = Math.sin(psi)
      let r = radius + tube * cp
      vertices[at] = r * dx
      vertices[at + 1] = tube * sp
      vertices[at + 2] = r * dz
      vertices[at + 3] = cp * dx
      vertices[at + 4] = sp
      vertices[at + 5] = cp * dz
      vertices[at + 6] = i / tubularSegments
      vertices[at + 7] = j / radialSegments
      at += stride
    }
  }
  return finishGeometry(vertices, gridIndices(tubularSegments, radialSegments), options)
}

/**
 * A disc in the XY plane facing +z, centered on the origin (rotate flat
 * like plane()). UVs are the planar map of the disc inscribed in the unit
 * square.
 */
export type CircleOptions = GeometryOptions & { radius?: number; segments?: number }

export function circle(options: CircleOptions = {}): Geometry {
  let { radius = 0.5, segments = 32 } = options
  let verts: number[] = [0, 0, 0, 0, 0, 1, 0.5, 0.5]
  let indices: number[] = []
  for (let i = 0; i < segments; i++) {
    let a = (i / segments) * Math.PI * 2
    let c = Math.cos(a)
    let s = Math.sin(a)
    verts.push(radius * c, radius * s, 0, 0, 0, 1, 0.5 + c * 0.5, 0.5 - s * 0.5)
  }
  for (let i = 0; i < segments; i++) {
    indices.push(0, 1 + i, 1 + ((i + 1) % segments))
  }
  return packGeometry(verts, indices, options)
}

/**
 * A flat annulus in the XY plane facing +z, centered on the origin. UVs
 * are the planar map of the OUTER disc, so a ring textures like the
 * matching circle() with the middle cut out.
 */
export type RingOptions = GeometryOptions & { innerRadius?: number; outerRadius?: number; segments?: number }

export function ring(options: RingOptions = {}): Geometry {
  let { innerRadius = 0.25, outerRadius = 0.5, segments = 32 } = options
  let verts: number[] = []
  let indices: number[] = []
  for (let i = 0; i < segments; i++) {
    let a = (i / segments) * Math.PI * 2
    let c = Math.cos(a)
    let s = Math.sin(a)
    for (let r of [innerRadius, outerRadius]) {
      verts.push(r * c, r * s, 0, 0, 0, 1, 0.5 + (r * c) / (2 * outerRadius), 0.5 - (r * s) / (2 * outerRadius))
    }
  }
  for (let i = 0; i < segments; i++) {
    let j = (i + 1) % segments
    indices.push(i * 2, i * 2 + 1, j * 2 + 1, i * 2, j * 2 + 1, j * 2)
  }
  return packGeometry(verts, indices, options)
}

/** A UV sphere centered on the origin (poles on the y axis). */
export type SphereOptions = GeometryOptions & { radius?: number; widthSegments?: number; heightSegments?: number }

export function sphere(options: SphereOptions = {}): Geometry {
  let { radius = 0.5, widthSegments = 24, heightSegments = 16 } = options
  let verts: number[] = []
  for (let iy = 0; iy <= heightSegments; iy++) {
    let v = iy / heightSegments
    let theta = v * Math.PI
    let sinT = Math.sin(theta)
    let cosT = Math.cos(theta)
    for (let ix = 0; ix <= widthSegments; ix++) {
      let u = ix / widthSegments
      let phi = u * Math.PI * 2
      let nx = -Math.cos(phi) * sinT
      let ny = cosT
      let nz = Math.sin(phi) * sinT
      verts.push(radius * nx, radius * ny, radius * nz, nx, ny, nz, u, v)
    }
  }
  // Both pole rows are collapsed to the pole point.
  let indices = gridIndices(heightSegments, widthSegments, true, true)
  return packGeometry(verts, indices, options)
}

// The polyhedron family, Three's PolyhedronGeometry: a convex solid's
// corner list projected onto its circumsphere. `detail` splits every face
// into (detail + 1)^2 triangles before the projection, so one builder is
// the flat-shaded solid at 0 and a sphere of uniform triangles above it:
// icosahedron({ detail: 3 }) is the icosphere, no pole pinch, which the
// lat/long sphere() cannot express. Non-indexed as in Three, every
// triangle owning its three vertices, which is what lets the normals
// switch with detail: face normals at 0 (a dodecahedron reads as twelve
// flat pentagons), radial above (smooth). UVs are the spherical map with
// Three's per-triangle seam patch, which stretches toward the poles: the
// textured sphere stays sphere(), these serve flat-shaded, low-poly and
// procedurally shaded looks. Three's corner tables port verbatim, their
// winding already CCW from outside. Godot and Unity ship neither the
// family nor an icosphere (their primitives are blockout shapes and
// modeling happens in Blender); nothing here touches the collision
// volumes, which are analytic.

// The seam patch, Three's thresholds: a triangle with a u above
// UV_SEAM_HIGH and one below UV_SEAM_LOW straddles the u = 0/1 seam, so
// its u values below UV_SEAM_WRAP are lifted by a full turn and the
// triangle samples across the seam instead of the whole texture backwards.
const UV_SEAM_HIGH = 0.9
const UV_SEAM_LOW = 0.1
const UV_SEAM_WRAP = 0.2

export type PolyhedronOptions = GeometryOptions & {
  /** Circumsphere radius, default 0.5 like sphere() (Three's is 1). */
  radius?: number
  /** Subdivision: every edge split `detail + 1` ways, a non-negative
   * integer. Default 0, the flat solid. */
  detail?: number
}

/**
 * The generic builder over Three's data form: `vertices` a flat xyz list,
 * `indices` its triangles, CCW seen from outside. F * (detail + 1)^2
 * triangles, three vertices each, indices 0..n-1.
 */
export function polyhedron(vertices: ArrayLike<number>, indices: ArrayLike<number>, options: PolyhedronOptions = {}): Geometry {
  let { radius = 0.5, detail = 0 } = options
  if (!Number.isInteger(detail) || detail < 0) throw new Error("polyhedron: detail must be a non-negative integer")
  if (vertices.length % 3 !== 0) throw new Error("polyhedron: vertices must be xyz triples")
  if (indices.length % 3 !== 0) throw new Error("polyhedron: indices must be triangles")
  let cols = detail + 1
  // Unit directions of the subdivided triangles' corners, three per
  // triangle. A grid point is the integer-weighted mean of its face's
  // corners (weights summing to cols), always summed in corner order: a
  // point on a shared edge has one zero weight, so both faces compute the
  // same two-term sum and it comes out bit-identical, one vertex to the
  // position weld in edgesGeometry/wireframeGeometry.
  let dirs: number[] = []
  let corner = (i: number): Vec3 => [vertices[i * 3]!, vertices[i * 3 + 1]!, vertices[i * 3 + 2]!]
  for (let t = 0; t < indices.length; t += 3) {
    let a = corner(indices[t]!)
    let b = corner(indices[t + 1]!)
    let c = corner(indices[t + 2]!)
    let point = (wa: number, wb: number, wc: number): void => {
      let x = (wa * a[0] + wb * b[0] + wc * c[0]) / cols
      let y = (wa * a[1] + wb * b[1] + wc * c[1]) / cols
      let z = (wa * a[2] + wb * b[2] + wc * c[2]) / cols
      let len = Math.hypot(x, y, z)
      dirs.push(x / len, y / len, z / len)
    }
    // Rows climb from the a-b edge to c, a row runs from a's side to b's;
    // row r holds span + 1 points with weights (span - j, j, r). Per cell
    // an upright triangle, and below the last an inverted one - Three's
    // order, so the two agree vertex for vertex.
    for (let r = 0; r < cols; r++) {
      let span = cols - r
      for (let j = 0; j < span; j++) {
        point(span - j - 1, j + 1, r)
        point(span - j - 1, j, r + 1)
        point(span - j, j, r)
        if (j < span - 1) {
          point(span - j - 1, j + 1, r)
          point(span - j - 2, j + 1, r + 1)
          point(span - j - 1, j, r + 1)
        }
      }
    }
  }
  let count = dirs.length / 3
  let verts = new Float32Array(count * STANDARD_FLOATS)
  for (let i = 0; i < count; i++) {
    let d = i * 3
    let x = dirs[d]!
    let y = dirs[d + 1]!
    let z = dirs[d + 2]!
    let o = i * STANDARD_FLOATS
    verts[o] = x * radius
    verts[o + 1] = y * radius
    verts[o + 2] = z * radius
    verts[o + 3] = x
    verts[o + 4] = y
    verts[o + 5] = z
    // The spherical map: u the azimuth around y (from -x, counter-clockwise
    // seen from above), v the inclination, 0 at the top like sphere().
    verts[o + 6] = Math.atan2(z, -x) / (2 * Math.PI) + 0.5
    verts[o + 7] = Math.atan2(-y, Math.hypot(x, z)) / Math.PI + 0.5
  }
  for (let t = 0; t < count; t += 3) {
    let o0 = t * STANDARD_FLOATS
    let o1 = o0 + STANDARD_FLOATS
    let o2 = o1 + STANDARD_FLOATS
    // Three's UV fixes, keyed on the triangle's own azimuth: a corner on
    // the y axis (atan2(0, 0) says nothing) takes it, and on the -x side
    // of the seam a u of exactly 1 becomes 0 so the triangle stays whole.
    let azimuth = Math.atan2(verts[o0 + 5]! + verts[o1 + 5]! + verts[o2 + 5]!, -(verts[o0 + 3]! + verts[o1 + 3]! + verts[o2 + 3]!))
    let centerU = azimuth / (2 * Math.PI) + 0.5
    for (let o of [o0, o1, o2]) {
      if (azimuth < 0 && verts[o + 6] === 1) verts[o + 6] = 0
      if (verts[o + 3] === 0 && verts[o + 5] === 0) verts[o + 6] = centerU
    }
    let u0 = verts[o0 + 6]!
    let u1 = verts[o1 + 6]!
    let u2 = verts[o2 + 6]!
    if (Math.max(u0, u1, u2) > UV_SEAM_HIGH && Math.min(u0, u1, u2) < UV_SEAM_LOW) {
      for (let o of [o0, o1, o2]) if (verts[o + 6]! < UV_SEAM_WRAP) verts[o + 6] = verts[o + 6]! + 1
    }
    if (detail === 0) {
      let p0: Vec3 = [verts[o0]!, verts[o0 + 1]!, verts[o0 + 2]!]
      let p1: Vec3 = [verts[o1]!, verts[o1 + 1]!, verts[o1 + 2]!]
      let p2: Vec3 = [verts[o2]!, verts[o2 + 1]!, verts[o2 + 2]!]
      let n = normalize(cross(sub(p1, p0), sub(p2, p0)))
      for (let o of [o0, o1, o2]) {
        verts[o + 3] = n[0]
        verts[o + 4] = n[1]
        verts[o + 5] = n[2]
      }
    }
  }
  let order: number[] = []
  for (let i = 0; i < count; i++) order.push(i)
  return packGeometry(verts, order, options)
}

// The golden ratio: the corner coordinate of the icosahedron and the
// dodecahedron.
const PHI = (1 + Math.sqrt(5)) / 2
const PHI_INV = 1 / PHI

// Three's corner tables: the regular solids centered on the origin, CCW
// triangles seen from outside (the dodecahedron's twelve pentagons fanned
// into 36).
const TETRAHEDRON_VERTICES = [1, 1, 1, -1, -1, 1, -1, 1, -1, 1, -1, -1]
const TETRAHEDRON_INDICES = [2, 1, 0, 0, 3, 2, 1, 3, 0, 2, 3, 1]
const OCTAHEDRON_VERTICES = [1, 0, 0, -1, 0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 1, 0, 0, -1]
const OCTAHEDRON_INDICES = [0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2]
// prettier-ignore
const ICOSAHEDRON_VERTICES = [
  -1, PHI, 0, 1, PHI, 0, -1, -PHI, 0, 1, -PHI, 0,
  0, -1, PHI, 0, 1, PHI, 0, -1, -PHI, 0, 1, -PHI,
  PHI, 0, -1, PHI, 0, 1, -PHI, 0, -1, -PHI, 0, 1,
]
// prettier-ignore
const ICOSAHEDRON_INDICES = [
  0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11,
  1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
  3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9,
  4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1,
]
// prettier-ignore
const DODECAHEDRON_VERTICES = [
  -1, -1, -1, -1, -1, 1, -1, 1, -1, -1, 1, 1,
  1, -1, -1, 1, -1, 1, 1, 1, -1, 1, 1, 1,
  0, -PHI_INV, -PHI, 0, -PHI_INV, PHI, 0, PHI_INV, -PHI, 0, PHI_INV, PHI,
  -PHI_INV, -PHI, 0, -PHI_INV, PHI, 0, PHI_INV, -PHI, 0, PHI_INV, PHI, 0,
  -PHI, 0, -PHI_INV, PHI, 0, -PHI_INV, -PHI, 0, PHI_INV, PHI, 0, PHI_INV,
]
// prettier-ignore
const DODECAHEDRON_INDICES = [
  3, 11, 7, 3, 7, 15, 3, 15, 13,
  7, 19, 17, 7, 17, 6, 7, 6, 15,
  17, 4, 8, 17, 8, 10, 17, 10, 6,
  8, 0, 16, 8, 16, 2, 8, 2, 10,
  0, 12, 1, 0, 1, 18, 0, 18, 16,
  6, 10, 2, 6, 2, 13, 6, 13, 15,
  2, 16, 18, 2, 18, 3, 2, 3, 13,
  18, 1, 9, 18, 9, 11, 18, 11, 3,
  4, 14, 12, 4, 12, 0, 4, 0, 8,
  11, 9, 5, 11, 5, 19, 11, 19, 7,
  19, 5, 14, 19, 14, 4, 19, 4, 17,
  1, 12, 14, 1, 14, 5, 1, 5, 9,
]

/** The regular tetrahedron: 4 faces. */
export function tetrahedron(options: PolyhedronOptions = {}): Geometry {
  return polyhedron(TETRAHEDRON_VERTICES, TETRAHEDRON_INDICES, options)
}

/** The regular octahedron: 8 faces, corners on the axes. */
export function octahedron(options: PolyhedronOptions = {}): Geometry {
  return polyhedron(OCTAHEDRON_VERTICES, OCTAHEDRON_INDICES, options)
}

/** The regular icosahedron: 20 faces; with `detail` above 0 the
 * icosphere. */
export function icosahedron(options: PolyhedronOptions = {}): Geometry {
  return polyhedron(ICOSAHEDRON_VERTICES, ICOSAHEDRON_INDICES, options)
}

/** The regular dodecahedron: 12 pentagons, 36 triangles. */
export function dodecahedron(options: PolyhedronOptions = {}): Geometry {
  return polyhedron(DODECAHEDRON_VERTICES, DODECAHEDRON_INDICES, options)
}
