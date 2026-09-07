import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { addGroup, removeGroup, setGroup, setGroupTransition } from "../layer.ts"
import type { SpriteGroup, SpritePointerEvent, SpriteTapEvent, SpriteWheelEvent, TransitionEndEvent } from "../layer.ts"
import type { NodeTransition } from "flux:spatial"
import { GroupContext, LayerContext } from "./context.ts"

export type GroupProps = {
  /** Position in the parent frame (layer pixels at the root). */
  x?: number
  y?: number
  /** Rotation, radians, clockwise. */
  rotation?: number
  /** Uniform scale on the whole subtree (child sprites scale with it). */
  scale?: number
  /** Show or hide the whole subtree (default true); see
   * GroupOptions.visible. */
  visible?: boolean
  /** Bubbled from a hit child sprite (see SpritePointerProps); a group
   * never receives enter/leave. */
  onPointerDown?: (event: SpritePointerEvent) => void
  onPointerMove?: (event: SpritePointerEvent) => void
  onPointerUp?: (event: SpritePointerEvent) => void
  onWheel?: (event: SpriteWheelEvent) => void
  onTap?: (event: SpriteTapEvent) => void
  /** How pose-prop changes animate (see setGroupTransition); the mount
   * pose always snaps. */
  transition?: NodeTransition | string | null
  /** A declared transition settled on one component. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  ref?: (group: SpriteGroup) => void
}

/**
 * A transform group: `<Sprite>` (and nested `<Group>`) children mount under
 * its spatial arena node, so their pose props read in the group's frame and
 * moving the group moves the subtree in one native recompute - a ship with
 * turrets is one `<Group>` with the hull and turret sprites inside. Renders
 * nothing itself.
 */
export let Group: ParentComponent<GroupProps> = props => {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let group = untrack(() => addGroup(layer, { parent }))
  createEffect(
    () => [props.x, props.y, props.rotation, props.scale, props.visible] as const,
    ([x, y, rotation, scale, visible]) => setGroup(group, { x, y, rotation, scale, visible: visible !== false }),
  )
  // After the pose effect, so the mount pose snaps before writes animate.
  createEffect(
    () => props.transition,
    transition => setGroupTransition(group, transition ?? null),
  )
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, wheel, tap, end]) => {
      group.onPointerDown = down
      group.onPointerMove = move
      group.onPointerUp = up
      group.onWheel = wheel
      group.onTap = tap
      group.onTransitionEnd = end
    },
  )
  untrack(() => props.ref)?.(group)
  onCleanup(() => removeGroup(group))
  return <GroupContext value={group}>{props.children}</GroupContext>
}
