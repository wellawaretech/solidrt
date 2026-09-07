import { createEffect, merge, onFrame, untrack, useContext } from "@solidrt/core"
import type { InputMap, VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createFirstPersonCamera } from "../first-person.ts"
import type { FirstPersonAxes, FirstPersonCamera as FirstPersonCameraHandle, FirstPersonCameraOptions } from "../first-person.ts"

// Cap on the frame loop's per-frame dt in seconds, the same guard as the
// orbit's: a resumed app must not teleport the walker.
const MAX_WALK_DT = 0.1

export type FirstPersonCameraProps = FirstPersonCameraOptions & {
  /** The input map driving the control (InputMap.drive): its `look`,
   * `move` and `rise` actions, or the ones `actions` names. Live: a new
   * map reconnects. Without one the control moves only through `ref`. */
  input?: InputMap<any>
  /** Action names per axis when the map's differ (null skips an axis). */
  actions?: Partial<Record<keyof FirstPersonAxes, string | null>>
  /** The control's handle (pose()/set()/eye()/forward()/active(), the
   * verbs, `axes` - also the debug command shape). */
  ref?: (camera: FirstPersonCameraHandle) => void
}

/**
 * createFirstPersonCamera as a Scene (or View3d) child: drives the
 * enclosing camera - the scene's, or inside a `<View3d>` that view's -
 * through context, and takes its input from the map in `input`, nothing
 * else (ARCHITECTURE.md: the app binds keys, a pointer feed, a pad or
 * anything else to the map, and spreads the map's key handlers where the
 * keys should be caught). Pointer lock stays the app's: call
 * `lockPointer(true)` from a click and `lockPointer(false)` from Escape;
 * the pointer feed's `mouseDelta` source looks around while locked. The
 * pose props (position, yaw, pitch) are initial values - change the pose
 * at runtime through `ref`'s set() or the verbs; every other prop is
 * live, forwarded to the control as a getter and read where it applies:
 * `fly={flying()}` swaps walk and fly on the running control (pose
 * carries over, no remount), `moveSpeed`, `lookSpeed` and
 * `clampPosition` follow their props, and a pitch clamp change re-clamps
 * at once. A frame loop runs only while `active()` - a rate driving - so
 * a still scene stays demand-driven idle.
 */
export let FirstPersonCamera: VoidComponent<FirstPersonCameraProps> = props => {
  let ctx = useContext(SceneContext)
  // Getters, not a spread: see OrbitCamera.
  let options: FirstPersonCameraOptions = merge(props, {})
  let camera = untrack(() => createFirstPersonCamera(ctx.viewport, options))
  createEffect(
    () => props.input,
    input => (input ? input.drive(camera.axes, untrack(() => props.actions)) : undefined),
  )
  createEffect(
    () => camera.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_WALK_DT)
        last = now
        camera.update(dt)
      })
    },
  )
  // A pitch clamp change re-clamps the pose at once; deferred and
  // untracked as OrbitCamera's.
  createEffect(
    () => [options.minPitch, options.maxPitch],
    () => untrack(() => camera.set({})),
    { defer: true },
  )
  untrack(() => props.ref)?.(camera)
  return null
}
