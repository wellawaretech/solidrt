// Pointer dispatch behind scene.handlers: element pointer events in,
// instance/mesh/group handler calls out - nearest hit wins, down/move/up
// bubble from the struck instance (or the mesh) up through the ancestors,
// pointer-down captures until up, enter/leave pair on hover changes. One
// system per scene; everything it needs of the scene is `pick` and the
// target size (makePointerInput's deps), the capture/hover bookkeeping is
// its own.

import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
import type { SceneNode, ScenePointerEvent } from "./node.ts"
import type { InstanceNode, Mesh } from "./mesh.ts"
import type { Hit, SceneHandlers } from "./scene.ts"

type BubbleName = "onPointerDown" | "onPointerMove" | "onPointerUp"
type InternalEvent = ScenePointerEvent & { _stopped: boolean }
// What a press captures and a hover tracks: the struck mesh, and the
// instance when the mesh is instanced.
type Target = { mesh: Mesh; instance: InstanceNode | null }

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
  let capture = new Map<number, Target>()
  let hover = new Map<number, Target>()

  let targetOf = (hit: Hit): Target => ({ mesh: hit.mesh, instance: hit.instance ?? null })
  let sameTarget = (hit: Hit, target: Target): boolean => hit.mesh === target.mesh && (hit.instance ?? null) === target.instance
  // The node the walk starts at, and the one enter/leave fire on.
  let nodeOf = (target: Target): SceneNode => target.instance ?? target.mesh

  let makeEvent = (e: ElementPointerEvent, target: Target, x: number, y: number, hit: Hit | null): InternalEvent => {
    let event: InternalEvent = {
      mesh: target.mesh,
      instance: target.instance,
      currentTarget: nodeOf(target),
      point: hit !== null ? hit.point : null,
      distance: hit !== null ? hit.distance : null,
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

  // The struck instance first (its chain passes through the mesh), then
  // the mesh, then its ancestors.
  let bubble = (name: BubbleName, event: InternalEvent): void => {
    for (let n: SceneNode | null = event.instance ?? event.mesh; n !== null && !event._stopped; n = n.parent) {
      let handler = n[name]
      if (handler) {
        event.currentTarget = n
        handler(event)
      }
    }
  }

  // The captured target's own hit, if the ray still strikes it.
  let hitOn = (target: Target, x: number, y: number): Hit | null => {
    for (let h of deps.pick(x, y)) if (sameTarget(h, target)) return h
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
        let target = targetOf(hit)
        capture.set(e.pointerId, target)
        bubble("onPointerDown", makeEvent(e, target, eventX, eventY, hit))
      },
      onPointerMove(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          bubble("onPointerMove", makeEvent(e, captured, eventX, eventY, hitOn(captured, eventX, eventY)))
          return
        }
        let hit = deps.pick(eventX, eventY)[0]
        let prev = hover.get(e.pointerId)
        let changed = prev === undefined ? hit !== undefined : hit === undefined || !sameTarget(hit, prev)
        if (changed) {
          if (prev !== undefined) {
            hover.delete(e.pointerId)
            nodeOf(prev).onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null))
          }
          if (hit !== undefined) {
            let target = targetOf(hit)
            hover.set(e.pointerId, target)
            nodeOf(target).onPointerEnter?.(makeEvent(e, target, eventX, eventY, hit))
          }
        }
        if (hit !== undefined) {
          bubble("onPointerMove", makeEvent(e, targetOf(hit), eventX, eventY, hit))
        }
      },
      onPointerUp(e) {
        toScene(e)
        let captured = capture.get(e.pointerId)
        if (captured !== undefined) {
          capture.delete(e.pointerId)
          bubble("onPointerUp", makeEvent(e, captured, eventX, eventY, hitOn(captured, eventX, eventY)))
          return
        }
        let hit = deps.pick(eventX, eventY)[0]
        if (hit !== undefined) {
          bubble("onPointerUp", makeEvent(e, targetOf(hit), eventX, eventY, hit))
        }
      },
      onPointerLeave(e) {
        let prev = hover.get(e.pointerId)
        if (prev !== undefined) {
          hover.delete(e.pointerId)
          toScene(e)
          nodeOf(prev).onPointerLeave?.(makeEvent(e, prev, eventX, eventY, null))
        }
      },
    }
  }

  return { handlers: makeHandlers(null), handlersFor: makeHandlers }
}
