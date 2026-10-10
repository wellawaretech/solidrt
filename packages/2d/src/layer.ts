// The live sprite layer, re-founded on the spatial core: every sprite is a
// SPATIAL ARENA node (arena slot: local pose, parent, world matrix, index
// leaf, record sink - no layout, no paint, no rendertree element) whose
// Pose2D record sink writes [x, y, angle, sx, sy] into the sprite's slot of
// the pose instance buffer at the core's flush. Rendering is one
// instanced draw per VIEW (views.ts: the layer shows through its views
// only, each a target with a camera of its own); what changed is who owns
// the pose upstream of the instance buffer - the arena, so every core
// producer (native transitions, animation clips, physics) reaches sprites
// through `sprite.node`, and picking walks the core BVH instead of a JS
// loop.
//
// Two instance-buffer slots split ownership: slot 0 is the pose buffer,
// written ONLY by the core (one coalesced write per flush however many
// nodes moved); slot 1 is the style buffer [u0, v0, u1, v1, tint rgba,
// renderOrder, minScreenPx, maxScreenPx, atlas], JS-owned and published
// through the zero-copy write lease. Never write the pose buffer from JS -
// the core's staging mirror is the owner and will overwrite. For motion
// only JS can compute at large populations, the records layer (records.ts)
// is the escape hatch.
//
// A layer draws from the ATLASES it declares at creation, bound together
// as one draw: every frame carries its texture, the style record stores
// that texture's index in the list, and the generated fragment stage
// (shaders.ts) picks the sampler per instance - so sprites from several
// sheets interleave freely in one draw, key order included, and a frame
// from a sheet the layer did not declare throws at the write.
//
// Sprites hold FIXED instance slots (freed slots recycle): draw order is
// slot order, so removal never shifts records and pose sinks never rebind.
// Layer space is pixels, top-left origin, y-down - the render tree's
// frame. A view's camera (offset, zoom, rotation about a pivot -
// CameraUpdate in camera.ts) is a shared-params write (uCamera +
// uCameraRot) on that view's target, never per-sprite; pointer dispatch
// (dispatch.ts) undoes it with unprojectCamera.
import { getOwner, onBeforeRender, onCleanup, runWithOwner } from "@solidrt/core"
import type { PointerEvent as ElementPointerEvent, WheelEvent as ElementWheelEvent } from "@solidrt/core"
import { checkScreenSize, createBuffer, createRecordStream, destroyBuffer, screenSizeScale } from "@solidrt/core/gpu"
import type { BlendMode, BufferId, RecordStream, TextureId } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type {
  Impact as CoreImpact,
  MoveOptions as CoreMoveOptions,
  NodeEndpoint,
  NodeId,
  NodeMotionSpec,
  NodeTransition,
  NodeTransitionSpec,
  QueryFilter,
} from "flux:spatial"
import { on } from "sol:events"
import { checkAtlases, frameIndex } from "./atlas.ts"
import type { Atlas } from "./atlas.ts"
import type { CameraState, CameraUpdate } from "./camera.ts"
import type { Frame } from "./frames.ts"
import { fullFrame, isFrame, writeFrame } from "./frames.ts"
import type { RecordLayer } from "./records.ts"
import { floorReach, pointInSprite } from "./pick.ts"
import { createSpritePipeline, INSTANCE_LAYOUTS_SPLIT, VERTEX_SPLIT } from "./shaders.ts"
import { createViews } from "./views.ts"
import type { ViewHandle, ViewOptions } from "./views.ts"

/** Floats per pose record (the core's Pose2D projection). */
export const POSE_FLOATS = 5

// Float offset of world y in a pose record - what `orderBy: "y"` keys on.
const POSE_Y_FIELD = 1
/** Floats per style record:
 * [u0, v0, u1, v1, tintR, tintG, tintB, tintA, renderOrder, minScreenPx,
 * maxScreenPx, atlas, outlineR, outlineG, outlineB, outlineWidth]. */
export const STYLE_FLOATS = 16

// Float offset of renderOrder in a style record - what `orderBy: "renderOrder"`
// keys on.
const STYLE_KEY_FIELD = 8
// Float offsets of the screen-size clamp in a style record.
const STYLE_MIN_PX_FIELD = 9
const STYLE_MAX_PX_FIELD = 10
// Float offset of the outline (rgb plus width in world pixels) a
// distance-field atlas draws under the sprite; text runs write it, a
// colour atlas ignores it.
const STYLE_OUTLINE_FIELD = 12
// Float offset of the atlas sampler index (the frame's texture's position
// in the layer's atlas list).
const STYLE_ATLAS_FIELD = 11

const RESOLVED = Promise.resolve()

// Shared marshalling scratch (the bindings copy synchronously).
const TRANSFORM = new Float32Array(10)
// Half depth of a sprite's index box along z, node units (a sprite's z
// scale is 1, so world units too). A sprite is a COLUMN in the index, not
// a flat quad: a volume query at z = 0 then only ever meets its side
// faces, so every contact normal and depth is in the plane, and a circle
// whose center lies over a sprite pushes out sideways instead of along z
// (the core's contact against a flat triangle is the plane normal). Far
// larger than any volume's reach, so the end faces never take part.
const SPRITE_DEPTH = 1e4
const COLUMN_BOUNDS = new Float32Array([-0.5, -0.5, -SPRITE_DEPTH, 0.5, 0.5, SPRITE_DEPTH])
const RAY_ORIGIN = new Float32Array(3)
const RAY_DIR = new Float32Array(3)
// Volume scratch: a "capsule" is a, b, radius (7 floats), a "box" is
// center, half extents, quaternion (10); both packed at z = 0.
const CAPSULE = new Float32Array(7)
const BOX = new Float32Array(10)
const MOTION = new Float32Array(3)
// A box volume's half depth: any positive value inside the columns.
const BOX_HALF_DEPTH = 1
// The pick ray starts this far in front of the plane; it crosses every
// column at z = 0 whatever the start, but a start inside the column meets
// only its far end face, which is fine for pick (draw order sorts, not
// distance).
const PICK_RAY_START = -1
// The direction floors face in a y-down world.
const UP_2D: [number, number] = [0, -1]
// worldPosition's world-matrix scratch (column-major; translation at 12, 13).
const WORLD = new Float32Array(16)

// Settle routing: the core's "spatialTransitionEnd" event carries the node
// id, so the handles with a transition DECLARED are indexed by node (only
// those can settle; adding a sprite costs nothing here) and one lazy
// subscription, started at the first declaration, routes to the handle's
// onTransitionEnd. Target-only, like the element transitions.
let declared = new Map<NodeId, Sprite | SpriteGroup>()
let subscribed = false

// Free routing: a destroyed sprite or group whose declaration carries an
// `exit` stays in the core as a LEAVING node (drawn, picked by nothing)
// until its exits settle, and the core's "spatialNodeFreed" event says
// when it is gone - the cue to recycle what the layer kept for it (a
// sprite's pose slot). One lazy subscription, keyed by node.
let freeing = new Map<NodeId, () => void>()
let freeingSubscribed = false

function onFreed(node: NodeId, done: () => void): void {
  freeing.set(node, done)
  if (freeingSubscribed) return
  freeingSubscribed = true
  on("spatialNodeFreed", (event: { node: NodeId }) => {
    let done = freeing.get(event.node)
    if (!done) return
    freeing.delete(event.node)
    done()
  })
}

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
  /** The owning layer, null after destroySprite (which owns the write). */
  readonly layer: SpriteLayer | RecordLayer | null
  /**
   * The sprite's SPATIAL ARENA node - the citizenship handle: bind core
   * producers to it or reach it through flux:spatial directly (the layer
   * still owns the node's life; destroy it only via destroySprite). Null
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
  /** The pointer's other end: the system took it away (see core's
   * onPointerCancel); bubbles like up, and the press it ends never taps. */
  onPointerCancel?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
  /** A wheel notch over the sprite; bubbles like down/move/up. */
  onWheel?: (event: SpriteWheelEvent) => void
  /** A press that released on this sprite without dragging (see
   * SpriteTapEvent); bubbles like down/move/up. */
  onTap?: (event: SpriteTapEvent) => void
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
  /** Atlas frame (a texture plus normalized UVs, from grid/namedFrames/
   * fullFrame); default the whole first atlas of the layer. Its texture
   * must be one of the layer's `atlases`, else the write throws. */
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
   * The screen-size clamp's floor, view pixels (default 0: off). Whatever
   * the camera's zoom, the sprite draws at least this big on its smaller
   * axis, scaled uniformly so its aspect holds - a selection ring that
   * stays findable at the overview, a map pin, a traffic dot that never
   * falls under two device pixels. A floored sprite is world-sized until
   * zoom shrinks it to the floor, then holds there. Applied in the vertex
   * stage over the pose the core writes (core's screenSizeScale), so it
   * comes after group scale and a size transition, and rotation stays
   * world-space; `pick` honours it at the zoom it is asked for. A
   * collapsed sprite (zero w or h) stays collapsed. Finite, >= 0.
   */
  minScreenPx?: number
  /**
   * The clamp's ceiling, view pixels (default 0: off): the sprite draws
   * at most this big on its smaller axis however far the camera zooms in
   * - a label marker that must not balloon. Equal to `minScreenPx` it is
   * a constant screen size, Three's `sizeAttenuation: false`, and `w`/`h`
   * then carry the aspect alone. Never below a floor that is on.
   */
  maxScreenPx?: number
  /**
   * Explicit draw-order key, read only by a layer created with `orderBy:
   * "renderOrder"` (default 0; ties keep slot order, so untouched sprites draw
   * as without one). The raise idiom: `setSprite(hit, { renderOrder: ++top })`
   * on interaction, back to 0 to restore. Node layer only - a record
   * layer's 16-float record has no key field (order it by one of its own
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

/**
 * addSprite's bag: the sprite fields plus the mount point. The pool idiom
 * for a game: reserve `capacity`, add every sprite at mount with `visible:
 * false`, then show/hide - a hidden sprite keeps its slot, so draw order
 * stays stable and a spawn is one setSprite, never an add.
 */
export type AddSpriteOptions = SpriteOptions & {
  /** Mount under this group (node layer only; null = the layer root, the
   * default); pose fields are then local to it. Reparent later with
   * setSpriteParent. */
  parent?: SpriteGroup | null
}

/** What every layer pointer event carries, whichever handler sees it. */
export type LayerEventBase = {
  /** Pointer position in LAYER pixels (the camera mapping undone). */
  x: number
  y: number
  /** When the event happened, ms: the element event's `timeStamp`. */
  timeStamp: number
  /** The element event's `predicted`: a move the pointer was never
   * reported at. Draw with it, do not measure with it. */
  predicted: boolean
  pointerId: number
  pointerType: string
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  /**
   * The element event the layer's leaf received, in the leaf's own frame
   * (localX/localY, clientX/clientY, movementX/Y): what core's
   * recognizers consume, so a sprite drags itself through `createPan` and
   * a camera rides `createTransform` on the same events the walk carries.
   */
  native: ElementPointerEvent
  /**
   * Stops the walk after the current handler: no enclosing group and none
   * of the layer's listeners see the event. Stopping a DOWN claims the
   * whole press - that pointer's move, up and tap never reach the layer
   * either, so a sprite that drags itself stops its down once and a
   * camera attached at the root never pans it.
   */
  stopPropagation(): void
}

/** The event as a sprite's or group's handler sees it. */
export type SpritePointerEvent = LayerEventBase & {
  /** The sprite the event is about (the topmost hit at the point, or the
   * captured sprite during a drag) - constant while the event bubbles. */
  sprite: Sprite
  /** The sprite or group whose handler is running; changes as the event
   * bubbles from the hit sprite through its enclosing groups. */
  currentTarget: Sprite | SpriteGroup
}

/**
 * The event as the VIEW's listeners see it (ViewHandle.listen), the last
 * stop of the walk: `sprite` is the hit sprite it bubbled from, or null
 * over empty space, where the view is the only target.
 */
export type LayerPointerEvent = LayerEventBase & {
  sprite: Sprite | null
  currentTarget: ViewHandle
}

type WheelFields = {
  /** The wheel delta as the element event reports it. */
  deltaX: number
  deltaY: number
  native: ElementWheelEvent
}
type TapFields = {
  /** 1 for a tap, 2 for the second of a double tap (the same target,
   * within the repeat interval and distance), and so on - DOM's `detail`,
   * Unity's `clickCount`. */
  tapCount: number
}

export type SpriteWheelEvent = SpritePointerEvent & WheelFields
export type LayerWheelEvent = LayerPointerEvent & WheelFields
/**
 * A press that released on the target it pressed without travelling past
 * the slop, the only pointer down for its whole press (a pinch never taps).
 * Dispatched after the up, bubbling the same way; `x`/`y` are the release
 * point.
 */
export type SpriteTapEvent = SpritePointerEvent & TapFields
export type LayerTapEvent = LayerPointerEvent & TapFields

/** A listener at a view's root; see ViewHandle.listen. */
export type LayerPointerListener = {
  onPointerDown?: (event: LayerPointerEvent) => void
  onPointerMove?: (event: LayerPointerEvent) => void
  onPointerUp?: (event: LayerPointerEvent) => void
  onPointerCancel?: (event: LayerPointerEvent) => void
  onWheel?: (event: LayerWheelEvent) => void
  onTap?: (event: LayerTapEvent) => void
}

/** The element handlers for the leaf that shows a view (`ViewHandle.handlers`);
 * spread onto the host element. */
export type LayerHandlers = {
  onPointerDown: (event: ElementPointerEvent) => void
  onPointerMove: (event: ElementPointerEvent) => void
  onPointerUp: (event: ElementPointerEvent) => void
  onPointerCancel: (event: ElementPointerEvent) => void
  onPointerLeave: (event: ElementPointerEvent) => void
  onWheel: (event: ElementWheelEvent) => void
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
  /** Layer tint, [r, g, b, a] in 0..1, multiplied over every sprite's own
   * tint in every view; default opaque white (sprites as-is). See
   * setTint. */
  tint?: [number, number, number, number]
  /**
   * How the sprites blend into the layer's targets, core gpu's BlendMode;
   * default "alpha" (premultiplied source-over in draw order). "add" for
   * glows, explosions and additive particles, "multiply" to darken, "none"
   * to overwrite. Pipeline state, fixed at creation: an additive layer is
   * a second layer over the same atlas.
   */
  blend?: BlendMode
  /** Names the GPU resources (buffers, pipeline; views default to
   * `<label>-view`); default "sprites". */
  label?: string
  /** Stagger (ms) on the layer's root: every sprite or group added
   * straight under the layer (no group) that enters or leaves in one
   * frame is spaced by `index * stagger` - the whole-layer form of a
   * group's `stagger` (a group declaring its own wins for what is under
   * it). See setStagger. */
  stagger?: number
  /** The rate the layer's native motion runs at (setTimeScale at
   * creation): 0 freezes every sprite and group transition, 1 is app
   * time; a group declaring its own wins for what is under it. */
  timeScale?: number
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
  /** The owning layer, null after destroyGroup (which owns the write). */
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
  /** The handles parented here (destroyGroup removes them with it);
   * internal -
   * membership writes go through addSprite/addGroup/setSpriteParent/
   * setGroup. */
  readonly _children: Set<SpriteState | GroupState>
  /** Bubbled from a hit child sprite; see SpritePointerEvent. A group
   * never receives enter/leave (hover pairs on the sprite alone). */
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerCancel?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  onTap?: (event: SpriteTapEvent) => void
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
/**
 * What both layer kinds share: the sprites and what applies to all of
 * them. A layer has no output of its own - it shows through its views
 * (createView), each a target with a camera, a size and pointer dispatch
 * of its own (ViewHandle), as a Unity scene shows only through Cameras
 * and a Godot World2D only through Viewports.
 */
export type LayerBase = {
  /**
   * The atlases the layer draws from, as declared at creation: every
   * frame written to the layer names one of their textures, and a
   * record's `atlas` field is the texture's index in this list (what a
   * raw record writer stores). Bound together as one draw per view -
   * the multi-texture batch, so sprites from different sheets interleave
   * in draw order.
   */
  readonly atlases: readonly Atlas[]
  /** Live sprite count. */
  readonly count: number
  /**
   * Tint the whole layer, [r, g, b, a] in 0..1: a uniform multiplied over
   * every sprite's own tint (day/night, a dimmed parallax plane, a
   * fade-in) in every view. One shared-params write per view - no record
   * touches, cheap to animate - and the same contract as
   * TileLayer.setTint, so one signal drives a whole scene across layer
   * kinds.
   */
  setTint(tint: [number, number, number, number]): void
  /** Every shown sprite whose rotated rect contains the layer-pixel point
   * (a hidden sprite or subtree is never hit), topmost first - the
   * all-hits shape of @solidrt/3d's pick; `pick(x, y)[0]` is the topmost.
   * Topmost means draw order (highest slot on the node layer, last added
   * on a record layer); an `orderBy` key is not consulted (see
   * SpriteLayerOptions.orderBy's known limitation). World space: a view
   * undoes its camera before asking, and passes its camera's `zoom` so a
   * sprite's screen-size floor (SpriteOptions.minScreenPx) is hit at the
   * size it draws; default 1 (the unzoomed world). The pointer walk and
   * `view.pick` do both. */
  pick(x: number, y: number, zoom?: number): Sprite[]
  /**
   * A rendering of the layer's world from a camera of its own - the main
   * view, a minimap, a zoomed inset, one split-screen pane -
   * @solidrt/3d's scene.createView one dimension down. One more target
   * drawing the SAME instance buffers (no sprite is mirrored, no per-frame
   * JS beyond the view's camera writes), key order included; the layer's
   * tint fans out to it. The view carries the viewport contract (texture,
   * setSize, setOversample, setCamera, listen and handlers with sprite
   * events walking to the view as root); sprites stay the layer's. Views
   * die with the layer.
   */
  createView(opts: ViewOptions): ViewHandle
  dispose(): void
  _add(opts?: AddSpriteOptions): Sprite
  _write(sprite: SpriteState, opts: SpriteOptions): void
  _read(sprite: Sprite): Required<SpriteOptions>
  _destroy(sprite: SpriteState): void
  _schedule(): void
  /** The layer's JS-owned record stream, what records()/updateRecords
   * read and mark: a records layer's whole records, a node layer's style
   * records (its poses are the core's). Internal. */
  _stream: RecordStream
  /** The stream's mirror as floats, re-pointed by growth; records() hands
   * it out. Internal. */
  _records: Float32Array
}

/** A query volume for overlap/sweep/moveAndSlide, in layer pixels: a
 * circle, a capsule (the radius swept along the segment a-b: a character)
 * or a rect (`rotation` radians about its center, 0 when absent) - Godot's
 * CircleShape2D/CapsuleShape2D/RectangleShape2D, @solidrt/3d's Volume one
 * dimension down. */
export type Circle = { x: number; y: number; radius: number }
export type Capsule = { ax: number; ay: number; bx: number; by: number; radius: number }
export type Rect = { x: number; y: number; width: number; height: number; rotation?: number }
export type Volume = Circle | Capsule | Rect

/** Filters for one raycast/overlap/sweep/moveAndSlide query. */
export type QueryOptions = {
  /** Only these sprites report hits (an include-list); every shown sprite
   * of the layer otherwise. */
  sprites?: Sprite[]
}

/** One sprite a raycast() ray strikes: the distance along the ray, the
 * point on the sprite's edge and the unit edge normal facing the ray. */
export type Hit = { sprite: Sprite; distance: number; point: [number, number]; normal: [number, number] }

/** One sprite an overlap() volume touches: its deepest contact - the
 * point on the sprite's edge, the unit direction out of the sprite, and
 * the depth along that direction that clears the contact. */
export type Overlap = { sprite: Sprite; point: [number, number]; normal: [number, number]; depth: number }

/** One sprite a sweep() volume touches on its way: `time` is the fraction
 * of the motion at first touch (0 for a volume already in contact and
 * moving in), `point` the touch point on the sprite's edge, `normal` the
 * unit normal there facing the volume. */
export type Impact = { sprite: Sprite; time: number; point: [number, number]; normal: [number, number] }

export type MoveOptions = QueryOptions & {
  /** The direction floors face (default [0, -1]: y-down, floors face up
   * the screen). */
  up?: [number, number]
  /** Largest angle (radians) between a contact normal and `up` that still
   * counts as floor (default 45 degrees). Steeper contacts are walls, and
   * the body slides down them. */
  floorMaxAngle?: number
  /** Most contacts one call slides along (default 6). */
  maxSlides?: number
  /** Gap kept from every surface, layer pixels (default 0.01). */
  skin?: number
  /** How far below the body a floor is pulled to when the motion does not
   * rise (default 0.1; 0 disables): what keeps a walker on a ramp going
   * down, and what decides `floor` at the end of the move - a body that
   * ends higher than this above its floor is airborne, whatever it
   * touched on the way. A rising motion (a jump) never snaps. */
  floorSnap?: number
}

export type MoveResult = {
  /** The displacement the body gets: add it to the body's position. */
  motion: [number, number]
  /** The unit normal of the floor the body ends the move on - within
   * `floorSnap` below it, snapped onto - else null (airborne, or on a
   * slope too steep to stand on). With `floorSnap: 0` it is a floor met
   * during the move instead. */
  floor: [number, number] | null
  /** Whether a wall (a contact steeper than a floor and flatter than a
   * ceiling) or a ceiling was met. */
  wall: boolean
  ceiling: boolean
  /** Every contact met, in order, the floor snap's last. */
  hits: Impact[]
}

export type SpriteLayer = LayerBase & {
  /**
   * Every shown sprite whose rotated rect overlaps the layer-pixel rect
   * (the core BVH overlap query, exact for rotated sprites), unordered -
   * the marquee query: `overlap` over an unrotated rect, sprites only.
   * Node layer only. A world-space query like overlap and sweep: the
   * sprites' world rects, never a screen-size floor (that is `pick`'s).
   */
  pickRect(x: number, y: number, width: number, height: number): Sprite[]
  /**
   * Every shown sprite the ray from (x, y) along (dx, dy) strikes, nearest
   * first, with the distance (layer pixels along the normalized
   * direction), the point on the sprite's edge and the edge normal.
   * `pick` is the point form; this is the shot and the line of sight.
   * Reads the index as of the last flush, the pending batch run first,
   * like pick.
   */
  raycast(x: number, y: number, dx: number, dy: number, opts?: QueryOptions): Hit[]
  /**
   * Every shown sprite the volume touches, each with its deepest contact
   * (Godot's intersect_shape, Unity's OverlapCircle/Box/Capsule: a blast
   * radius, a pickup range, a melee arc), unordered. Sprites are tested
   * as their rotated rects, so any rotation holds; a volume wholly inside
   * a sprite touches it (a sprite is a solid rect to a volume, its
   * push-out the nearest edge's). Same index contract as raycast.
   */
  overlap(volume: Volume, opts?: QueryOptions): Overlap[]
  /**
   * The volume moved by (dx, dy): every shown sprite it touches on the
   * way, at its first touch, earliest first (Godot's cast_motion, Unity's
   * CircleCast/BoxCast: a bullet, a dash). A volume already in contact
   * reports time 0 while the motion closes in, and nothing while it leaves
   * or slides along the contact, which is what lets a slide along a wall
   * proceed. A zero motion touches nothing.
   */
  sweep(volume: Volume, dx: number, dy: number, opts?: QueryOptions): Impact[]
  /**
   * Move a body by (dx, dy) through the layer's sprites, sliding along
   * what it hits (Godot's CharacterBody2D.move_and_slide, the platformer
   * and top-down character mover), as one pure call: no sprite, no
   * velocity state - it takes a volume where the body IS and the motion
   * it WANTS, and returns the motion it gets plus what it touched. The
   * body first pushes out of anything it starts inside, then sweeps and
   * slides up to `maxSlides` times, then, unless the motion rises, snaps
   * down onto a floor within `floorSnap` - the floor it reports is the
   * one it ends on. The loop runs in the spatial core: one call per body
   * per frame. Gravity is the caller's: fold the fall into the motion
   * each frame and zero it while `floor` is set; a walkable floor absorbs
   * the vertical part, so a body never creeps down a slope it can stand
   * on. Colliders are the layer's shown sprites, or `opts.sprites`.
   */
  moveAndSlide(volume: Volume, dx: number, dy: number, opts?: MoveOptions): MoveResult
  _groups: Set<GroupState>
  /** Set (or with null clear) the stagger on the layer's root: the
   * spacing, in ms, of the enters and exits of the sprites and groups
   * straight under the layer that begin in one frame (SpriteLayerOptions
   * `stagger`). */
  setStagger(ms: number | null): void
  /** Set (or with null clear) the time scale on the layer's root: the
   * rate every sprite and group transition in the layer runs at, with
   * their delays, enters, exits and stagger slots - 0 a pause that leaves
   * the app's UI moving, 1 app time, other values slow or fast motion
   * (SpriteLayerOptions `timeScale`). The nearest declaring node wins:
   * a group or sprite with its own (setGroupTimeScale,
   * setSpriteTimeScale) keeps its rate inside a frozen layer. A frozen
   * layer asks for no frames; this write brings the one that resumes it.
   * Not covered: frame animations (createAnimation), which are JS clocks
   * on app time unless given this layer as their `clock`, and the app's
   * own onFrame logic, which multiplies its step by `timeRate()`. */
  setTimeScale(scale: number | null): void
  /** The rate the layer's root runs at (setTimeScale); 1 after dispose. */
  timeRate(): number
  /** The core node every sprite and group of the layer sits under: what
   * scopes the layer's queries in the arena shared with 3d scenes. */
  _root: NodeId
  _destroyGroup(group: GroupState): void
  /** Write a sprite's outline (rgb 0..1 and a width in world pixels)
   * for a distance-field atlas to draw under it; internal, the text
   * runs' write (text.ts). A colour atlas ignores the field. */
  _outline(sprite: Sprite, r: number, g: number, b: number, width: number): void
}

/** The stored UVs at `at` un-mirrored by the sprite's flags, over the
 * texture the record's atlas index resolved to: the frame as the caller
 * gave it. Internal - records.ts reads through it too. */
export function readFrame(data: Float32Array, at: number, sprite: Sprite, texture: TextureId): Frame {
  let u0 = data[at]!, v0 = data[at + 1]!, u1 = data[at + 2]!, v1 = data[at + 3]!
  return sprite._flipX || sprite._flipY
    ? { texture, u0: sprite._flipX ? u1 : u0, v0: sprite._flipY ? v1 : v0, u1: sprite._flipX ? u0 : u1, v1: sprite._flipY ? v0 : v1 }
    : { texture, u0, v0, u1, v1 }
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

// The core mover's options, refilled per call (the binding reads it
// synchronously); `up` is the 2d direction lifted into the plane.
const MOVE_OPTIONS: CoreMoveOptions & { up: [number, number, number] } = { up: [0, -1, 0] }

/** Pack a 2d volume into the core's scratch at z = 0: a circle is a
 * capsule with a == b, a rect a "box" turned about z. Returns the kind. */
function packVolume(volume: Volume, site: string): "capsule" | "box" {
  if ("width" in volume) {
    if (!(volume.width >= 0 && volume.height >= 0)) {
      throw new Error(site + ": width and height must be >= 0, got " + volume.width + " x " + volume.height)
    }
    let half = (volume.rotation ?? 0) / 2
    BOX[0] = volume.x + volume.width / 2
    BOX[1] = volume.y + volume.height / 2
    BOX[2] = 0
    BOX[3] = volume.width / 2
    BOX[4] = volume.height / 2
    BOX[5] = BOX_HALF_DEPTH
    BOX[6] = 0
    BOX[7] = 0
    BOX[8] = Math.sin(half)
    BOX[9] = Math.cos(half)
    return "box"
  }
  if (!(volume.radius >= 0)) throw new Error(site + ": radius must be >= 0, got " + volume.radius)
  if ("x" in volume) {
    CAPSULE[0] = volume.x
    CAPSULE[1] = volume.y
    CAPSULE[3] = volume.x
    CAPSULE[4] = volume.y
  } else {
    CAPSULE[0] = volume.ax
    CAPSULE[1] = volume.ay
    CAPSULE[3] = volume.bx
    CAPSULE[4] = volume.by
  }
  CAPSULE[2] = 0
  CAPSULE[5] = 0
  CAPSULE[6] = volume.radius
  return "capsule"
}

/** Compose and push a node-backed sprite's local transform - through the
 * node's transition declaration, so with one set (setSpriteTransition)
 * the write is a target the core animates toward. */
function writeTransform(sprite: Sprite): void {
  fillTransform(sprite._x, sprite._y, sprite._rot, sprite._w, sprite._h)
  spatial.writeTransform(sprite.node!, TRANSFORM)
}

/**
 * Create a sprite layer over its atlases: the list of sheets (createAtlas
 * records, or `{ texture, width, height }` literals) every frame written
 * to it must come from, bound together as ONE draw - up to
 * `limits.maxTextureUnits` of them (16 on every GLES 3.0 device), of any
 * sizes and sampler states, interleaving freely in draw order. It
 * renders nothing by itself: `layer.createView({ width, height })` (or a
 * `<SpriteLayer>` / `<View2d>`) is where it shows, once or many times.
 * Disposed automatically with the owning reactive scope (opt out with
 * `{ autoFree: false }`); the atlases are NOT owned - dispose them
 * yourself (they commonly outlive layers).
 */
export function createSpriteLayer(atlases: Atlas[], opts?: SpriteLayerOptions): SpriteLayer {
  let atlasIndex = checkAtlases("createSpriteLayer", atlases)
  let capacity = opts?.capacity ?? 1024
  if (!(capacity > 0 && Number.isInteger(capacity))) {
    throw new Error(`createSpriteLayer: capacity must be a positive integer, got ${capacity}`)
  }
  let label = opts?.label ?? "sprites"
  let tint = opts?.tint ?? [1, 1, 1, 1]
  checkTint("createSpriteLayer", tint)
  let pose: BufferId = createBuffer(capacity * POSE_FLOATS * 4, { label: `${label}-pose`, autoFree: false })
  // The style records: JS-written through the sprite verbs (writeStyle),
  // records()/updateRecords for a bulk restyle, published by the flush as
  // the dirty slot range, or the used prefix whole under an order (the
  // core gathers both buffers under the one permutation).
  let style = createRecordStream(STYLE_FLOATS * 4, capacity, { label: `${label}-style`, autoFree: false })
  let ordered = opts?.orderBy !== undefined
  let gpu = createSpritePipeline(label, VERTEX_SPLIT, INSTANCE_LAYOUTS_SPLIT, opts?.blend ?? "alpha", atlases)
  // The sprites with a screen-size clamp (either bound on), and the
  // furthest any of them reaches from its center at its floor (floorReach,
  // view pixels): the candidate box a pick searches around the pointer. A
  // write that could only widen the reach raises it in place; one that may
  // have shrunk it (a clamped sprite leaving, its floor or aspect dropping)
  // marks it dirty and the next pick recomputes over the set - exact, and
  // no cost while the clamps are untouched. Empty set: the raycast fast
  // path, and no exact test at all.
  let clamped = new Set<SpriteState>()
  let reach = 0
  let reachDirty = false
  let dropClamp = (sprite: SpriteState): void => {
    if (clamped.delete(sprite)) reachDirty = true
  }
  // The pair check against the STORED bounds, before any write lands: a
  // ceiling set alone must not drop below the floor already there.
  let checkClamp = (verb: string, sprite: SpriteState, opts: SpriteOptions): void => {
    if (opts.minScreenPx === undefined && opts.maxScreenPx === undefined) return
    let at = sprite._slot * STYLE_FLOATS
    checkScreenSize(verb, opts.minScreenPx ?? styleData[at + STYLE_MIN_PX_FIELD]!, opts.maxScreenPx ?? styleData[at + STYLE_MAX_PX_FIELD]!)
  }

  let disposed = false
  let scheduled = false
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

  // Slot allocation: freed slots recycle, the high-water mark is the
  // published instance count (a freed slot's pose zeroes - zero scale
  // collapses the instance - so holes draw nothing).
  let highWater = 0
  let freeSlots: number[] = []
  // Destroyed handles still leaving in the core (see onFreed): a sprite's
  // slot stays taken until the core says it is gone; dispose frees them.
  let leaving = new Set<NodeId>()
  let gpuCapacity = capacity
  let styleData = new Float32Array(style.bytes.buffer)
  let byNode = new Map<NodeId, SpriteState>()
  // The layer's root node (identity): parentless sprites and groups hang
  // off it, so a query scoped to it sees exactly this layer.
  fillTransform(0, 0, 0, 1, 1)
  let root = spatial.createNode(TRANSFORM, true)
  let filter: QueryFilter = { root }
  if (opts?.stagger !== undefined) spatial.setTransition(root, { stagger: opts.stagger })
  if (opts?.timeScale !== undefined) spatial.setTimeScale(root, opts.timeScale)
  // Refill the layer's one filter for a query: the root always, the
  // include-list as the sprites' nodes when given.
  let queryFilter = (opts: QueryOptions | undefined, site: string): void => {
    if (opts?.sprites === undefined) {
      filter.nodes = undefined
      return
    }
    let nodes: NodeId[] = []
    for (let sprite of opts.sprites) {
      if (sprite.layer !== layer) throw new Error(site + ": a sprite in `sprites` belongs to another layer")
      nodes.push(sprite.node!)
    }
    filter.nodes = nodes
  }
  let impactOf = (h: CoreImpact): Impact | null => {
    let sprite = byNode.get(h.node)
    if (!sprite) return null
    return { sprite, time: h.time, point: [h.point[0], h.point[1]], normal: [h.normal[0], h.normal[1]] }
  }

  let flush = () => {
    scheduled = false
    if (disposed) return
    if (style.dirty) style.publish(ordered, highWater)
    if (published !== highWater) {
      views.setCount(highWater)
      published = highWater
    }
    // The core recomputes moved subtrees and publishes every dirty pose
    // slot as one coalesced write per buffer.
    spatial.flush()
  }

  // Grow both instance buffers to `next` slots: the pose sinks move in one
  // retargetRecords call (the whole used range republishes at the next
  // flush), the style stream grows into a replacement marked whole, which
  // the next flush publishes. The entries hold the old buffers alive until
  // the swaps land, so the destroys are safe to issue right after.
  let grow = (next: number) => {
    let newPose = createBuffer(next * POSE_FLOATS * 4, { label: `${label}-pose`, autoFree: false })
    spatial.retargetRecords(pose, newPose)
    let oldStyle = style.grow(next)
    styleData = new Float32Array(style.bytes.buffer)
    views.setBuffers([newPose, style.buffer])
    destroyBuffer(pose)
    destroyBuffer(oldStyle)
    pose = newPose
    gpuCapacity = next
  }

  // A frame's atlas index, or a throw: run before ANY field of a write
  // lands (pose included), so a frame from an undeclared sheet leaves the
  // sprite as it was.
  let frameAtlas = (verb: string, opts: SpriteOptions): number => {
    if (opts.frame === undefined) return -1
    if (!isFrame(opts.frame)) throw new Error(`${verb}: not a frame, got ${JSON.stringify(opts.frame)}`)
    return frameIndex(verb, atlasIndex, opts.frame)
  }

  let writeStyle = (sprite: SpriteState, opts: SpriteOptions, atlas: number) => {
    let at = sprite._slot * STYLE_FLOATS
    let changed = false
    let flipX = opts.flipX !== undefined && opts.flipX !== sprite._flipX
    let flipY = opts.flipY !== undefined && opts.flipY !== sprite._flipY
    if (flipX) sprite._flipX = !sprite._flipX
    if (flipY) sprite._flipY = !sprite._flipY
    if (opts.frame !== undefined) {
      let f = opts.frame
      writeFrame(styleData, at, f.u0, f.v0, f.u1, f.v1, sprite._flipX, sprite._flipY)
      styleData[at + STYLE_ATLAS_FIELD] = atlas
      changed = true
    } else if (flipX || flipY) {
      // No new frame: toggle the changed axes on the stored UVs.
      writeFrame(styleData, at, styleData[at]!, styleData[at + 1]!, styleData[at + 2]!, styleData[at + 3]!, flipX, flipY)
      changed = true
    }
    if (opts.tint !== undefined) {
      styleData[at + 4] = opts.tint[0]
      styleData[at + 5] = opts.tint[1]
      styleData[at + 6] = opts.tint[2]
      styleData[at + 7] = opts.tint[3]
      changed = true
    }
    if (opts.renderOrder !== undefined) {
      styleData[at + STYLE_KEY_FIELD] = opts.renderOrder
      changed = true
    }
    if (opts.minScreenPx !== undefined) {
      styleData[at + STYLE_MIN_PX_FIELD] = opts.minScreenPx
      changed = true
    }
    if (opts.maxScreenPx !== undefined) {
      styleData[at + STYLE_MAX_PX_FIELD] = opts.maxScreenPx
      changed = true
    }
    if (changed) style.mark(sprite._slot, sprite._slot + 1)
    let min = styleData[at + STYLE_MIN_PX_FIELD]!
    if (min > 0 || styleData[at + STYLE_MAX_PX_FIELD]! > 0) {
      clamped.add(sprite)
      let r = floorReach(sprite._w, sprite._h, min)
      if (r >= reach) reach = r
      else reachDirty = true
    } else {
      dropClamp(sprite)
    }
  }

  let views = createViews({
    label,
    pipeline: gpu.pipeline,
    quad: gpu.quad,
    atlases: atlases.map(a => a.texture),
    buffers: () => [pose, style.buffer],
    count: () => published,
    tint: () => tint,
    pick: (x, y, zoom) => layer.pick(x, y, zoom),
    // The order key one view entry declares: "y" on world y in the pose
    // record (slot 0), "renderOrder" on the app-owned key in the style
    // record (slot 1); either way the core gathers BOTH buffers under the
    // one permutation at every publish and republishes the sibling itself
    // when the key buffer re-orders.
    order:
      opts?.orderBy === "y"
        ? { field: POSE_Y_FIELD }
        : opts?.orderBy === "renderOrder"
          ? { field: STYLE_KEY_FIELD, buffer: 2 }
          : undefined,
  })

  let layer: SpriteLayer = {
    atlases,
    get count() {
      return byNode.size
    },
    setTint(next) {
      if (disposed) return
      checkTint("setTint", next)
      tint = next
      views.setTint(next)
    },
    createView(vopts) {
      if (disposed) throw new Error("createView: layer is disposed")
      return views.create(vopts)
    },
    pick(x, y, zoom = 1) {
      // The index reads as of the last core flush; run any pending batch
      // first so a write followed by a pick sees the write.
      if (scheduled) flush()
      filter.nodes = undefined
      if (!(zoom > 0)) throw new Error(`pick: zoom must be positive, got ${zoom}`)
      let out: Sprite[] = []
      RAY_ORIGIN[0] = x
      RAY_ORIGIN[1] = y
      RAY_ORIGIN[2] = PICK_RAY_START
      RAY_DIR[0] = 0
      RAY_DIR[1] = 0
      RAY_DIR[2] = 1
      if (clamped.size === 0) {
        for (let hit of spatial.raycast(RAY_ORIGIN, RAY_DIR, filter)) {
          let sprite = byNode.get(hit.node)
          if (sprite) out.push(sprite)
        }
      } else {
        // A clamped sprite draws at another size than its index column:
        // larger under a floor, smaller under a ceiling. The candidates
        // are every column containing the point (the ray) plus, when a
        // floor is on, every column within the widest floor's reach of it
        // (a box overlap - a SURFACE contact in the core, so a box lying
        // inside a big column reports nothing, which is why the ray stays
        // in the union), each then tested exactly against its drawn rect:
        // the world pose from the node's world matrix (the in-flight pose,
        // group scale composed), scaled by its clamp at this zoom - what
        // the vertex stage draws.
        if (reachDirty) {
          reach = 0
          for (let s of clamped) reach = Math.max(reach, floorReach(s._w, s._h, styleData[s._slot * STYLE_FLOATS + STYLE_MIN_PX_FIELD]!))
          reachDirty = false
        }
        let candidates = new Set<NodeId>()
        for (let hit of spatial.raycast(RAY_ORIGIN, RAY_DIR, filter)) candidates.add(hit.node)
        if (reach > 0) {
          let half = reach / zoom
          packVolume({ x: x - half, y: y - half, width: 2 * half, height: 2 * half }, "pick")
          for (let hit of spatial.overlap("box", BOX, filter)) candidates.add(hit.node)
        }
        for (let node of candidates) {
          let sprite = byNode.get(node)
          if (!sprite) continue
          spatial.worldMatrix(sprite.node!, WORLD)
          let w = Math.hypot(WORLD[0]!, WORLD[1]!)
          let h = Math.hypot(WORLD[4]!, WORLD[5]!)
          let at = sprite._slot * STYLE_FLOATS
          let scale = screenSizeScale(w, h, zoom, styleData[at + STYLE_MIN_PX_FIELD]!, styleData[at + STYLE_MAX_PX_FIELD]!)
          if (pointInSprite(x, y, WORLD[12]!, WORLD[13]!, w * scale, h * scale, Math.atan2(WORLD[1]!, WORLD[0]!))) out.push(sprite)
        }
      }
      // Topmost first: higher slot = drawn later = on top.
      return out.sort((a, b) => b._slot - a._slot)
    },
    pickRect(x, y, width, height) {
      let out: Sprite[] = []
      for (let hit of layer.overlap({ x, y, width, height })) out.push(hit.sprite)
      return out
    },
    raycast(x, y, dx, dy, opts) {
      if (scheduled) flush()
      RAY_ORIGIN[0] = x
      RAY_ORIGIN[1] = y
      RAY_ORIGIN[2] = 0
      RAY_DIR[0] = dx
      RAY_DIR[1] = dy
      RAY_DIR[2] = 0
      queryFilter(opts, "raycast")
      let out: Hit[] = []
      for (let h of spatial.raycast(RAY_ORIGIN, RAY_DIR, filter)) {
        let sprite = byNode.get(h.node)
        if (sprite) out.push({ sprite, distance: h.distance, point: [h.point[0], h.point[1]], normal: [h.normal[0], h.normal[1]] })
      }
      return out
    },
    overlap(volume, opts) {
      if (scheduled) flush()
      let kind = packVolume(volume, "overlap")
      queryFilter(opts, "overlap")
      let out: Overlap[] = []
      for (let h of spatial.overlap(kind, kind === "box" ? BOX : CAPSULE, filter)) {
        let sprite = byNode.get(h.node)
        if (sprite) out.push({ sprite, point: [h.point[0], h.point[1]], normal: [h.normal[0], h.normal[1]], depth: h.depth })
      }
      return out
    },
    sweep(volume, dx, dy, opts) {
      if (scheduled) flush()
      let kind = packVolume(volume, "sweep")
      MOTION[0] = dx
      MOTION[1] = dy
      MOTION[2] = 0
      queryFilter(opts, "sweep")
      let out: Impact[] = []
      for (let h of spatial.sweep(kind, kind === "box" ? BOX : CAPSULE, MOTION, filter)) {
        let impact = impactOf(h)
        if (impact) out.push(impact)
      }
      return out
    },
    moveAndSlide(volume, dx, dy, opts) {
      if (scheduled) flush()
      let kind = packVolume(volume, "moveAndSlide")
      MOTION[0] = dx
      MOTION[1] = dy
      MOTION[2] = 0
      queryFilter(opts, "moveAndSlide")
      let up = opts?.up ?? UP_2D
      MOVE_OPTIONS.up[0] = up[0]
      MOVE_OPTIONS.up[1] = up[1]
      MOVE_OPTIONS.floorMaxAngle = opts?.floorMaxAngle
      MOVE_OPTIONS.maxSlides = opts?.maxSlides
      MOVE_OPTIONS.skin = opts?.skin
      MOVE_OPTIONS.floorSnap = opts?.floorSnap
      let r = spatial.moveAndSlide(kind, kind === "box" ? BOX : CAPSULE, MOTION, MOVE_OPTIONS, filter)
      let hits: Impact[] = []
      for (let h of r.hits) {
        let impact = impactOf(h)
        if (impact) hits.push(impact)
      }
      return {
        motion: [r.motion[0], r.motion[1]],
        floor: r.floor === null ? null : [r.floor[0], r.floor[1]],
        wall: r.wall,
        ceiling: r.ceiling,
        hits,
      }
    },
    dispose() {
      if (disposed) return
      disposed = true
      unhook()
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
      clamped.clear()
      // Corpses mid-exit go now: their slots must not outlive the buffer.
      for (let node of leaving) {
        freeing.delete(node)
        spatial.destroyNode(node)
      }
      leaving.clear()
      spatial.destroyNode(root)
      // Let the core emit its final slot-zeroing writes while the pose
      // buffer still exists, then free everything.
      spatial.flush()
      views.dispose()
      destroyBuffer(pose)
      style.destroy()
      gpu.dispose()
    },
    _add(opts) {
      if (disposed) throw new Error("addSprite: layer is disposed")
      // The style bag with its defaults - renderOrder and the clamp at 0
      // explicitly, since a recycled slot holds the previous occupant's
      // values otherwise - resolved before a slot or node is taken, so a
      // bad frame allocates nothing. An absent key and an explicit
      // undefined both take the default (the options convention
      // everywhere; a plain spread would let undefined win).
      let style: SpriteOptions = {
        ...opts,
        frame: opts?.frame ?? fullFrame(atlases[0]!),
        tint: opts?.tint ?? [1, 1, 1, 1],
        renderOrder: opts?.renderOrder ?? 0,
        minScreenPx: opts?.minScreenPx ?? 0,
        maxScreenPx: opts?.maxScreenPx ?? 0,
      }
      let atlas = frameAtlas("addSprite", style)
      if (opts?.parent && opts.parent.layer !== layer) throw new Error("addSprite: parent group belongs to another layer")
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
        spatial.setParent(node, opts.parent.node)
        opts.parent._children.add(sprite)
      } else {
        spatial.setParent(node, root)
      }
      spatial.setBounds(node, COLUMN_BOUNDS)
      spatial.bindPoseRecord(node, pose, slot)
      byNode.set(node, sprite)
      writeStyle(sprite, style, atlas)
      // No outline until a text run sets one: a recycled slot would
      // otherwise leak the previous occupant's.
      styleData.fill(0, slot * STYLE_FLOATS + STYLE_OUTLINE_FIELD, slot * STYLE_FLOATS + STYLE_OUTLINE_FIELD + 4)
      layer._schedule()
      return sprite
    },
    _outline(sprite, r, g, b, width) {
      let at = sprite._slot * STYLE_FLOATS + STYLE_OUTLINE_FIELD
      styleData[at] = r
      styleData[at + 1] = g
      styleData[at + 2] = b
      styleData[at + 3] = width
      style.mark(sprite._slot, sprite._slot + 1)
      layer._schedule()
    },
    _write(sprite, opts) {
      checkClamp("setSprite", sprite, opts)
      let atlas = frameAtlas("setSprite", opts)
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
        opts.renderOrder !== undefined ||
        opts.minScreenPx !== undefined ||
        opts.maxScreenPx !== undefined ||
        (moved && clamped.has(sprite))
      ) {
        writeStyle(sprite, opts, atlas)
      }
      if (moved || style.dirty) layer._schedule()
    },
    _read(sprite) {
      let at = sprite._slot * STYLE_FLOATS
      return {
        x: sprite._x,
        y: sprite._y,
        w: sprite._w,
        h: sprite._h,
        frame: readFrame(styleData, at, sprite, atlases[styleData[at + STYLE_ATLAS_FIELD]!]!.texture),
        flipX: sprite._flipX,
        flipY: sprite._flipY,
        rotation: sprite._rot,
        tint: [styleData[at + 4]!, styleData[at + 5]!, styleData[at + 6]!, styleData[at + 7]!],
        minScreenPx: styleData[at + STYLE_MIN_PX_FIELD]!,
        maxScreenPx: styleData[at + STYLE_MAX_PX_FIELD]!,
        renderOrder: styleData[at + STYLE_KEY_FIELD]!,
        visible: sprite._visible,
      }
    },
    _destroy(sprite) {
      sprite.layer = null
      dropClamp(sprite)
      if (sprite._parent) {
        sprite._parent._children.delete(sprite)
        sprite._parent = null
      }
      let node = sprite.node!
      byNode.delete(node)
      declared.delete(node)
      // A declared exit keeps the node leaving: its slot keeps drawing the
      // exit until the core frees it (the freed event lands after the
      // flush that zeroed the slot), then recycles. Otherwise the free is
      // immediate: the slot zeroes at the next core flush (zero scale =
      // nothing drawn) and recycles now.
      let slot = sprite._slot
      if (spatial.exitNode(node)) {
        leaving.add(node)
        onFreed(node, () => {
          leaving.delete(node)
          freeSlots.push(slot)
        })
      } else {
        freeSlots.push(slot)
      }
      layer._schedule()
    },
    _destroyGroup(group) {
      let node = group.node
      declared.delete(node)
      if (spatial.exitNode(node)) {
        leaving.add(node)
        onFreed(node, () => leaving.delete(node))
      }
      layer._schedule()
    },
    setStagger(ms) {
      if (disposed) return
      spatial.setTransition(root, ms === null ? null : { stagger: ms })
    },
    setTimeScale(scale) {
      if (disposed) return
      spatial.setTimeScale(root, scale)
    },
    timeRate() {
      return disposed ? 1 : spatial.timeRate(root)
    },
    _schedule() {
      if (disposed || scheduled) return
      scheduled = true
      RESOLVED.then(flush)
    },
    _groups: new Set(),
    _root: root,
    _stream: style,
    get _records() {
      return styleData
    },
  }
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
 * core retarget); reserve with `capacity` to avoid the copies. An option
 * left out or passed as undefined takes its default (zero pose and size,
 * the first atlas whole, an opaque white tint).
 */
export function addSprite(layer: SpriteLayer | RecordLayer, opts?: AddSpriteOptions): Sprite {
  if (opts) checkSpriteOptions("addSprite", opts)
  return layer._add(opts)
}

/** The one write path: absent keys keep their values (the params rule). */
export function setSprite(sprite: Sprite, opts: SpriteOptions): void {
  checkSpriteOptions("setSprite", opts)
  sprite.layer?._write(sprite, opts)
}

/** The validated sprite fields (throws - the dev validation policy),
 * checked before any state changes: a NaN tint channel or clamp bound
 * would otherwise blank the sprite with no error. The clamp pair is also
 * checked against the stored bounds by the layer's write. */
function checkSpriteOptions(verb: string, opts: SpriteOptions): void {
  if (opts.tint !== undefined) checkTint(verb, opts.tint)
  if (opts.minScreenPx !== undefined || opts.maxScreenPx !== undefined) checkScreenSize(verb, opts.minScreenPx ?? 0, opts.maxScreenPx ?? 0)
}

/** Read a sprite's current fields (a fresh object; mutating it does nothing). */
export function getSprite(sprite: Sprite): Required<SpriteOptions> | null {
  return sprite.layer ? sprite.layer._read(sprite) : null
}

/**
 * Destroy a sprite - PixiJS's `sprite.destroy()`, Unity's Destroy: the
 * handle goes inert (layer null; further setSprite calls are no-ops) and
 * the sprite is gone for good. With an `exit` in its transition
 * declaration it animates out first: each component with one plays to
 * its exit value and the sprite stays drawn meanwhile, a ghost that no
 * pick, raycast, overlap or sweep sees, then frees when the last exit
 * settles (no onTransitionEnd fires for an exit: the app already let go).
 * The one removal verb on a sprite: it cannot exist outside its layer, so
 * there is no detach; a sprite that should come back is hidden
 * (`visible: false`), Unity's SetActive.
 */
export function destroySprite(sprite: Sprite): void {
  sprite.layer?._destroy(sprite)
}

/**
 * A sprite's or group's position in LAYER pixels: its local x/y composed
 * through every enclosing group (@solidrt/3d's worldPosition, Three's
 * getWorldPosition), read from the core's world matrix as the tree stands
 * now, pending writes included. Under a transition this is the
 * mid-flight pose - what picking and the screen show - where the handle's
 * own fields hold the target. A record layer's sprite has no groups, so
 * its world position is its own, read as getSprite reads it (the records
 * array). A fresh pair per call; null once the handle is inert.
 */
export function worldPosition(target: Sprite | SpriteGroup): [number, number] | null {
  if (target.layer === null) return null
  // The set only groups carry tells the two handle kinds apart (as in
  // destroyGroup); a node-less sprite is a record layer's.
  if (!("_children" in target) && target.node === null) {
    let s = target.layer._read(target)
    return [s.x, s.y]
  }
  spatial.worldMatrix(target.node!, WORLD)
  return [WORLD[12]!, WORLD[13]!]
}

/**
 * Re-parent a sprite under a group (null = back to the layer root); its
 * pose fields then read in the new parent's frame, where the sprite keeps
 * them (it holds its local pose, not its world pose; worldPosition reads
 * the composed one). Node layer only.
 */
export function setSpriteParent(sprite: Sprite, parent: SpriteGroup | null): void {
  let layer = sprite.layer
  if (!layer) return
  if (sprite.node === null) throw new Error("setSpriteParent: record layers have no groups")
  if (parent && parent.layer !== layer) throw new Error("setSpriteParent: group belongs to another layer")
  // A sprite with a node is on a node layer (the throw above), which TS
  // cannot narrow through sprite.layer.
  spatial.setParent(sprite.node, parent ? parent.node : (layer as SpriteLayer)._root)
  let s: SpriteState = sprite
  if (s._parent) s._parent._children.delete(s)
  s._parent = parent
  if (parent) parent._children.add(s)
  layer._schedule()
}

/** A lifecycle endpoint in the object form: the value (2d units, as the
 * bare form) plus the motion that direction plays - a field left out is
 * the entry's; naming a `curve` or a `bounce` decides the kind outright,
 * so an ease-out enter pairs with an ease-in exit. */
export type SpriteEndpoint<Value> = {
  value: Value
  duration?: number
  curve?: Extract<NodeTransitionSpec, { curve: unknown }>["curve"]
  bounce?: number
  delay?: number
}

/** One spec of a sprite or group transition: the node vocabulary
 * (`{ duration, bounce? }` a spring, `{ duration, curve }` a tween, or a
 * shorthand string like "300ms ease-out 100ms"; `delay` in ms holds every
 * write that long first) with the lifecycle endpoints in 2d units, each a
 * bare value or a SpriteEndpoint: `from` is the component's enter value,
 * where a freshly added sprite starts before animating to its mount pose;
 * `exit` where it animates to when destroySprite/destroyGroup lets go of
 * it. */
export type SpriteTransitionSpec<Value> =
  | { duration: number; bounce?: number; delay?: number; from?: Value | SpriteEndpoint<Value>; exit?: Value | SpriteEndpoint<Value> }
  | {
      duration: number
      curve: Extract<NodeTransitionSpec, { curve: unknown }>["curve"]
      delay?: number
      from?: Value | SpriteEndpoint<Value>
      exit?: Value | SpriteEndpoint<Value>
    }
  | string

/** A sprite or group transition declaration: a spec per pose component
 * plus `all` as a catch-all. `position` is x/y (`from: [x, y]`),
 * `rotation` the rotation in radians (`from: angle`), `scale` a sprite's
 * w/h (`from: [w, h]`) or a group's uniform scale (`from: s`); `exit`
 * takes the same units. `stagger` (ms) goes on a GROUP's declaration and
 * spaces the enters and exits of the sprites and groups under it that
 * begin in one frame by `index * stagger` (add order for enters, tree
 * order for exits; the nearest declaring group wins); a sprite has no
 * children, so on a sprite it does nothing. */
export type SpriteTransition = {
  position?: SpriteTransitionSpec<[number, number]>
  rotation?: SpriteTransitionSpec<number>
  scale?: SpriteTransitionSpec<[number, number] | number>
  /** The catch-all: a motion alone, no `from`/`exit` (those name a
   * component's units, so they live on the per-component specs). */
  all?: NodeMotionSpec
  stagger?: number
}

/** The 2d declaration in the arena's own lanes: endpoint values lifted
 * into the plane (z = 0, a unit z scale, the rotation a quaternion about
 * z), bare or inside the endpoint object; everything else passes
 * through. */
function toNodeTransition(transition: SpriteTransition | string | null): NodeTransition | string | null {
  if (transition === null || typeof transition === "string") return transition
  let out: NodeTransition = {}
  if (transition.all !== undefined) out.all = transition.all
  if (transition.stagger !== undefined) out.stagger = transition.stagger
  if (transition.position !== undefined) out.position = liftSpec(transition.position, ([x, y]) => [x, y, 0])
  if (transition.rotation !== undefined) {
    out.rotation = liftSpec(transition.rotation, angle => [0, 0, Math.sin(angle / 2), Math.cos(angle / 2)])
  }
  if (transition.scale !== undefined) {
    out.scale = liftSpec(transition.scale, s => (typeof s === "number" ? [s, s, 1] : [s[0], s[1], 1]))
  }
  return out
}

function liftSpec<Value>(spec: SpriteTransitionSpec<Value>, lift: (value: Value) => number[]): NodeTransitionSpec {
  if (typeof spec === "string") return spec
  let { from, exit, ...rest } = spec
  let out: NodeTransitionSpec = rest
  if (from !== undefined) out.from = liftEndpoint(from, lift)
  if (exit !== undefined) out.exit = liftEndpoint(exit, lift)
  return out
}

function liftEndpoint<Value>(endpoint: Value | SpriteEndpoint<Value>, lift: (value: Value) => number[]): number[] | NodeEndpoint {
  // The object form is the one shape here with a `value` key; a bare
  // value is a number or a pair.
  if (typeof endpoint === "object" && endpoint !== null && !Array.isArray(endpoint)) {
    let { value, ...motion } = endpoint as SpriteEndpoint<Value>
    return { ...motion, value: lift(value) }
  }
  return lift(endpoint as Value)
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
 * "300ms ease-out". A `from` on a component is its enter value: a sprite
 * declared in the tick that added it starts there and animates to its
 * mount pose (once, at add; a later declaration animates writes only).
 * An `exit` is where it animates to when destroySprite lets go of it
 * (see there); either endpoint takes a SpriteEndpoint to own its
 * direction's motion, and `delay` holds writes. Clearing cancels running
 * tracks in place (the sprite keeps its mid-flight pose) and later writes
 * snap. Each natural settle calls the sprite's `onTransitionEnd` with the
 * component. Node layer only.
 */
export function setSpriteTransition(sprite: Sprite, transition: SpriteTransition | string | null): void {
  if (sprite.layer === null) return
  if (sprite.node === null) throw new Error("setSpriteTransition: record sprites have no node transitions")
  let node = toNodeTransition(transition)
  spatial.setTransition(sprite.node, node)
  declareTransition(sprite.node, sprite, node)
}

/** The group counterpart of setSpriteTransition (`scale` is the group's
 * uniform scale), and the home of `stagger`: a group declaring one spaces
 * the enters and exits of everything under it (see SpriteTransition). */
export function setGroupTransition(group: SpriteGroup, transition: SpriteTransition | string | null): void {
  if (group.layer === null) return
  let node = toNodeTransition(transition)
  spatial.setTransition(group.node, node)
  declareTransition(group.node, group, node)
}

/**
 * Declare (null clears) the rate the sprite's own native motion runs at
 * (its transitions, with their delays, enter and exit): 0 freezes it, 1
 * is app time, other values slow or fast motion. Overrides the layer's
 * and the enclosing groups' rate for this sprite alone (the nearest
 * declaring node wins); see SpriteLayer.setTimeScale. Node layer only.
 */
export function setSpriteTimeScale(sprite: Sprite, scale: number | null): void {
  if (sprite.layer === null) return
  if (sprite.node === null) throw new Error("setSpriteTimeScale: record sprites have no native motion")
  spatial.setTimeScale(sprite.node, scale)
}

/** The group counterpart of setSpriteTimeScale: the rate everything under
 * the group runs at, down to the next group or sprite declaring its own
 * - a frozen world with one live group inside it. */
export function setGroupTimeScale(group: SpriteGroup, scale: number | null): void {
  if (group.layer === null) return
  spatial.setTimeScale(group.node, scale)
}

/**
 * The rate a sprite's or group's motion runs at: its own time scale, else
 * the nearest declaring group's, else the layer's, else 1. What JS-driven
 * motion of the handle multiplies its own frame step by. Null for a
 * destroyed handle or a record layer's sprite (no native motion).
 */
export function timeRate(target: Sprite | SpriteGroup): number | null {
  if (target.layer === null || target.node === null) return null
  return spatial.timeRate(target.node)
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
  } else {
    spatial.setParent(group.node, layer._root)
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
    spatial.setParent(group.node, opts.parent ? opts.parent.node : layer._root)
    if (g._parent) g._parent._children.delete(g)
    g._parent = opts.parent
    if (opts.parent) opts.parent._children.add(g)
    moved = true
  }
  if (moved) layer._schedule()
}

/**
 * Destroy a group AND everything under it: child sprites and groups die
 * with it - Unity's Destroy, Godot's queue_free, the subtree form of
 * destroySprite (@solidrt/3d's `destroy` is the same verb one dimension
 * up). To keep a child, re-parent it out first (setSpriteParent /
 * setGroup's `parent`). Every destroyed handle goes inert. Exits play
 * through the whole subtree: children are let go of first, each
 * animating its own `exit`, and the group - its own exit or not - stays
 * in the core as their frame until the last of them has settled, then
 * frees. A component tree unmounts children first and never sees the
 * recursion.
 */
export function destroyGroup(group: SpriteGroup): void {
  let layer = group.layer
  if (!layer) return
  let g: GroupState = group
  // Children first, over a snapshot (each removal edits _children); the
  // set only groups carry tells the two handle kinds apart.
  for (let child of [...g._children]) {
    if ("_children" in child) destroyGroup(child)
    else destroySprite(child)
  }
  g.layer = null
  if (g._parent) {
    g._parent._children.delete(g)
    g._parent = null
  }
  layer._groups.delete(group)
  layer._destroyGroup(g)
}
