import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps } from "./node-props.ts"
import { add, remove } from "../node.ts"
import { createDirectionalLight, setLight } from "../light.ts"
import type { DirectionalLight as DirectionalLightNode, ShadowOptions } from "../light.ts"
import type { Vec3 } from "../math.ts"

export type DirectionalLightProps = TransformProps & {
  /** Travel direction in the node's local space; default [0, -1, 0]. */
  direction?: Vec3
  color?: Vec3
  intensity?: number
  /** Render a shadow map from this light (any directional light may;
   * each is a pass). Its shadow camera sits at the light's WORLD
   * position, so give a casting light a `position` above the scene. */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, camera frustum,
   * cascades, distance), merged key by key. */
  shadow?: ShadowOptions
  ref?: (light: DirectionalLightNode) => void
}

/** A directional light node (createDirectionalLight): a parent Group's
 * rotation turns it; up to MAX_LIGHTS per scene, in mount order. */
export let DirectionalLight: VoidComponent<DirectionalLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createDirectionalLight({
      direction: props.direction,
      color: props.color,
      intensity: props.intensity,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () => [props.direction, props.color, props.intensity, props.castShadow, props.shadow] as const,
    ([direction, color, intensity, castShadow, shadow]) => setLight(light, { direction, color, intensity, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}
