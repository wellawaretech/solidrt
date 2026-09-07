import { onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import type { TransformProps, PointerEventProps } from "./node-props.ts"
import { syncMesh } from "./mesh.tsx"
import { add, remove } from "../node.ts"
import { createSprite } from "../mesh.ts"
import type { Mesh as MeshNode } from "../mesh.ts"
import type { ShaderParams } from "@solidrt/core/gpu"
import type { Material } from "../material.ts"

export type SpriteProps = TransformProps & PointerEventProps & {
  /** A `sprite()` material (any material draws, only a sprite one turns). */
  material: Material
  /** Per-mesh uniforms, merge semantics - as on Mesh. */
  params?: ShaderParams
  /** Explicit draw-order key (setRenderOrder as a prop); default 0. */
  renderOrder?: number
  /** Layer membership bitmask (setLayers as a prop; default 1). */
  layers?: number
  /** Frustum culling switch (setCulling as a prop; default true). */
  frustumCulled?: boolean
  /** World units the culled box grows by (setCulling as a prop; default 0). */
  cullMargin?: number
  ref?: (mesh: MeshNode) => void
}

/** A camera-facing unit quad (createSprite as a component): no geometry
 * prop, `scale` is its world size, rotation is ignored. */
export let Sprite: VoidComponent<SpriteProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createSprite(props.material))
  add(ctx.parent, mesh)
  syncMesh(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => remove(mesh))
  return null
}
