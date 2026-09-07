import { createContext, useContext } from "@solidrt/core"
import type { SpriteGroup, SpriteLayer as LayerHandle } from "../layer.ts"

export let LayerContext = createContext<LayerHandle>()
export let GroupContext = createContext<SpriteGroup | null>(null)

type LayerCtx = { layer: LayerHandle; parent: SpriteGroup | null }

/**
 * The enclosing layer and parent group - the imperative escape hatch
 * inside a component subtree (throws outside a `<SpriteLayer>`), the same
 * shape as @solidrt/3d's useScene. `parent` is the enclosing `<Group>`'s
 * handle (null at the layer root); pass it on to addSprite/addGroup so
 * imperative sprites mount where the JSX sits.
 */
export function useSpriteLayer(): LayerCtx {
  return { layer: useContext(LayerContext), parent: useContext(GroupContext) }
}
