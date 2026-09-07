import { createContext, useContext } from "@solidrt/core"
import type { Element, PointerFeed } from "@solidrt/core"
import type { SceneNode } from "../node.ts"
import type { Scene as SceneHandle } from "../scene.ts"
import type { CameraState, CameraUpdate } from "../camera.ts"

/** The camera state a camera control drives: the scene, or a view (both
 * expose setCamera and camera()). */
export type CameraTarget = { setCamera(update: CameraUpdate): void; camera(): CameraState }

export type SceneCtx = { scene: SceneHandle; parent: SceneNode; viewport: CameraTarget; pointer: PointerFeed | null }
export let SceneContext = createContext<SceneCtx>()

/**
 * The enclosing scene, parent node, viewport and pointer feed - the
 * imperative escape hatch inside a component subtree (throws outside a
 * `<Scene>`). `viewport` and `pointer` are the nearest OWNER's: the
 * scene, or inside a `<View3d>` that view - what the camera-control
 * components drive, and the feed the owner's `pointer` prop handed in
 * (null when none was: the owner's leaf then feeds no gestures). The
 * same shape as @solidrt/2d's useSpriteLayer.
 */
export function useScene(): SceneCtx {
  return useContext(SceneContext)
}

// The scene context for a node component's children. Solid's provider is
// a root and two memos over the children, paid even when there are none,
// so it is built only when the JSX has children: the compiler emits the
// `children` getter only then, so the test is structural, not a read
// (probes/3d-instance-mount-bench.tsx has the per-row cost).
export function provide(ctx: SceneCtx, parent: SceneNode, props: { children?: Element }): Element | undefined {
  if (!("children" in props)) return undefined
  return <SceneContext value={{ scene: ctx.scene, parent, viewport: ctx.viewport, pointer: ctx.pointer }}>{props.children}</SceneContext>
}
