import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps, PointerEventProps } from "./node-props.ts"
import { add, destroy, setMorphWeights } from "../node.ts"
import type { MorphWeights } from "../node.ts"
import { createMesh, setCastShadow, setCulling, setGeometry, setLayers, setMaterial, setMeshParams, setRenderOrder } from "../mesh.ts"
import type { Mesh as MeshNode } from "../mesh.ts"
import type { ShaderParams } from "@solidrt/core/gpu"
import type { Geometry } from "../geometry.ts"
import type { Material } from "../material.ts"

export type MeshProps = TransformProps & PointerEventProps & {
  geometry: Geometry
  material: Material
  /** Per-mesh uniforms for a custom material (setMeshParams as a prop).
   * Keys merge - a key that disappears keeps its old value; there is no
   * unset. Names must be declared by the material's shaders. For values
   * changing every frame prefer `ref` + setMeshParams from onFrame, the
   * same split as setTransform. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  /** Draw into the scene's shadow map (setCastShadow as a prop); default
   * false. Needs a `castShadow` light to show. */
  castShadow?: boolean
  /** Layer membership bitmask (setLayers as a prop; default 1): a target
   * draws the mesh when its mask intersects this. Not inherited from
   * ancestor Groups. */
  layers?: number
  /** Frustum culling switch (setCulling as a prop; default true). */
  frustumCulled?: boolean
  /** World units the culled box grows by (setCulling as a prop; default 0). */
  cullMargin?: number
  /** Morph target weights (setMorphWeights as a prop) for geometry that
   * carries `morphs` under a `morph: true` material: by name (keys merge,
   * a key that disappears keeps its weight) or every weight in target
   * order. A `weights` entry in `transition` animates each change. For
   * weights changing every frame prefer `ref` + setMorphWeights from
   * onFrame, the same split as setTransform. */
  morphWeights?: MorphWeights
  ref?: (mesh: MeshNode) => void
}

// The mesh-side props every mesh component shares (Sprite has no
// geometry and casts no shadow, so those stay with the components that
// take them); the
// ref and the cleanup are each component's own (a populated mesh frees
// its buffers, a plain one is removed).
export function syncMesh(mesh: MeshNode, props: Omit<MeshProps, "geometry" | "material" | "castShadow" | "ref"> & { material?: Material }): void {
  // Absent on an <InstancedLod>, whose levels each carry their own.
  createEffect(
    () => props.material,
    m => {
      if (m !== undefined) setMaterial(mesh, m)
    },
    { defer: true },
  )
  createEffect(
    () => props.params,
    p => {
      if (p !== undefined) setMeshParams(mesh, p)
    },
  )
  createEffect(
    () => props.renderOrder,
    o => setRenderOrder(mesh, o ?? 0),
  )
  createEffect(
    () => props.layers,
    l => setLayers(mesh, l ?? 1),
  )
  createEffect(
    () => [props.frustumCulled, props.cullMargin] as const,
    ([culled, margin]) => setCulling(mesh, { frustumCulled: culled !== false, cullMargin: margin ?? 0 }),
  )
  syncNode(mesh, props)
}

/** One draw entry: geometry drawn with a material at a transform. */
export let Mesh: VoidComponent<MeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createMesh(props.geometry, props.material))
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
  createEffect(
    () => props.morphWeights,
    w => {
      if (w !== undefined) setMorphWeights(mesh, w)
    },
  )
  syncMesh(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => destroy(mesh))
  return null
}

// The props both populated meshes share with Mesh: everything but the
// geometry/material pair (documented per component), the ref and the
// morph weights (a populated mesh does not morph yet).
export type PopulatedMeshProps = Omit<MeshProps, "geometry" | "material" | "ref" | "morphWeights">
