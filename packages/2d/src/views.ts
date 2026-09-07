// A layer's targets: every rendering of a layer's world is a view - one
// draw target holding ONE entry over the layer's pipeline and instance
// buffers, with a camera and a viewport of its own - and the layer's own
// output is simply the first one (Unity's scene renders only through
// Cameras, Godot's World2D only through Viewports; @solidrt/3d's
// scene.createView one dimension down). Further views are the minimap,
// the radar strip, the zoomed inset: no sprite is mirrored, no record is
// written twice, and the core's key-order gather (the own entry's
// instanceOrder - one ordered entry per buffer) happens in the buffers at
// publish, so a view entry declaring no order reads them already sorted.
// The layer fans buffer swaps (growth), the instance count and the tint
// out to every target through the registry below, its own first; the
// camera and the viewport are each target's own params. Pointer events
// run the layer's dispatch (dispatch.ts) with the target's camera undone
// over the layer's pick, the target as the root of the walk: a sprite
// under a minimap gets its ordinary handlers, and the root's listeners
// are the last stop.
import { addDraw, createDrawTarget, destroyTexture, setDrawBuffers, setDrawRange, setTargetParams, setTargetSize } from "@solidrt/core/gpu"
import type { BufferId, BufferUpdate, DrawId, InstanceOrder, RenderPipelineId, TextureId } from "@solidrt/core/gpu"
import { applyCamera, cameraParams, checkCamera, defaultCamera, projectCamera, unprojectCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"
import { spriteDispatch } from "./dispatch.ts"
import type { LayerBase, LayerPointerListener, Sprite, SpriteHandlers, SpriteLayer } from "./layer.ts"
import type { RecordLayer } from "./records.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"

export type ViewOptions = {
  /** View pixels: the viewport its camera maps the world onto. */
  width: number
  height: number
  /** The camera to start from (a setCamera update); default the identity. */
  camera?: CameraUpdate
  /** Target texels per view pixel (see LayerBase.setOversample); default 1. */
  oversample?: number
  clearColor?: [number, number, number, number]
  label?: string
}

/**
 * A view of a layer (LayerBase.createView): the layer's own viewport
 * contract - texture, size, oversample, camera and pointer dispatch -
 * over the same sprites, with a camera of its own. The sprites are the
 * layer's: add, write and pick through the layer, tint through the layer
 * (a view follows it). `dispose` is idempotent; views also die with their
 * layer.
 */
export type ViewHandle = Pick<
  LayerBase,
  "texture" | "handlers" | "width" | "height" | "setSize" | "listen" | "oversample" | "setOversample" | "setCamera" | "camera" | "project" | "unproject" | "handlersFor" | "dispose"
>

// What a layer hands its targets: the pipeline and quad every entry draws
// with, the atlas, and live reads of the state the layer fans out.
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
}

/** The layer's OWN target, created first: its entry declares the
 * instance order, and the walk's root is the layer itself - built after
 * the target, hence the getter. */
export type OwnTarget = {
  root: () => SpriteLayer | RecordLayer
  order?: InstanceOrder
}

/** The layer side of its targets: create, and fan out what changes. */
export type Views = {
  create(opts: ViewOptions, own?: OwnTarget): ViewHandle
  /** The layer swapped its instance buffers (growth): every entry
   * follows, the layer's own first. */
  setBuffers(update: BufferUpdate): void
  setCount(count: number): void
  setTint(tint: [number, number, number, number]): void
  dispose(): void
}

type ViewRecord = { texture: TextureId; entry: DrawId; dispose(): void }

export function createViews(deps: ViewDeps): Views {
  let views = new Set<ViewRecord>()
  return {
    create(opts, own) {
      let width = opts.width
      let height = opts.height
      // The layer's own size is its caller's (a fill layer rounds it).
      if (!own && !(Number.isInteger(width) && width > 0 && Number.isInteger(height) && height > 0)) {
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
      let entry = addDraw(texture, deps.pipeline, null, {
        buffer: deps.quad,
        vertexCount: 4,
        ...deps.buffers(),
        instanceOrder: own?.order,
        instanceCount: deps.count(),
      })
      let thrash = thrashSentinel(own ? `layer "${label}"` : `view "${label}"`)
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
          // The entry dies with its target.
          destroyTexture(texture)
        },
      }
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
        root: own?.root ?? (() => view),
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
