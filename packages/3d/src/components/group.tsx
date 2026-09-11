import { onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { SceneContext, provide } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, BubblingPointerEventProps } from "./node-props.ts"
import { add, createGroup, destroy } from "../node.ts"
import type { SceneNode } from "../node.ts"

export type GroupProps = TransformProps & BubblingPointerEventProps & { ref?: (node: SceneNode) => void }

/** A transform node: children inherit its position/rotation/scale. Its
 * pointer events are the ones bubbling up from a hit descendant; a group
 * is never the struck node, so it takes no enter/leave. */
export let Group: ParentComponent<GroupProps> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  untrack(() => props.ref)?.(node)
  onCleanup(() => destroy(node))
  return provide(ctx, node, props)
}
