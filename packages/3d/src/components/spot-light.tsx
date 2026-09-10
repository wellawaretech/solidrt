import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps } from "./node-props.ts"
import { add, destroy } from "../node.ts"
import { createSpotLight, setLight } from "../light.ts"
import type { SpotLight as SpotLightNode, SpotShadowOptions } from "../light.ts"
import type { Vec3 } from "../math.ts"

export type SpotLightProps = TransformProps & {
  /** Aim direction in the node's local space; default [0, -1, 0], a
   * lamp pointing straight down. */
  direction?: Vec3
  color?: Vec3
  intensity?: number
  /** Falloff cutoff in world units (0 = no cutoff). */
  distance?: number
  /** Cone half-angle in DEGREES, (0, 90]; default 60 (degrees like
   * camera fov; Three's radians convert as `angle * 180 / PI`). */
  angle?: number
  /** 0..1 fraction of the cone fading to the rim; default 0. */
  penumbra?: number
  /** Falloff exponent; default 2 (inverse square). */
  decay?: number
  /** Render a shadow map from this light (a perspective map of its
   * cone; one shadow slot). */
  castShadow?: boolean
  /** Shadow-map options (mapSize, bias, normalBias, near), merged key
   * by key. */
  shadow?: SpotShadowOptions
  ref?: (light: SpotLightNode) => void
}

/** A spot light node (createSpotLight): a cone from the node's world
 * position along its local `direction` - give it a `position`, and aim
 * it with `direction` or a parent's rotation. Counts against MAX_LIGHTS
 * with the other non-ambient lights. */
export let SpotLight: VoidComponent<SpotLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() =>
    createSpotLight({
      direction: props.direction,
      color: props.color,
      intensity: props.intensity,
      distance: props.distance,
      angle: props.angle,
      penumbra: props.penumbra,
      decay: props.decay,
      castShadow: props.castShadow,
      shadow: props.shadow,
    }),
  )
  add(ctx.parent, light)
  syncNode(light, props)
  createEffect(
    () =>
      [props.direction, props.color, props.intensity, props.distance, props.angle, props.penumbra, props.decay, props.castShadow, props.shadow] as const,
    ([direction, color, intensity, distance, angle, penumbra, decay, castShadow, shadow]) =>
      setLight(light, { direction, color, intensity, distance, angle, penumbra, decay, castShadow, shadow }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => destroy(light))
  return null
}
