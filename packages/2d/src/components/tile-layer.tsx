import { callRef, createEffect, createMemo, createSignal, followBox, For, onCleanup, pct, untrack } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import type { FilterMode, ResourcePool, ShaderParams, TextureId } from "@solidrt/core/gpu"
import type { Atlas } from "../atlas.ts"
import type { Material } from "../material.ts"
import { unprojectCamera } from "../camera.ts"
import type { CameraUpdate } from "../camera.ts"
import type { Frame } from "../frames.ts"
import { createTileLayer } from "../tiles.ts"
import type { TileChunk, TileLayer as TileLayerHandle, TileRect } from "../tiles.ts"
import { applyOversample } from "./auto-oversample.ts"

/**
 * The tile layer's camera - the same CameraUpdate vocabulary as the sprite
 * layer (see camera.ts for the pivot/rotation semantics and the
 * heading-upward convention), so one signal drives a whole rotating scene
 * across both layers. Here the camera is a transform on the composite and
 * the view the residency gate keeps chunks resident around, never a
 * re-bake; projectCamera/unprojectCamera are the same mapping as plain
 * functions.
 */
export type TileCamera = CameraUpdate

export type TileLayerProps = {
  /** Cell pixel size - creation-fixed. Required unless `layer` is given. */
  tileW?: number
  tileH?: number
  /** The tilesets the cells draw from (createAtlas records), bound as one
   * bake; every frame set names one of them. Fixed at mount. Give these
   * (a layer of the component's own, disposed with it) or `layer`, not
   * both. */
  atlases?: Atlas[]
  /**
   * An existing layer (createTileLayer) to show instead of making one: a
   * second composite of the same world under a camera of its own (a
   * minimap beside the main view; the layer's chunks are resident where
   * any composite wants them). Its lifetime, its hooks (`onEnter`,
   * `onLeave`) and its cells stay its maker's, so the creation props and
   * the hook props throw with it.
   */
  layer?: TileLayerHandle
  /** The frames table `setTiles`' index cells name (a tileset, `grid()`'s
   * array); see TileLayerOptions. Fixed at creation. */
  frames?: Frame[]
  /** Per-chunk clear color (the name says the scope: chunks holding no
   * cell render nothing, so a full-bleed ground color belongs on the
   * container behind the layer). */
  chunkClearColor?: [number, number, number, number]
  /** The material the cells bake with (see TileLayerOptions.material):
   * its blend, its look, its per-cell style record; default `unlit()`.
   * Fixed at mount. */
  material?: Material
  /** Uniform values written to every resident chunk (layer.setParams as
   * a prop, over the material's own). Live, merge semantics; each change
   * re-bakes every resident chunk - slow state, not per frame. */
  params?: ShaderParams
  /** Sampler filter for the baked chunk textures at composite time; default
   * "linear" (hard pixels belong to the atlas sampler, see TileLayerOptions). */
  filter?: FilterMode
  /**
   * Target texels per world pixel in the baked chunks. Absent, the component
   * picks it every layout from its viewport's on-screen size (display
   * scale, any designSize fit) times the camera zoom, so tiles resample
   * properly at any scale.
   */
  oversample?: number
  /**
   * Cap on the auto-picked oversample (integer >= 1): tiles stay adaptive
   * up to it, never above. A tile world's texture memory is resident chunks
   * x n squared, so this is the lever that bounds it on high-scale displays
   * without giving up adaptivity. Ignored when `oversample` is set.
   */
  maxOversample?: number
  /**
   * Cap on a baked chunk's texel count under the auto pick, the budget
   * form of `maxOversample`: the pick stays under it on any display
   * (the window's own pixel count caps it always). Ignored when
   * `oversample` is set; same name and meaning as `<SpriteLayer>`'s and
   * the 3d `<Scene>`'s.
   */
  maxPixels?: number
  /** Chunk edge in tiles (default ~512px worth); see TileLayerOptions. */
  chunkTiles?: number
  /**
   * Layer tint, [r, g, b, a] in 0..1, multiplied over every cell's own
   * tint (day/night, a dimmed parallax plane). A change re-renders every
   * resident chunk GPU-side (no record uploads) - drive it from slow
   * state, not per frame.
   */
  tint?: [number, number, number, number]
  /** The world's cell bounds, wrap periods, overhang margin, residency
   * band, retention and fill (TileLayerOptions). Fixed at creation. */
  bounds?: TileRect
  wrap?: { cols: number; rows: number }
  overhang?: number
  enter?: number
  leave?: number
  retain?: "records" | "none"
  fill?: (chunk: TileChunk) => void
  /** The resource pool the layer's chunks take their buffers and targets
   * from (TileLayerOptions.pool); the layer's own by default. */
  pool?: ResourcePool
  /**
   * Pan/zoom/rotate over the world: a transform on the composite and the
   * view the gate keeps chunks resident around, never a re-bake. The
   * component clips to its own box, the viewport.
   */
  camera?: TileCamera
  /** The viewport's layout size. Absent, the component FILLS its parent's
   * box (give it a sized parent, as on the web). */
  width?: number
  height?: number
  /** The layer's residency hooks (TileLayer.onEnter / onLeave), for a
   * layer of the component's own. */
  onEnter?: (chunk: TileChunk) => void
  onLeave?: (chunk: TileChunk) => void
  label?: string
  ref?: (layer: TileLayerHandle) => void
}

/** One leaf of the composite: a resident chunk's texture at one image of
 * its rect (several under wrap), in world pixels at zoom 1. */
type Image = { key: string; texture: TextureId; x: number; y: number }

/**
 * Shows a baked tile layer (createTileLayer) through a camera: a clipping
 * `<view>` sized to its box (the viewport, filling its parent when
 * unsized) composites each resident chunk as a `d-texture` leaf placed
 * RELATIVE to the camera (world minus camera in JS, so the transform's
 * floats stay small however far the world reaches), inside a carrier
 * `<view>` with the camera's pivot, rotation and zoom - a handful of
 * quads however many tiles exist, the leaves in row-major order so a
 * lower row's overhang draws over the row above. The component pushes
 * its camera over its viewport to the residency gate (layer.addViewer),
 * which brings the chunks near the view and takes the far ones; under
 * `wrap` every image of a resident chunk that intersects the view is a
 * leaf, so the seam is two leaves of one texture. A layout component:
 * the viewport is laid out, so it cannot sit inside a d-* subtree. Tiles
 * are data, not children: write them through `ref` with `setTile` and
 * `setTiles` - there is no `<Tile>` component on purpose (a component per
 * tile would re-introduce the per-element cost the bake removes).
 */
export let TileLayer: VoidComponent<TileLayerProps> = props => {
  let own = untrack(() => props.layer === undefined)
  let layer = untrack(() => {
    if (props.layer !== undefined) {
      if (props.tileW !== undefined || props.tileH !== undefined || props.atlases !== undefined || props.frames !== undefined || props.material !== undefined || props.chunkTiles !== undefined || props.bounds !== undefined || props.wrap !== undefined || props.fill !== undefined) {
        throw new Error("TileLayer: `layer` adopts an existing layer, so the creation props (tileW, tileH, atlases, frames, material, chunkTiles, bounds, wrap, fill) have nothing to apply to")
      }
      if (props.onEnter !== undefined || props.onLeave !== undefined) throw new Error("TileLayer: with `layer` the residency hooks belong to the layer's maker (layer.onEnter / onLeave)")
      return props.layer
    }
    if (props.tileW === undefined || props.tileH === undefined || props.atlases === undefined) throw new Error("TileLayer: tileW, tileH and atlases are required without `layer`")
    return createTileLayer(props.tileW, props.tileH, props.atlases, {
      chunkClearColor: props.chunkClearColor,
      material: props.material,
      params: props.params,
      filter: props.filter,
      chunkTiles: props.chunkTiles,
      tint: props.tint,
      frames: props.frames,
      bounds: props.bounds,
      wrap: props.wrap,
      overhang: props.overhang,
      enter: props.enter,
      leave: props.leave,
      retain: props.retain,
      fill: props.fill,
      pool: props.pool,
      label: props.label,
    })
  })
  if (own) {
    layer.onEnter = chunk => untrack(() => props.onEnter)?.(chunk)
    layer.onLeave = chunk => untrack(() => props.onLeave)?.(chunk)
  }
  callRef(() => props.ref, layer)
  createEffect(
    () => props.oversample,
    n => {
      if (n !== undefined) layer.setOversample(n)
    },
  )
  if (own) {
    createEffect(
      () => props.tint,
      tint => {
        if (tint !== undefined) layer.setTint(tint)
      },
    )
    createEffect(
      () => props.params,
      params => {
        if (params !== undefined) layer.setParams(params)
      },
      { defer: true },
    )
  }
  let margin = layer.overhang
  let leafW = layer.chunkW + 2 * margin
  let leafH = layer.chunkH + 2 * margin
  // The viewport: the root's layout box, followed. Its device box over its
  // layout box is the display scale times any ancestor fit; times the
  // camera zoom that is the device pixels one world pixel covers, the
  // auto oversample's input (a zoom change re-runs the pick through deps).
  // Rotation is not a resolution factor and never enters.
  let root: { id: number } | undefined
  let sameSize = (a: { width: number; height: number }, b: { width: number; height: number }) => a.width === b.width && a.height === b.height
  let [viewport, setViewport] = createSignal({ width: 0, height: 0 }, { equals: sameSize })
  followBox(
    "TileLayer",
    () => root,
    box => {
      setViewport({ width: box.layout.width, height: box.layout.height })
      if (props.oversample !== undefined || box.layout.width <= 0 || box.layout.height <= 0) return
      let zoom = props.camera?.zoom ?? 1
      let scale = Math.max(box.device.width / box.layout.width, box.device.height / box.layout.height) * zoom
      applyOversample("TileLayer", layer, scale, leafW, leafH, props.maxOversample, props.maxPixels)
    },
    { deps: () => [props.camera?.zoom, props.maxOversample, props.maxPixels] },
  )
  // The viewer: the camera over the viewport, pushed on every change of
  // either; the gate does the rest.
  let viewer = layer.addViewer()
  onCleanup(() => viewer.dispose())
  createEffect(
    () => [props.camera, viewport()] as const,
    ([camera, size]) => {
      if (size.width > 0 && size.height > 0) viewer.setCamera(camera ?? {}, size.width, size.height)
    },
  )
  // The resident chunks with a texture, row-major: the layer says when the
  // set changed (a texture came or went, from the gate's events or a
  // write, never inside an owner, hence the internal-state signal).
  let [version, setVersion] = createSignal(0, { ownedWrite: true })
  onCleanup(layer._watch(() => setVersion(v => v + 1)))
  let shown = createMemo(() => {
    version()
    return layer
      .resident()
      .filter(c => c.texture !== null)
      .sort((a, b) => a.row - b.row || a.col - b.col)
  })
  let camX = () => props.camera?.x ?? 0
  let camY = () => props.camera?.y ?? 0
  // The leaves: one per resident chunk, at its rect minus the margin; under
  // wrap, one per image of the chunk inside the view's world AABB (the
  // viewport's corners unprojected through the camera), the period in
  // world pixels along each axis. Keyed, so a camera move re-places the
  // leaves instead of recreating them.
  let images = createMemo((): Image[] => {
    let list = shown()
    let wrap = layer.wrap
    if (wrap === null) return list.map(c => ({ key: `${c.col},${c.row}`, texture: c.texture!, x: c.x - margin, y: c.y - margin }))
    let size = viewport()
    let camera = props.camera ?? {}
    let minX = Infinity
    let minY = Infinity
    let maxX = -Infinity
    let maxY = -Infinity
    for (let [sx, sy] of [
      [0, 0],
      [size.width, 0],
      [0, size.height],
      [size.width, size.height],
    ]) {
      let [wx, wy] = unprojectCamera(camera, sx!, sy!)
      minX = Math.min(minX, wx)
      maxX = Math.max(maxX, wx)
      minY = Math.min(minY, wy)
      maxY = Math.max(maxY, wy)
    }
    let periodX = wrap.cols * layer.tileW
    let periodY = wrap.rows * layer.tileH
    let out: Image[] = []
    for (let c of list) {
      let lx = c.x - margin
      let ly = c.y - margin
      // The leaf at image (ix, iy) spans [lx + ix * periodX, + leafW): the
      // images whose span overlaps the AABB.
      let ix0 = Math.floor((minX - leafW - lx) / periodX) + 1
      let ix1 = Math.ceil((maxX - lx) / periodX) - 1
      let iy0 = Math.floor((minY - leafH - ly) / periodY) + 1
      let iy1 = Math.ceil((maxY - ly) / periodY) - 1
      for (let iy = iy0; iy <= iy1; iy++) {
        for (let ix = ix0; ix <= ix1; ix++) {
          out.push({ key: `${c.col},${c.row},${ix},${iy}`, texture: c.texture!, x: lx + ix * periodX, y: ly + iy * periodY })
        }
      }
    }
    return out.sort((a, b) => a.y - b.y || a.x - b.x)
  })
  // World -> screen: p maps to pivot + R(rotation) * zoom * (p - camera) -
  // projectCamera (camera.ts): the leaves sit at (p - camera), the carrier
  // rotates and scales them about its own origin and sits at the pivot.
  // tests/camera.test.ts holds the two spellings together.
  return (
    <view ref={(n: { id: number }) => (root = n)} width={props.width ?? pct(100)} height={props.height ?? pct(100)} overflow="clip" flexShrink={0}>
      <view position="absolute" originX={0} originY={0} rotate={props.camera?.rotation ?? 0} scale={props.camera?.zoom ?? 1} x={props.camera?.pivotX ?? 0} y={props.camera?.pivotY ?? 0}>
        <For each={images()} keyed={im => im.key}>
          {im => <d-texture src={im().texture} x={im().x - camX()} y={im().y - camY()} w={leafW} h={leafH} />}
        </For>
      </view>
    </view>
  )
}
