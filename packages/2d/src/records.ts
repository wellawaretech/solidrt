// The records layer: the raw escape hatch for motion only JS can compute
// (bespoke flocking, per-frame gameplay logic over every entity at large
// populations). Sprites are 20 JS-owned floats in one canonical
// Float32Array ordered by draw order (insertion order - painter's
// algorithm, later over earlier; `orderBy` swaps that for a core-produced
// key order at publish, records untouched); mutations batch, and the flush
// (ahead of the frame's paint, or at a microtask outside a frame)
// publishes what changed through the core's record stream
// (`createRecordStream` in @solidrt/core/gpu, the publish model every
// JS-stepped population shares): the dirty record range as one buffer
// write, or under `orderBy` the drawn prefix whole through the zero-copy
// write lease (the core gathers it into key order during the copy, and a
// byte range has no stable position under a permutation). A moved sprite
// is 20 float stores plus its share of one memcpy per dirty frame; a
// static layer publishes nothing and therefore costs nothing.
//
// The record vocabulary is @solidrt/3d's record mesh's, one dimension
// down: `records(layer, stream?)` is the mirror (stream 0 the layer's
// own record, 1.. the material's style streams), `updateRecords(layer, {
// stream?, first?, count? })` publishes a range, `setRecordCount(layer,
// n)` dials the drawn prefix, `setInstanceStyle(layer, sprite, values)`
// and `instanceAttribute(layer, name)` write the material's per-sprite
// style record (style.ts). The POPULATION differs by kind and keeps its
// own verbs - a record mesh's records are opaque data (setRecords), a
// record layer's are sprites (addSprite/destroySprite) - so the copy-in
// here is `records(layer).set(src)` followed by updateRecords.
//
// This is NOT the default live layer - that is layer.ts, where sprites are
// spatial arena nodes core producers can reach. Use this when a JS loop
// writes every record every frame anyway: `records(layer)` +
// `updateRecords(layer)` is ~2.4x faster than setSprite at 30k sprites
// (measured 12.9ms raw vs 30.8ms via setSprite, purely call overhead). It
// shrinks as producers land; it is the where-motion-is-computed axis, not
// a "game tier".
//
// Layer space, camera, pointer dispatch and the sprite functions
// (addSprite/setSprite/...) are shared with the node layer; picking here is
// the JS reverse walk (pointInSprite), since records have no nodes.
import { getOwner, onBeforeRender, onCleanup, runWithOwner } from "@solidrt/core"
import { checkScreenSize, createRecordStream, destroyBuffer, screenSizeScale } from "@solidrt/core/gpu"
import type { BufferId } from "@solidrt/core/gpu"
import type { AttributeAccess, ShaderParams } from "@solidrt/core/gpu"
import { checkAtlases, frameIndex } from "./atlas.ts"
import type { Atlas } from "./atlas.ts"
import { fullFrame, isFrame, writeFrame } from "./frames.ts"
import { atlasBindings, RECORD_ATTRIBUTES } from "./glsl.ts"
import { checkParams, checkTint, readFrame } from "./layer.ts"
import type { LayerBase, Sprite, SpriteLayer, SpriteLayerOptions, SpriteOptions, SpriteState } from "./layer.ts"
import { materialSamplers, unlit } from "./material.ts"
import { pointInSprite } from "./pick.ts"
import { blankStyle, createStyleStreams, growStyle, shiftStyle, writeStyle } from "./style.ts"
import { createViews } from "./views.ts"

// Floats per instance record:
// [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA,
//  minScreenPx, maxScreenPx, atlas, outlineR, outlineG, outlineB,
//  outlineWidth]
export const INSTANCE_FLOATS = 20
// Bytes per record: the stride of the GPU buffer and the mirror alike.
const RECORD_BYTES = INSTANCE_FLOATS * Float32Array.BYTES_PER_ELEMENT

// Float offset of cy in a record - what `orderBy: "y"` keys on.
const Y_FIELD_OFFSET = 1
// Float offsets of the screen-size clamp in a record.
const MIN_PX_FIELD_OFFSET = 13
const MAX_PX_FIELD_OFFSET = 14
// Float offset of the atlas sampler index (the frame's texture's position
// in the layer's atlas list).
const ATLAS_FIELD_OFFSET = 15
// Float offset of the outline (rgb plus a width in world pixels) a
// distance-field atlas draws under the sprite; zero on a colour atlas and
// on every sprite the raw writer leaves alone.
const OUTLINE_FIELD_OFFSET = 16
const RESOLVED = Promise.resolve()

export type RecordLayerOptions = Omit<SpriteLayerOptions, "orderBy"> & {
  /**
   * Draw records in KEY order instead of record order, produced by core at
   * each publish (the gpu `instanceOrder` primitive - the flush's lease
   * copy arrives gathered, no per-record JS anywhere): `"y"` keys on the
   * record's cy, so a perspective crowd paints back to front (smaller y =
   * further up the screen = drawn first); or an explicit `{ field,
   * descending? }` float offset into the record for a custom sort key.
   * Record slots stay stable - record i keeps meaning sprite i, and
   * destroySprite still shifts - only the draw order changes. Ties keep
   * record order, so an unset key draws exactly as before. Known
   * limitation: pick() resolves overlapping sprites by record order, not
   * visual order, when a key is set.
   */
  orderBy?: "y" | { field: number; descending?: boolean }
}

export type RecordLayer = LayerBase & {
  /** The count dial (setRecordCount): how many leading sprites draw and
   * pick; Infinity (the identity of the min against the live count)
   * until a dial is set. */
  _dial: number
  _order: SpriteState[]
}

/** Options of `updateRecords`: the stream (default 0, the layer's own
 * record; 1.. the material's style streams in its instanceBuffers
 * order) and the record range (default the whole mirror, capacity
 * wide). */
export type UpdateRecordsOptions = { stream?: number; first?: number; count?: number }

/**
 * The layer's record mirror - the raw power path. Stream 0 (the default)
 * is the layer's own record; stream 1.. are the material's style
 * streams in its `instanceBuffers` order, each a Float32Array over an
 * all-float layout and bytes otherwise (write those through
 * instanceAttribute's codecs, or a typed view of your own over the
 * format), slot-indexed like the layer's own, blank (the material's
 * instanceStyle) until written. On a records layer the
 * layout per sprite is INSTANCE_FLOATS floats: [cx, cy, w, h, u0, v0, u1,
 * v1, rot, tintR, tintG, tintB, tintA, minScreenPx, maxScreenPx, atlas,
 * outlineR, outlineG, outlineB, outlineWidth], record i at i *
 * INSTANCE_FLOATS; `atlas` is the frame's texture as its index in the
 * layer's `atlases` list, and the outline is read only by a
 * distance-field atlas (see `Atlas.sdf`): rgb in 0..1 and a width in
 * world pixels, zero for none. Record order is draw order - unless the
 * layer was created with `orderBy`, which draws in key order while record
 * i keeps meaning sprite i. Do not cache indices across destroySprite -
 * records shift.
 *
 * On a node layer (createSpriteLayer) the records are its STYLE records,
 * SPRITE_FLOATS floats per sprite slot: [u0, v0, u1, v1, tintR, tintG,
 * tintB, tintA, renderOrder, minScreenPx, maxScreenPx, atlas, outlineR,
 * outlineG, outlineB, outlineWidth], slot i at i * SPRITE_FLOATS (a
 * sprite's slot is fixed for its life; a freed slot recycles) - the pose
 * lives in the core and is written through setSprite. The 3d
 * `records(instancedMesh)`, one dimension down: a bulk restyle (a palette
 * cycle over thousands of sprites) is one loop over this and one
 * updateRecords.
 *
 * Write fields directly for large per-frame populations, then
 * updateRecords the range. Read it AT USE TIME: an add past capacity
 * replaces the array, and a hoisted reference becomes a dead copy whose
 * writes publish nothing. @solidrt/3d's records(mesh).
 */
export function records(layer: RecordLayer | SpriteLayer): Float32Array
export function records(layer: RecordLayer | SpriteLayer, stream: number): ArrayBufferView
export function records(layer: RecordLayer | SpriteLayer, stream = 0): ArrayBufferView {
  if (stream === 0) return layer._records
  return styleStream("records", layer, stream).data
}

// The material style stream at index `stream` (1-based after the layer's
// own record), or a throw naming the range.
function styleStream(verb: string, layer: RecordLayer | SpriteLayer, stream: number) {
  let s = layer._styles[stream - 1]
  if (!Number.isInteger(stream) || stream < 1 || s === undefined) {
    throw new Error(`${verb}: stream ${stream} is out of range; the layer has record streams 0..${layer._styles.length} (0 its own record, then the material's instance buffers)`)
  }
  return s
}

/**
 * Write a sprite's style record - the material's per-sprite data, its
 * FIRST instance buffer (any format; `values` one per component, in
 * attribute order, as the shader sees them: a unorm8x4 tint as 0..1) -
 * marking the slot for the next flush; a frame-rate path like
 * setSprite, any number of sprites restyled between two frames
 * costing one coalesced buffer write. Throws on a material without an
 * instance buffer or a record of the wrong length; a no-op on a
 * destroyed sprite. instanceAttribute + updateRecords is the same
 * write by attribute name, and the only way into a second stream.
 * @solidrt/3d's setInstanceStyle(instance, values).
 */
export function setInstanceStyle(layer: RecordLayer | SpriteLayer, sprite: Sprite, values: ArrayLike<number>): void {
  if (sprite.layer !== layer) {
    if (sprite.layer === null) return
    throw new Error("setInstanceStyle: the sprite belongs to another layer")
  }
  let style = layer._styles[0]
  if (style === undefined) throw new Error("setInstanceStyle: the layer's material declares no instance buffer (no style record)")
  writeStyle(style, sprite._slot, values, "setInstanceStyle")
  layer._schedule()
}

/**
 * The accessor for the material's per-sprite attribute `name` over the
 * layer's style streams: slot in (a sprite's `_slot`), component values
 * as the shader sees them (a unorm8x4 reads and writes as 0..1),
 * encoded through the format's codec. Write through it, then
 * updateRecords the range of that stream; the accessor stays valid
 * across growth. Null when no stream carries the name (the layer's own
 * record fields are records(layer)'s, not reachable here).
 * @solidrt/3d's instanceAttribute(mesh, name).
 */
export function instanceAttribute(layer: RecordLayer | SpriteLayer, name: string): AttributeAccess | null {
  for (let s of layer._styles) {
    let i = s.layout.findIndex(a => a.name === name)
    if (i >= 0) return s.fields[i]!
  }
  return null
}

/**
 * Publish records [first, first + count) of a mirror (`stream` 0 the
 * layer's own record, 1.. the material's style streams) at the layer's
 * next flush - the partial rewrite a population stepped in JS wants (ten
 * moved records of ten thousand cost ten); the whole mirror by default.
 * Write the mirror first, through records(), then call this. The range
 * is against the mirror's capacity, not the live sprites: a record past
 * the last live sprite (on a node layer, past the highest slot in use)
 * is accepted and never drawn. Under `orderBy` any range republishes the
 * drawn prefix whole - the core gathers it into key order, and a byte
 * range has no stable position under a permutation. @solidrt/3d's
 * updateRecords(mesh).
 */
export function updateRecords(layer: RecordLayer | SpriteLayer, options: UpdateRecordsOptions = {}): void {
  let index = options.stream ?? 0
  let stream = index === 0 ? layer._stream : styleStream("updateRecords", layer, index).stream
  let capacity = stream.capacity
  let first = options.first ?? 0
  let count = options.count ?? capacity - first
  if (!Number.isInteger(first) || !Number.isInteger(count) || first < 0 || count < 0 || first + count > capacity) {
    throw new Error("updateRecords: range [" + first + ", " + (first + count) + ") is outside the layer's " + capacity + " records")
  }
  if (count === 0) return
  stream.mark(first, first + count)
  layer._schedule()
}

/**
 * Dial how many leading sprites draw and pick: the first `count` of the
 * live sprites (a count past them is all of them), in force as sprites
 * come and go - a sprite added past the dial stays undrawn, and unpicked,
 * until the dial moves. The write-ahead idiom: add a pool at mount, write
 * the live prefix each frame, then dial it. An undialed layer draws every
 * live sprite; `layer.count` stays the live total either way.
 * @solidrt/3d's setRecordCount(mesh).
 */
export function setRecordCount(layer: RecordLayer, count: number): void {
  if (!(Number.isInteger(count) && count >= 0)) throw new Error("setRecordCount: count must be a non-negative integer, got " + count)
  if (count === layer._dial) return
  layer._dial = count
  layer._schedule()
}

// Mark records [lo, hi) of the layer's stream and schedule the flush that
// publishes them.
function markRecords(layer: RecordLayer | SpriteLayer, lo: number, hi: number): void {
  layer._stream.mark(lo, hi)
  layer._schedule()
}

/**
 * Create a records layer over its atlases (see createSpriteLayer: the
 * list every frame must come from, bound as one draw; not owned). Like
 * createSpriteLayer it renders nothing by itself: `createView` is where
 * it shows. Disposed automatically with the owning reactive scope (opt
 * out with `{ autoFree: false }`).
 */
export function createRecordLayer(atlases: Atlas[], opts?: RecordLayerOptions): RecordLayer {
  let atlasIndex = checkAtlases("createRecordLayer", atlases)
  let capacity = opts?.capacity ?? 1024
  if (!(capacity > 0 && Number.isInteger(capacity))) {
    throw new Error(`createRecordLayer: capacity must be a positive integer, got ${capacity}`)
  }
  let label = opts?.label ?? "sprites"
  let stream = createRecordStream(RECORD_BYTES, capacity, { label: `${label}-records`, autoFree: false })
  let tint = opts?.tint ?? [1, 1, 1, 1]
  checkTint("createRecordLayer", tint)
  let material = opts?.material ?? unlit()
  checkAtlases("createRecordLayer", atlases, materialSamplers(material))
  let params: ShaderParams = { ...material.params, ...opts?.params }
  // The material's style streams, after the layer's own record: the
  // same publish, the same growth, the same shift on destroy.
  let styles = createStyleStreams(material, capacity, label)
  let orderBy = opts?.orderBy
  let instanceOrder =
    orderBy === undefined
      ? undefined
      : orderBy === "y"
        ? { field: Y_FIELD_OFFSET }
        : { field: orderBy.field, descending: orderBy.descending }
  let ordered = instanceOrder !== undefined
  let gpu = material.pipeline(atlases, [RECORD_ATTRIBUTES], label)
  // The instance buffers in pipeline order after the quad.
  let buffers = () => [stream.buffer, ...styles.map(s => s.stream.buffer)]

  let disposed = false
  let scheduled = false
  // The drawn count the views were last told.
  let published = 0
  // The publish pass of every frame runs the pending flush ahead of the
  // paint, so a write made anywhere in the frame's JS is in that frame's
  // picture; the microtask stays for writes made outside a frame.
  // Registered outside any owner: dispose unhooks it (an autoFree: false
  // layer outlives the owner it was created in).
  let unhook = runWithOwner(null, () =>
    onBeforeRender(
      () => {
        if (scheduled) flush()
      },
      { publish: true },
    ),
  )

  // How many leading sprites draw: the dial against the live count.
  let drawn = () => Math.min(layer._dial, layer._order.length)

  // The buffers growth replaced since the last flush (addSprite grows the
  // stream at once; the views still draw from the first of these until the
  // flush swaps the replacement in). The entries hold a swapped-out buffer
  // alive until the swap lands, so the destroys are safe to issue right
  // after.
  let grownFrom: BufferId[] = []
  let flush = () => {
    scheduled = false
    if (disposed) return
    let live = layer._order.length
    let count = drawn()
    if (grownFrom.length > 0) {
      // The replacements are filled whole before the swap (the entry must
      // never point at an unwritten buffer): the live prefix, or under
      // orderBy the drawn one, which is all the gather may see.
      stream.publish(true, ordered ? count : live)
      for (let s of styles) s.stream.publish(true, ordered ? count : live)
      views.setBuffers(buffers())
      views.setCount(count)
      if (ordered) {
        // That publish landed BEFORE the swap, so the order had not yet
        // followed to the grown buffers and it went out ungathered. One
        // more publish, now under the swapped-in order, restores key
        // order - growth frames only.
        stream.publish(true, count)
        for (let s of styles) s.stream.publish(true, count)
      }
      for (let old of grownFrom) destroyBuffer(old)
      grownFrom.length = 0
      published = count
      return
    }
    if (ordered) {
      // The gathered set IS the drawn prefix: a dirty record or a moved
      // dial republishes it whole, every stream alike (the core gathers
      // them under one permutation).
      if (stream.dirty || count !== published) {
        stream.publish(true, count)
        for (let s of styles) s.stream.publish(true, count)
      }
    } else {
      // The dirty range, clipped to the live sprites (a record past them
      // is never drawn): one partial write at its byte offset, per
      // dirty stream.
      stream.publish(false, live)
      for (let s of styles) s.stream.publish(false, live)
    }
    if (count !== published) {
      views.setCount(count)
      published = count
    }
  }

  // The checks a write runs before ANY field lands - the fields a record
  // has no room for, and the frame's atlas index (or a throw for a frame
  // from an undeclared sheet) - so a rejected write leaves the record, and
  // on addSprite the layer, as it was.
  let checkWrite = (verb: string, opts: SpriteOptions): number => {
    if (opts.renderOrder !== undefined) {
      throw new Error(`${verb}: record layers have no renderOrder field; order by a record field with orderBy { field }`)
    }
    if (opts.visible !== undefined) {
      throw new Error(`${verb}: record sprites have no visibility; hide by zeroing w or h`)
    }
    if (opts.frame === undefined) return -1
    if (!isFrame(opts.frame)) throw new Error(`${verb}: not a frame, got ${JSON.stringify(opts.frame)}`)
    return frameIndex(verb, atlasIndex, opts.frame)
  }
  // Record layout: [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB,
  // tintA, minScreenPx, maxScreenPx, atlas, outline rgb + width]
  let writeRecord = (sprite: SpriteState, opts: SpriteOptions, atlas: number) => {
    let at = sprite._slot * INSTANCE_FLOATS
    let r = layer._records
    if (opts.x !== undefined) r[at] = opts.x
    if (opts.y !== undefined) r[at + 1] = opts.y
    if (opts.w !== undefined) r[at + 2] = opts.w
    if (opts.h !== undefined) r[at + 3] = opts.h
    let flipX = opts.flipX !== undefined && opts.flipX !== sprite._flipX
    let flipY = opts.flipY !== undefined && opts.flipY !== sprite._flipY
    if (flipX) sprite._flipX = !sprite._flipX
    if (flipY) sprite._flipY = !sprite._flipY
    if (opts.frame !== undefined) {
      let f = opts.frame
      writeFrame(r, at + 4, f.u0, f.v0, f.u1, f.v1, sprite._flipX, sprite._flipY)
      r[at + ATLAS_FIELD_OFFSET] = atlas
    } else if (flipX || flipY) {
      // No new frame: toggle the changed axes on the stored UVs.
      writeFrame(r, at + 4, r[at + 4]!, r[at + 5]!, r[at + 6]!, r[at + 7]!, flipX, flipY)
    }
    if (opts.rotation !== undefined) r[at + 8] = opts.rotation
    if (opts.tint !== undefined) {
      r[at + 9] = opts.tint[0]
      r[at + 10] = opts.tint[1]
      r[at + 11] = opts.tint[2]
      r[at + 12] = opts.tint[3]
    }
    if (opts.minScreenPx !== undefined) r[at + MIN_PX_FIELD_OFFSET] = opts.minScreenPx
    if (opts.maxScreenPx !== undefined) r[at + MAX_PX_FIELD_OFFSET] = opts.maxScreenPx
  }
  // The clamp pair against the STORED bounds, before any write lands.
  let checkClamp = (verb: string, sprite: SpriteState, opts: SpriteOptions): void => {
    if (opts.minScreenPx === undefined && opts.maxScreenPx === undefined) return
    let at = sprite._slot * INSTANCE_FLOATS
    let r = layer._records
    checkScreenSize(verb, opts.minScreenPx ?? r[at + MIN_PX_FIELD_OFFSET]!, opts.maxScreenPx ?? r[at + MAX_PX_FIELD_OFFSET]!)
  }

  let views = createViews({
    label,
    pipeline: gpu.pipeline,
    quad: gpu.quad,
    textures: { ...atlasBindings(atlases.map(a => a.texture)), ...material.textures },
    buffers,
    count: () => published,
    tint: () => tint,
    params: () => params,
    pick: (x, y, zoom) => layer.pick(x, y, zoom),
    order: instanceOrder,
  })

  let layer: RecordLayer = {
    atlases,
    material,
    get count() {
      return layer._order.length
    },
    setTint(next) {
      if (disposed) return
      checkTint("setTint", next)
      tint = next
      views.setTint(next)
    },
    setParams(next) {
      if (disposed) return
      checkParams("setParams", next)
      Object.assign(params, next)
      views.setParams(next)
    },
    createView(vopts) {
      if (disposed) throw new Error("createView: layer is disposed")
      return views.create(vopts)
    },
    pick(x, y, zoom = 1) {
      // Topmost first: reverse draw order over the DRAWN prefix (a sprite
      // past the dial is undrawn and unpicked), exact rotated-rect
      // containment of the drawn rect (the record's size under its clamp
      // at this zoom).
      if (!(zoom > 0)) throw new Error(`pick: zoom must be positive, got ${zoom}`)
      let out: Sprite[] = []
      let r = layer._records
      for (let i = drawn() - 1; i >= 0; i--) {
        let at = i * INSTANCE_FLOATS
        let w = r[at + 2]!
        let h = r[at + 3]!
        let scale = screenSizeScale(w, h, zoom, r[at + MIN_PX_FIELD_OFFSET]!, r[at + MAX_PX_FIELD_OFFSET]!)
        if (pointInSprite(x, y, r[at]!, r[at + 1]!, w * scale, h * scale, r[at + 8]!)) {
          out.push(layer._order[i]!)
        }
      }
      return out
    },
    dispose() {
      if (disposed) return
      disposed = true
      unhook()
      for (let sprite of layer._order) sprite.layer = null
      layer._order.length = 0
      views.dispose()
      for (let old of grownFrom) destroyBuffer(old)
      grownFrom.length = 0
      stream.destroy()
      for (let s of styles) s.stream.destroy()
    },
    _add(opts) {
      if (opts?.parent) {
        throw new Error("addSprite: record layers have no groups (parent is the node layer's)")
      }
      // An absent key and an explicit undefined both take the default (the
      // options convention everywhere; a plain spread would let undefined
      // win), the node layer's rule.
      let record: SpriteOptions = {
        ...opts,
        x: opts?.x ?? 0,
        y: opts?.y ?? 0,
        w: opts?.w ?? 0,
        h: opts?.h ?? 0,
        frame: opts?.frame ?? fullFrame(atlases[0]!),
        rotation: opts?.rotation ?? 0,
        tint: opts?.tint ?? [1, 1, 1, 1],
        minScreenPx: opts?.minScreenPx ?? 0,
        maxScreenPx: opts?.maxScreenPx ?? 0,
      }
      let atlas = checkWrite("addSprite", record)
      let index = layer._order.length
      if (index + 1 > stream.capacity) {
        // Growth replaces the mirrors and the buffers (doubling); the
        // flush fills the replacements whole and swaps them in.
        let next = stream.capacity * 2
        grownFrom.push(stream.grow(next))
        for (let s of styles) grownFrom.push(growStyle(s, next))
        layer._records = new Float32Array(stream.bytes.buffer)
      }
      let sprite: SpriteState = { layer, node: null, _slot: index, _x: 0, _y: 0, _w: 0, _h: 0, _rot: 0, _flipX: false, _flipY: false, _visible: true, _parent: null }
      layer._order.push(sprite)
      writeRecord(sprite, record, atlas)
      // No outline until a raw writer sets one: the slot may hold a
      // shifted-out sprite's record otherwise.
      layer._records.fill(0, index * INSTANCE_FLOATS + OUTLINE_FIELD_OFFSET, index * INSTANCE_FLOATS + OUTLINE_FIELD_OFFSET + 4)
      // The material's style slot starts blank for the same reason.
      for (let s of styles) blankStyle(s, index)
      markRecords(layer, index, index + 1)
      return sprite
    },
    _write(sprite, opts) {
      checkClamp("setSprite", sprite, opts)
      let atlas = checkWrite("setSprite", opts)
      writeRecord(sprite, opts, atlas)
      markRecords(layer, sprite._slot, sprite._slot + 1)
    },
    _read(sprite) {
      let at = sprite._slot * INSTANCE_FLOATS
      let r = layer._records
      return {
        x: r[at]!,
        y: r[at + 1]!,
        w: r[at + 2]!,
        h: r[at + 3]!,
        frame: readFrame(r, at + 4, sprite, atlases[r[at + ATLAS_FIELD_OFFSET]!]!.texture),
        flipX: sprite._flipX,
        flipY: sprite._flipY,
        rotation: r[at + 8]!,
        tint: [r[at + 9]!, r[at + 10]!, r[at + 11]!, r[at + 12]!],
        minScreenPx: r[at + MIN_PX_FIELD_OFFSET]!,
        maxScreenPx: r[at + MAX_PX_FIELD_OFFSET]!,
        // Record sprites have no key field and no visibility (see
        // writeRecord's throws).
        renderOrder: 0,
        visible: true,
      }
    },
    _destroy(sprite) {
      // Later sprites shift down one draw slot (order preserved): the
      // shifted records are dirty, and the count shrinks either way.
      sprite.layer = null
      let index = sprite._slot
      let order = layer._order
      let r = layer._records
      let last = order.length - 1
      r.copyWithin(index * INSTANCE_FLOATS, (index + 1) * INSTANCE_FLOATS, order.length * INSTANCE_FLOATS)
      if (index < last) for (let s of styles) shiftStyle(s, index + 1, order.length)
      order.splice(index, 1)
      for (let i = index; i < order.length; i++) order[i]!._slot = i
      if (index < last) markRecords(layer, index, last)
      else layer._schedule()
    },
    _schedule() {
      if (disposed || scheduled) return
      scheduled = true
      RESOLVED.then(flush)
    },
    _stream: stream,
    _records: new Float32Array(stream.bytes.buffer),
    _styles: styles,
    _dial: Infinity,
    _order: [],
  }
  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}
