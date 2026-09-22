import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { ParentComponent } from "@solidrt/core"
import { LodContext, SceneContext, provide } from "./context.tsx"
import type { LodRegistry } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, BubblingPointerEventProps } from "./node-props.ts"
import { syncMesh } from "./mesh.tsx"
import type { PopulatedMeshProps } from "./mesh.tsx"
import { InstancedMeshContext } from "./instanced-mesh.tsx"
import { add, createGroup, destroy } from "../node.ts"
import type { SceneNode } from "../node.ts"
import { createInstancedLod, setLod } from "../lod.ts"
import type { InstancedLodLevel } from "../lod.ts"
import { disposeInstances, setCastShadow } from "../mesh.ts"
import type { InstancedMesh as InstancedMeshNode } from "../mesh.ts"

export type LodProps = TransformProps &
  BubblingPointerEventProps & {
    /** The cross-fade band fraction (createLod's `fade`); default 0. */
    fade?: number
    ref?: (node: SceneNode) => void
  }

/**
 * A LOD group (createLod as a component): its levels are the direct
 * children carrying `lodSize` - the projected size below which each hands
 * over - in any JSX order (levels sort by size, never by position, since
 * attach order is not JSX order); a child without one is drawn always.
 * Levels may come and go (`<Show>`): the group re-declares itself.
 */
export let Lod: ParentComponent<LodProps> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  let sizes = new Map<SceneNode, number>()
  let declare = () => {
    let levels = [...sizes.entries()].sort((a, b) => b[1] - a[1]).map(([n, size]) => ({ node: n, size }))
    setLod(node, levels, { fade: untrack(() => props.fade) })
  }
  let registry: LodRegistry = {
    group: node,
    register(child, size) {
      if (child.parent !== node) throw new Error("lodSize goes on a direct child of <Lod>")
      sizes.set(child, size)
      declare()
    },
    unregister(child) {
      if (sizes.delete(child)) declare()
    },
  }
  createEffect(
    () => props.fade,
    () => declare(),
    { defer: true },
  )
  untrack(() => props.ref)?.(node)
  onCleanup(() => destroy(node))
  return <LodContext value={registry}>{provide(ctx, node, props)}</LodContext>
}

// Bubbling pointer props only, as InstancedMesh: the group's node is not
// a hit target.
export type InstancedLodProps = Omit<PopulatedMeshProps, "onPointerEnter" | "onPointerLeave"> & {
  /** The levels nearest first, each what the population draws at it and
   * the per-instance projected size below which it hands over (see
   * createInstancedLod); fixed at creation. */
  levels: InstancedLodLevel[]
  /** Instance slots reserved up front (default 64; fixed at creation). */
  capacity?: number
  /** LOCAL bounds covering the population, fixed at creation (see
   * InstancedMesh). */
  bounds?: ArrayLike<number>
  label?: string
  /** Every level draws into the scene's shadow map; default false. */
  castShadow?: boolean
  ref?: (mesh: InstancedMeshNode) => void
}

/**
 * An instanced LOD (createInstancedLod as a component): `<Instance>`
 * children populate it exactly as under `<InstancedMesh>`, and each
 * instance draws the level its own projected size picks.
 */
export let InstancedLod: ParentComponent<InstancedLodProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createInstancedLod(props.levels, { capacity: props.capacity, bounds: props.bounds, label: props.label }))
  add(ctx.parent, mesh)
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
