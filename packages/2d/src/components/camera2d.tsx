import { merge } from "@solidjs/signals"
import { createEffect, onFrame, untrack } from "@solidrt/core"
import type { InputMap, VoidComponent } from "@solidrt/core"
import { createCamera2d } from "../camera2d.ts"
import type { Camera2d as Camera2dHandle, Camera2dAxes, Camera2dOptions } from "../camera2d.ts"
import { useSpriteLayer } from "./context.ts"

export type Camera2dProps = Omit<Camera2dOptions, "viewport"> & {
  /** Viewport in layer pixels; defaults to the driven view's own size
   * (live: a fill layer's box, a setSize, a `<View2d>` resize). */
  viewport?: () => { width: number; height: number }
  /** The input map driving the control (InputMap.drive): its `pan`,
   * `zoom` and `roll` actions, or the ones `actions` names. Live: a new
   * map reconnects. Without one the control moves only through `ref`. */
  input?: InputMap<any>
  /** Action names per axis when the map's differ (null skips an axis). */
  actions?: Partial<Record<keyof Camera2dAxes, string | null>>
  ref?: (camera: Camera2dHandle) => void
}

// Cap on the camera loop's per-frame dt in seconds, so the first tick
// after a suspended stretch (a resumed app, a reload) cannot leap a glide.
const MAX_CAMERA_DT = 0.1

/**
 * createCamera2d as a SpriteLayer child: drives the nearest view's camera
 * - the `<SpriteLayer>`'s own, or inside a `<View2d>` that view
 * (useSpriteLayer's `viewport`) - and takes its input from the map in
 * `input`, nothing else (ARCHITECTURE.md: no device wiring in a
 * component; the app binds the view's pointer feed, a pad or anything
 * else to the map). Under a `<SpriteLayer output={false}>` there is no
 * view to drive: put the `<Camera2d>` inside one of its `<View2d>`
 * children. The pose props (x, y, zoom, rotation) are initial values:
 * change the pose at runtime through `ref`'s set/glideTo/fit/follow and
 * the verbs. Every other prop is live, forwarded to the control as a
 * getter and read where it applies, so a world, a zoom range, a pivot,
 * a dead zone or a rate follows its prop, and a bounds or pivot change
 * re-clamps and pushes the pose at once - the `<OrbitCamera>` rule.
 * `viewport` defaults to the view's own size. Frames run only while the
 * camera moves (`active()`), so a resting camera leaves the app
 * demand-driven idle.
 */
export let Camera2d: VoidComponent<Camera2dProps> = props => {
  let target = useSpriteLayer().viewport
  // Through merge, not a spread: a props object hands out getters, and
  // merge keeps them, so the control reads each option live where it
  // applies.
  let options: Camera2dOptions = merge(props, {
    get viewport() {
      return props.viewport ?? (() => ({ width: target.width, height: target.height }))
    },
  })
  let cam = untrack(() => createCamera2d(target, options))
  createEffect(
    () => props.input,
    input => (input ? input.drive(cam.axes, untrack(() => props.actions)) : undefined),
  )
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
  // A bounds or pivot prop change re-clamps and pushes the pose at once
  // (set({}) applies the clamps). Deferred: the creation already clamped
  // and pushed the initial pose. Untracked: the control reads the options
  // through the getters while it applies them, and the apply wants the
  // values of that moment (the compute above is what tracks them).
  createEffect(
    () => [options.world, options.minZoom, options.maxZoom, options.pivot],
    () => untrack(() => cam.set({})),
    { defer: true },
  )
  untrack(() => props.ref)?.(cam)
  return null
}
