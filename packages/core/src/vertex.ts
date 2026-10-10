// The vertex vocabulary as data: one codec per vertex format (the engine's
// table in the same spelling, so it is also the list a declared format
// must be one of), and the arithmetic over an attribute LIST - the stride
// it interleaves to, its identity key, where a name sits, whether a
// Float32Array can hold it - plus the accessors that read and write one
// attribute of a record through its codec, so no caller ever knows an
// offset, a stride or a format. @solidrt/3d's geometry layouts and
// instance streams and @solidrt/2d's material style streams run on this;
// the named presets (a 3d geometry's "base" layout) stay in the packages.
//
// Every format is a multiple of 4 bytes (WebGPU's alignment rule), which
// is what keeps every offset and stride 4-aligned with no padding
// arithmetic anywhere, and a Float32Array view over any record buffer
// always valid.

import type { VertexAttribute, VertexFormat } from "flux:gpu"

/** The shader `in` family a vertex format feeds (WebGPU's rule: the
 * format decides): "float" for the float and normalized formats (`in
 * vec*`), "uint" for uint* (`in uvec*`), "sint" for sint* (`in ivec*`). */
export type FormatKind = "float" | "uint" | "sint"

/** One vertex format's codec: its size, its component count, its kind,
 * and the read and write of component `k` of an attribute at byte `at`
 * of a DataView, in the value the shader sees (a normalized integer
 * decodes to 0..1 / -1..1, an unnormalized one to its exact value). */
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
 * holds directly. */
export function isFloatFormat(format: VertexFormat): boolean {
  return format.startsWith("float32")
}

/** Bytes per record of an attribute list - its interleave stride. */
export function layoutStride(layout: readonly VertexAttribute[]): number {
  let stride = 0
  for (let attr of layout) stride += VERTEX_FORMATS[attr.format].bytes
  return stride
}

/** Values per record of an attribute list: its attributes' components
 * summed - what a record written as a flat value list counts in. */
export function layoutComponents(layout: readonly VertexAttribute[]): number {
  let n = 0
  for (let attr of layout) n += VERTEX_FORMATS[attr.format].components
  return n
}

/** An attribute list's identity as a string (name:format per attribute,
 * in order): two lists with equal keys interleave identically. */
export function layoutKey(layout: readonly VertexAttribute[]): string {
  return layout.map(a => a.name + ":" + a.format).join(",")
}

/** Where an attribute sits in the interleave: its byte offset, format
 * and component count. Null when the list does not carry that name. */
export function layoutSlot(layout: readonly VertexAttribute[], name: string): { offset: number; format: VertexFormat; components: number } | null {
  let offset = 0
  for (let attr of layout) {
    let codec = VERTEX_FORMATS[attr.format]
    if (attr.name === name) return { offset, format: attr.format, components: codec.components }
    offset += codec.bytes
  }
  return null
}

/** Whether every attribute of a list is float32-family: such a buffer
 * is a Float32Array of `layoutStride / 4` floats per record. */
export function isFloatLayout(layout: readonly VertexAttribute[]): boolean {
  return layout.every(a => isFloatFormat(a.format))
}

/** The check an attribute list must pass before it names a buffer: a
 * known format on every attribute and no name twice. Throws naming
 * `where` (the dev validation policy). */
export function checkLayout(layout: readonly VertexAttribute[], where: string): void {
  let seen = new Set<string>()
  for (let attr of layout) {
    if (!(attr.format in VERTEX_FORMATS)) {
      throw new Error(where + ": unknown vertex format '" + String(attr.format) + "' for " + attr.name + " (expected " + Object.keys(VERTEX_FORMATS).join(", ") + ")")
    }
    if (seen.has(attr.name)) throw new Error(where + ": duplicate attribute '" + attr.name + "'")
    seen.add(attr.name)
  }
}

/** The bytes of a record buffer, whatever view holds them. */
export function layoutBytes(records: ArrayBufferView): Uint8Array {
  return new Uint8Array(records.buffer, records.byteOffset, records.byteLength)
}

/** The view a record buffer of `layout` is handed out as: a Float32Array
 * over an all-float layout (indexable as floats), a Uint8Array over a
 * layout with a packed channel. */
export function layoutView(layout: readonly VertexAttribute[], buffer: ArrayBuffer, byteOffset = 0, byteLength = buffer.byteLength - byteOffset): ArrayBufferView {
  return isFloatLayout(layout)
    ? new Float32Array(buffer, byteOffset, byteLength / Float32Array.BYTES_PER_ELEMENT)
    : new Uint8Array(buffer, byteOffset, byteLength)
}

/** A view on one attribute of a record buffer: component `k` of record
 * `i`, read and written as the value the shader sees, through the
 * format's codec. The one way anything touches record data, so no
 * caller knows an offset, a stride or a format. */
export type AttributeAccess = {
  format: VertexFormat
  components: number
  get(i: number, k: number): number
  set(i: number, k: number, v: number): void
}

/** The accessor for `name` over a bare record buffer of `layout`, bound
 * to that buffer; null when the layout lacks the name. */
export function attributeAccess(records: ArrayBufferView, layout: readonly VertexAttribute[], name: string): AttributeAccess | null {
  let slot = layoutSlot(layout, name)
  if (slot === null) return null
  let stride = layoutStride(layout)
  let codec = VERTEX_FORMATS[slot.format]
  let offset = slot.offset
  let dv = new DataView(records.buffer, records.byteOffset, records.byteLength)
  return {
    format: slot.format,
    components: codec.components,
    get: (i, k) => codec.get(dv, i * stride + offset, k),
    set: (i, k, v) => codec.set(dv, i * stride + offset, k, v),
  }
}

/** One accessor per attribute of `layout`, in order, reading through
 * `view()` at every call - the form for a buffer that growth replaces:
 * the accessors stay valid, the DataView behind them moves. */
export function layoutFields(layout: readonly VertexAttribute[], view: () => DataView): AttributeAccess[] {
  let stride = layoutStride(layout)
  let offset = 0
  return layout.map(attr => {
    let codec = VERTEX_FORMATS[attr.format]
    let at = offset
    offset += codec.bytes
    return {
      format: attr.format,
      components: codec.components,
      get: (i, k) => codec.get(view(), i * stride + at, k),
      set: (i, k, v) => codec.set(view(), i * stride + at, k, v),
    }
  })
}

/** Encode `values` (one per component, in layout order, as the shader
 * sees them) into record `i` of a buffer: indexed float stores over an
 * all-float layout given as `floats` (the values ARE the bytes; indexed
 * stores beat TypedArray.set over a plain array under QuickJS, which
 * walks the array-like generically), the accessors' codecs otherwise.
 * Throws naming `site` on a value count that is not the layout's. */
export function writeRecord(layout: readonly VertexAttribute[], fields: readonly AttributeAccess[], floats: Float32Array | null, i: number, values: ArrayLike<number>, site: string): void {
  let n = layoutComponents(layout)
  if (values.length !== n) throw new Error(site + ": a " + layoutKey(layout) + " record is " + n + " values, got " + values.length)
  if (floats !== null) {
    let at = i * n
    for (let k = 0; k < n; k++) floats[at + k] = values[k]!
    return
  }
  let j = 0
  for (let f of fields) for (let k = 0; k < f.components; k++) f.set(i, k, values[j++]!)
}

/** One record of `layout` encoded from `values`: the bytes a blank slot
 * copies (a material's fresh style record). */
export function encodeRecord(layout: readonly VertexAttribute[], values: ArrayLike<number>, site: string): Uint8Array {
  let buffer = new ArrayBuffer(layoutStride(layout))
  let dv = new DataView(buffer)
  let fields = layoutFields(layout, () => dv)
  writeRecord(layout, fields, isFloatLayout(layout) ? new Float32Array(buffer) : null, 0, values, site)
  return new Uint8Array(buffer)
}
