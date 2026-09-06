// The records layer: the raw escape hatch for motion only JS can compute
// (bespoke flocking, per-frame gameplay logic over every entity at large
// populations). Sprites are 13 JS-owned floats in one canonical
// Float32Array ordered by draw order (insertion order - painter's
// algorithm, later over earlier; `orderBy` swaps that for a core-produced
// key order at publish, records untouched); mutations batch to a microtask
// whose flush publishes the live prefix through the zero-copy buffer write
// lease.
// A moved sprite is 13 float stores plus one bulk memcpy per dirty frame; a
// static layer publishes nothing and therefore costs nothing.
//
// This is NOT the default live layer - that is layer.ts, where sprites are
// spatial arena nodes core producers can reach. Use this when a JS loop
// writes every record every frame anyway: `layer.records` + `touch()` is
// ~2.4x faster than setSprite at 30k sprites (measured 12.9ms raw vs
// 30.8ms via setSprite, purely call overhead). It shrinks as producers
// land; it is the where-motion-is-computed axis, not a "game tier".
//
// Layer space, camera, pointer dispatch and the sprite functions
// (addSprite/setSprite/...) are shared with the node layer; picking here is
// the JS reverse walk (pointInSprite), since records have no nodes.
import { getOwner, onCleanup } from "@solidrt/core"
import {
  beginBufferWrite,
  createBuffer,
  createPipelineTexture,
  destroyBuffer,
  destroyTexture,
  endBufferWrite,
  setDraw,
  setTargetParams,
  setTargetSize,
} from "@solidrt/core/gpu"
import type { BufferId, TextureId } from "@solidrt/core/gpu"
import { checkCamera, projectCamera, unprojectCamera } from "./camera.ts"
import { FULL_FRAME, writeFrame } from "./frames.ts"
import { spriteDispatch } from "./dispatch.ts"
import { checkTint, readFrame } from "./layer.ts"
import type { LayerBase, LayerPointerListener, Sprite, SpriteHandlers, SpriteLayerOptions, SpriteOptions, SpriteState } from "./layer.ts"
import { pointInSprite } from "./pick.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"
import { FRAGMENT, INSTANCE_ATTRIBUTES, VERTEX } from "./shaders.ts"

// Floats per instance record:
// [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA]
export const FLOATS_PER_SPRITE = 13

// Float offset of cy in a record - what `orderBy: "y"` keys on.
const Y_FIELD_OFFSET = 1

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
   * removeSprite still shifts - only the draw order changes. Ties keep
   * record order, so an unset key draws exactly as before. Known
   * limitation: pick() resolves overlapping sprites by record order, not
   * visual order, when a key is set.
   */
  orderBy?: "y" | { field: number; descending?: boolean }
}

export type RecordLayer = LayerBase & {
  /**
   * The canonical record array - the raw power path. Layout per sprite is
   * FLOATS_PER_SPRITE floats: [cx, cy, w, h, u0, v0, u1, v1, rot, tintR,
   * tintG, tintB, tintA], record i at i * FLOATS_PER_SPRITE. Record order
   * is draw order - unless the layer was created with `orderBy`, which
   * draws in key order while record i keeps meaning sprite i. Write fields
   * directly for large per-frame populations, then call touch() once. Do
   * not cache indices across removeSprite - records shift - and do not
   * cache the array across addSprite - growth replaces it.
   */
  records: Float32Array
  /**
   * Run `fn` with the CURRENT record array and return its result - the
   * hoist-proof form of `records`. A cached array reference silently
   * becomes a dead copy after growth (writes publish nothing); reading
   * through withRecords at use time always hits the live array.
   */
  withRecords<T>(fn: (records: Float32Array) => T): T
  /** Mark the records dirty and schedule the publish (the raw-path commit). */
  touch(): void
  _order: SpriteState[]
}

/**
 * Create a records layer rendering into a `width` x `height` texture from
 * one atlas texture (same target shape as createSpriteLayer; the atlas is
 * NOT owned). Disposed automatically with the owning reactive scope (opt
 * out with `{ autoFree: false }`).
 */
export function createRecordLayer(
  width: number,
  height: number,
  atlas: TextureId,
  opts?: RecordLayerOptions,
): RecordLayer {
  let capacity = opts?.capacity ?? 1024
  if (!(capacity > 0 && Number.isInteger(capacity))) {
    throw new Error(`createRecordLayer: capacity must be a positive integer, got ${capacity}`)
  }
  let label = opts?.label ?? "sprites"
  // One unit quad (triangle strip), reused by every instance.
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), {
    label: `${label}-quad`,
    autoFree: false,
  })
  let records: BufferId = createBuffer(capacity * FLOATS_PER_SPRITE * 4, {
    label: `${label}-records`,
    autoFree: false,
  })
  let oversample = opts?.oversample ?? 1
  checkOversample("createRecordLayer", oversample, width, height)
  let tint = opts?.tint ?? [1, 1, 1, 1]
  checkTint("createRecordLayer", tint)
  let orderBy = opts?.orderBy
  let instanceOrder =
    orderBy === undefined
      ? undefined
      : orderBy === "y"
        ? { field: Y_FIELD_OFFSET }
        : { field: orderBy.field, descending: orderBy.descending }
  let thrash = thrashSentinel(`record layer "${label}"`)
  let texture = createPipelineTexture(
    VERTEX,
    FRAGMENT,
    width * oversample,
    height * oversample,
    { uViewport: [width, height], uCamera: [0, 0, 1, 1], uCameraRot: [1, 0, 0, 0], uTint: tint },
    {
      label,
      topology: "triangle-strip",
      vertexCount: 4,
      attributes: [{ name: "aPos", format: "vec2" }],
      buffer: quad,
      instanceAttributes: INSTANCE_ATTRIBUTES,
      instanceBuffer: records,
      instanceOrder,
      instanceCount: 0,
      blend: "alpha",
      textures: { uAtlas: atlas },
      clearColor: opts?.clearColor ?? [0, 0, 0, 0],
      autoFree: false,
    },
  )

  // Camera state, mirrored for picking (the inverse mapping).
  let camX = 0
  let camY = 0
  let camZoom = 1
  let camRot = 0
  let camPivotX = 0
  let camPivotY = 0
  let disposed = false
  let dirty = false
  let scheduled = false
  let published = 0

  // The GPU buffer's record capacity; the canonical array grows ahead of it
  // (addSprite) and the publish catches the buffer up: a larger buffer is
  // created, written in full, swapped in, and the old one destroyed. The
  // entry holds the old buffer alive until the swap lands, so the destroy
  // is safe to issue right after.
  let gpuCapacity = capacity
  let flush = () => {
    scheduled = false
    if (disposed || !dirty) return
    dirty = false
    let count = layer._order.length
    let grown: BufferId | null = null
    if (layer.records.length > gpuCapacity * FLOATS_PER_SPRITE) {
      gpuCapacity = layer.records.length / FLOATS_PER_SPRITE
      grown = createBuffer(layer.records.length * 4, { label: `${label}-records`, autoFree: false })
    }
    let target = grown ?? records
    let out = beginBufferWrite(target)
    out.set(layer.records.subarray(0, count * FLOATS_PER_SPRITE))
    endBufferWrite(target, count * FLOATS_PER_SPRITE * 4)
    if (grown !== null) {
      setDraw(texture, { instanceBuffer: grown, instanceCount: count })
      if (instanceOrder !== undefined) {
        // The growth publish above landed BEFORE the swap (the entry must
        // never point at an unwritten buffer), so the order had not yet
        // followed to the grown buffer and that publish went out ungathered.
        // One more publish, now under the swapped-in order, restores key
        // order - growth frames only.
        let again = beginBufferWrite(grown)
        again.set(layer.records.subarray(0, count * FLOATS_PER_SPRITE))
        endBufferWrite(grown, count * FLOATS_PER_SPRITE * 4)
      }
      destroyBuffer(records)
      records = grown
      published = count
    } else if (count !== published) {
      setDraw(texture, { instanceCount: count })
      published = count
    }
  }

  // Record layout: [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA]
  let writeRecord = (sprite: SpriteState, opts: SpriteOptions) => {
    if (opts.renderOrder !== undefined) {
      throw new Error("setSprite: record layers have no renderOrder field; order by a record field with orderBy { field }")
    }
    if (opts.visible !== undefined) {
      throw new Error("setSprite: record sprites have no visibility; hide by zeroing w or h")
    }
    let at = sprite._slot * FLOATS_PER_SPRITE
    let r = layer.records
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
  }

  let listeners = new Set<LayerPointerListener>()

  let layer: RecordLayer = {
    texture,
    handlers: undefined as unknown as SpriteHandlers,
    get count() {
      return layer._order.length
    },
    get width() {
      return width
    },
    get height() {
      return height
    },
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      checkOversample("setSize", oversample, w, h)
      width = w
      height = h
      setTargetSize(texture, w * oversample, h * oversample)
      setTargetParams(texture, { uViewport: [w, h] })
    },
    listen(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get oversample() {
      return oversample
    },
    setOversample(n) {
      if (disposed || n === oversample) return
      checkOversample("setOversample", n, width, height)
      thrash()
      oversample = n
      setTargetSize(texture, width * n, height * n)
    },
    setCamera(update) {
      if (disposed) return
      checkCamera(update)
      if (update.x !== undefined) camX = update.x
      if (update.y !== undefined) camY = update.y
      if (update.zoom !== undefined) camZoom = update.zoom
      if (update.rotation !== undefined) camRot = update.rotation
      if (update.pivotX !== undefined) camPivotX = update.pivotX
      if (update.pivotY !== undefined) camPivotY = update.pivotY
      setTargetParams(texture, {
        uCamera: [camX, camY, camZoom, camZoom],
        uCameraRot: [Math.cos(camRot), Math.sin(camRot), camPivotX, camPivotY],
      })
    },
    camera() {
      return { x: camX, y: camY, zoom: camZoom, rotation: camRot, pivotX: camPivotX, pivotY: camPivotY }
    },
    project(x, y) {
      return projectCamera(layer.camera(), x, y)
    },
    unproject(x, y) {
      return unprojectCamera(layer.camera(), x, y)
    },
    setTint(next) {
      if (disposed) return
      checkTint("setTint", next)
      setTargetParams(texture, { uTint: next })
    },
    pick(x, y) {
      // Topmost first: reverse draw order, exact rotated-rect containment.
      let out: Sprite[] = []
      let r = layer.records
      for (let i = layer._order.length - 1; i >= 0; i--) {
        let at = i * FLOATS_PER_SPRITE
        if (pointInSprite(x, y, r[at]!, r[at + 1]!, r[at + 2]!, r[at + 3]!, r[at + 8]!)) {
          out.push(layer._order[i]!)
        }
      }
      return out
    },
    handlersFor(layout) {
      return dispatch(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      listeners.clear()
      for (let sprite of layer._order) sprite.layer = null
      layer._order.length = 0
      destroyTexture(texture)
      destroyBuffer(records)
      destroyBuffer(quad)
    },
    _add(opts) {
      if (opts?.parent) {
        throw new Error("addSprite: record layers have no groups (parent is the node layer's)")
      }
      let index = layer._order.length
      if ((index + 1) * FLOATS_PER_SPRITE > layer.records.length) {
        let next = new Float32Array(layer.records.length * 2)
        next.set(layer.records)
        layer.records = next
      }
      let sprite: SpriteState = { layer, node: null, _slot: index, _x: 0, _y: 0, _w: 0, _h: 0, _rot: 0, _flipX: false, _flipY: false, _visible: true, _parent: null }
      layer._order.push(sprite)
      writeRecord(sprite, {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        frame: FULL_FRAME,
        rotation: 0,
        tint: [1, 1, 1, 1],
        ...opts,
      })
      layer._schedule()
      return sprite
    },
    _write(sprite, opts) {
      writeRecord(sprite, opts)
      layer._schedule()
    },
    _read(sprite) {
      let at = sprite._slot * FLOATS_PER_SPRITE
      let r = layer.records
      return {
        x: r[at]!,
        y: r[at + 1]!,
        w: r[at + 2]!,
        h: r[at + 3]!,
        frame: readFrame(r, at + 4, sprite),
        flipX: sprite._flipX,
        flipY: sprite._flipY,
        rotation: r[at + 8]!,
        tint: [r[at + 9]!, r[at + 10]!, r[at + 11]!, r[at + 12]!],
        // Record sprites have no key field and no visibility (see
        // writeRecord's throws).
        renderOrder: 0,
        visible: true,
      }
    },
    _remove(sprite) {
      // Later sprites shift down one draw slot (order preserved).
      sprite.layer = null
      let index = sprite._slot
      let order = layer._order
      let r = layer.records
      r.copyWithin(index * FLOATS_PER_SPRITE, (index + 1) * FLOATS_PER_SPRITE, order.length * FLOATS_PER_SPRITE)
      order.splice(index, 1)
      for (let i = index; i < order.length; i++) order[i]!._slot = i
      layer._schedule()
    },
    _schedule() {
      if (disposed) return
      dirty = true
      if (scheduled) return
      scheduled = true
      RESOLVED.then(flush)
    },
    records: new Float32Array(capacity * FLOATS_PER_SPRITE),
    withRecords(fn) {
      return fn(layer.records)
    },
    touch() {
      layer._schedule()
    },
    _order: [],
  }
  let dispatch = spriteDispatch({
    size: () => [width, height],
    camera: () => layer.camera(),
    pick: (x, y) => layer.pick(x, y),
    root: layer,
    listeners,
  })
  layer.handlers = dispatch(null)

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}
