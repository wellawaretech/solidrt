// The Solid face: PascalCase components over context, syncing the retained
// layer (layer.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained records and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// sprite with `ref` and call setSprite from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the layer.
// The same split, with the same reasoning, as @solidrt/3d's components.
import { merge } from "@solidjs/signals"
import { createContext, createEffect, createSignal, displayScale, For, getBoundingBoxViewport, getLayoutBox, onCleanup, onFrame, onLayout, untrack, useContext, windowSize } from "@solidrt/core"
import type { Element, ParentComponent, TextureId, VoidComponent } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import type { FilterMode } from "@solidrt/core/gpu"
import { addGroup, addSprite, createSpriteLayer, removeGroup, removeSprite, setGroup, setGroupTransition, setSprite, setSpriteTransition } from "./layer.ts"
import type { NodeTransition } from "flux:spatial"
import type { CameraUpdate } from "./camera.ts"
import { createCamera2d } from "./camera2d.ts"
import type { Camera2d as Camera2dHandle, Camera2dOptions } from "./camera2d.ts"
import type { LayerPointerEvent, LayerTapEvent, LayerWheelEvent, Sprite as SpriteHandle, SpriteGroup, SpriteLayer as LayerHandle, SpriteOptions, SpritePointerEvent, SpriteTapEvent, SpriteWheelEvent, TransitionEndEvent } from "./layer.ts"
import { pickOversample, tileWorldScale } from "./oversample-math.ts"

// The window's device pixel count: the texel budget an auto-picked
// oversample target stays within (see fitOversample).
function windowTexels(): number {
  let win = windowSize()
  let scale = displayScale()
  return win.width * scale * (win.height * scale)
}

// The auto-pick apply: the decision itself (cap, shrink hysteresis,
// validation) is pure and lives in oversample-math.ts.
function applyOversample(
  layer: { readonly oversample: number; setOversample(n: number): void },
  scale: number,
  targetW: number,
  targetH: number,
  max: number | undefined,
): void {
  let n = pickOversample(layer.oversample, scale, targetW, targetH, windowTexels(), limits.maxTextureSize, max)
  if (n !== null) layer.setOversample(n)
}
import { createTileLayer } from "./tiles.ts"
import type { TileChunk, TileLayer as TileLayerHandle } from "./tiles.ts"

let LayerContext = createContext<LayerHandle>()
let GroupContext = createContext<SpriteGroup | null>(null)

type LayerCtx = { layer: LayerHandle; parent: SpriteGroup | null }

/**
 * The enclosing layer and parent group - the imperative escape hatch
 * inside a component subtree (throws outside a `<SpriteLayer>`), the same
 * shape as @solidrt/3d's useScene. `parent` is the enclosing `<Group>`'s
 * handle (null at the layer root); pass it on to addSprite/addGroup so
 * imperative sprites mount where the JSX sits.
 */
export function useSpriteLayer(): LayerCtx {
  return { layer: useContext(LayerContext), parent: useContext(GroupContext) }
}

/**
 * Sprite pointer events, the element vocabulary one tree deeper: the
 * topmost hit sprite receives the event, down/move/up/wheel/tap bubble to
 * enclosing Groups and end at the `<SpriteLayer>` (stopPropagation stops
 * the walk; stopping a down claims the whole press), enter/leave pair on
 * the sprite alone. Events flow while the element showing the layer
 * carries layer.handlers - the built-in `<SpriteLayer>` leaf does (opt
 * out with events={false}); an `output` leaf spreads them itself.
 */
export type SpritePointerProps = {
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  /** Sprites only: a Group never receives enter/leave. */
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  /** A press released on the sprite without dragging; `tapCount` counts
   * repeats (2 = double tap). */
  onTap?: (event: SpriteTapEvent) => void
}

/**
 * The layer's own pointer events, the root of the walk: every event
 * arrives after the hit sprite and its Groups (`event.sprite` set) or as
 * the only stop over empty space (`event.sprite` null), unless a handler
 * stopped it. Deselect on a miss, start a marquee, drive a camera - the
 * `<Camera2d>` child listens at the same root.
 */
export type LayerPointerProps = {
  onPointerDown?: (event: LayerPointerEvent) => void
  onPointerMove?: (event: LayerPointerEvent) => void
  onPointerUp?: (event: LayerPointerEvent) => void
  onWheel?: (event: LayerWheelEvent) => void
  onTap?: (event: LayerTapEvent) => void
}

export type SpriteLayerProps = LayerPointerProps & {
  /**
   * Layer pixels - give both, or neither. Omitted, the layer FILLS: the
   * built-in leaf is laid out at 100% of its parent's box (give it a sized
   * parent, as on the web) and the layer sizes to that box, so layer
   * pixels are the leaf's own coordinates and sprites place in element
   * units - the auto oversample still supplies display density on top.
   * Fill or fixed is decided at mount, and matches @solidrt/3d's `<Scene>`
   * one dimension down. `output` needs explicit sizes (the layer cannot
   * follow a leaf it does not own); the leaf's own width/height are then
   * layout, so render size and display size separate.
   */
  width?: number
  height?: number
  /** The atlas texture every sprite samples (create with createAtlas). */
  atlas: TextureId
  /** Initial record reservation (grows on demand); default 1024. */
  capacity?: number
  clearColor?: [number, number, number, number]
  /** Pan/zoom/rotate over the world (in-shader); a shared-params write,
   * never per-sprite. */
  camera?: CameraUpdate
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every sprite's own
   * tint (day/night, a dimmed parallax plane, a fade-in). One shared
   * uniform write, cheap to animate - unlike TileLayer's, which re-bakes.
   */
  tint?: [number, number, number, number]
  /**
   * Target texels per layer pixel. Absent, the component picks it every
   * layout from the built-in leaf's on-screen size (display scale, any
   * designSize fit, layout scaling), so the layer resamples properly at any
   * scale; with `output` there is no built-in leaf, so set it yourself.
   */
  oversample?: number
  /**
   * Cap on the auto-picked oversample (integer >= 1): the layer stays
   * adaptive up to it, never above - the lever that bounds target memory on
   * high-scale displays without giving up adaptivity. Ignored when
   * `oversample` is set (explicit already opts out of the pick).
   */
  maxOversample?: number
  /**
   * Draw sprites in key order instead of slot order, produced by core at
   * every publish: `"y"` orders by world y (back-to-front for a
   * perspective scene, zero JS per frame even under native transitions),
   * `"renderOrder"` by the per-sprite `renderOrder` field (explicit layering:
   * raise-on-drag, click-to-front). Creation-fixed (see
   * SpriteLayerOptions.orderBy).
   */
  orderBy?: "y" | "renderOrder"
  label?: string
  ref?: (layer: LayerHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the layer's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf. Sprite pointer events then need the layer's handlers on your
   * leaf: `<texture src={texture} {...useSpriteLayer().layer.handlers} />`.
   */
  output?: (texture: TextureId) => Element
  /**
   * Pointer events (default on): the built-in leaf carries layer.handlers,
   * so sprites, groups, the layer's own handlers and a `<Camera2d>` child
   * receive input. `false` detaches them - the leaf then costs no pointer
   * routing at all.
   */
  events?: boolean
}

/**
 * Owns a sprite layer and composites it as an ordinary `<texture>` leaf, so
 * the output takes layout, transforms, blendMode, and pointer events like
 * any element - or hand `output` the texture id and compose it yourself.
 * A layout component: it cannot sit inside a d-* subtree; `output` with a
 * `<d-texture>` is the detached form.
 * Children (`<Sprite>`) render nothing themselves - they populate the
 * retained layer through context.
 */
// Creation size of a fill-mode layer: the first onLayout replaces it
// before the first paint, so it only has to be a valid target size.
const FILL_INITIAL_SIZE = 1

export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  // Fill vs fixed layer is decided at mount, like `output`: both width
  // and height (fixed layer pixels) or neither (fill).
  let fill = untrack(() => {
    if ((props.width === undefined) !== (props.height === undefined)) {
      throw new Error("SpriteLayer: width and height come together - give both (fixed layer) or neither (fill)")
    }
    if (props.width === undefined && props.output) {
      throw new Error("SpriteLayer: fill needs the built-in leaf - with output, give width and height")
    }
    return props.width === undefined
  })
  let layer = untrack(() =>
    createSpriteLayer(props.width ?? FILL_INITIAL_SIZE, props.height ?? FILL_INITIAL_SIZE, props.atlas, {
      capacity: props.capacity,
      clearColor: props.clearColor,
      tint: props.tint,
      orderBy: props.orderBy,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => {
      if ((w === undefined) !== (h === undefined) || (w === undefined) !== fill) {
        throw new Error("SpriteLayer: fill/fixed is mount-fixed - width/height cannot appear or disappear")
      }
      if (!fill) layer.setSize(w!, h!)
    },
  )
  createEffect(
    () => props.camera,
    camera => {
      if (camera) layer.setCamera(camera)
    },
  )
  createEffect(
    () => props.oversample,
    n => {
      if (n !== undefined) layer.setOversample(n)
    },
  )
  createEffect(
    () => props.tint,
    tint => {
      if (tint !== undefined) layer.setTint(tint)
    },
  )
  untrack(() => props.ref)?.(layer)
  // The layer's own handlers at the root of the walk; the props are read
  // per event, so a handler prop may change without re-registering.
  onCleanup(
    layer.listen({
      onPointerDown: e => props.onPointerDown?.(e),
      onPointerMove: e => props.onPointerMove?.(e),
      onPointerUp: e => props.onPointerUp?.(e),
      onWheel: e => props.onWheel?.(e),
      onTap: e => props.onTap?.(e),
    }),
  )
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  let leaf: { id: number } | undefined
  // The layer's pixel size: the props in fixed mode, the built-in leaf's
  // laid-out box in fill mode (getLayoutBox, the untransformed read, so a
  // designSize fit or ancestor transform never enters the world space -
  // display density rides on the oversample pick below). Null before the
  // first layout.
  let layerSize = (): { width: number; height: number } | null => {
    if (!fill) return { width: props.width!, height: props.height! }
    let box = leaf && getLayoutBox(leaf)
    if (!box || box.width <= 0 || box.height <= 0) return null
    return { width: Math.max(1, Math.round(box.width)), height: Math.max(1, Math.round(box.height)) }
  }
  // Fill: the layer follows the leaf's box. Registered before the pick, so
  // one onLayout applies size then oversample in order; setSize no-ops
  // when nothing changed.
  if (fill) {
    onLayout(() => {
      let size = layerSize()
      if (size) layer.setSize(size.width, size.height)
    })
  }
  // Auto oversample: the built-in leaf's window box, in device pixels, per
  // layer pixel. Picked after every layout, and again when the display scale
  // changes: the first layout runs before the resize event that reports the
  // scale, and a scale change alone lays nothing out. pick runs as an
  // onLayout handler and as an effect apply, both untracked scopes, so its
  // prop reads are wrapped in an explicit untrack.
  let pick = () =>
    untrack(() => {
      if (!leaf || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(leaf)
      let size = layerSize()
      if (!box || !size) return
      let scale = displayScale() * Math.max(box.width / size.width, box.height / size.height)
      applyOversample(layer, scale, size.width, size.height, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  // Sprite events on the built-in leaf: at layer size the plain handlers,
  // in fill mode scaled from the laid-out box (the box can be fractional,
  // the layer size never is).
  let layerHandlers = fill
    ? layer.handlersFor(() => (leaf && getLayoutBox(leaf)) || { width: 0, height: 0 })
    : layer.handlers
  return (
    <LayerContext value={layer}>
      {output ? (
        untrack(() => output(layer.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leaf = n)}
          src={layer.texture}
          width={fill ? "100%" : props.width}
          height={fill ? "100%" : props.height}
          onPointerDown={events ? layerHandlers.onPointerDown : undefined}
          onPointerMove={events ? layerHandlers.onPointerMove : undefined}
          onPointerUp={events ? layerHandlers.onPointerUp : undefined}
          onPointerLeave={events ? layerHandlers.onPointerLeave : undefined}
          onWheel={events ? layerHandlers.onWheel : undefined}
        />
      )}
      {props.children}
    </LayerContext>
  )
}

export type GroupProps = {
  /** Position in the parent frame (layer pixels at the root). */
  x?: number
  y?: number
  /** Rotation, radians, clockwise. */
  rotation?: number
  /** Uniform scale on the whole subtree (child sprites scale with it). */
  scale?: number
  /** Show or hide the whole subtree (default true); see
   * GroupOptions.visible. */
  visible?: boolean
  /** Bubbled from a hit child sprite (see SpritePointerProps); a group
   * never receives enter/leave. */
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  onTap?: (event: SpriteTapEvent) => void
  /** How pose-prop changes animate (see setGroupTransition); the mount
   * pose always snaps. */
  transition?: NodeTransition | string | null
  /** A declared transition settled on one component. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  ref?: (group: SpriteGroup) => void
}

/**
 * A transform group: `<Sprite>` (and nested `<Group>`) children mount under
 * its spatial arena node, so their pose props read in the group's frame and
 * moving the group moves the subtree in one native recompute - a ship with
 * turrets is one `<Group>` with the hull and turret sprites inside. Renders
 * nothing itself.
 */
export let Group: ParentComponent<GroupProps> = props => {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let group = untrack(() => addGroup(layer, { parent }))
  createEffect(
    () => [props.x, props.y, props.rotation, props.scale, props.visible] as const,
    ([x, y, rotation, scale, visible]) => setGroup(group, { x, y, rotation, scale, visible: visible !== false }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setGroupTransition(group, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, wheel, tap, end]) => {
      group.onPointerDown = down
      group.onPointerMove = move
      group.onPointerUp = up
      group.onWheel = wheel
      group.onTap = tap
      group.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(group)
  onCleanup(() => removeGroup(group))
  return <GroupContext value={group}>{props.children}</GroupContext>
}

export type SpriteProps = SpriteOptions &
  SpritePointerProps & {
    /** How pose-prop changes animate (see setSpriteTransition); the mount
     * pose always snaps. */
    transition?: NodeTransition | string | null
    /** A declared transition settled on one component. */
    onTransitionEnd?: (event: TransitionEndEvent) => void
    ref?: (sprite: SpriteHandle) => void
  }

/** One sprite: a frame drawn at a position (in the enclosing `<Group>`'s
 * frame when there is one). */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let sprite = untrack(() => addSprite(layer, { parent }))
  createEffect(
    () => [props.x, props.y, props.w, props.h, props.frame, props.flipX, props.flipY, props.rotation, props.tint, props.renderOrder, props.visible] as const,
    ([x, y, w, h, frame, flipX, flipY, rotation, tint, renderOrder, visible]) =>
      setSprite(sprite, { x, y, w, h, frame, flipX, flipY, rotation, tint, renderOrder, visible: visible !== false }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setSpriteTransition(sprite, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerEnter, props.onPointerLeave, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, enter, leave, wheel, tap, end]) => {
      sprite.onPointerDown = down
      sprite.onPointerMove = move
      sprite.onPointerUp = up
      sprite.onPointerEnter = enter
      sprite.onPointerLeave = leave
      sprite.onWheel = wheel
      sprite.onTap = tap
      sprite.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(sprite)
  onCleanup(() => removeSprite(sprite))
  return null
}

export type Camera2dProps = Omit<Camera2dOptions, "viewport"> & {
  /** Viewport in layer pixels; defaults to the enclosing layer's own size
   * (live: a fill layer's box, a setSize). */
  viewport?: () => { w: number; h: number }
  ref?: (camera: Camera2dHandle) => void
}

// Cap on the camera loop's per-frame dt in seconds, so the first tick
// after a suspended stretch (a resumed app, a reload) cannot leap a glide.
const MAX_CAMERA_DT = 0.1

/**
 * createCamera2d as a SpriteLayer child: drives the enclosing layer's
 * camera and takes its input from the layer's root through context - no
 * ref plumbing, no handler spreads, no onFrame of your own. A sprite that
 * claims its press (stopPropagation on its down) keeps the camera out of
 * that drag; everything else pans, pinches and wheels. The options are
 * read at mount (the motion reads them once): change the pose at runtime
 * through `ref`'s set/glideTo/fit/follow, and remount (a keyed `<Show>`)
 * for new bounds. `viewport` defaults to the layer's own size. Frames run
 * only while the camera moves (`active()`), so a resting camera leaves
 * the app demand-driven idle.
 */
export let Camera2d: VoidComponent<Camera2dProps> = props => {
  let layer = useContext(LayerContext)
  // Through merge, not a spread: a props object hands out getters, and
  // merge keeps them (the motion reads each once at creation, viewport
  // live).
  let options: Camera2dOptions = merge(props, {
    get viewport() {
      return props.viewport ?? (() => ({ w: layer.width, h: layer.height }))
    },
  })
  let cam = untrack(() => createCamera2d(layer, options))
  onCleanup(cam.attach(layer))
  createEffect(
    () => cam.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_CAMERA_DT)
        last = now
        cam.update(dt)
      })
    },
  )
  untrack(() => props.ref)?.(cam)
  return null
}

/**
 * The tile layer's camera - the same CameraUpdate vocabulary as the sprite
 * layer (see camera.ts for the pivot/rotation semantics and the
 * heading-upward convention), so one signal drives a whole rotating scene
 * across both layers. Here the camera is a transform on the composited
 * world view, never a re-bake; projectCamera/unprojectCamera are the same
 * mapping as plain functions.
 */
export type TileCamera = CameraUpdate

export type TileLayerProps = {
  /** Grid shape and tile pixel size - creation-fixed (recreate to resize). */
  cols: number
  rows: number
  tileW: number
  tileH: number
  /** The atlas texture every tile samples (create with createAtlas). */
  atlas: TextureId
  /** Per-chunk clear color (the name says the scope: never-written regions
   * have no chunk and render nothing, so a full-bleed ground color belongs
   * on the container behind the layer). */
  chunkClearColor?: [number, number, number, number]
  /** Sampler filter for the baked chunk textures at composite time; default
   * "linear" (hard pixels belong to the atlas sampler, see TileLayerOptions). */
  filter?: FilterMode
  /**
   * Target texels per world pixel in the baked chunks. Absent, the component
   * picks it every layout from the world view's on-screen size (display
   * scale, camera zoom, any designSize fit), so tiles resample properly at
   * any scale.
   */
  oversample?: number
  /**
   * Cap on the auto-picked oversample (integer >= 1): tiles stay adaptive
   * up to it, never above. A tile world's texture memory is resident chunks
   * x n squared, so this is the lever that bounds it on high-scale displays
   * without giving up adaptivity. Ignored when `oversample` is set.
   */
  maxOversample?: number
  /** Chunk edge in tiles (default ~512px worth); see TileLayerOptions. */
  chunkTiles?: number
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every cell's own
   * tint (day/night, a dimmed parallax plane). A change re-renders every
   * resident chunk GPU-side (no record uploads) - drive it from slow
   * state, not per frame.
   */
  tint?: [number, number, number, number]
  /**
   * Pan/zoom/rotate over the world - a transform on the composited world
   * view, never a re-bake. The world view is WORLD sized; put the layer
   * inside a clipping container (`overflow="clip"`) sized to the viewport.
   */
  camera?: TileCamera
  label?: string
  ref?: (layer: TileLayerHandle) => void
}

/**
 * Owns a baked tile layer (createTileLayer) and composites its chunks as
 * `d-texture` leaves at their world rects inside a `<view>` carrying the
 * camera transform - a handful of quads however many tiles exist. A layout
 * component: the world view is laid out, so it cannot sit inside a d-*
 * subtree. Tiles
 * are data, not children: write them through `ref` with `setTile` - there
 * is no `<Tile>` component on purpose (a component per tile would
 * re-introduce the per-element cost the bake removes).
 */
export let TileLayer: VoidComponent<TileLayerProps> = props => {
  let layer = untrack(() =>
    createTileLayer(props.cols, props.rows, props.tileW, props.tileH, props.atlas, {
      chunkClearColor: props.chunkClearColor,
      filter: props.filter,
      chunkTiles: props.chunkTiles,
      tint: props.tint,
      label: props.label,
    }),
  )
  untrack(() => props.ref)?.(layer)
  createEffect(
    () => props.oversample,
    n => {
      if (n !== undefined) layer.setOversample(n)
    },
  )
  createEffect(
    () => props.tint,
    tint => {
      if (tint !== undefined) layer.setTint(tint)
    },
  )
  // Auto oversample: the world view's window box, in device pixels, per
  // world pixel. Rotation is not a resolution factor - texels per world
  // pixel do not change as the camera turns - but the measured box is the
  // AABB of the ROTATED view, which swells by up to sqrt(2) and under an
  // animated rotation would sweep the scale across an integer boundary,
  // re-baking every resident chunk on each flip. So the swell is divided
  // back out for the known camera rotation, leaving displayScale, the
  // ancestor fit and the camera zoom. The divide-out's basis is the view's
  // laid-out box, which flexShrink 0 below pins to layer.width x
  // layer.height: a flex container would otherwise compress the box (the
  // chunks are detached and draw at world coordinates either way) and the
  // measurement would mix layout compression into the scale. pick runs
  // untracked (onLayout handler / effect apply), so its prop reads are
  // wrapped explicitly.
  let world: { id: number } | undefined
  let pick = () =>
    untrack(() => {
      if (!world || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(world)
      if (!box) return
      let r = props.camera?.rotation ?? 0
      let scale = displayScale() * tileWorldScale(box.width, box.height, layer.width, layer.height, r)
      applyOversample(layer, scale, layer.chunkW, layer.chunkH, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  // Chunk allocations arrive through the layer's hook; the signal carries a
  // fresh array so <For> sees the growth.
  let [chunks, setChunks] = createSignal<TileChunk[]>(layer.chunks.slice())
  layer.onChunk = () => setChunks(layer.chunks.slice())
  // World -> screen: p maps to pivot + R(rotation) * zoom * (p - camera) -
  // projectCamera (camera.ts) spelled with element transforms as origin at
  // the camera point, rotate + scale there, then translate the camera point
  // onto the pivot. checks/camera-check.ts holds the two spellings together.
  let camX = () => props.camera?.x ?? 0
  let camY = () => props.camera?.y ?? 0
  return (
    <view
      ref={(n: { id: number }) => (world = n)}
      width={layer.width}
      height={layer.height}
      flexShrink={0}
      originX={camX()}
      originY={camY()}
      rotate={props.camera?.rotation ?? 0}
      scale={props.camera?.zoom ?? 1}
      x={(props.camera?.pivotX ?? 0) - camX()}
      y={(props.camera?.pivotY ?? 0) - camY()}
    >
      <For each={chunks()}>
        {chunk => <d-texture src={chunk.texture} x={chunk.x} y={chunk.y} w={chunk.width} h={chunk.height} />}
      </For>
    </view>
  )
}
