import { callRef, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { add, destroy } from "../node.ts"
import { disposeInstances, setRecordCount, setRecordLod } from "../mesh.ts"
import { createSplatMesh } from "../splat.ts"
import type { SplatMesh as SplatMeshNode, SplatMeshOptions } from "../splat.ts"
import type { SplatData } from "../splat-data.ts"

export type SplatMeshProps = PopulatedMeshProps & {
  /** The baked cloud (loadSplat). Write-once: the records went to the
   * engine and the data textures at creation, so treat the SplatData as
   * consumed and swap clouds by remounting (a keyed <Show>), not by a new
   * `data` value. */
  data: SplatData
  /** The most splats drawn; default all. A budget the engine's
   * selection fits by thinning every chunk in view by importance (see
   * SplatMeshOptions). */
  count?: number
  /** The LOD threshold in pixels of contribution (see SplatMeshOptions);
   * default SPLAT_LOD_THRESHOLD, 0 for the budget alone. */
  lodThreshold?: number
  /** A custom splat material (see SplatMeshOptions); fixed at creation. */
  material?: SplatMeshOptions["material"]
  ref?: (mesh: SplatMeshNode) => void
}

/** A baked splat cloud in the scene (createSplatMesh as a component): an
 * ordinary record mesh in the indexed form, culled and thinned by chunk
 * and drawn back to front by the core-side order with the scene feeding
 * the view - no per-frame JS anywhere. The id buffer, the engine's record
 * copy and the data textures are component-owned and freed on unmount. */
export let SplatMesh: VoidComponent<SplatMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createSplatMesh(props.data, { count: props.count, lodThreshold: props.lodThreshold, material: props.material }))
  add(ctx.parent, mesh)
  createEffect(
    () => props.count,
    c => {
      if (c !== undefined) setRecordCount(mesh, c)
    },
    { defer: true },
  )
  createEffect(
    () => props.lodThreshold,
    t => {
      if (t !== undefined) setRecordLod(mesh, t)
    },
    { defer: true },
  )
  syncMesh(mesh, props)
  callRef(() => props.ref, mesh)
  onCleanup(() => {
    destroy(mesh)
    disposeInstances(mesh)
  })
  return null
}
