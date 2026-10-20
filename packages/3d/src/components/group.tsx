import { callRef, createEffect, onCleanup, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { SceneContext, provide } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, BubblingNodePointerProps } from "./node-props.ts"
import { add, createGroup, destroy } from "../node.ts"
import type { SceneNode } from "../node.ts"
import { setOccluder } from "../occluder.ts"
import type { OccluderSpec } from "../occluder.ts"

export type GroupProps = TransformProps &
  BubblingNodePointerProps & {
    /** An explicit occluder shape at this node (setOccluder as a prop): a
     * `{ quad }` wall, a `{ box }`, a `{ sphere }` or a proxy
     * `{ geometry }`; absent or null, nothing. See Culling. */
    occluder?: Exclude<OccluderSpec, string> | null
    ref?: (node: SceneNode) => void
  }

/** A transform node: children inherit its position/rotation/scale. Its
 * pointer events are the ones bubbling up from a hit descendant; a group
 * is never the struck node, so it takes no enter/leave. */
export let Group: ParentComponent<GroupProps> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  createEffect(
    () => props.occluder,
    o => setOccluder(node, o ?? null),
  )
  callRef(() => props.ref, node)
  onCleanup(() => destroy(node))
  return provide(ctx, node, props)
}
