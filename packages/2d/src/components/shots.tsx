import { createContext, createEffect, onCleanup, onFrame, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent } from "@solidrt/core"
import { ViewportContext, useSpriteLayer } from "./context.ts"
import { createShots } from "../shots.ts"
import type { ShotsHandle } from "../shots.ts"
import type { ViewHandle } from "../views.ts"

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
 * createShots as a SpriteLayer (or View2d) child: owns the nearest
 * view's camera and blends between its `<Shot>` children (Cinemachine's
 * virtual cameras), each of which provides the view with its camera
 * redirected to the shot's recording target - so a `<Camera2d>` inside
 * a `<Shot>` drives that shot, not the view, while its size, handlers
 * and everything else stay the view's. The blend runs on frames only
 * while one is in flight.
 */
export let Shots: ParentComponent<ShotsProps> = props => {
  let view = useSpriteLayer().viewport
  let shots = untrack(() => createShots(view, { blend: props.blend }))
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
  let view = useSpriteLayer().viewport
  let name = untrack(() => props.name)
  let target = untrack(() => shots.shot(name, { priority: props.priority }))
  // The view with its camera redirected: a Proxy keeps the view's live
  // fields (width, height, handlers) live.
  let redirected = new Proxy(view, {
    get(t, key, receiver) {
      if (key === "setCamera") return target.setCamera
      if (key === "camera") return target.camera
      return Reflect.get(t, key, receiver)
    },
  }) as ViewHandle
  createEffect(
    () => props.active ?? true,
    on => {
      let blend = untrack(() => props.blend)
      if (on) shots.activate(name, { blend })
      else shots.deactivate(name, { blend })
    },
  )
  onCleanup(() => shots.remove(name, { blend: untrack(() => props.blend) }))
  return <ViewportContext value={redirected}>{props.children}</ViewportContext>
}
