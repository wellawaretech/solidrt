import { createContext, createEffect, onCleanup, onFrame, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import { createShots } from "../shots.ts"
import type { ShotsHandle } from "../shots.ts"

// Cap on the blend loop's per-frame dt in seconds, so the first tick
// after a suspended stretch cannot leap a blend to its end.
const MAX_BLEND_DT = 0.1

let ShotsContext = createContext<ShotsHandle>()

export type ShotsProps = {
  /** The default blend time in seconds when the live shot changes
   * (0.5); 0 cuts. */
  blend?: number
  /** The blender's handle (activate/deactivate/live/camera). */
  ref?: (shots: ShotsHandle) => void
  children?: Element
}

export type ShotProps = {
  name: string
  /** The enabled shot with the highest priority is live (default 0;
   * the most recently enabled wins a tie). Read at mount. */
  priority?: number
  /** Whether the shot takes part (default true); live: false withdraws
   * it and the next by priority goes live, blended. */
  active?: boolean
  /** This shot's own blend time when it goes live or withdraws
   * (default: the container's). */
  blend?: number
  children?: Element
}

/**
 * createShots as a Scene (or View3d) child: owns the enclosing camera
 * and blends between its `<Shot>` children (Cinemachine's virtual
 * cameras), each of which provides a context whose `viewport` is the
 * shot's recording target - so an `<OrbitCamera>` or
 * `<FirstPersonCamera>` inside a `<Shot>` drives that shot, not the
 * scene. The blend runs on frames only while one is in flight. A shot's
 * picks and raycasts (`anchor`, `occlusion`) go through the owner's
 * actual camera, which is the shot's own only while it is live and at
 * rest.
 */
export let Shots: ParentComponent<ShotsProps> = props => {
  let ctx = useContext(SceneContext)
  let shots = untrack(() => createShots(ctx.viewport, { blend: props.blend }))
  createEffect(
    () => shots.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_BLEND_DT)
        last = now
        shots.update(dt)
      })
    },
  )
  untrack(() => props.ref)?.(shots)
  return <ShotsContext value={shots}>{props.children}</ShotsContext>
}

export let Shot: ParentComponent<ShotProps> = props => {
  let shots = useContext(ShotsContext)
  let ctx = useContext(SceneContext)
  let owner = ctx.viewport
  let name = untrack(() => props.name)
  let target = untrack(() => shots.shot(name, { priority: props.priority }))
  // The shot's context: its own camera, the owner's projection.
  let viewport = {
    setCamera: target.setCamera,
    camera: target.camera,
    size: () => owner.size(),
    pick: (x: number, y: number) => owner.pick(x, y),
    unproject: (x: number, y: number, w: number, out?: [number, number, number]) => owner.unproject(x, y, w, out),
    screenRay: (x: number, y: number) => owner.screenRay(x, y),
    raycast: (origin: [number, number, number], direction: [number, number, number], opts?: Parameters<typeof owner.raycast>[2]) => owner.raycast(origin, direction, opts),
  }
  createEffect(
    () => props.active ?? true,
    on => {
      let blend = untrack(() => props.blend)
      if (on) shots.activate(name, { blend })
      else shots.deactivate(name, { blend })
    },
  )
  onCleanup(() => shots.remove(name, { blend: untrack(() => props.blend) }))
  return <SceneContext value={{ scene: ctx.scene, parent: ctx.parent, viewport, pointer: ctx.pointer }}>{props.children}</SceneContext>
}
