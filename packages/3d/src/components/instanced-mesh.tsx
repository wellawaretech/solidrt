import { createContext, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { SceneContext, provide } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, PointerEventProps } from "./node-props.ts"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { add, destroy } from "../node.ts"
import { addInstance, createInstancedMesh, disposeInstances, setCastShadow, setGeometry, setInstanceStyle } from "../mesh.ts"
import type { InstancedMesh as InstancedMeshNode, InstanceNode } from "../mesh.ts"
import type { Geometry } from "../geometry.ts"
import type { Material } from "../material.ts"

export type InstancedMeshProps = PopulatedMeshProps & {
  geometry: Geometry
  /** An instanced material: a stock one with `instanced` (or
   * `instanceColors`, for a per-instance tint), or a class declaring
   * INSTANCE_MATRIX_ATTRIBUTES in slot 0 and any style layout in slot 1. */
  material: Material
  /** Instance slots reserved up front (default 64; fixed at creation).
   * Past it the buffers double into replacements - amortized, but size it
   * realistically to skip the copies. */
  capacity?: number
  /** LOCAL bounds covering the population ([minX..maxZ]), fixed at
   * creation: the mesh node's own box for the frustum test and the
   * transparent sort. Optional here - instances pick by themselves, and
   * without it the mesh culls by the union of their boxes. */
  bounds?: ArrayLike<number>
  /** Debug label for the record buffers. */
  label?: string
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. The stock instanced materials cast; a custom class needs a
   * `shadowVertex`. */
  castShadow?: boolean
  ref?: (mesh: InstancedMeshNode) => void
}

let InstancedMeshContext = createContext<InstancedMeshNode | null>(null)

/**
 * One draw entry covering N instance NODES (createInstancedMesh as a
 * component): `<Instance>` children populate it - each a scene node the
 * core places, so their `transition` props spring with zero per-frame
 * JS, and picking, pointer events, overlap and sweep name the instance
 * struck. `<Group>` children between the mesh and its instances are
 * squads (the records stay mesh-relative through them). The record
 * buffers are component-owned and freed on unmount.
 */
export let InstancedMesh: ParentComponent<InstancedMeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() =>
    createInstancedMesh(props.geometry, props.material, { capacity: props.capacity, bounds: props.bounds, label: props.label }),
  )
  add(ctx.parent, mesh)
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
  return (
    <SceneContext value={{ scene: ctx.scene, parent: mesh, viewport: ctx.viewport, pointer: ctx.pointer }}>
      <InstancedMeshContext value={mesh}>{props.children}</InstancedMeshContext>
    </SceneContext>
  )
}

export type InstanceProps = TransformProps & PointerEventProps & {
  /** The instance's style record (setInstanceStyle as a prop): the
   * floats of the material's slot-1 instance attributes - `[r, g, b, a]`
   * under a stock material's `instanceColors`. Reactive; absent, the
   * instance keeps the material's instanceStyle (white for a tint). */
  style?: ArrayLike<number>
  ref?: (instance: InstanceNode) => void
}

/**
 * One instance of the enclosing `<InstancedMesh>` (addInstance as a
 * component): a node placed under the nearest `<Group>` or `<Instance>`
 * inside the mesh, with the transform, transition and pointer props of
 * a `<Group>` plus `style`. Children (a `<Mesh>` headlight under a car
 * instance) mount under it. Throws outside an `<InstancedMesh>`.
 */
export let Instance: ParentComponent<InstanceProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = useContext(InstancedMeshContext)
  if (mesh === null) throw new Error("<Instance> must be inside an <InstancedMesh>")
  let instance = addInstance(mesh, undefined, ctx.parent)
  syncNode(instance, props)
  createEffect(
    () => props.style,
    s => {
      if (s !== undefined) setInstanceStyle(instance, s)
    },
  )
  untrack(() => props.ref)?.(instance)
  onCleanup(() => destroy(instance))
  return provide(ctx, instance, props)
}
