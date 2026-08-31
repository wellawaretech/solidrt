// The Solid face: PascalCase components over context, syncing the retained
// layer (layer.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained records and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// sprite with `ref` and call setSprite from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the layer.
// The same split, with the same reasoning, as @solidrt/3d's components.
import { createContext, createEffect, createSignal, displayScale, For, getBoundingBoxViewport, onCleanup, onLayout, untrack, useContext, windowSize } from "@solidrt/core"
import type { Element, ParentComponent, TextureId, VoidComponent } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import type { FilterMode } from "@solidrt/core/gpu"
import { addGroup, addSprite, createSpriteLayer, removeGroup, removeSprite, setGroup, setGroupTransition, setSprite, setSpriteTransition } from "./layer.ts"
import type { NodeTransition } from "flux:spatial"
import type { CameraUpdate } from "./camera.ts"
import type { Sprite as SpriteHandle, SpriteGroup, SpriteLayer as LayerHandle, SpriteOptions, SpritePointerEvent, TransitionEndEvent } from "./layer.ts"
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

/**
 * The enclosing layer - the imperative escape hatch inside a component
 * subtree (throws outside a `<SpriteLayer>`).
 */
export function useSpriteLayer(): LayerHandle {
  return useContext(LayerContext)
}

export type SpritePointerProps = {
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onPointerEnter?: (event: SpritePointerEvent) => void
  onPointerLeave?: (event: SpritePointerEvent) => void
}

export type SpriteLayerProps = {
  /** Layer pixels. With `output`, the leaf's own width/height are layout, so
   * render size and display size separate. */
  width: number
  height: number
  /** The atlas texture every sprite samples (create with createAtlas). */
  atlas: TextureId
  /** Initial record reservation (grows on demand); default 1024. */
  capacity?: number
  clearColor?: [number, number, number, number]
  /** Pan/zoom/rotate over the world (in-shader); a shared-params write,
   * never per-sprite. */
  camera?: CameraUpdate
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
   * `"sortKey"` by the per-sprite `sortKey` field (explicit layering:
   * raise-on-drag, click-to-front). Creation-fixed (see
   * SpriteLayerOptions.orderBy).
   */
  orderBy?: "y" | "sortKey"
  label?: string
  ref?: (layer: LayerHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the layer's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf. Sprite pointer events then need the layer's handlers on your
   * leaf: `<texture src={texture} {...useSpriteLayer().handlers} />`.
   */
  output?: (texture: TextureId) => Element
  /**
   * Sprite pointer events (default on): the built-in leaf carries
   * layer.handlers. `false` detaches them - the leaf then costs no pointer
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
export let SpriteLayer: ParentComponent<SpriteLayerProps> = props => {
  let layer = untrack(() =>
    createSpriteLayer(props.width, props.height, props.atlas, {
      capacity: props.capacity,
      clearColor: props.clearColor,
      orderBy: props.orderBy,
      label: props.label,
    }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => layer.setSize(w, h),
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
  untrack(() => props.ref)?.(layer)
  let output = untrack(() => props.output)
  let events = untrack(() => props.events) !== false
  // Auto oversample: the built-in leaf's window box, in device pixels, per
  // layer pixel. Picked after every layout, and again when the display scale
  // changes: the first layout runs before the resize event that reports the
  // scale, and a scale change alone lays nothing out. pick runs as an
  // onLayout handler and as an effect apply, both untracked scopes, so its
  // prop reads are wrapped in an explicit untrack.
  let leaf: { id: number } | undefined
  let pick = () =>
    untrack(() => {
      if (!leaf || props.oversample !== undefined) return
      let box = getBoundingBoxViewport(leaf)
      if (!box) return
      let scale = displayScale() * Math.max(box.width / props.width, box.height / props.height)
      applyOversample(layer, scale, props.width, props.height, props.maxOversample)
    })
  onLayout(pick)
  createEffect(() => [displayScale(), props.maxOversample], pick)
  return (
    <LayerContext value={layer}>
      {output ? (
        untrack(() => output(layer.texture))
      ) : (
        <texture
          ref={(n: { id: number }) => (leaf = n)}
          src={layer.texture}
          width={props.width}
          height={props.height}
          onPointerDown={events ? layer.handlers.onPointerDown : undefined}
          onPointerMove={events ? layer.handlers.onPointerMove : undefined}
          onPointerUp={events ? layer.handlers.onPointerUp : undefined}
          onPointerLeave={events ? layer.handlers.onPointerLeave : undefined}
        />
      )}
      {props.children}
    </LayerContext>
  )
}

let GroupContext = createContext<SpriteGroup | null>(null)

export type GroupProps = {
  /** Position in the parent frame (layer pixels at the root). */
  x?: number
  y?: number
  /** Rotation, radians, clockwise. */
  rotation?: number
  /** Uniform scale on the whole subtree (child sprites scale with it). */
  scale?: number
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
    () => [props.x, props.y, props.rotation, props.scale] as const,
    ([x, y, rotation, scale]) => setGroup(group, { x, y, rotation, scale }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setGroupTransition(group, transition ?? null),
  )
  createEffect(
    () => props.onTransitionEnd,
    end => {
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
  let sprite = untrack(() => addSprite(layer, parent ? { parent } : undefined))
  createEffect(
    () => [props.x, props.y, props.w, props.h, props.frame, props.flipX, props.flipY, props.rotation, props.tint, props.sortKey] as const,
    ([x, y, w, h, frame, flipX, flipY, rotation, tint, sortKey]) => setSprite(sprite, { x, y, w, h, frame, flipX, flipY, rotation, tint, sortKey }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setSpriteTransition(sprite, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerEnter, props.onPointerLeave, props.onTransitionEnd] as const,
    ([down, move, up, enter, leave, end]) => {
      sprite.onPointerDown = down
      sprite.onPointerMove = move
      sprite.onPointerUp = up
      sprite.onPointerEnter = enter
      sprite.onPointerLeave = leave
      sprite.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(sprite)
  onCleanup(() => removeSprite(sprite))
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
