// The live sprite layer, re-founded on the spatial core: every sprite is a
// SPATIAL ARENA node (arena slot: local pose, parent, world matrix, index
// leaf, record sink - no layout, no paint, no rendertree element) whose
// Pose2D record sink writes [x, y, angle, sx, sy] into the sprite's slot of
// the pose instance buffer at the core's flush. Rendering stays one
// instanced draw into one pipeline target, composited as a single
// `<texture>` leaf; what changed is who owns the pose upstream of the
// instance buffer - the arena, so every core producer (native transitions,
// animation clips, physics) reaches sprites through `sprite.node`, and
// picking walks the core BVH instead of a JS loop.
//
// Two instance-buffer slots split ownership: slot 0 is the pose buffer,
// written ONLY by the core (one coalesced write per flush however many
// nodes moved); slot 1 is the style buffer [u0, v0, u1, v1, tint rgba],
// JS-owned and published through the zero-copy write lease. Never write
// the pose buffer from JS - the core's staging mirror is the owner and
// will overwrite. For motion only JS can compute at large populations, the
// records layer (records.ts) is the escape hatch.
//
// Sprites hold FIXED instance slots (freed slots recycle): draw order is
// slot order, so removal never shifts records and pose sinks never rebind.
// Layer space is pixels, top-left origin, y-down - the render tree's
// frame. The camera is a shared-params write (uCamera), never per-sprite.
import { getOwner, onCleanup } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
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
import * as spatial from "flux:spatial"
import type { NodeId, NodeTransition } from "flux:spatial"
import { on } from "srt:events"
import type { Frame } from "./frames.ts"
import { FULL_FRAME } from "./frames.ts"
import type { RecordLayer } from "./records.ts"
import { FRAGMENT, INSTANCE_ATTRIBUTES_SPLIT, VERTEX_SPLIT } from "./shaders.ts"

/** Floats per pose record (the core's Pose2D projection). */
export const POSE_FLOATS = 5
/** Floats per style record: [u0, v0, u1, v1, tintR, tintG, tintB, tintA]. */
export const STYLE_FLOATS = 8

const RESOLVED = Promise.resolve()

// Shared marshalling scratch (the bindings copy synchronously).
const TRANSFORM = new Float32Array(10)
const FLAT_BOUNDS = new Float32Array([-0.5, -0.5, 0, 0.5, 0.5, 0])
const RAY_ORIGIN = new Float32Array(3)
const RAY_DIR = new Float32Array([0, 0, 1])
const BOX = new Float32Array(6)

// Settle routing: the core's "spatialTransitionEnd" event carries the node
// id, so the handles with a transition DECLARED are indexed by node (only
// those can settle; adding a sprite costs nothing here) and one lazy
// subscription, started at the first declaration, routes to the handle's
// onTransitionEnd. Target-only, like the element transitions.
let declared = new Map<NodeId, Sprite | SpriteGroup>()
let subscribed = false

function declare(node: NodeId, handle: Sprite | SpriteGroup, transition: NodeTransition | string | null): void {
  if (transition === null) {
    declared.delete(node)
    return
  }
  declared.set(node, handle)
  if (subscribed) return
  subscribed = true
  on("spatialTransitionEnd", (event: { node: NodeId; component: TransitionEndEvent["component"] }) => {
    let handle = declared.get(event.node)
    if (!handle) return
    try {
      handle.onTransitionEnd?.({ component: event.component })
    } catch (err) {
      console.error("Error in onTransitionEnd handler:", err)
    }
  })
}

/**
 * One sprite: a handle into its layer. Read via getSprite; write through
 * setSprite so changes publish. The pointer handlers are plain assignable
 * fields - they touch no GPU state (the scene-graph handler rule).
 */
export type Sprite = {
  /** The owning layer, null after removeSprite. */
  layer: SpriteLayer | RecordLayer | null
  /**
   * The sprite's SPATIAL ARENA node - the citizenship handle: bind core
   * producers to it or reach it through flux:spatial directly (the layer
   * still owns the node's life; destroy it only via removeSprite). Null
   * on a record layer's sprites.
   */
  node: NodeId | null
  /** Instance slot: fixed for the sprite's life on the node layer, the
   * shifting draw-order index on a record layer. */
  _slot: number
  /** Pose mirror (node layer): what setSprite composes transforms from. */
  _x: number
  _y: number
  _w: number
  _h: number
  _rot: number
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
  /** A declared transition (setSpriteTransition) settled naturally on
   * one component; a cancel or snap never fires. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
}

/** The settled component of a node transition: `position` is x/y,
 * `scale` w/h (a group's uniform scale). */
export type TransitionEndEvent = {
  component: "position" | "rotation" | "scale"
}

/** Sprite fields, all optional at every call site: absent keys keep values. */
export type SpriteOptions = {
  /** Center position in layer pixels (in the parent group's frame when the
   * sprite is grouped). */
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

export type AddSpriteOptions = SpriteOptions & {
  /** Mount under this group (node layer only); pose fields are then local
   * to it. Reparent later with setSpriteParent. */
  parent?: SpriteGroup
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
   * Initial slot reservation; default 1024. The layer grows past it on
   * demand (doubling), so this is a hint that avoids regrowth copies, not a
   * limit.
   */
  capacity?: number
  clearColor?: [number, number, number, number]
  label?: string
  /** Skip the owner-scoped auto-dispose (see createSpriteLayer). */
  autoFree?: boolean
}

/**
 * A transform group: a plain spatial arena node (position, rotation,
 * uniform scale - never a sprite size) that sprites and other groups
 * parent under, so a ship with turrets or a dragged stack moves as one
 * subtree recomputed in native code. Groups render nothing and cannot be
 * picked; sprites are always the leaves.
 */
export type SpriteGroup = {
  /** The owning layer, null after removeGroup. */
  layer: SpriteLayer | null
  /** The group's spatial arena node. */
  node: NodeId
  _x: number
  _y: number
  _rot: number
  _scale: number
  /** See Sprite.onTransitionEnd. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
}

/** Group fields, all optional: absent keys keep values. */
export type GroupOptions = {
  /** Position in the parent frame (layer pixels at the root). */
  x?: number
  y?: number
  /** Rotation, radians, clockwise (y-down space). */
  rotation?: number
  /** Uniform scale on the whole subtree (this one scales child sprites -
   * a group is a frame, not a sprite size). */
  scale?: number
  /** Reparent (null = make the group a root). */
  parent?: SpriteGroup | null
}

/** What both layer kinds share; the free sprite functions dispatch on it. */
export type LayerBase = {
  /** The layer's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** Element handlers wiring sprite pointer events; see handlersFor. */
  handlers: SpriteHandlers
  /** Live sprite count. */
  readonly count: number
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
  _add(opts?: AddSpriteOptions): Sprite
  _write(sprite: Sprite, opts: SpriteOptions): void
  _read(sprite: Sprite): Required<SpriteOptions>
  _remove(sprite: Sprite): void
  _schedule(): void
}

export type SpriteLayer = LayerBase & {
  /**
   * Every sprite whose rotated rect overlaps the layer-pixel rect (the
   * core BVH overlap query, exact for rotated sprites), unordered - the
   * marquee query. Node layer only.
   */
  pickRect(x: number, y: number, w: number, h: number): Sprite[]
  _groups: Set<SpriteGroup>
}

/**
 * Pointer dispatch shared by both layer kinds: capture per pointer, hover
 * pairing, no bubbling (the sprite list is flat). Layout null = the leaf is
 * laid out at layer size, so localX/localY are layer pixels already (the
 * element hit test undid every ancestor transform - never getBoundingBox
 * here). Internal - layers expose the result as handlers/handlersFor.
 */
export function spriteDispatch(state: {
  size: () => [number, number]
  camera: () => [number, number, number]
  pick: (x: number, y: number) => Sprite | null
}): (layout: (() => { width: number; height: number } | null) | null) => SpriteHandlers {
  let capture = new Map<number, Sprite>()
  let hover = new Map<number, Sprite>()
  return layout => {
    let toLayer = (e: ElementPointerEvent): [number, number] => {
      let x = e.localX
      let y = e.localY
      let l = layout?.()
      let [width, height] = state.size()
      if (l && l.width > 0 && l.height > 0) {
        x *= width / l.width
        y *= height / l.height
      }
      // Undo the camera: screen -> world.
      let [camX, camY, camZoom] = state.camera()
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
        let hit = state.pick(x, y)
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
        let hit = state.pick(x, y)
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
        let hit = state.pick(x, y)
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
}

/** Fill the shared transform scratch: xy translation, z rotation, xy scale
 * (a sprite's scale is its w/h - every sprite is a scaled unit quad). */
function fillTransform(x: number, y: number, rot: number, sx: number, sy: number): void {
  let half = rot / 2
  TRANSFORM[0] = x
  TRANSFORM[1] = y
  TRANSFORM[2] = 0
  TRANSFORM[3] = 0
  TRANSFORM[4] = 0
  TRANSFORM[5] = Math.sin(half)
  TRANSFORM[6] = Math.cos(half)
  TRANSFORM[7] = sx
  TRANSFORM[8] = sy
  TRANSFORM[9] = 1
}

/** Compose and push a node-backed sprite's local transform - through the
 * node's transition declaration, so with one set (setSpriteTransition)
 * the write is a target the core animates toward. */
function writeTransform(sprite: Sprite): void {
  fillTransform(sprite._x, sprite._y, sprite._rot, sprite._w, sprite._h)
  spatial.writeTransform(sprite.node!, TRANSFORM)
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
  let pose: BufferId = createBuffer(capacity * POSE_FLOATS * 4, { label: `${label}-pose`, autoFree: false })
  let style: BufferId = createBuffer(capacity * STYLE_FLOATS * 4, { label: `${label}-style`, autoFree: false })
  let texture = createPipelineTexture(
    VERTEX_SPLIT,
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
      instanceAttributes: INSTANCE_ATTRIBUTES_SPLIT,
      instanceBuffers: [pose, style],
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
  let scheduled = false
  let styleDirty = false
  let published = 0

  // Slot allocation: freed slots recycle, the high-water mark is the
  // published instance count (a freed slot's pose zeroes - zero scale
  // collapses the instance - so holes draw nothing).
  let highWater = 0
  let freeSlots: number[] = []
  let gpuCapacity = capacity
  let styleData = new Float32Array(capacity * STYLE_FLOATS)
  let byNode = new Map<NodeId, Sprite>()

  let flush = () => {
    scheduled = false
    if (disposed) return
    if (styleDirty) {
      styleDirty = false
      let out = beginBufferWrite(style)
      out.set(styleData.subarray(0, highWater * STYLE_FLOATS))
      endBufferWrite(style, highWater * STYLE_FLOATS * 4)
    }
    if (published !== highWater) {
      setDraw(texture, { instanceCount: highWater })
      published = highWater
    }
    // The core recomputes moved subtrees and publishes every dirty pose
    // slot as one coalesced write per buffer.
    spatial.flush()
  }

  // Grow both instance buffers to `next` slots: the pose sinks move in one
  // retargetRecords call (the whole used range republishes at the next
  // flush), the style mirror grows in JS and republishes through the lease.
  // The entry holds the old buffers alive until the swap lands, so the
  // destroys are safe to issue right after.
  let grow = (next: number) => {
    let newPose = createBuffer(next * POSE_FLOATS * 4, { label: `${label}-pose`, autoFree: false })
    let newStyle = createBuffer(next * STYLE_FLOATS * 4, { label: `${label}-style`, autoFree: false })
    spatial.retargetRecords(pose, newPose)
    let grownStyle = new Float32Array(next * STYLE_FLOATS)
    grownStyle.set(styleData)
    styleData = grownStyle
    setDraw(texture, { instanceBuffers: [newPose, newStyle] })
    destroyBuffer(pose)
    destroyBuffer(style)
    pose = newPose
    style = newStyle
    gpuCapacity = next
    styleDirty = true
  }

  let writeStyle = (slot: number, opts: SpriteOptions) => {
    let at = slot * STYLE_FLOATS
    if (opts.frame !== undefined) {
      styleData[at] = opts.frame.u0
      styleData[at + 1] = opts.frame.v0
      styleData[at + 2] = opts.frame.u1
      styleData[at + 3] = opts.frame.v1
    }
    if (opts.tint !== undefined) {
      styleData[at + 4] = opts.tint[0]
      styleData[at + 5] = opts.tint[1]
      styleData[at + 6] = opts.tint[2]
      styleData[at + 7] = opts.tint[3]
    }
    styleDirty = true
  }

  let dispatch = spriteDispatch({
    size: () => [width, height],
    camera: () => [camX, camY, camZoom],
    pick: (x, y) => layer.pick(x, y),
  })

  let layer: SpriteLayer = {
    texture,
    handlers: undefined as unknown as SpriteHandlers,
    get count() {
      return byNode.size
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
      // The index reads as of the last core flush; run any pending batch
      // first so a write followed by a pick sees the write.
      if (scheduled) flush()
      RAY_ORIGIN[0] = x
      RAY_ORIGIN[1] = y
      RAY_ORIGIN[2] = -1
      let best: Sprite | null = null
      for (let hit of spatial.raycast(RAY_ORIGIN, RAY_DIR)) {
        // The arena is shared (a 3d scene lives in the same index): only
        // this layer's nodes count. Topmost = highest slot (draw order).
        let sprite = byNode.get(hit.node)
        if (sprite && (best === null || sprite._slot > best._slot)) best = sprite
      }
      return best
    },
    pickRect(x, y, w, h) {
      if (scheduled) flush()
      BOX[0] = x
      BOX[1] = y
      BOX[2] = -1
      BOX[3] = x + w
      BOX[4] = y + h
      BOX[5] = 1
      let out: Sprite[] = []
      for (let node of spatial.overlap(BOX)) {
        let sprite = byNode.get(node)
        if (sprite) out.push(sprite)
      }
      return out
    },
    handlersFor(layout) {
      return dispatch(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      for (let sprite of byNode.values()) {
        sprite.layer = null
        declared.delete(sprite.node!)
        spatial.destroyNode(sprite.node!)
      }
      byNode.clear()
      for (let group of layer._groups) {
        group.layer = null
        declared.delete(group.node)
        spatial.destroyNode(group.node)
      }
      layer._groups.clear()
      // Let the core emit its final slot-zeroing writes while the pose
      // buffer still exists, then free everything.
      spatial.flush()
      destroyTexture(texture)
      destroyBuffer(pose)
      destroyBuffer(style)
      destroyBuffer(quad)
    },
    _add(opts) {
      if (disposed) throw new Error("addSprite: layer is disposed")
      let slot = freeSlots.pop() ?? highWater++
      if (slot >= gpuCapacity) grow(gpuCapacity * 2)
      let sprite: Sprite = {
        layer,
        node: null,
        _slot: slot,
        _x: opts?.x ?? 0,
        _y: opts?.y ?? 0,
        _w: opts?.w ?? 0,
        _h: opts?.h ?? 0,
        _rot: opts?.rotation ?? 0,
      }
      fillTransform(sprite._x, sprite._y, sprite._rot, sprite._w, sprite._h)
      let node = spatial.createNode(TRANSFORM, true)
      sprite.node = node
      if (opts?.parent) {
        if (opts.parent.layer !== layer) throw new Error("addSprite: parent group belongs to another layer")
        spatial.setParent(node, opts.parent.node)
      }
      spatial.setBounds(node, FLAT_BOUNDS)
      spatial.bindPoseRecord(node, pose, slot)
      byNode.set(node, sprite)
      writeStyle(slot, { frame: FULL_FRAME, tint: [1, 1, 1, 1], ...opts })
      layer._schedule()
      return sprite
    },
    _write(sprite, opts) {
      let moved = false
      if (opts.x !== undefined && opts.x !== sprite._x) (sprite._x = opts.x), (moved = true)
      if (opts.y !== undefined && opts.y !== sprite._y) (sprite._y = opts.y), (moved = true)
      if (opts.w !== undefined && opts.w !== sprite._w) (sprite._w = opts.w), (moved = true)
      if (opts.h !== undefined && opts.h !== sprite._h) (sprite._h = opts.h), (moved = true)
      if (opts.rotation !== undefined && opts.rotation !== sprite._rot) (sprite._rot = opts.rotation), (moved = true)
      if (moved) writeTransform(sprite)
      if (opts.frame !== undefined || opts.tint !== undefined) writeStyle(sprite._slot, opts)
      if (moved || styleDirty) layer._schedule()
    },
    _read(sprite) {
      let at = sprite._slot * STYLE_FLOATS
      return {
        x: sprite._x,
        y: sprite._y,
        w: sprite._w,
        h: sprite._h,
        frame: { u0: styleData[at]!, v0: styleData[at + 1]!, u1: styleData[at + 2]!, v1: styleData[at + 3]! },
        rotation: sprite._rot,
        tint: [styleData[at + 4]!, styleData[at + 5]!, styleData[at + 6]!, styleData[at + 7]!],
      }
    },
    _remove(sprite) {
      sprite.layer = null
      // Destroying the node zeroes its pose slot at the next core flush
      // (zero scale = nothing drawn); the slot then recycles.
      byNode.delete(sprite.node!)
      declared.delete(sprite.node!)
      spatial.destroyNode(sprite.node!)
      freeSlots.push(sprite._slot)
      layer._schedule()
    },
    _schedule() {
      if (disposed || scheduled) return
      scheduled = true
      RESOLVED.then(flush)
    },
    _groups: new Set(),
  }
  layer.handlers = dispatch(null)

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}

/**
 * Add a sprite. Its instance slot is fixed for its life: draw order is slot
 * order, and a removed sprite's slot recycles to the next add - so unlike
 * the record layer there is no painter's-insertion-order guarantee across
 * removals. Opaque-or-transparent pixel art (the overwhelming case) never
 * notices; z-ordered translucency is the sort-key backlog item. Past the
 * layer's reservation both instance buffers double (pose sinks move in one
 * core retarget); reserve with `capacity` to avoid the copies.
 */
export function addSprite(layer: SpriteLayer | RecordLayer, opts?: AddSpriteOptions): Sprite {
  return layer._add(opts)
}

/** The one write path: absent keys keep their values (the params rule). */
export function setSprite(sprite: Sprite, opts: SpriteOptions): void {
  sprite.layer?._write(sprite, opts)
}

/** Read a sprite's current fields (a fresh object; mutating it does nothing). */
export function getSprite(sprite: Sprite): Required<SpriteOptions> | null {
  return sprite.layer ? sprite.layer._read(sprite) : null
}

/**
 * Remove a sprite. The handle goes inert (layer null); further setSprite
 * calls are no-ops.
 */
export function removeSprite(sprite: Sprite): void {
  sprite.layer?._remove(sprite)
}

/**
 * Re-parent a sprite under a group (null = back to the layer root); its
 * pose fields then read in the new parent's frame, where the sprite keeps
 * them (it holds its local pose, not its world pose). Node layer only.
 */
export function setSpriteParent(sprite: Sprite, parent: SpriteGroup | null): void {
  let layer = sprite.layer
  if (!layer) return
  if (sprite.node === null) throw new Error("setSpriteParent: record layers have no groups")
  if (parent && parent.layer !== layer) throw new Error("setSpriteParent: group belongs to another layer")
  spatial.setParent(sprite.node, parent ? parent.node : null)
  layer._schedule()
}

/**
 * Declare (or with null clear) how the sprite's pose writes animate: once
 * set, setSprite writes are TARGETS the core animates toward - JS writes
 * once per target change, the core interpolates every frame, and a
 * settled sprite costs nothing. The spatial vocabulary: a spec per
 * component plus `all`, where `position` is x/y, `rotation` the sprite's
 * rotation (always the short arc) and `scale` its w/h; each spec is
 * `{ duration, bounce? }` (a spring, the retargeting-safe default) /
 * `{ duration, curve }` (a tween) / a shorthand string like
 * "300ms ease-out". Clearing cancels running tracks in place (the sprite
 * keeps its mid-flight pose) and later writes snap. Each natural settle
 * calls the sprite's `onTransitionEnd` with the component. Node layer
 * only.
 */
export function setSpriteTransition(sprite: Sprite, transition: NodeTransition | string | null): void {
  if (sprite.layer === null) return
  if (sprite.node === null) throw new Error("setSpriteTransition: record sprites have no node transitions")
  spatial.setTransition(sprite.node, transition)
  declare(sprite.node, sprite, transition)
}

/** The group counterpart of setSpriteTransition (`scale` is the group's
 * uniform scale). */
export function setGroupTransition(group: SpriteGroup, transition: NodeTransition | string | null): void {
  if (group.layer === null) return
  spatial.setTransition(group.node, transition)
  declare(group.node, group, transition)
}

/** Add a transform group (see SpriteGroup). */
export function addGroup(layer: SpriteLayer, opts?: GroupOptions): SpriteGroup {
  let group = {
    layer,
    _x: opts?.x ?? 0,
    _y: opts?.y ?? 0,
    _rot: opts?.rotation ?? 0,
    _scale: opts?.scale ?? 1,
  } as SpriteGroup
  writeGroupTransform(group)
  group.node = spatial.createNode(TRANSFORM, true)
  if (opts?.parent) {
    if (opts.parent.layer !== layer) throw new Error("addGroup: parent group belongs to another layer")
    spatial.setParent(group.node, opts.parent.node)
  }
  layer._groups.add(group)
  layer._schedule()
  return group
}

function writeGroupTransform(group: SpriteGroup): void {
  fillTransform(group._x, group._y, group._rot, group._scale, group._scale)
}

/** Update a group: absent keys keep values (the params rule). */
export function setGroup(group: SpriteGroup, opts: GroupOptions): void {
  let layer = group.layer
  if (!layer) return
  let moved = false
  if (opts.x !== undefined && opts.x !== group._x) (group._x = opts.x), (moved = true)
  if (opts.y !== undefined && opts.y !== group._y) (group._y = opts.y), (moved = true)
  if (opts.rotation !== undefined && opts.rotation !== group._rot) (group._rot = opts.rotation), (moved = true)
  if (opts.scale !== undefined && opts.scale !== group._scale) (group._scale = opts.scale), (moved = true)
  if (moved) {
    writeGroupTransform(group)
    spatial.writeTransform(group.node, TRANSFORM)
  }
  if (opts.parent !== undefined) {
    if (opts.parent && opts.parent.layer !== layer) throw new Error("setGroup: parent group belongs to another layer")
    spatial.setParent(group.node, opts.parent ? opts.parent.node : null)
    moved = true
  }
  if (moved) layer._schedule()
}

/**
 * Remove a group: its children (sprites and groups) become layer roots and
 * KEEP THEIR LOCAL POSE, so they jump to root frame unless the caller
 * removes or re-parents them too (a component tree unmounts children first
 * and never sees this). The handle goes inert.
 */
export function removeGroup(group: SpriteGroup): void {
  let layer = group.layer
  if (!layer) return
  group.layer = null
  layer._groups.delete(group)
  declared.delete(group.node)
  spatial.destroyNode(group.node)
  layer._schedule()
}
