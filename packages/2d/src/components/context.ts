import { createContext, useContext } from "@solidrt/core"
import type { SpriteGroup, SpriteLayer as LayerHandle } from "../layer.ts"
import type { ViewHandle } from "../views.ts"

export let LayerContext = createContext<LayerHandle>()
export let GroupContext = createContext<SpriteGroup | null>(null)

/**
 * The viewport a camera control drives and listens at: the layer, or a
 * view (a layer is a ViewHandle structurally). Wider than @solidrt/3d's
 * CameraTarget because a 2d control takes its input from the same
 * object's root (`listen`) and defaults its viewport to its size.
 */
export type CameraTarget = Pick<ViewHandle, "width" | "height" | "setCamera" | "camera" | "project" | "unproject" | "listen">

// The nearest view (null at the layer root): set by <View2d>, so the
// camera target below resolves to it inside one. Null-default on purpose,
// like GroupContext.
export let CameraContext = createContext<CameraTarget | null>(null)

type LayerCtx = { layer: LayerHandle; parent: SpriteGroup | null; camera: CameraTarget }

/**
 * The enclosing layer, parent group and camera target - the imperative
 * escape hatch inside a component subtree (throws outside a
 * `<SpriteLayer>`), the same shape as @solidrt/3d's useScene. `parent` is
 * the enclosing `<Group>`'s handle (null at the layer root); pass it on to
 * addSprite/addGroup so imperative sprites mount where the JSX sits.
 * `camera` is the nearest OWNER's viewport: the layer's, or inside a
 * `<View2d>` that view's - what a `<Camera2d>` drives.
 */
export function useSpriteLayer(): LayerCtx {
  let layer = useContext(LayerContext)
  return { layer, parent: useContext(GroupContext), camera: useContext(CameraContext) ?? layer }
}
