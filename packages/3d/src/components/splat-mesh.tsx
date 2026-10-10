import { callRef, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { add, destroy } from "../node.ts"
import { disposeInstances, setRecordCount } from "../mesh.ts"
import { createSplatMesh } from "../splat.ts"
import type { SplatMesh as SplatMeshNode, SplatMeshOptions } from "../splat.ts"
import type { SplatData } from "../splat-data.ts"

export type SplatMeshProps = PopulatedMeshProps & {
  /** The baked cloud (loadSplat). Write-once: the records went to the
   * engine and the data textures at creation, so treat the SplatData as
   * consumed and swap clouds by remounting (a keyed <Show>), not by a new
   * `data` value. */
  data: SplatData
  /** How many splats draw; default all. Records are importance-sorted at
   * bake, so the first n are the scene at n - the LOD/bench dial (rounded
   * up to whole groups of SPLAT_GROUP). */
  count?: number
  /** A custom splat material (see SplatMeshOptions); fixed at creation. */
  material?: SplatMeshOptions["material"]
  ref?: (mesh: SplatMeshNode) => void
}

/** A baked splat cloud in the scene (createSplatMesh as a component): an
 * ordinary record mesh in the indexed form, drawn back to front by the
 * core-side order with the scene feeding the view direction - no
 * per-frame JS anywhere. The id buffer, the engine's record copy and the
 * data textures are component-owned and freed on unmount. */
export let SplatMesh: VoidComponent<SplatMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createSplatMesh(props.data, { count: props.count, material: props.material }))
  add(ctx.parent, mesh)
  createEffect(
    () => props.count,
    c => {
      if (c !== undefined) setRecordCount(mesh, c)
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
