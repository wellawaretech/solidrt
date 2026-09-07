import { createEffect } from "@solidrt/core"
import { setTransform, setTransition, setVisible } from "../node.ts"
import type { SceneNode, ScenePointerEvent, TransitionEndEvent } from "../node.ts"
import type { NodeTransition } from "flux:spatial"
import type { Quat, Vec3 } from "../math.ts"

export type TransformProps = {
  position?: Vec3
  /** Euler radians in XYZ order (x first), Three's `Euler` default. */
  rotation?: Vec3
  /** The rotation as a quaternion - what the node stores. Pass this or
   * `rotation`, not both. */
  quaternion?: Quat
  scale?: Vec3 | number
  visible?: boolean
  /** How transform-prop changes animate (see setTransition); the mount
   * transform always snaps. */
  transition?: NodeTransition | string | null
  /** A declared transition settled on one component. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
}

/**
 * Mesh pointer events, the element vocabulary one tree deeper: the nearest
 * hit mesh receives the event, down/move/up bubble to ancestor Groups
 * (stopPropagation stops the walk), enter/leave pair on the mesh alone.
 * Events flow while the element showing the scene carries scene.handlers -
 * the built-in <Scene> leaf does (opt out with events={false}); an `output`
 * leaf spreads them itself.
 */
export type PointerEventProps = {
  onPointerDown?: (event: ScenePointerEvent) => void
  onPointerMove?: (event: ScenePointerEvent) => void
  onPointerUp?: (event: ScenePointerEvent) => void
  /** Meshes only: a Group never receives enter/leave. */
  onPointerEnter?: (event: ScenePointerEvent) => void
  onPointerLeave?: (event: ScenePointerEvent) => void
}

export function syncNode(node: SceneNode, props: TransformProps & PointerEventProps): void {
  // One effect for the transform and the handlers: re-assigning six
  // handler fields on a transform write is free, an effect of its own is
  // not (a node component's mount cost is mostly its effects and its
  // context provider, see probes/3d-instance-mount-bench.tsx).
  createEffect(
    () =>
      [
        props.position,
        props.rotation,
        props.quaternion,
        props.scale,
        props.visible,
        props.onPointerDown,
        props.onPointerMove,
        props.onPointerUp,
        props.onPointerEnter,
        props.onPointerLeave,
        props.onTransitionEnd,
      ] as const,
    ([position, rotation, quaternion, scale, visible, down, move, up, enter, leave, end]) => {
      setTransform(node, { position, rotation, quaternion, scale })
      setVisible(node, visible !== false)
      node.onPointerDown = down
      node.onPointerMove = move
      node.onPointerUp = up
      node.onPointerEnter = enter
      node.onPointerLeave = leave
      node.onTransitionEnd = end
    },
  )
  // After the transform effect, so the mount transform snaps before writes
  // animate. Its own effect: merged above, every transform write would
  // re-send the config to the core.
  createEffect(
    () => props.transition,
    transition => setTransition(node, transition ?? null),
  )
}
