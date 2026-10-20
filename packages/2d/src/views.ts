// A layer's views: every rendering of a layer's world is a view - one
// draw target with a camera and a viewport of its own - and a layer has
// no output but its views (Unity's scene renders only through Cameras,
// Godot's World2D only through Viewports; @solidrt/3d's scene.createView
// one dimension down). The window-filling main view, the minimap, the
// radar strip, the zoomed inset, two split-screen panes: all the same
// object, on every layer kind.
//
// The VIEW SHELL (createViewShell) is what the kinds share: the target,
// its size and oversample, the camera and its params write, projection,
// and the pointer dispatch with the view as the root of the walk. What
// a view DRAWS is the kind's: a sprite or record layer draws ONE entry
// over the layer's pipeline and instance buffers per view (createViews
// below: the core's key-order gather happens in the buffers at publish,
// ONE entry per buffer declares the order - the first live view's,
// re-homed to the next when that view goes - and every other entry reads
// the buffers already sorted; the layer fans buffer swaps, the instance
// count and the tint out to every view), while a tile layer draws one
// entry per resident chunk image (tiles.ts). The camera and the viewport
// are each view's own params, so a view costs no per-frame JS beyond its
// camera writes. Pointer events run the layer's dispatch (dispatch.ts)
// with the view's camera undone over the layer's pick, the view as the
// root of the walk: a sprite under a minimap gets its ordinary handlers,
// and the view's listeners are the last stop.
//
// Every view of a layer with regions is a VIEWER to the spatial core's
// residency gate: the shell pushes its camera over its viewport as an
// orthographic view (viewMatrices, under a key of its own: flux:spatial
// allocViewKey) on creation and on every camera or size write, scoped to
// the layer's world (its root node, so a 3d scene's cells and another
// layer's are never wanted by it), and withdraws it at dispose. So a
// region grid over a sprite layer (region.ts) and a tile layer's chunks
// (tiles.ts) are wanted by whatever shows that world - the main view, a
// minimap, a split-screen pane - with no viewer bookkeeping in the layer
// kinds. The record layer has no regions, so its views push none.
import { addDraw, createDrawTarget, destroyTexture, removeDraw, setDrawBuffers, setDrawRange, setTargetParams, setTargetSize } from "@solidrt/core/gpu"
import type { BufferId, DrawId, InstanceOrder, RenderPipelineId, ShaderParams, TextureBindings, TextureId } from "@solidrt/core/gpu"
import { applyCamera, cameraParams, checkCamera, defaultCamera, projectCamera, unprojectCamera, viewMatrices } from "./camera.ts"
import { requestFrame } from "@solidrt/core"
import * as spatial from "flux:spatial"
import type { NodeId, ViewKey } from "flux:spatial"
import type { CameraState, CameraUpdate } from "./camera.ts"
import { spriteDispatch } from "./dispatch.ts"
import type { PointerFeed } from "@solidrt/core"
import type { LayerPointerListener, Sprite, LayerHandlers } from "./layer.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"

export type ViewOptions = {
  /** View pixels: the viewport its camera maps the world onto. */
  width: number
  height: number
  /** The camera to start from (a setCamera update); default the identity. */
  camera?: CameraUpdate
  /** Target texels per view pixel (see ViewHandle.setOversample); default 1. */
  oversample?: number
  clearColor?: [number, number, number, number]
  label?: string
}

/**
 * A view of a layer (LayerBase.createView, TileLayer.createView): the
 * viewport contract - texture, size, oversample, camera and pointer
 * dispatch - over the layer's world, with a camera of its own. The
 * sprites are the layer's: add, write and pick through the layer, tint
 * through the layer (every view follows it); a tile view hits no sprite,
 * so its `pick` is empty and its listeners see every event over the
 * world. `dispose` is idempotent; views also die with their layer.
 */
export type ViewHandle = {
  /** The view's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** Element handlers wiring the view's pointer events (sprites, groups
   * and the root listeners); see handlersFor. */
  handlers: LayerHandlers
  /** View pixels, as created or last set by setSize (a fresh object per
   * call) - @solidrt/3d's view.size(), what a camera control reads. */
  size(): { width: number; height: number }
  setSize(width: number, height: number): void
  /**
   * Listen at the root of the event walk. Every down, move, up, wheel and
   * tap arrives here after the hit sprite and its enclosing groups
   * (`sprite` set) or as the walk's only stop over empty space (`sprite`
   * null), unless a handler stopped it on the way. Listeners run in
   * registration order and all of them run - the root is the last stop,
   * there is nothing left to claim. Returns the remover. The app's own
   * root handling (deselect on a miss, a marquee) and controls
   * (createCamera2d's attach) meet here, which is why the root is a list
   * where a sprite has plain fields.
   */
  listen(listener: LayerPointerListener): () => void
  /** Target texels per view pixel; see setOversample. */
  readonly oversample: number
  /**
   * Re-render at `n` target texels per view pixel (positive integer): the
   * target resizes in place at its stable id; view pixels, records, camera
   * and picking are untouched. Pick `n` as the ceiling of the device pixels
   * one view pixel covers on screen (display scale times any designSize
   * fit or layout scaling), which the components do in onLayout.
   */
  setOversample(n: number): void
  setCamera(update: CameraUpdate): void
  /** The camera as last set (a fresh object per call, every field
   * present): the argument for projectCamera/unprojectCamera -
   * @solidrt/3d's scene.camera(). */
  camera(): CameraState
  /** World (layer) pixels -> view pixels under the current camera:
   * projectCamera over camera(). */
  project(x: number, y: number): [number, number]
  /** View pixels -> world (layer) pixels, the inverse: what pointer
   * dispatch applies to every event. */
  unproject(x: number, y: number): [number, number]
  /** The layer's pick through this view: the view pixel unprojected, the
   * sprites' screen-size floors measured at this camera's zoom - what
   * the pointer walk hits. Empty on a tile view. */
  pick(x: number, y: number): Sprite[]
  /**
   * handlers for a leaf whose LAYOUT size differs from the view size
   * (events scale by view/layout; a leaf laid out AT view size just uses
   * `handlers`). `layout` is read per event, so a resize-reactive layout
   * just works - @solidrt/3d's handlersFor, one dimension down.
   */
  handlersFor(layout: () => { width: number; height: number }): LayerHandlers
  dispose(): void
}

/** What a layer kind hands the view shell: how its target is made and
 * how its camera is spelled, plus the hooks the kind acts on. */
export type ViewShellDeps = {
  label: string
  /** The layer's pick, `zoom` the camera zoom its floors are measured at. */
  pick: (x: number, y: number, zoom: number) => Sprite[]
  /** The shared params the kind's vertex stage reads for a camera:
   * cameraParams for the sprite stage, the tile stage's own split. */
  cameraParams: (camera: CameraState) => ShaderParams
  /** Create the view's draw target of `width` x `height` TEXELS seeded
   * with `params` (the viewport, the camera, the kind's own). */
  createTarget: (width: number, height: number, params: ShaderParams, opts: { clearColor: [number, number, number, number]; label: string }) => TextureId
  /** The world the view is a viewer of, for the residency gate: the
   * layer's root node, which scopes the viewer to the layer's own
   * regions (flux:spatial setView `root`); null for a layer with no
   * regions to want (the record layer), whose views push no viewer. */
  root: NodeId | null
  /** After the camera, the size or the oversample changed (the tile
   * layer takes its density wish here). */
  changed?: (view: ViewHandle) => void
  /** Before the view's target is destroyed. */
  disposing?: (view: ViewHandle) => void
}

/**
 * The view shell: a draw target with a camera, a viewport and the
 * pointer dispatch, the ViewHandle every layer kind returns. The kind
 * adds its entries to `view.texture` after this returns.
 */
export function createViewShell(opts: ViewOptions, deps: ViewShellDeps): ViewHandle {
  let width = opts.width
  let height = opts.height
  if (!(Number.isInteger(width) && width > 0 && Number.isInteger(height) && height > 0)) {
    throw new Error(`createView: width and height must be positive integers, got ${width} x ${height}`)
  }
  let oversample = opts.oversample ?? 1
  checkOversample("createView", oversample, width, height)
  let label = opts.label ?? `${deps.label}-view`
  let cam = defaultCamera()
  if (opts.camera) {
    checkCamera(opts.camera)
    applyCamera(cam, opts.camera)
  }
  let texture = deps.createTarget(width * oversample, height * oversample, { uViewport: [width, height], ...deps.cameraParams(cam) }, { clearColor: opts.clearColor ?? [0, 0, 0, 0], label })
  let thrash = thrashSentinel(`view "${label}"`)
  let disposed = false
  let listeners = new Set<LayerPointerListener>()
  // The gate's viewer: the camera over the viewport, pushed whenever
  // either changes, scoped to the layer's world. The target's params
  // write demands the frame whose flush tests the regions; the first
  // push, before any entry, asks for it itself.
  let viewer: ViewKey | null = deps.root === null ? null : spatial.allocViewKey()
  let gateView = new Float32Array(16)
  let gateProj = new Float32Array(16)
  let pushViewer = () => {
    if (viewer === null) return
    viewMatrices(cam, width, height, gateView, gateProj)
    spatial.setView(viewer, gateView, gateProj, deps.root!)
  }
  pushViewer()
  if (viewer !== null) requestFrame()
  let view: ViewHandle = {
    texture,
    handlers: undefined as unknown as LayerHandlers,
    size: () => ({ width, height }),
    setSize(w, h) {
      if (disposed || (w === width && h === height)) return
      checkOversample("setSize", oversample, w, h)
      width = w
      height = h
      setTargetSize(texture, w * oversample, h * oversample)
      setTargetParams(texture, { uViewport: [w, h] })
      pushViewer()
      deps.changed?.(view)
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
      deps.changed?.(view)
    },
    setCamera(update) {
      if (disposed) return
      checkCamera(update)
      applyCamera(cam, update)
      setTargetParams(texture, deps.cameraParams(cam))
      pushViewer()
      deps.changed?.(view)
    },
    camera() {
      return { ...cam }
    },
    project(x, y) {
      return projectCamera(cam, x, y)
    },
    unproject(x, y) {
      return unprojectCamera(cam, x, y)
    },
    pick(x, y) {
      let [wx, wy] = unprojectCamera(cam, x, y)
      return deps.pick(wx, wy, cam.zoom)
    },
    handlersFor(layout) {
      return dispatch(layout)
    },
    dispose() {
      if (disposed) return
      disposed = true
      listeners.clear()
      // The viewer goes first: what it alone wanted leaves with it.
      if (viewer !== null) {
        spatial.setView(viewer, null)
        requestFrame()
      }
      deps.disposing?.(view)
      // The entries die with their target.
      destroyTexture(texture)
    },
  }
  let dispatch = spriteDispatch({
    size: () => [width, height],
    camera: () => cam,
    pick: deps.pick,
    root: view,
    listeners,
  })
  view.handlers = dispatch(null)
  return view
}

// What a sprite or record layer hands its views: the pipeline and quad
// every entry draws with, the atlases, the key order one entry declares,
// and live reads of the state the layer fans out.
export type ViewDeps = {
  label: string
  pipeline: RenderPipelineId
  quad: BufferId
  /** The sampler bindings every view binds: the layer's atlases by
   * sampler index (the pipeline was compiled for exactly those) and the
   * material's own textures. */
  textures: TextureBindings
  /** The layer's current instance buffers, in the pipeline's layout order
   * after the quad (the pose and sprite pair on the node layer, the one
   * record buffer on records, the material's style buffers after them). */
  buffers: () => BufferId[]
  /** The instance count as last published. */
  count: () => number
  tint: () => [number, number, number, number]
  /** The params every view seeds beyond the viewport, the camera and
   * the tint: the material's, with the layer's own over them. */
  params: () => ShaderParams
  /** The layer's pick, `zoom` the camera zoom its floors are measured at. */
  pick: (x: number, y: number, zoom: number) => Sprite[]
  /** The layer's key order (`orderBy`), declared by ONE live entry. */
  order?: InstanceOrder
  /** The layer's root node, the world its views are viewers of (see
   * ViewShellDeps.root); null for a layer with no regions. */
  root: NodeId | null
}

/** The layer side of its views: create, and fan out what changes. */
export type Views = {
  create(opts: ViewOptions): ViewHandle
  /** The layer swapped its instance buffers (growth): every entry
   * follows. */
  setBuffers(instanceBuffers: BufferId[]): void
  setCount(count: number): void
  setTint(tint: [number, number, number, number]): void
  /** The layer's params changed: every view's target takes the write. */
  setParams(params: ShaderParams): void
  dispose(): void
}

type ViewRecord = { view: ViewHandle; entry: DrawId }

/** The sprite and record layers' views: one entry over the layer's
 * instance buffers per view, the key order on one of them. */
export function createViews(deps: ViewDeps): Views {
  let views = new Set<ViewRecord>()
  // The one entry declaring the key order (null until a view exists, or
  // while the layer has no order).
  let ordered: ViewRecord | null = null
  let entryFor = (texture: TextureId, order: InstanceOrder | undefined): DrawId =>
    addDraw(texture, deps.pipeline, null, {
      buffers: [deps.quad, ...deps.buffers()],
      vertexCount: 4,
      instanceOrder: order,
      instanceCount: deps.count(),
    })
  // The ordered view went: the next live view re-adds its entry with the
  // order, so key order survives any one view's lifetime.
  let rehome = () => {
    if (deps.order === undefined) return
    let next = views.values().next().value
    if (!next) return
    removeDraw(next.view.texture, next.entry)
    next.entry = entryFor(next.view.texture, deps.order)
    ordered = next
  }
  return {
    create(opts) {
      let record: ViewRecord = { view: undefined as unknown as ViewHandle, entry: 0 as DrawId }
      let view = createViewShell(opts, {
        label: deps.label,
        root: deps.root,
        pick: deps.pick,
        cameraParams,
        createTarget: (w, h, params, o) => createDrawTarget(w, h, { ...params, uTint: deps.tint(), ...deps.params() }, { textures: deps.textures, clearColor: o.clearColor, label: o.label, autoFree: false }),
        disposing() {
          views.delete(record)
          // The entry dies with its target, releasing the buffers' order.
          if (ordered === record) {
            ordered = null
            rehome()
          }
        },
      })
      let order = ordered === null ? deps.order : undefined
      record.view = view
      record.entry = entryFor(view.texture, order)
      if (order !== undefined) ordered = record
      views.add(record)
      return view
    },
    setBuffers(instanceBuffers) {
      for (let v of views) setDrawBuffers(v.view.texture, v.entry, { buffers: [deps.quad, ...instanceBuffers] })
    },
    setCount(count) {
      for (let v of views) setDrawRange(v.view.texture, v.entry, { instanceCount: count })
    },
    setTint(tint) {
      for (let v of views) setTargetParams(v.view.texture, { uTint: tint })
    },
    setParams(params) {
      for (let v of views) setTargetParams(v.view.texture, params)
    },
    dispose() {
      for (let v of [...views]) v.view.dispose()
    },
  }
}

/**
 * Feed a pointer feed (createPointerFeed) from a view's root: the events
 * the sprites let through - a sprite that claims its press
 * (stopPropagation on its down) keeps a camera bound to the feed out of
 * that drag, a drag on empty space or an unclaimed sprite pans, a wheel
 * anywhere zooms. Returns the detach. The components do this for their
 * `pointer` prop; an imperative view calls it directly.
 */
export function feedPointer(view: ViewHandle, feed: PointerFeed): () => void {
  return view.listen({
    onPointerDown: e => feed.handlers.onPointerDown(e.native),
    onPointerMove: e => feed.handlers.onPointerMove(e.native),
    onPointerUp: e => feed.handlers.onPointerUp(e.native),
    onPointerCancel: e => feed.handlers.onPointerCancel(e.native),
    onWheel: e => feed.handlers.onWheel(e.native),
  })
}
