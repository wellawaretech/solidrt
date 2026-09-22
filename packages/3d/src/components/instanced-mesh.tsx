import { createContext, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { SceneContext, provide } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, PointerEventProps } from "./node-props.ts"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { add, destroy, setMorphWeights } from "../node.ts"
import type { MorphWeights, SceneNode } from "../node.ts"
import { addInstance, createInstancedMesh, disposeInstances, setCastShadow, setGeometry, setInstanceStyle } from "../mesh.ts"
import type { InstancedMesh as InstancedMeshNode, InstanceNode } from "../mesh.ts"
import type { Geometry } from "../geometry.ts"
import type { Material } from "../material.ts"

// The mesh's own node is never a hit target (its instances are the
// leaves that pick, its bounds cull only), so like a Group it bubbles
// and omits the hover pair.
export type InstancedMeshProps = Omit<PopulatedMeshProps, "onPointerEnter" | "onPointerLeave"> & {
  geometry: Geometry
  /** An instanced material: a stock one with `instanced` (or
   * `instanceColors`, for a per-instance tint), or a class declaring
   * INSTANCE_MATRIX_ATTRIBUTES first and any style layout second. */
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
  /** The node the instance records are relative to (default the mesh;
   * fixed at creation): an ANCESTOR the mesh sits under at identity, so
   * `<Instance mesh={...}>` children may live under any node of that
   * ancestor's subtree instead of inside this element - a population
   * whose copies ride a hierarchy the mesh is not the root of. Take the
   * ancestor from its `ref`; the chain between it and the mesh must stay
   * identity (see createInstancedMesh's `anchor`). */
  anchor?: SceneNode
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. The stock instanced materials cast; a custom class needs a
   * `shadowVertex`. */
  castShadow?: boolean
  ref?: (mesh: InstancedMeshNode) => void
}

/** The enclosing population, what `<Instance>` adds itself to: an
 * `<InstancedMesh>`, or an `<InstancedLod>`'s first level. */
export let InstancedMeshContext = createContext<InstancedMeshNode | null>(null)

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
    createInstancedMesh(props.geometry, props.material, { capacity: props.capacity, bounds: props.bounds, anchor: props.anchor, label: props.label }),
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
  /** The instance's style record (setInstanceStyle as a prop): one value
   * per component of the material's second instance buffer, in order -
   * `[r, g, b, a]` under a stock material's `instanceColors`. Reactive;
   * absent, the instance keeps the material's instanceStyle (white for
   * a tint). */
  style?: ArrayLike<number>
  /** The instance's morph target weights (setMorphWeights as a prop),
   * for a population over geometry with `morphs` under a `morph: true`
   * material: by name (keys merge) or every weight in target order; a
   * `weights` entry in `transition` animates each change. */
  morphWeights?: MorphWeights
  /** The population this instance belongs to when it is NOT nested inside
   * its `<InstancedMesh>` (fixed at creation): a mesh created with an
   * `anchor`, whose instances may sit under any node of the anchor's
   * subtree - take it from the mesh's `ref`. Absent, the enclosing
   * `<InstancedMesh>` is the population. */
  mesh?: InstancedMeshNode
  ref?: (instance: InstanceNode) => void
}

/**
 * One instance of the enclosing `<InstancedMesh>` (addInstance as a
 * component): a node placed under the nearest `<Group>` or `<Instance>`
 * inside the mesh, with the transform, transition and pointer props of
 * a `<Group>` plus `style`. Children (a `<Mesh>` headlight under a car
 * instance) mount under it. Throws outside an `<InstancedMesh>` unless
 * `mesh` names one (an anchored population's instance placed elsewhere
 * in the anchor's subtree).
 */
export let Instance: ParentComponent<InstanceProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => props.mesh) ?? useContext(InstancedMeshContext)
  if (mesh === null) throw new Error("<Instance> must be inside an <InstancedMesh>, or name its population with `mesh`")
  let instance = addInstance(mesh, undefined, ctx.parent)
  syncNode(instance, props)
  createEffect(
    () => props.style,
    s => {
      if (s !== undefined) setInstanceStyle(instance, s)
    },
  )
  createEffect(
    () => props.morphWeights,
    w => {
      if (w !== undefined) setMorphWeights(instance, w)
    },
  )
  untrack(() => props.ref)?.(instance)
  onCleanup(() => destroy(instance))
  return provide(ctx, instance, props)
}
