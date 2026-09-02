// Pointer dispatch behind scene.handlers: element pointer events in,
// mesh/group handler calls out - nearest hit wins, down/move/up bubble
// mesh -> ancestors, pointer-down captures until up, enter/leave pair on
// hover changes. One system per scene; everything it needs of the scene
// is `pick` and the target size (makePointerInput's deps), the
// capture/hover bookkeeping is its own.

import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
import type { Vec3 } from "./math.ts"
import type { SceneNode, ScenePointerEvent } from "./node.ts"
import type { Mesh } from "./mesh.ts"
import type { Hit, SceneHandlers } from "./scene.ts"

type BubbleName = "onPointerDown" | "onPointerMove" | "onPointerUp"
type InternalEvent = ScenePointerEvent & { _stopped: boolean }

export type PointerInputDeps = {
  /** The scene's pick: camera ray through a scene pixel, hits nearest
   * first. */
  pick(x: number, y: number): Hit[]
  /** The scene target's current size (handlersFor's layout scaling). */
  targetSize(): { width: number; height: number }
}

export type PointerInput = {
  /** scene.handlers: for a leaf laid out at the target size. */
  handlers: SceneHandlers
  /** scene.handlersFor: for a leaf whose layout size differs (the
   * supersampling pattern); `layout` is read per event. */
  handlersFor(layout: () => { width: number; height: number }): SceneHandlers
}

export function makePointerInput(deps: PointerInputDeps): PointerInput {
  let capture = new Map<number, Mesh>()
  let hover = new Map<number, Mesh>()

  let makeEvent = (e: ElementPointerEvent, mesh: Mesh, x: number, y: number, point: Vec3 | null, distance: number | null): InternalEvent => {
    let event: InternalEvent = {
      mesh,
      currentTarget: mesh,
      point,
      distance,
      x,
      y,
      pointerId: e.pointerId,
      pointerType: e.pointerType,
      button: e.button,
      shiftKey: e.shiftKey,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      metaKey: e.metaKey,
      _stopped: false,
      stopPropagation() {
        event._stopped = true
      },
    }
    return event
  }

  let bubble = (name: BubbleName, event: InternalEvent): void => {
    for (let n: SceneNode | null = event.mesh; n !== null && !event._stopped; n = n.parent) {
      let handler = n[name]
      if (handler) {
        event.currentTarget = n
        handler(event)
      }
    }
  }

  // The captured mesh's own hit, if the ray still strikes it.
  let hitOn = (mesh: Mesh, x: number, y: number): Hit | null => {
    for (let h of deps.pick(x, y)) if (h.mesh === mesh) return h
    return null
  }

  // localX/localY arrive in the leaf's LAYOUT frame (the hit test undoes
  // every transform above it, design-size fits included), so a leaf laid out at
  // the target size - the built-in <Scene> leaf, a d-texture at natural
  // size - is already in scene pixels. Only a leaf deliberately laid out at
  // a DIFFERENT size (the supersampling pattern) needs the ratio, and only
  // the app knows that layout: handlersFor takes it.
  let makeHandlers = (layout: (() => { width: number; height: number }) | null): SceneHandlers => {
    let eventX = 0
    let eventY = 0
    let toScene = (e: ElementPointerEvent): void => {
      if (layout === null) {
        eventX = e.localX
        eventY = e.localY
        return
      }
      let l = layout()
      let size = deps.targetSize()
      eventX = e.localX * (l.width > 0 ? size.width / l.width : 1)
      eventY = e.localY * (l.height > 0 ? size.height / l.height : 1)
    }
    return {
      onPointerDown(e) {
        toScene(e)
        let hit = deps.pick(eventX, eventY)[0]
        if (hit === undefined) return
        capture.set(e.pointerId, hit.mesh)
        bubble("onPointerDown", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
      },
      onPointerMove(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          let hit = hitOn(captured, eventX, eventY)
          bubble("onPointerMove", makeEvent(e, captured, eventX, eventY, hit ? hit.point : null, hit ? hit.distance : null))
          return
        }
        let hit = deps.pick(eventX, eventY)[0]
        let prev = hover.get(e.pointerId)
        if (prev !== hit?.mesh) {
          if (prev !== undefined) {
            hover.delete(e.pointerId)
            prev.onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null, null))
          }
          if (hit !== undefined) {
            hover.set(e.pointerId, hit.mesh)
            hit.mesh.onPointerEnter?.(makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
          }
        }
        if (hit !== undefined) {
          bubble("onPointerMove", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
        }
      },
      onPointerUp(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          capture.delete(e.pointerId)
          let hit = hitOn(captured, eventX, eventY)
          bubble("onPointerUp", makeEvent(e, captured, eventX, eventY, hit ? hit.point : null, hit ? hit.distance : null))
          return
        }
        let hit = deps.pick(eventX, eventY)[0]
        if (hit !== undefined) {
          bubble("onPointerUp", makeEvent(e, hit.mesh, eventX, eventY, hit.point, hit.distance))
        }
      },
      onPointerLeave(e) {
        let prev = hover.get(e.pointerId)
        if (prev !== undefined) {
          hover.delete(e.pointerId)
          toScene(e)
          prev.onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null, null))
        }
      },
    }
  }

  return { handlers: makeHandlers(null), handlersFor: makeHandlers }
}
