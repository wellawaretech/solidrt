import { merge } from "@solidjs/signals"
import { createEffect, onFrame, untrack, useContext } from "@solidrt/core"
import type { InputMap, VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createOrbitCamera } from "../orbit.ts"
import type { OrbitAxes, OrbitCamera as OrbitCameraHandle, OrbitCameraOptions } from "../orbit.ts"

// Cap on the frame loop's per-frame dt in seconds, so the first tick after
// a suspended stretch (a resumed app, a reload) cannot leap the pose.
const MAX_ORBIT_DT = 0.1

export type OrbitCameraProps = OrbitCameraOptions & {
  /** The input map driving the control (InputMap.drive): its `rotate`,
   * `zoom` and `pan` actions, or the ones `actions` names. Live: a new
   * map reconnects. Without one the control moves only through `ref`. */
  input?: InputMap<any>
  /** Action names per axis when the map's differ (null skips an axis). */
  actions?: Partial<Record<keyof OrbitAxes, string | null>>
  /** The control's handle (pose()/set()/eye()/orbiting()/active(), the
   * verbs, `axes` - also the debug command shape). */
  ref?: (orbit: OrbitCameraHandle) => void
}

/**
 * createOrbitCamera as a Scene (or View3d) child: drives the enclosing
 * camera - the scene's, or inside a `<View3d>` that view's - through
 * context, and takes its input from the map in `input`, nothing else
 * (ARCHITECTURE.md: no device wiring in a component; the app binds a
 * pointer feed, a pad or anything else to the map). The pose props
 * (target, azimuth, elevation, distance) are initial values - change the
 * pose at runtime through `ref`'s set() or the verbs; every other prop is
 * live, forwarded to the control as a getter and read where it applies,
 * so a clamp, a rate or an anchor callback follows its prop, and a clamp
 * change re-clamps the pose at once. The frame loop runs only while
 * `active()` (auto-orbit on, or an axis rate driving); a camera moved
 * by drags alone leaves the app demand-driven idle.
 */
export let OrbitCamera: VoidComponent<OrbitCameraProps> = props => {
  let ctx = useContext(SceneContext)
  // The control keeps this object and reads each option where it applies,
  // so the props go through as getters (merge), never a spread: a spread
  // would snapshot every prop once and a change could only remount.
  let options: OrbitCameraOptions = merge(props, {})
  let orbit = untrack(() => createOrbitCamera(ctx.viewport, options))
  createEffect(
    () => props.input,
    input => (input ? input.drive(orbit.axes, untrack(() => props.actions)) : undefined),
  )
  createEffect(
    () => orbit.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_ORBIT_DT)
        last = now
        orbit.update(dt)
      })
    },
  )
  // A clamp prop change re-clamps the pose at once (set({}) applies the
  // clamps and pushes) - the update() call a Three OrbitControls app
  // makes after setting minDistance, done by the prop. Deferred: the
  // creation already clamped and pushed the initial pose. Untracked: the
  // control reads the clamps through the getters while it applies them,
  // and the apply wants the values of that moment (the compute above is
  // what tracks them).
  createEffect(
    () => [options.minDistance, options.maxDistance, options.minElevation, options.maxElevation],
    () => untrack(() => orbit.set({})),
    { defer: true },
  )
  untrack(() => props.ref)?.(orbit)
  return null
}
