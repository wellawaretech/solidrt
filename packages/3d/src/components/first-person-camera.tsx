import { merge } from "@solidjs/signals"
import { createEffect, onCleanup, onFrame, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createFirstPersonCamera } from "../first-person.ts"
import type { FirstPersonCamera as FirstPersonCameraHandle, FirstPersonCameraOptions, FirstPersonPose } from "../first-person.ts"

// Cap on the first-person loop's per-frame dt in seconds, the same guard
// as the orbit's: a resumed app must not teleport the walker.
const MAX_WALK_DT = 0.1

export type FirstPersonCameraProps = FirstPersonCameraOptions & {
  /** The control's handle (pose()/set()/eye()/forward()/active() - also
   * the debug command shape). This handle's set() pushes the pose itself,
   * so a caller never touches update(). */
  ref?: (camera: FirstPersonCameraHandle) => void
}

/**
 * createFirstPersonCamera as a Scene (or View3d) child: drives the
 * enclosing camera - the scene's, or inside a `<View3d>` that view's - and
 * takes its input from that owner's leaf through context. The
 * leaf becomes focusable and takes focus on pointer down, which is what
 * routes WASD to the control (keys go to the focused node; a click on the
 * scene is the same gesture a web canvas needs). With a custom `output`,
 * spread `useScene().input.handlersFor(layout, node)` on your leaf and
 * mark it `focusable`. Pointer lock stays yours: call `lockPointer(true)`
 * from a click and `lockPointer(false)` from Escape. The pose props
 * (position, yaw, pitch) are initial values - change the pose at runtime
 * through `ref`'s set(); every other prop is live, forwarded to the
 * control as a getter and read where it applies: `fly={flying()}` swaps
 * walk and fly on the running control (pose and held keys carry over, no
 * remount), `moveSpeed`, `lookSpeed` and `clampPosition` follow their
 * props, and a pitch clamp change re-clamps at once. `viewport` defaults
 * to the leaf's laid-out size, so a drag is viewport-relative. A frame
 * loop runs only while `active()` - a key held or a stick deflected - so
 * a still scene stays demand-driven idle.
 */
export let FirstPersonCamera: VoidComponent<FirstPersonCameraProps> = props => {
  let ctx = useContext(SceneContext)
  let leafViewport = () => {
    let layout = ctx.input.layout()
    return layout === null ? null : { height: layout.height }
  }
  // Getters, not a spread: see OrbitCamera.
  let options: FirstPersonCameraOptions = merge(props, {
    get viewport() {
      return props.viewport ?? leafViewport
    },
  })
  let camera = untrack(() => createFirstPersonCamera(ctx.viewport, options))
  // Look input applies synchronously inside the control's handlers, so a
  // mouse move under lock needs no frame loop; only movement integrates.
  onCleanup(ctx.input.add(camera.handlers))
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
    () =>
      untrack(() => {
        camera.set({})
        camera.update(0)
      }),
    { defer: true },
  )
  untrack(() => props.ref)?.({
    ...camera,
    set: (pose: FirstPersonPose) => {
      camera.set(pose)
      camera.update(0)
    },
  })
  return null
}
