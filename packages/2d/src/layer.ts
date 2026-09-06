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
// nodes moved); slot 1 is the style buffer [u0, v0, u1, v1, tint rgba,
// renderOrder], JS-owned and published through the zero-copy write lease. Never write
// the pose buffer from JS - the core's staging mirror is the owner and
// will overwrite. For motion only JS can compute at large populations, the
// records layer (records.ts) is the escape hatch.
//
// Sprites hold FIXED instance slots (freed slots recycle): draw order is
// slot order, so removal never shifts records and pose sinks never rebind.
// Layer space is pixels, top-left origin, y-down - the render tree's
// frame. The camera (offset, zoom, rotation about a pivot - CameraUpdate
// in camera.ts) is a shared-params write (uCamera + uCameraRot), never
// per-sprite; pointer dispatch undoes it with unprojectCamera.
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
import { checkCamera, unprojectCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"
import type { Frame } from "./frames.ts"
import { FULL_FRAME, writeFrame } from "./frames.ts"
import type { RecordLayer } from "./records.ts"
import { FRAGMENT, INSTANCE_ATTRIBUTES_SPLIT, VERTEX_SPLIT } from "./shaders.ts"

/** Floats per pose record (the core's Pose2D projection). */
export const POSE_FLOATS = 5

// Float offset of world y in a pose record - what `orderBy: "y"` keys on.
const POSE_Y_FIELD = 1
/** Floats per style record:
 * [u0, v0, u1, v1, tintR, tintG, tintB, tintA, renderOrder]. */
export const STYLE_FLOATS = 9

// Float offset of renderOrder in a style record - what `orderBy: "renderOrder"`
// keys on.
const STYLE_KEY_FIELD = 8

const RESOLVED = Promise.resolve()

// Shared marshalling scratch (the bindings copy synchronously).
const TRANSFORM = new Float32Array(10)
const FLAT_BOUNDS = new Float32Array([-0.5, -0.5, 0, 0.5, 0.5, 0])
const RAY_ORIGIN = new Float32Array(3)
const RAY_DIR = new Float32Array([0, 0, 1])
const BOX = new Float32Array([0, 0, 0, 0, 0, 1, 0, 0, 0, 1])

// Settle routing: the core's "spatialTransitionEnd" event carries the node
// id, so the handles with a transition DECLARED are indexed by node (only
// those can settle; adding a sprite costs nothing here) and one lazy
// subscription, started at the first declaration, routes to the handle's
// onTransitionEnd. Target-only, like the element transitions.
let declared = new Map<NodeId, Sprite | SpriteGroup>()
let subscribed = false

function declareTransition(node: NodeId, handle: Sprite | SpriteGroup, transition: NodeTransition | string | null): void {
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
  /** The owning layer, null after removeSprite (which owns the write). */
  readonly layer: SpriteLayer | RecordLayer | null
  /**
   * The sprite's SPATIAL ARENA node - the citizenship handle: bind core
   * producers to it or reach it through flux:spatial directly (the layer
   * still owns the node's life; destroy it only via removeSprite). Null
   * on a record layer's sprites.
   */
  readonly node: NodeId | null
  /** Instance slot: fixed for the sprite's life on the node layer, the
   * shifting draw-order index on a record layer. Readable (readonly, like
   * the other underscore fields: cheap reads without getSprite's
   * allocation); writes go through setSprite. */
  readonly _slot: number
  /** Pose mirror (node layer): what setSprite composes transforms from.
   * On a RECORD layer this mirror goes stale once records are written
   * raw - there the records array is the truth. */
  readonly _x: number
  readonly _y: number
  readonly _w: number
  readonly _h: number
  readonly _rot: number
  /** Mirror flags (both layer kinds): re-applied to every frame write.
   * The cheap way to read flip state. */
  readonly _flipX: boolean
  readonly _flipY: boolean
  /** Visibility mirror (node layer; always true on a record layer's
   * sprites). The cheap read; writes go through setSprite. */
  readonly _visible: boolean
  /** The enclosing group (null at the layer root, always null on a record
   * layer's sprites) - the bubble path; writes go through
   * setSpriteParent/addSprite's `parent`. */
  readonly _parent: SpriteGroup | null
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
  /** A declared transition (setSpriteTransition) settled naturally on
   * one component; a cancel or snap never fires. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
}

/** The layer-internal mutable view of a Sprite: the readonly on the
 * underscore fields is for the public surface; internal write sites
 * annotate with this (readonly does not affect assignability, so the two
 * types flow into each other freely). */
export type SpriteState = { -readonly [K in keyof Sprite]: Sprite[K] }

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
  /** Mirror the frame horizontally / vertically about the sprite's center.
   * A UV-side mirror: w/h stay the drawn size, a scale transition never
   * sees it, picking is unchanged. Default false. */
  flipX?: boolean
  flipY?: boolean
  /** Rotation about the center, radians, clockwise (y-down space). */
  rotation?: number
  /** RGBA multiplier 0..1 each; default opaque white (the texture as-is). */
  tint?: [number, number, number, number]
  /**
   * Explicit draw-order key, read only by a layer created with `orderBy:
   * "renderOrder"` (default 0; ties keep slot order, so untouched sprites draw
   * as without one). The raise idiom: `setSprite(hit, { renderOrder: ++top })`
   * on interaction, back to 0 to restore. Node layer only - a record
   * layer's 13-float record has no key field (order it by one of its own
   * fields with `orderBy: { field }`); setting this there throws.
   */
  renderOrder?: number
  /**
   * Show or hide the sprite (default true), @solidrt/3d's setVisible in
   * the setSprite bag: hidden, its pose slot zeroes (nothing drawn) and
   * pick/pickRect skip it; the handle, slot, style records and any
   * running transition state stay, so showing again restores the sprite
   * as it was. Node layer only - a record sprite has no node (hide it by
   * zeroing w or h); setting this there throws.
   */
  visible?: boolean
}

export type AddSpriteOptions = SpriteOptions & {
  /** Mount under this group (node layer only; null = the layer root, the
   * default); pose fields are then local to it. Reparent later with
   * setSpriteParent. */
  parent?: SpriteGroup | null
}

export type SpritePointerEvent = {
  /** The sprite the event is about (the topmost hit at the point, or the
   * captured sprite during a drag) - constant while the event bubbles. */
  sprite: Sprite
  /** The sprite or group whose handler is running; changes as the event
   * bubbles from the hit sprite through its enclosing groups. */
  currentTarget: Sprite | SpriteGroup
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
  /** Stops the bubble walk after the current handler. */
  stopPropagation(): void
}

export type SpriteHandlers = {
  onPointerDown: (event: ElementPointerEvent) => void
  onPointerMove: (event: ElementPointerEvent) => void
  onPointerUp: (event: ElementPointerEvent) => void
  onPointerLeave: (event: ElementPointerEvent) => void
}

/** Validate an [r, g, b, a] tint (throws - the dev validation policy).
 * Internal - every layer kind (sprite, record, tile) calls it. */
export function checkTint(verb: string, tint: [number, number, number, number]): void {
  if (!(Array.isArray(tint) && tint.length === 4 && tint.every(Number.isFinite))) {
    throw new Error(`${verb}: tint must be [r, g, b, a], got ${JSON.stringify(tint)}`)
  }
}

export type SpriteLayerOptions = {
  /**
   * Initial slot reservation; default 1024. The layer grows past it on
   * demand (doubling), so this is a hint that avoids regrowth copies, not a
   * limit.
   */
  capacity?: number
  clearColor?: [number, number, number, number]
  /** Layer tint, [r, g, b, a] in 0..1, multiplied over every sprite's own
   * tint; default opaque white (sprites as-is). See setTint. */
  tint?: [number, number, number, number]
  label?: string
  /**
   * Target texels per layer pixel (positive integer, default 1). The layer
   * renders at `oversample` times its size and is composited down, so a
   * fractional or HiDPI display scale resamples properly instead of snapping
   * (nearest) or smearing (linear); see setOversample. The components pick
   * it from the leaf's on-screen size - set it here when composing the
   * output yourself.
   */
  oversample?: number
  /** Skip the owner-scoped auto-dispose (see createSpriteLayer). */
  autoFree?: boolean
  /**
   * Draw sprites in KEY order instead of slot order, produced by core at
   * every publish (the gpu `instanceOrder` primitive across the pose/style
   * buffer pair - no per-sprite JS anywhere): `"y"` keys on the sprite's
   * WORLD y, the pose the core itself writes, so a perspective crowd
   * paints back to front (smaller y = further up the screen = drawn
   * first) - and because the key is core-owned, sprites moved by native
   * transitions or any other core producer re-sort with zero JS per
   * frame. Slots stay fixed - handles, picking and the style records are
   * untouched, only the draw order changes - and ties keep slot order, so
   * sprites at equal y draw exactly as without it.
   *
   * `"renderOrder"` keys on the app-owned per-sprite `renderOrder` field instead
   * (setSprite, default 0): explicit layering for painter-order scenes -
   * raise a dragged piece, click-to-front, hover emphasis - with stable
   * handles and no record churn.
   *
   * Known limitation for both keys: pick() resolves overlapping sprites by
   * slot order, not visual order, when a key is set. (The record layer's
   * RecordLayerOptions instead takes `"y"` or a raw `{ field }` offset
   * into its own records; pose records are core-owned, so the node layer
   * names its keys.)
   */
  orderBy?: "y" | "renderOrder"
}

/**
 * A transform group: a plain spatial arena node (position, rotation,
 * uniform scale - never a sprite size) that sprites and other groups
 * parent under, so a ship with turrets or a dragged stack moves as one
 * subtree recomputed in native code. Groups render nothing and cannot be
 * picked - sprites are always the leaves - but down/move/up events bubble
 * from a hit child sprite through its enclosing groups (enter/leave pair
 * on the sprite alone), so one group handler covers a whole assembly.
 */
export type SpriteGroup = {
  /** The owning layer, null after removeGroup (which owns the write). */
  readonly layer: SpriteLayer | null
  /** The group's spatial arena node. */
  readonly node: NodeId
  /** Pose mirror (readonly, like Sprite's underscore fields: cheap reads);
   * writes go through setGroup. */
  readonly _x: number
  readonly _y: number
  readonly _rot: number
  readonly _scale: number
  /** Visibility mirror; writes go through setGroup. */
  readonly _visible: boolean
  /** See Sprite._parent. */
  readonly _parent: SpriteGroup | null
  /** The handles parented here (removeGroup removes them with it);
   * internal -
   * membership writes go through addSprite/addGroup/setSpriteParent/
   * setGroup. */
  readonly _children: Set<SpriteState | GroupState>
  /** Bubbled from a hit child sprite; see SpritePointerEvent. A group
   * never receives enter/leave (hover pairs on the sprite alone). */
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  /** See Sprite.onTransitionEnd. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
}

/** The layer-internal mutable view of a SpriteGroup (see SpriteState). */
export type GroupState = { -readonly [K in keyof SpriteGroup]: SpriteGroup[K] }

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
  /** Show or hide the group's WHOLE subtree (default true); each child
   * keeps its own `visible`, so showing the group again restores the
   * subtree as it was. See SpriteOptions.visible. */
  visible?: boolean
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
  /** Target texels per layer pixel; see setOversample. */
  readonly oversample: number
  /**
   * Re-render at `n` target texels per layer pixel (positive integer): the
   * target resizes in place at its stable id, layer pixels, records, camera
   * and picking are untouched. Pick `n` as the ceiling of the device pixels
   * one layer pixel covers on screen (display scale times any designSize
   * fit or layout scaling), which the components do in onLayout.
   */
  setOversample(n: number): void
  setCamera(update: CameraUpdate): void
  /**
   * Tint the whole layer, [r, g, b, a] in 0..1: a uniform multiplied over
   * every sprite's own tint (day/night, a dimmed parallax plane, a
   * fade-in). One shared-params write - no record touches, cheap to
   * animate - and the same contract as TileLayer.setTint, so one signal
   * drives a whole scene across layer kinds.
   */
  setTint(tint: [number, number, number, number]): void
  /** Every shown sprite whose rotated rect contains the layer-pixel point
   * (a hidden sprite or subtree is never hit), topmost first - the
   * all-hits shape of @solidrt/3d's pick; `pick(x, y)[0]` is the topmost.
   * Topmost means draw order (highest slot on the node layer, last added
   * on a record layer); an `orderBy` key is not consulted (see
   * SpriteLayerOptions.orderBy's known limitation). */
  pick(x: number, y: number): Sprite[]
  /**
   * handlers for a leaf whose LAYOUT size differs from the layer size
   * (events scale by layer/layout; a leaf laid out AT layer size just uses
   * `handlers`). `layout` is read per event, so a resize-reactive layout
   * just works - @solidrt/3d's handlersFor, one dimension down.
   */
  handlersFor(layout: () => { width: number; height: number }): SpriteHandlers
  dispose(): void
  _add(opts?: AddSpriteOptions): Sprite
  _write(sprite: SpriteState, opts: SpriteOptions): void
  _read(sprite: Sprite): Required<SpriteOptions>
  _remove(sprite: SpriteState): void
  _schedule(): void
}

export type SpriteLayer = LayerBase & {
  /**
   * Every shown sprite whose rotated rect overlaps the layer-pixel rect
   * (the core BVH overlap query, exact for rotated sprites), unordered -
   * the marquee query. Node layer only.
   */
  pickRect(x: number, y: number, w: number, h: number): Sprite[]
  _groups: Set<GroupState>
}

/**
 * Pointer dispatch shared by both layer kinds: capture per pointer, hover
 * pairing, and the element bubbling rule one tree deeper - down/move/up
 * dispatch on the hit sprite and bubble through its enclosing groups
 * (stopPropagation stops the walk; a record layer has no groups, so the
 * walk ends at the sprite), enter/leave pair on the sprite alone, the
 * same model as @solidrt/3d's scene dispatch. Layout null = the leaf is
 * laid out at layer size, so localX/localY are layer pixels already (the
 * element hit test undid every ancestor transform - never getBoundingBox
 * here). Internal - layers expose the result as handlers/handlersFor.
 */
export function spriteDispatch(state: {
  size: () => [number, number]
  camera: () => CameraUpdate
  pick: (x: number, y: number) => Sprite[]
}): (layout: (() => { width: number; height: number }) | null) => SpriteHandlers {
  let capture = new Map<number, Sprite>()
  let hover = new Map<number, Sprite>()
  type BubbleName = "onPointerDown" | "onPointerMove" | "onPointerUp"
  type InternalEvent = SpritePointerEvent & { _stopped: boolean }
  let bubble = (name: BubbleName, event: InternalEvent): void => {
    for (let n: Sprite | SpriteGroup | null = event.sprite; n !== null && !event._stopped; n = n._parent) {
      let handler = n[name]
      if (handler) {
        event.currentTarget = n
        handler(event)
      }
    }
  }
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
      return unprojectCamera(state.camera(), x, y)
    }
    let makeEvent = (sprite: Sprite, x: number, y: number, e: ElementPointerEvent): InternalEvent => {
      let event: InternalEvent = {
        sprite,
        currentTarget: sprite,
        x,
        y,
        pointerId: e.pointerId,
        pointerType: e.pointerType,
        button: e.button,
        shiftKey: e.shiftKey,
        ctrlKey: e.ctrlKey,
        altKey: e.altKey,
        metaKey: e.metaKey,
        _stopped: false,
        stopPropagation() {
          event._stopped = true
        },
      }
      return event
    }
    return {
      onPointerDown(e) {
        let [x, y] = toLayer(e)
        let hit = state.pick(x, y)[0] ?? null
        if (!hit) return
        capture.set(e.pointerId, hit)
        bubble("onPointerDown", makeEvent(hit, x, y, e))
      },
      onPointerMove(e) {
        let [x, y] = toLayer(e)
        let captured = capture.get(e.pointerId)
        if (captured) {
          if (captured.layer) bubble("onPointerMove", makeEvent(captured, x, y, e))
          return
        }
        let hit = state.pick(x, y)[0] ?? null
        let prev = hover.get(e.pointerId) ?? null
        if (prev !== hit) {
          if (prev && prev.layer) prev.onPointerLeave?.(makeEvent(prev, x, y, e))
          if (hit) hit.onPointerEnter?.(makeEvent(hit, x, y, e))
          if (hit) hover.set(e.pointerId, hit)
          else hover.delete(e.pointerId)
        }
        if (hit) bubble("onPointerMove", makeEvent(hit, x, y, e))
      },
      onPointerUp(e) {
        let [x, y] = toLayer(e)
        let captured = capture.get(e.pointerId)
        if (captured) {
          capture.delete(e.pointerId)
          if (captured.layer) bubble("onPointerUp", makeEvent(captured, x, y, e))
          return
        }
        let hit = state.pick(x, y)[0] ?? null
        if (hit) bubble("onPointerUp", makeEvent(hit, x, y, e))
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

/** The stored UVs at `at` un-mirrored by the sprite's flags: the frame as
 * the caller gave it. Internal - records.ts reads through it too. */
export function readFrame(data: Float32Array, at: number, sprite: Sprite): Frame {
  let u0 = data[at]!, v0 = data[at + 1]!, u1 = data[at + 2]!, v1 = data[at + 3]!
  return sprite._flipX || sprite._flipY
    ? { u0: sprite._flipX ? u1 : u0, v0: sprite._flipY ? v1 : v0, u1: sprite._flipX ? u0 : u1, v1: sprite._flipY ? v0 : v1 }
    : { u0, v0, u1, v1 }
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
  let oversample = opts?.oversample ?? 1
  checkOversample("createSpriteLayer", oversample, width, height)
  let tint = opts?.tint ?? [1, 1, 1, 1]
  checkTint("createSpriteLayer", tint)
  let thrash = thrashSentinel(`sprite layer "${label}"`)
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
    width * oversample,
    height * oversample,
    { uViewport: [width, height], uCamera: [0, 0, 1, 1], uCameraRot: [1, 0, 0, 0], uTint: tint },
    {
      label,
      topology: "triangle-strip",
      vertexCount: 4,
      attributes: [{ name: "aPos", format: "vec2" }],
      buffer: quad,
      instanceAttributes: INSTANCE_ATTRIBUTES_SPLIT,
      instanceBuffers: [pose, style],
      // "y" keys on world y in the pose record (slot 0), "renderOrder" on the
      // app-owned key in the style record (slot 1); either way the core
      // gathers BOTH buffers under the one permutation at every publish
      // and republishes the sibling itself when the key buffer re-orders.
      instanceOrder:
        opts?.orderBy === "y"
          ? { field: POSE_Y_FIELD }
          : opts?.orderBy === "renderOrder"
            ? { field: STYLE_KEY_FIELD, slot: 1 }
            : undefined,
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
  let byNode = new Map<NodeId, SpriteState>()

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

  let writeStyle = (sprite: SpriteState, opts: SpriteOptions) => {
    let at = sprite._slot * STYLE_FLOATS
    let flipX = opts.flipX !== undefined && opts.flipX !== sprite._flipX
    let flipY = opts.flipY !== undefined && opts.flipY !== sprite._flipY
    if (flipX) sprite._flipX = !sprite._flipX
    if (flipY) sprite._flipY = !sprite._flipY
    if (opts.frame !== undefined) {
      let f = opts.frame
      writeFrame(styleData, at, f.u0, f.v0, f.u1, f.v1, sprite._flipX, sprite._flipY)
      styleDirty = true
    } else if (flipX || flipY) {
      // No new frame: toggle the changed axes on the stored UVs.
      writeFrame(styleData, at, styleData[at]!, styleData[at + 1]!, styleData[at + 2]!, styleData[at + 3]!, flipX, flipY)
      styleDirty = true
    }
    if (opts.tint !== undefined) {
      styleData[at + 4] = opts.tint[0]
      styleData[at + 5] = opts.tint[1]
      styleData[at + 6] = opts.tint[2]
      styleData[at + 7] = opts.tint[3]
      styleDirty = true
    }
    if (opts.renderOrder !== undefined) {
      styleData[at + STYLE_KEY_FIELD] = opts.renderOrder
      styleDirty = true
    }
  }

  let dispatch = spriteDispatch({
    size: () => [width, height],
    camera: () => ({ x: camX, y: camY, zoom: camZoom, rotation: camRot, pivotX: camPivotX, pivotY: camPivotY }),
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
      checkOversample("setSize", oversample, w, h)
      width = w
      height = h
      setTargetSize(texture, w * oversample, h * oversample)
      setTargetParams(texture, { uViewport: [w, h] })
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
    setTint(next) {
      if (disposed) return
      checkTint("setTint", next)
      setTargetParams(texture, { uTint: next })
    },
    pick(x, y) {
      // The index reads as of the last core flush; run any pending batch
      // first so a write followed by a pick sees the write.
      if (scheduled) flush()
      RAY_ORIGIN[0] = x
      RAY_ORIGIN[1] = y
      RAY_ORIGIN[2] = -1
      let out: Sprite[] = []
      for (let hit of spatial.raycast(RAY_ORIGIN, RAY_DIR)) {
        // The arena is shared (a 3d scene lives in the same index): only
        // this layer's nodes count.
        let sprite = byNode.get(hit.node)
        if (sprite) out.push(sprite)
      }
      // Topmost first: higher slot = drawn later = on top.
      return out.sort((a, b) => b._slot - a._slot)
    },
    pickRect(x, y, w, h) {
      if (scheduled) flush()
      // The marquee as a "box" volume: center, half extents, no rotation,
      // a unit deep so every sprite plane (z = 0) lies inside it.
      BOX[0] = x + w / 2
      BOX[1] = y + h / 2
      BOX[2] = 0
      BOX[3] = w / 2
      BOX[4] = h / 2
      BOX[5] = 1
      let out: Sprite[] = []
      for (let hit of spatial.overlap("box", BOX)) {
        let sprite = byNode.get(hit.node)
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
      let sprite: SpriteState = {
        layer,
        node: null,
        _slot: slot,
        _x: opts?.x ?? 0,
        _y: opts?.y ?? 0,
        _w: opts?.w ?? 0,
        _h: opts?.h ?? 0,
        _rot: opts?.rotation ?? 0,
        _flipX: false,
        _flipY: false,
        _visible: opts?.visible ?? true,
        _parent: opts?.parent ?? null,
      }
      fillTransform(sprite._x, sprite._y, sprite._rot, sprite._w, sprite._h)
      let node = spatial.createNode(TRANSFORM, sprite._visible)
      sprite.node = node
      if (opts?.parent) {
        if (opts.parent.layer !== layer) throw new Error("addSprite: parent group belongs to another layer")
        spatial.setParent(node, opts.parent.node)
        opts.parent._children.add(sprite)
      }
      spatial.setBounds(node, FLAT_BOUNDS)
      spatial.bindPoseRecord(node, pose, slot)
      byNode.set(node, sprite)
      // renderOrder defaults to 0 explicitly: a recycled slot holds the
      // previous occupant's key otherwise.
      writeStyle(sprite, { frame: FULL_FRAME, tint: [1, 1, 1, 1], renderOrder: 0, ...opts })
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
      if (opts.visible !== undefined && opts.visible !== sprite._visible) {
        sprite._visible = opts.visible
        spatial.setVisible(sprite.node!, opts.visible)
        moved = true
      }
      if (
        opts.frame !== undefined ||
        opts.tint !== undefined ||
        opts.flipX !== undefined ||
        opts.flipY !== undefined ||
        opts.renderOrder !== undefined
      ) {
        writeStyle(sprite, opts)
      }
      if (moved || styleDirty) layer._schedule()
    },
    _read(sprite) {
      let at = sprite._slot * STYLE_FLOATS
      return {
        x: sprite._x,
        y: sprite._y,
        w: sprite._w,
        h: sprite._h,
        frame: readFrame(styleData, at, sprite),
        flipX: sprite._flipX,
        flipY: sprite._flipY,
        rotation: sprite._rot,
        tint: [styleData[at + 4]!, styleData[at + 5]!, styleData[at + 6]!, styleData[at + 7]!],
        renderOrder: styleData[at + STYLE_KEY_FIELD]!,
        visible: sprite._visible,
      }
    },
    _remove(sprite) {
      sprite.layer = null
      if (sprite._parent) {
        sprite._parent._children.delete(sprite)
        sprite._parent = null
      }
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
 * notices; a scene that needs depth order sorts by world y with the
 * layer's `orderBy: "y"` (core-produced, zero JS per frame). Past the
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
  let s: SpriteState = sprite
  if (s._parent) s._parent._children.delete(s)
  s._parent = parent
  if (parent) parent._children.add(s)
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
  declareTransition(sprite.node, sprite, transition)
}

/** The group counterpart of setSpriteTransition (`scale` is the group's
 * uniform scale). */
export function setGroupTransition(group: SpriteGroup, transition: NodeTransition | string | null): void {
  if (group.layer === null) return
  spatial.setTransition(group.node, transition)
  declareTransition(group.node, group, transition)
}

/** Add a transform group (see SpriteGroup). */
export function addGroup(layer: SpriteLayer, opts?: GroupOptions): SpriteGroup {
  let group = {
    layer,
    _x: opts?.x ?? 0,
    _y: opts?.y ?? 0,
    _rot: opts?.rotation ?? 0,
    _scale: opts?.scale ?? 1,
    _visible: opts?.visible ?? true,
    _parent: opts?.parent ?? null,
    _children: new Set(),
  } as GroupState
  writeGroupTransform(group)
  group.node = spatial.createNode(TRANSFORM, group._visible)
  if (opts?.parent) {
    if (opts.parent.layer !== layer) throw new Error("addGroup: parent group belongs to another layer")
    spatial.setParent(group.node, opts.parent.node)
    opts.parent._children.add(group)
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
  let g: GroupState = group
  let moved = false
  if (opts.x !== undefined && opts.x !== g._x) (g._x = opts.x), (moved = true)
  if (opts.y !== undefined && opts.y !== g._y) (g._y = opts.y), (moved = true)
  if (opts.rotation !== undefined && opts.rotation !== g._rot) (g._rot = opts.rotation), (moved = true)
  if (opts.scale !== undefined && opts.scale !== g._scale) (g._scale = opts.scale), (moved = true)
  if (moved) {
    writeGroupTransform(group)
    spatial.writeTransform(group.node, TRANSFORM)
  }
  if (opts.visible !== undefined && opts.visible !== g._visible) {
    g._visible = opts.visible
    spatial.setVisible(group.node, opts.visible)
    moved = true
  }
  if (opts.parent !== undefined) {
    if (opts.parent && opts.parent.layer !== layer) throw new Error("setGroup: parent group belongs to another layer")
    spatial.setParent(group.node, opts.parent ? opts.parent.node : null)
    if (g._parent) g._parent._children.delete(g)
    g._parent = opts.parent
    if (opts.parent) opts.parent._children.add(g)
    moved = true
  }
  if (moved) layer._schedule()
}

/**
 * Remove a group AND everything under it: child sprites and groups are
 * removed with it - Unity's Destroy, Godot's free, the subtree form of
 * removeSprite. To keep a child, re-parent it out first (setSpriteParent /
 * setGroup's `parent`). Every removed handle goes inert. A component tree
 * unmounts children first and never sees the recursion. (@solidrt/3d's
 * `remove` DETACHES a re-addable subtree instead - its nodes exist outside
 * a scene; a sprite cannot exist outside its layer, so here remove means
 * destroy, exactly as it does for removeSprite.)
 */
export function removeGroup(group: SpriteGroup): void {
  let layer = group.layer
  if (!layer) return
  let g: GroupState = group
  // Children first, over a snapshot (each removal edits _children); the
  // set only groups carry tells the two handle kinds apart.
  for (let child of [...g._children]) {
    if ("_children" in child) removeGroup(child)
    else removeSprite(child)
  }
  g.layer = null
  if (g._parent) {
    g._parent._children.delete(g)
    g._parent = null
  }
  layer._groups.delete(group)
  declared.delete(group.node)
  spatial.destroyNode(group.node)
  layer._schedule()
}
