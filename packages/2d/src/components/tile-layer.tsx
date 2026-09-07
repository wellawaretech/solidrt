import { createEffect, createSignal, displayScale, For, getBoundingBoxViewport, onLayout, untrack } from "@solidrt/core"
import type { TextureId, VoidComponent } from "@solidrt/core"
import type { FilterMode } from "@solidrt/core/gpu"
import type { CameraUpdate } from "../camera.ts"
import { tileWorldScale } from "../oversample-math.ts"
import { createTileLayer } from "../tiles.ts"
import type { TileChunk, TileLayer as TileLayerHandle } from "../tiles.ts"
import { applyOversample } from "./auto-oversample.ts"

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
