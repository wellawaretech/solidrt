import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { add, destroy } from "../node.ts"
import { createRecordMesh, disposeInstances, setCastShadow, setGeometry, setRecordCount, setRecords } from "../mesh.ts"
import type { RecordMesh as RecordMeshNode } from "../mesh.ts"
import type { Geometry } from "../geometry.ts"
import type { Material } from "../material.ts"

export type RecordMeshProps = PopulatedMeshProps & {
  geometry: Geometry
  /** Must declare instanceBuffers (shaderMaterialClass), the record buffer only. */
  material: Material
  /** Interleaved per-instance records (stride = the material's instance
   * attributes summed). Reactive; a later array larger than the buffer
   * grows it (capacity doubles into a replacement buffer). */
  records: Float32Array
  /** How many records draw; default all of the latest `records`. */
  count?: number
  /** LOCAL bounds covering every instance ([minX..maxZ]), fixed at
   * creation. Without them the mesh has no picking leaf, so pointer events
   * never target it. */
  bounds?: ArrayLike<number>
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. Needs a `castShadow` light AND a material class declaring
   * `shadowVertex` (the depth pass with the instance placement) - the
   * shadow views skip a populated mesh without one. */
  castShadow?: boolean
  ref?: (mesh: RecordMeshNode) => void
}

/** One draw entry covering N records: geometry repeated per record of
 * `records` (createRecordMesh as a component, the JS-written population).
 * The record buffer is component-owned and freed on unmount. */
export let RecordMesh: VoidComponent<RecordMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() =>
    createRecordMesh(props.geometry, props.material, props.records, props.count, { bounds: props.bounds }),
  )
  add(ctx.parent, mesh)
  createEffect(
    () => props.records,
    r => setRecords(mesh, r, untrack(() => props.count)),
    { defer: true },
  )
  createEffect(
    () => props.count,
    c => {
      if (c !== undefined) setRecordCount(mesh, c)
    },
    { defer: true },
  )
  createEffect(
    () => props.geometry,
    g => setGeometry(mesh, g),
    { defer: true },
  )
  createEffect(
    () => props.castShadow,
    c => setCastShadow(mesh, c === true),
  )
  syncMesh(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => {
    destroy(mesh)
    disposeInstances(mesh)
  })
  return null
}
