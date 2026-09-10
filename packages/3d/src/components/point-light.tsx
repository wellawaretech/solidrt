import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps } from "./node-props.ts"
import { add, destroy } from "../node.ts"
import { createPointLight, setLight } from "../light.ts"
import type { PointLight as PointLightNode, SpotShadowOptions } from "../light.ts"
import type { Vec3 } from "../math.ts"

export type PointLightProps = TransformProps & {
  color?: Vec3
  intensity?: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance?: number
  /** Falloff exponent; default 2 (inverse square). */
  decay?: number
  /** Render shadow maps from this light (six 90-degree face maps, one
   * per cube face; six shadow slots). Far = `distance`, so give a
   * casting bulb one. */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, near), merged key
   * by key. */
  shadow?: SpotShadowOptions
  ref?: (light: PointLightNode) => void
}

/** A point light node (createPointLight): light in every direction from
 * the node's world position - give it a `position`; rotation does not
 * matter. Counts against MAX_LIGHTS with the other non-ambient lights. */
export let PointLight: VoidComponent<PointLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createPointLight({
      color: props.color,
      intensity: props.intensity,
      distance: props.distance,
      decay: props.decay,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () => [props.color, props.intensity, props.distance, props.decay, props.castShadow, props.shadow] as const,
    ([color, intensity, distance, decay, castShadow, shadow]) => setLight(light, { color, intensity, distance, decay, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => destroy(light))
  return null
}
