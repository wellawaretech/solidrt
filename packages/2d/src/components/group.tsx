import { callRef, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { addGroup, destroyGroup, setGroup, setGroupTimeScale, setGroupTransition } from "../layer.ts"
import type { SpriteGroup, TransitionEndEvent } from "../layer.ts"
import type { SpriteTransition } from "../layer.ts"
import { GroupContext, LayerContext } from "./context.ts"
import type { BubblingSpritePointerProps } from "./sprite.tsx"

/** A group's props: the pose, the bubbling pointer events (from a hit
 * child sprite; a group never receives enter/leave) and the transition. */
export type GroupProps = BubblingSpritePointerProps & {
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
  /** How pose-prop changes animate (see setGroupTransition). The mount
   * pose snaps, unless a component's `from` animates it in from there. */
  transition?: SpriteTransition | string | null
  /** A declared transition settled on one component. */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  /** The rate the native motion under this group runs at (see
   * setGroupTimeScale): 0 freezes the subtree, 1 is app time; the nearest
   * declaring node wins. Undefined or null inherits. */
  timeScale?: number | null
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
  // Built only when the prop is present (the compiler emits the getter
  // only then), so a plain group pays no effect for it.
  if ("timeScale" in props) {
    createEffect(
      () => props.timeScale,
      scale => setGroupTimeScale(group, scale ?? null),
    )
  }
  createEffect(
    () => [props.onPointerDown, props.onPointerMove, props.onPointerUp, props.onPointerCancel, props.onWheel, props.onTap, props.onTransitionEnd] as const,
    ([down, move, up, cancel, wheel, tap, end]) => {
      group.onPointerDown = down
      group.onPointerMove = move
      group.onPointerUp = up
      group.onPointerCancel = cancel
      group.onWheel = wheel
      group.onTap = tap
      group.onTransitionEnd = end
    },
  )
  callRef(() => props.ref, group)
  onCleanup(() => destroyGroup(group))
  return <GroupContext value={group}>{props.children}</GroupContext>
}
