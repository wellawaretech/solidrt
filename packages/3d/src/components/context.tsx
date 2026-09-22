import { createContext, useContext } from "@solidrt/core"
import type { Element, PointerFeed } from "@solidrt/core"
import type { SceneNode } from "../node.ts"
import type { Scene as SceneHandle } from "../scene.ts"
import type { CameraState, CameraUpdate } from "../camera.ts"
import type { Hit, QueryOptions, ScreenRay } from "../scene.ts"
import type { Vec3 } from "../math.ts"

/** The camera state a camera control drives: the scene, or a view (both
 * expose setCamera, camera(), size(), and the projection the control
 * components build their anchor, focus and occlusion hooks from). */
export type CameraTarget = {
  setCamera(update: CameraUpdate): void
  camera(): CameraState
  size(): { width: number; height: number }
  pick(x: number, y: number): Hit[]
  unproject(x: number, y: number, w: number, out?: Vec3): Vec3
  screenRay(x: number, y: number): ScreenRay
  raycast(origin: Vec3, direction: Vec3, opts?: QueryOptions): Hit[]
}

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

/** What a `<Lod>` hands its children: a direct child with `lodSize`
 * registers itself as a level. */
export type LodRegistry = {
  group: SceneNode
  register(node: SceneNode, size: number): void
  unregister(node: SceneNode): void
}
export let LodContext = createContext<LodRegistry | undefined>(undefined)
