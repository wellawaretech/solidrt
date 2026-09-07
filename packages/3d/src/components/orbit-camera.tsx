import { merge } from "@solidjs/signals"
import { createEffect, onCleanup, onFrame, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createOrbitCamera } from "../orbit.ts"
import type { OrbitCamera as OrbitCameraHandle, OrbitCameraOptions, OrbitPose } from "../orbit.ts"

// Cap on the auto-orbit's per-frame dt in seconds, so the first tick after
// a suspended stretch (a resumed app, a reload) cannot leap the pose.
const MAX_ORBIT_DT = 0.1

export type OrbitCameraProps = OrbitCameraOptions & {
  /** The control's handle (pose()/set()/eye()/orbiting()/active() - also
   * the debug command shape). This handle's set() pushes the pose itself,
   * so a caller never touches update(). */
  ref?: (orbit: OrbitCameraHandle) => void
}

/**
 * createOrbitCamera as a Scene (or View3d) child: drives the enclosing
 * camera - the scene's, or inside a `<View3d>` that view's - and takes its
 * input from that owner's leaf through context - no ref plumbing, no
 * handler spreads, no onFrame of your own (with a custom `output`, spread
 * `useScene().input.handlersFor(layout)` on your leaf).
 * The pose props (target, azimuth, elevation, distance) are initial
 * values - change the pose at runtime through `ref`'s set(); every other
 * prop is live, forwarded to the control as a getter and read where it
 * applies, so a clamp, a rate, an anchor callback or `viewport` follows
 * its prop, and a clamp change re-clamps the pose at once. `viewport`
 * defaults to the leaf's laid-out size plus the scene camera's fov, so
 * rotation is viewport-relative and two-finger pan works out of the box
 * (pass your own to override). Auto-orbit runs a frame loop only while
 * `active()` (orbiting with a non-zero `orbitSpeed`); a paused or
 * drag-only camera leaves the app demand-driven idle.
 */
export let OrbitCamera: VoidComponent<OrbitCameraProps> = props => {
  let ctx = useContext(SceneContext)
  let leafViewport = () => {
    let layout = ctx.input.layout()
    return layout === null ? null : { height: layout.height, fov: ctx.camera.camera().fov }
  }
  // The control keeps this object and reads each option where it applies,
  // so the props go through as getters (merge), never a spread: a spread
  // would snapshot every prop once and a change could only remount.
  let options: OrbitCameraOptions = merge(props, {
    get viewport() {
      return props.viewport ?? leafViewport
    },
  })
  let orbit = untrack(() => createOrbitCamera(ctx.camera, options))
  // Input pushes the pose synchronously (update(0)), so a drag needs no
  // frame loop and the next paint carries the new camera.
  onCleanup(
    ctx.input.add({
      onPointerDown: e => {
        orbit.handlers.onPointerDown(e)
        orbit.update(0)
      },
      onPointerMove: e => {
        orbit.handlers.onPointerMove(e)
        orbit.update(0)
      },
      onPointerUp: e => orbit.handlers.onPointerUp(e),
      onWheel: e => {
        orbit.handlers.onWheel(e)
        orbit.update(0)
      },
    }),
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
  // clamps, update(0) pushes the result) - the update() call a Three
  // OrbitControls app makes after setting minDistance, done by the prop.
  // Deferred: the creation already clamped and pushed the initial pose.
  // Untracked: the control reads the clamps through the getters while it
  // applies them, and the apply wants the values of that moment (the
  // compute above is what tracks them).
  createEffect(
    () => [options.minDistance, options.maxDistance, options.minElevation, options.maxElevation],
    () =>
      untrack(() => {
        orbit.set({})
        orbit.update(0)
      }),
    { defer: true },
  )
  untrack(() => props.ref)?.({
    ...orbit,
    set: (pose: OrbitPose) => {
      orbit.set(pose)
      orbit.update(0)
    },
  })
  return null
}
