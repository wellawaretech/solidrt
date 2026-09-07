import { createEffect, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import type { Vec3 } from "../math.ts"

export type PerspectiveCameraProps = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  lookAt?: Vec3
  up?: Vec3
}

/**
 * Drives the enclosing camera from props - the scene's, or inside a
 * `<View3d>` that view's (both have a default camera, so this component is
 * optional). The camera is target state, not a tree node: to orbit it,
 * update `position`/`lookAt`. The Scene and View3d `camera` props drive the
 * same state (a partial CameraUpdate, ortho included) - use one form, not
 * both.
 */
export let PerspectiveCamera: VoidComponent<PerspectiveCameraProps> = props => {
  let ctx = useContext(SceneContext)
  createEffect(
    () => [props.fov, props.near, props.far, props.position, props.lookAt, props.up] as const,
    ([fov, near, far, position, lookAt, up]) =>
      ctx.camera.setCamera({ fov, near, far, position, target: lookAt, up }),
  )
  return null
}
