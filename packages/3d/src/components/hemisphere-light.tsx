import { createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { VoidComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { add, remove } from "../node.ts"
import { createHemisphereLight, setLight } from "../light.ts"
import type { HemisphereLight as HemisphereLightNode } from "../light.ts"
import type { Vec3 } from "../math.ts"

export type HemisphereLightProps = { sky?: Vec3; ground?: Vec3; intensity?: number; ref?: (light: HemisphereLightNode) => void }

/** The scene's ambient term as a node (createHemisphereLight); one per
 * scene, the last mounted wins. */
export let HemisphereLight: VoidComponent<HemisphereLightProps> = props => {
  let ctx = useContext(SceneContext)
  let light = untrack(() => createHemisphereLight({ sky: props.sky, ground: props.ground, intensity: props.intensity }))
  add(ctx.parent, light)
  createEffect(
    () => [props.sky, props.ground, props.intensity] as const,
    ([sky, ground, intensity]) => setLight(light, { sky, ground, intensity }),
  )
  untrack(() => props.ref)?.(light)
  onCleanup(() => remove(light))
  return null
}
