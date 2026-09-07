import { onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { SceneContext, provide } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, PointerEventProps } from "./node-props.ts"
import { add, createGroup, remove } from "../node.ts"
import type { SceneNode } from "../node.ts"

/** A transform node: children inherit its position/rotation/scale. */
export let Group: ParentComponent<TransformProps & PointerEventProps & { ref?: (node: SceneNode) => void }> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  untrack(() => props.ref)?.(node)
  onCleanup(() => remove(node))
  return provide(ctx, node, props)
}
