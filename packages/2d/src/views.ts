// A layer's views: every rendering of a layer's world is a view - one
// draw target holding ONE entry over the layer's pipeline and instance
// buffers, with a camera and a viewport of its own - and a layer has no
// output but its views (Unity's scene renders only through Cameras,
// Godot's World2D only through Viewports; @solidrt/3d's scene.createView
// one dimension down). The window-filling main view, the minimap, the
// radar strip, the zoomed inset, two split-screen panes: all the same
// object. No sprite is mirrored, no record is written twice, and the
// core's key-order gather (the entry's instanceOrder) happens in the
// buffers at publish: ONE entry per buffer declares the order - the first
// live view's, re-homed to the next when that view goes - and every other
// entry reads the buffers already sorted. The layer fans buffer swaps
// (growth), the instance count and the tint out to every view through
// the registry below; the camera and the viewport are each view's own
// params, so a view costs no per-frame JS beyond its camera writes.
// Pointer events run the layer's dispatch (dispatch.ts) with the view's
// camera undone over the layer's pick, the view as the root of the walk:
// a sprite under a minimap gets its ordinary handlers, and the view's
// listeners are the last stop.
import { addDraw, createDrawTarget, destroyTexture, removeDraw, setDrawBuffers, setDrawRange, setTargetParams, setTargetSize } from "@solidrt/core/gpu"
import type { BufferId, BufferUpdate, DrawId, InstanceOrder, RenderPipelineId, TextureId } from "@solidrt/core/gpu"
import { applyCamera, cameraParams, checkCamera, defaultCamera, projectCamera, unprojectCamera } from "./camera.ts"
import type { CameraState, CameraUpdate } from "./camera.ts"
import { spriteDispatch } from "./dispatch.ts"
import type { LayerPointerListener, Sprite, SpriteHandlers } from "./layer.ts"
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
 * A view of a layer (LayerBase.createView): the viewport contract -
 * texture, size, oversample, camera and pointer dispatch - over the
 * layer's sprites, with a camera of its own. The sprites are the layer's:
 * add, write and pick through the layer, tint through the layer (every
 * view follows it). `dispose` is idempotent; views also die with their
 * layer.
 */
export type ViewHandle = {
  /** The view's output: an ordinary texture id (`<texture src>`). */
  texture: TextureId
  /** Element handlers wiring the view's pointer events (sprites, groups
   * and the root listeners); see handlersFor. */
  handlers: SpriteHandlers
  /** View pixels, as created or last set by setSize. */
  readonly width: number
  readonly height: number
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
  /**
   * handlers for a leaf whose LAYOUT size differs from the view size
   * (events scale by view/layout; a leaf laid out AT view size just uses
   * `handlers`). `layout` is read per event, so a resize-reactive layout
   * just works - @solidrt/3d's handlersFor, one dimension down.
   */
  handlersFor(layout: () => { width: number; height: number }): SpriteHandlers
  dispose(): void
}

// What a layer hands its views: the pipeline and quad every entry draws
// with, the atlas, the key order one entry declares, and live reads of
// the state the layer fans out.
export type ViewDeps = {
  label: string
  pipeline: RenderPipelineId
  quad: BufferId
  atlas: TextureId
  /** The layer's current instance binding, in the entry's own spelling
   * (`instanceBuffers` on the node layer, `instanceBuffer` on records). */
  buffers: () => BufferUpdate
  /** The instance count as last published. */
  count: () => number
  tint: () => [number, number, number, number]
  pick: (x: number, y: number) => Sprite[]
  /** The layer's key order (`orderBy`), declared by ONE live entry. */
  order?: InstanceOrder
}

/** The layer side of its views: create, and fan out what changes. */
export type Views = {
  create(opts: ViewOptions): ViewHandle
  /** The layer swapped its instance buffers (growth): every entry
   * follows. */
  setBuffers(update: BufferUpdate): void
  setCount(count: number): void
  setTint(tint: [number, number, number, number]): void
  dispose(): void
}

type ViewRecord = { texture: TextureId; entry: DrawId; dispose(): void }

export function createViews(deps: ViewDeps): Views {
  let views = new Set<ViewRecord>()
  // The one entry declaring the key order (null until a view exists, or
  // while the layer has no order).
  let ordered: ViewRecord | null = null
  let entryFor = (texture: TextureId, order: InstanceOrder | undefined): DrawId =>
    addDraw(texture, deps.pipeline, null, {
      buffer: deps.quad,
      vertexCount: 4,
      ...deps.buffers(),
      instanceOrder: order,
      instanceCount: deps.count(),
    })
  // The ordered view went: the next live view re-adds its entry with the
  // order, so key order survives any one view's lifetime.
  let rehome = () => {
    if (deps.order === undefined) return
    let next = views.values().next().value
    if (!next) return
    removeDraw(next.texture, next.entry)
    next.entry = entryFor(next.texture, deps.order)
    ordered = next
  }
  return {
    create(opts) {
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
      let texture = createDrawTarget(
        width * oversample,
        height * oversample,
        { uViewport: [width, height], ...cameraParams(cam), uTint: deps.tint() },
        { textures: { uAtlas: deps.atlas }, clearColor: opts.clearColor ?? [0, 0, 0, 0], label, autoFree: false },
      )
      let order = ordered === null ? deps.order : undefined
      let entry = entryFor(texture, order)
      let thrash = thrashSentinel(`view "${label}"`)
      let disposed = false
      let listeners = new Set<LayerPointerListener>()
      let record: ViewRecord = {
        texture,
        entry,
        dispose() {
          if (disposed) return
          disposed = true
          listeners.clear()
          views.delete(record)
          // The entry dies with its target, releasing the buffers' order.
          destroyTexture(texture)
          if (ordered === record) {
            ordered = null
            rehome()
          }
        },
      }
      if (order !== undefined) ordered = record
      let view: ViewHandle = {
        texture,
        handlers: undefined as unknown as SpriteHandlers,
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
          applyCamera(cam, update)
          setTargetParams(texture, cameraParams(cam))
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
        handlersFor(layout) {
          return dispatch(layout)
        },
        dispose: record.dispose,
      }
      let dispatch = spriteDispatch({
        size: () => [width, height],
        camera: () => cam,
        pick: deps.pick,
        root: view,
        listeners,
      })
      view.handlers = dispatch(null)
      views.add(record)
      return view
    },
    setBuffers(update) {
      for (let v of views) setDrawBuffers(v.texture, v.entry, update)
    },
    setCount(count) {
      for (let v of views) setDrawRange(v.texture, v.entry, { instanceCount: count })
    },
    setTint(tint) {
      for (let v of views) setTargetParams(v.texture, { uTint: tint })
    },
    dispose() {
      for (let v of [...views]) v.dispose()
    },
  }
}
