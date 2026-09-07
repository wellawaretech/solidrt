import { merge } from "@solidjs/signals"
import { createEffect, onCleanup, onFrame, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { createCamera2d } from "../camera2d.ts"
import type { Camera2d as Camera2dHandle, Camera2dOptions } from "../camera2d.ts"
import { LayerContext } from "./context.ts"

export type Camera2dProps = Omit<Camera2dOptions, "viewport"> & {
  /** Viewport in layer pixels; defaults to the enclosing layer's own size
   * (live: a fill layer's box, a setSize). */
  viewport?: () => { width: number; height: number }
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
      return props.viewport ?? (() => ({ width: layer.width, height: layer.height }))
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
