import { createContext, useContext } from "@solidrt/core"
import type { SpriteGroup, SpriteLayer as LayerHandle } from "../layer.ts"
import type { ViewHandle } from "../views.ts"

export let LayerContext = createContext<LayerHandle>()
export let GroupContext = createContext<SpriteGroup | null>(null)

// The nearest view: the <SpriteLayer>'s own, or the enclosing <View2d>'s;
// null under a leafless layer (output={false}) until a <View2d>
// provides one. Null-default on purpose, like GroupContext.
export let ViewportContext = createContext<ViewHandle | null>(null)

type LayerCtx = { layer: LayerHandle; parent: SpriteGroup | null; readonly viewport: ViewHandle }

/**
 * The enclosing layer, parent group and view - the imperative escape
 * hatch inside a component subtree (throws outside a `<SpriteLayer>`),
 * the same shape as @solidrt/3d's useScene. `parent` is the enclosing
 * `<Group>`'s handle (null at the layer root); pass it on to
 * addSprite/addGroup so imperative sprites mount where the JSX sits.
 * `viewport` is the nearest view: the `<SpriteLayer>`'s own, or inside a
 * `<View2d>` that view - what a `<Camera2d>` drives, and where a custom
 * `output` leaf takes its `handlers`. Reading it under a
 * `<SpriteLayer output={false}>` throws: such a layer shows only through
 * its `<View2d>` children, so controls and leaves go inside one.
 */
export function useSpriteLayer(): LayerCtx {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let viewport = useContext(ViewportContext)
  return {
    layer,
    parent,
    get viewport() {
      if (viewport === null) {
        throw new Error("useSpriteLayer: no view here - a <SpriteLayer output={false}> shows only through its <View2d> children; put camera controls and leaves inside one")
      }
      return viewport
    },
  }
}
