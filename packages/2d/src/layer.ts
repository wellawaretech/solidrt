// The retained sprite layer: one instanced draw over one atlas, plain objects
// and dirty flags, no signals - reactivity stays at the component boundary
// (components.tsx), the same split as @solidrt/3d's scene.ts. A layer owns a
// pipeline target and an instance buffer; every sprite is 13 floats in a
// canonical Float32Array ordered by draw order (insertion order - painter's
// algorithm, later sprites over earlier). Mutations batch to a microtask
// whose flush publishes the live prefix through the zero-copy buffer write
// lease (beginBufferWrite/endBufferWrite) and updates the instance count: a
// moved sprite is 13 float stores plus one bulk memcpy per dirty frame, a
// static layer publishes nothing and therefore costs nothing.
//
// Layer space is pixels, top-left origin, y-down - the render tree's frame.
// The pipeline's clip space is y-down too (core gpu.ts pixel contract), so
// the vertex stage maps world -> clip with no flip anywhere. The camera is
// a shared-params write (uCamera), never a per-sprite touch.
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
import {
  beginBufferWrite,
  createBuffer,
  createPipelineTexture,
  destroyBuffer,
  destroyTexture,
  endBufferWrite,
  glsl,
  setDraw,
  setTargetParams,
  setTargetSize,
} from "@solidrt/core/gpu"
import type { BufferId, TextureId } from "@solidrt/core/gpu"
import type { Frame } from "./frames.ts"
import { FULL_FRAME } from "./frames.ts"
import { pointInSprite } from "./pick.ts"

// Floats per instance record:
// [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA]
export const FLOATS_PER_SPRITE = 13

const RESOLVED = Promise.resolve()

let VERTEX = glsl`
  in vec2 aPos;
  in vec2 iCenter;
  in vec2 iSize;
  in vec4 iUv;
  in float iRot;
  in vec4 iTint;
  out vec2 vUv;
  out vec4 vTint;
  uniform vec2 uViewport;
  uniform vec4 uCamera;

  void main() {
    vec2 corner = aPos * iSize;
    float c = cos(iRot), s = sin(iRot);
    vec2 world = iCenter + vec2(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    vec2 screen = (world - uCamera.xy) * uCamera.zw;
    // World and clip are both y-down, so the mapping carries no flip.
    gl_Position = vec4(screen / uViewport * 2.0 - 1.0, 0.0, 1.0);
    vUv = mix(iUv.xy, iUv.zw, aPos + 0.5);
    vTint = iTint;
  }
`

let FRAGMENT = glsl`
  in vec2 vUv;
  in vec4 vTint;
  uniform sampler2D uAtlas;

  void main() {
    fragColor = texture(uAtlas, vUv) * vTint;
  }
`

/**
 * One sprite: a handle into its layer's record array. Read via getSprite;
 * write through setSprite so changes publish. The pointer handlers are plain
 * assignable fields - they touch no GPU state (the scene-graph handler rule).
 */
export type Sprite = {
  /** The owning layer, null after removeSprite. */
  layer: SpriteLayer | null
  /** Index into the draw order; maintained by the layer. */
  _index: number
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
}

/** Sprite fields, all optional at every call site: absent keys keep values. */
export type SpriteOptions = {
  /** Center position in layer pixels. */
  x?: number
  y?: number
  /** Drawn size in layer pixels. */
  w?: number
  h?: number
  /** Atlas frame (normalized UVs); default the whole atlas. */
  frame?: Frame
  /** Rotation about the center, radians, clockwise (y-down space). */
  rotation?: number
  /** RGBA multiplier 0..1 each; default opaque white (the texture as-is). */
  tint?: [number, number, number, number]
}

export type SpritePointerEvent = {
  /** The sprite hit (the topmost at the point), constant while captured. */
  sprite: Sprite
  /** Pointer position in LAYER pixels (the camera mapping undone). */
  x: number
  y: number
  pointerId: number
  pointerType: string
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
}

export type SpriteHandlers = {
  onPointerDown: (event: ElementPointerEvent) => void
  onPointerMove: (event: ElementPointerEvent) => void
  onPointerUp: (event: ElementPointerEvent) => void
  onPointerLeave: (event: ElementPointerEvent) => void
}

export type CameraUpdate = {
  /** World pixel at the viewport's top-left corner. */
  x?: number
  y?: number
  /** World-to-screen scale; 1 is pixel-for-pixel. */
  zoom?: number
}

export type SpriteLayerOptions = {
  /**
   * Initial record reservation; default 1024. The layer grows past it on
   * demand (doubling), so this is a hint that avoids regrowth copies, not a
   * limit.
   */
  capacity?: number
  clearColor?: [number, number, number, number]
  label?: string
  /** Skip the owner-scoped auto-dispose (see createSpriteLayer). */
  autoFree?: boolean
}

export type SpriteLayer = {
  /** The layer's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** Element handlers wiring sprite pointer events; see handlersFor. */
  handlers: SpriteHandlers
  /** Live sprite count (draw order length). */
  count: number
  setSize(width: number, height: number): void
  setCamera(update: CameraUpdate): void
  /** Topmost sprite whose rotated rect contains the layer-pixel point. */
  pick(x: number, y: number): Sprite | null
  /**
   * Handlers for a leaf whose LAYOUT size differs from the layer size
   * (events scale by layer/layout; null layout means "already layer pixels",
   * which is what the built-in leaf uses).
   */
  handlersFor(layout: (() => { width: number; height: number } | null) | null): SpriteHandlers
  dispose(): void
  /**
   * The canonical record array - the raw power path. Layout per sprite is
   * FLOATS_PER_SPRITE floats: [cx, cy, w, h, u0, v0, u1, v1, rot, tintR,
   * tintG, tintB, tintA], record i at i * FLOATS_PER_SPRITE in draw order.
   * Write fields directly for large per-frame populations (setSprite is
   * ~2.4x slower at scale purely from call overhead - measured 30k sprites:
   * 12.9ms raw vs 30.8ms via setSprite), then call touch() once. Do not
   * cache indices across removeSprite - records shift - and do not cache
   * the array across addSprite - growth replaces it.
   */
  records: Float32Array
  /** Mark the records dirty and schedule the publish (the raw-path commit). */
  touch(): void
  _order: Sprite[]
  _schedule(): void
}

/**
 * Create a sprite layer rendering into a `width` x `height` texture from one
 * atlas texture. Disposed automatically with the owning reactive scope (opt
 * out with `{ autoFree: false }`); the atlas is NOT owned - dispose it
 * yourself (it commonly outlives layers).
 */
export function createSpriteLayer(
  width: number,
  height: number,
  atlas: TextureId,
  opts?: SpriteLayerOptions,
): SpriteLayer {
  let capacity = opts?.capacity ?? 1024
  if (!(capacity > 0 && Number.isInteger(capacity))) {
    throw new Error(`createSpriteLayer: capacity must be a positive integer, got ${capacity}`)
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
  let texture = createPipelineTexture(
    VERTEX,
    FRAGMENT,
    width,
    height,
    { uViewport: [width, height], uCamera: [0, 0, 1, 1] },
    {
      label,
      topology: "triangle-strip",
      vertexCount: 4,
      attributes: [{ name: "aPos", format: "vec2" }],
      buffer: quad,
      instanceAttributes: [
        { name: "iCenter", format: "vec2" },
        { name: "iSize", format: "vec2" },
        { name: "iUv", format: "vec4" },
        { name: "iRot", format: "f32" },
        { name: "iTint", format: "vec4" },
      ],
      instanceBuffer: records,
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
      destroyBuffer(records)
      records = grown
      published = count
    } else if (count !== published) {
      setDraw(texture, { instanceCount: count })
      published = count
    }
  }

  // Pointer dispatch: capture per pointer, hover pairing, no bubbling (the
  // sprite list is flat). Layout null = the leaf is laid out at layer size,
  // so localX/localY are layer pixels already (the element hit test undid
  // every ancestor transform - never getBoundingBox here).
  let capture = new Map<number, Sprite>()
  let hover = new Map<number, Sprite>()

  let makeHandlers = (layout: (() => { width: number; height: number } | null) | null): SpriteHandlers => {
    let toLayer = (e: ElementPointerEvent): [number, number] => {
      let x = e.localX
      let y = e.localY
      let l = layout?.()
      if (l && l.width > 0 && l.height > 0) {
        x *= width / l.width
        y *= height / l.height
      }
      // Undo the camera: screen -> world.
      return [x / camZoom + camX, y / camZoom + camY]
    }
    let makeEvent = (sprite: Sprite, x: number, y: number, e: ElementPointerEvent): SpritePointerEvent => ({
      sprite,
      x,
      y,
      pointerId: e.pointerId,
      pointerType: e.pointerType,
      button: e.button,
      shiftKey: e.shiftKey,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      metaKey: e.metaKey,
    })
    return {
      onPointerDown(e) {
        let [x, y] = toLayer(e)
        let hit = layer.pick(x, y)
        if (!hit) return
        capture.set(e.pointerId, hit)
        hit.onPointerDown?.(makeEvent(hit, x, y, e))
      },
      onPointerMove(e) {
        let [x, y] = toLayer(e)
        let captured = capture.get(e.pointerId)
        if (captured) {
          if (captured.layer) captured.onPointerMove?.(makeEvent(captured, x, y, e))
          return
        }
        let hit = layer.pick(x, y)
        let prev = hover.get(e.pointerId) ?? null
        if (prev !== hit) {
          if (prev && prev.layer) prev.onPointerLeave?.(makeEvent(prev, x, y, e))
          if (hit) hit.onPointerEnter?.(makeEvent(hit, x, y, e))
          if (hit) hover.set(e.pointerId, hit)
          else hover.delete(e.pointerId)
        }
        hit?.onPointerMove?.(makeEvent(hit, x, y, e))
      },
      onPointerUp(e) {
        let [x, y] = toLayer(e)
        let captured = capture.get(e.pointerId)
        if (captured) {
          capture.delete(e.pointerId)
          if (captured.layer) captured.onPointerUp?.(makeEvent(captured, x, y, e))
          return
        }
        let hit = layer.pick(x, y)
        hit?.onPointerUp?.(makeEvent(hit, x, y, e))
      },
      onPointerLeave(e) {
        let [x, y] = toLayer(e)
        let prev = hover.get(e.pointerId)
        if (prev) {
          hover.delete(e.pointerId)
          if (prev.layer) prev.onPointerLeave?.(makeEvent(prev, x, y, e))
        }
      },
    }
  }

  let layer: SpriteLayer = {
    texture,
    handlers: undefined as unknown as SpriteHandlers,
    get count() {
      return layer._order.length
    },
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      width = w
      height = h
      setTargetSize(texture, w, h)
      setTargetParams(texture, { uViewport: [w, h] })
    },
    setCamera(update) {
      if (disposed) return
      if (update.x !== undefined) camX = update.x
      if (update.y !== undefined) camY = update.y
      if (update.zoom !== undefined) {
        if (!(update.zoom > 0)) throw new Error(`setCamera: zoom must be positive, got ${update.zoom}`)
        camZoom = update.zoom
      }
      setTargetParams(texture, { uCamera: [camX, camY, camZoom, camZoom] })
    },
    pick(x, y) {
      // Topmost first: reverse draw order, exact rotated-rect containment.
      let r = layer.records
      for (let i = layer._order.length - 1; i >= 0; i--) {
        let at = i * FLOATS_PER_SPRITE
        if (pointInSprite(x, y, r[at]!, r[at + 1]!, r[at + 2]!, r[at + 3]!, r[at + 8]!)) {
          return layer._order[i]!
        }
      }
      return null
    },
    handlersFor(layout) {
      return makeHandlers(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      for (let sprite of layer._order) sprite.layer = null
      layer._order.length = 0
      destroyTexture(texture)
      destroyBuffer(records)
      destroyBuffer(quad)
    },
    records: new Float32Array(capacity * FLOATS_PER_SPRITE),
    touch() {
      layer._schedule()
    },
    _order: [],
    _schedule() {
      if (disposed) return
      dirty = true
      if (scheduled) return
      scheduled = true
      RESOLVED.then(flush)
    },
  }
  layer.handlers = makeHandlers(null)

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}

// Record layout: [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA]
function writeSprite(layer: SpriteLayer, sprite: Sprite, opts: SpriteOptions): void {
  let at = sprite._index * FLOATS_PER_SPRITE
  let r = layer.records
  if (opts.x !== undefined) r[at] = opts.x
  if (opts.y !== undefined) r[at + 1] = opts.y
  if (opts.w !== undefined) r[at + 2] = opts.w
  if (opts.h !== undefined) r[at + 3] = opts.h
  if (opts.frame !== undefined) {
    r[at + 4] = opts.frame.u0
    r[at + 5] = opts.frame.v0
    r[at + 6] = opts.frame.u1
    r[at + 7] = opts.frame.v1
  }
  if (opts.rotation !== undefined) r[at + 8] = opts.rotation
  if (opts.tint !== undefined) {
    r[at + 9] = opts.tint[0]
    r[at + 10] = opts.tint[1]
    r[at + 11] = opts.tint[2]
    r[at + 12] = opts.tint[3]
  }
}

/**
 * Add a sprite at the top of the draw order (later sprites paint over
 * earlier - the painter's rule; there is no z field in v1). Past the
 * layer's reservation the record store doubles (the GPU buffer follows at
 * the next publish); reserve with `capacity` to avoid the copies.
 */
export function addSprite(layer: SpriteLayer, opts?: SpriteOptions): Sprite {
  let index = layer._order.length
  if ((index + 1) * FLOATS_PER_SPRITE > layer.records.length) {
    let next = new Float32Array(layer.records.length * 2)
    next.set(layer.records)
    layer.records = next
  }
  let sprite: Sprite = { layer, _index: index }
  layer._order.push(sprite)
  writeSprite(layer, sprite, {
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
}

/** The one write path: absent keys keep their values (the params rule). */
export function setSprite(sprite: Sprite, opts: SpriteOptions): void {
  let layer = sprite.layer
  if (!layer) return
  writeSprite(layer, sprite, opts)
  layer._schedule()
}

/** Read a sprite's current fields (a fresh object; mutating it does nothing). */
export function getSprite(sprite: Sprite): Required<SpriteOptions> | null {
  let layer = sprite.layer
  if (!layer) return null
  let at = sprite._index * FLOATS_PER_SPRITE
  let r = layer.records
  return {
    x: r[at]!,
    y: r[at + 1]!,
    w: r[at + 2]!,
    h: r[at + 3]!,
    frame: { u0: r[at + 4]!, v0: r[at + 5]!, u1: r[at + 6]!, v1: r[at + 7]! },
    rotation: r[at + 8]!,
    tint: [r[at + 9]!, r[at + 10]!, r[at + 11]!, r[at + 12]!],
  }
}

/**
 * Remove a sprite: later sprites shift down one draw slot (order preserved).
 * The handle goes inert (layer null); further setSprite calls are no-ops.
 */
export function removeSprite(sprite: Sprite): void {
  let layer = sprite.layer
  if (!layer) return
  sprite.layer = null
  let index = sprite._index
  let order = layer._order
  let r = layer.records
  r.copyWithin(index * FLOATS_PER_SPRITE, (index + 1) * FLOATS_PER_SPRITE, order.length * FLOATS_PER_SPRITE)
  order.splice(index, 1)
  for (let i = index; i < order.length; i++) order[i]!._index = i
  layer._schedule()
}
