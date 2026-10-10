// A material's style streams on a layer: one core record stream per
// instance buffer the material declares (createRecordStream: the GPU
// buffer, the byte mirror, the dirty range and the range-or-whole
// publish), bound after the layer's own records, each slot starting as
// the material's blank record. The node layer and the records layer
// hold them beside their own stream and publish them in the same flush
// (the tile layer bakes per chunk and keeps plain arrays, tiles.ts);
// the shared verbs (setInstanceStyle, instanceAttribute, records,
// updateRecords in records.ts) write through the accessors here. The
// codecs are core's (vertex.ts), the same ones @solidrt/3d's instance
// streams encode through.
import { createRecordStream, isFloatLayout, layoutFields, layoutStride, layoutView, writeRecord } from "@solidrt/core/gpu"
import type { AttributeAccess, BufferId, RecordStream, VertexAttribute } from "@solidrt/core/gpu"
import { materialBlank } from "./material.ts"
import type { Material } from "./material.ts"

/** One style stream: the layout, the core stream behind it, the blank a
 * fresh slot copies, the accessors (valid across growth), and the
 * mirror's view as the layout wants it (re-derived by growth: read it
 * at use time). */
export type StyleStream = {
  layout: VertexAttribute[]
  stream: RecordStream
  blank: Uint8Array
  fields: AttributeAccess[]
  /** A Float32Array over an all-float layout, bytes otherwise. */
  data: ArrayBufferView
  /** The float view writeRecord stores through on an all-float layout,
   * null otherwise. */
  _floats: Float32Array | null
  _view: DataView
}

/** The material's style streams for a layer of `capacity` slots: empty
 * without instance buffers. */
export function createStyleStreams(material: Material, capacity: number, label: string): StyleStream[] {
  let out: StyleStream[] = []
  material.instanceBuffers?.forEach((buffer, i) => {
    let layout = buffer.attributes
    let stream = createRecordStream(layoutStride(layout), capacity, { label: `${label}-style${i}`, autoFree: false })
    let style: StyleStream = {
      layout,
      stream,
      blank: materialBlank(material, i),
      fields: [],
      data: new Uint8Array(0),
      _floats: null,
      _view: new DataView(new ArrayBuffer(0)),
    }
    viewStyle(style)
    style.fields = layoutFields(layout, () => style._view)
    // Every slot starts blank: a material's instanceStyle is what an
    // unwritten sprite shows, not zeros.
    for (let slot = 0; slot < capacity; slot++) stream.bytes.set(style.blank, slot * stream.stride)
    out.push(style)
  })
  return out
}

// Point the views at the stream's current mirror.
function viewStyle(style: StyleStream): void {
  let buffer = style.stream.bytes.buffer
  style.data = layoutView(style.layout, buffer)
  style._floats = isFloatLayout(style.layout) ? (style.data as Float32Array) : null
  style._view = new DataView(buffer)
}

/** Grow a style stream to `next` slots (the new slots blank) and return
 * the old buffer for the caller to destroy once its draws point at the
 * new one. */
export function growStyle(style: StyleStream, next: number): BufferId {
  let previous = style.stream.capacity
  let old = style.stream.grow(next)
  for (let slot = previous; slot < next; slot++) style.stream.bytes.set(style.blank, slot * style.stream.stride)
  viewStyle(style)
  return old
}

/** Reset slot `slot` to the blank record and mark it. */
export function blankStyle(style: StyleStream, slot: number): void {
  style.stream.bytes.set(style.blank, slot * style.stream.stride)
  style.stream.mark(slot, slot + 1)
}

/** Encode `values` (one per component, in layout order, as the shader
 * sees them) into slot `slot` and mark it. */
export function writeStyle(style: StyleStream, slot: number, values: ArrayLike<number>, site: string): void {
  writeRecord(style.layout, style.fields, style._floats, slot, values, site)
  style.stream.mark(slot, slot + 1)
}

/** Shift slots [from, to) down to `from - 1` (a records layer's destroy)
 * and mark the moved range. */
export function shiftStyle(style: StyleStream, from: number, to: number): void {
  let stride = style.stream.stride
  style.stream.bytes.copyWithin((from - 1) * stride, from * stride, to * stride)
  style.stream.mark(from - 1, to - 1)
}
