// Pointer dispatch behind scene.handlers (and a view's): the element event
// model one tree deeper, with the SCENE - or the view whose leaf received
// the event - as the root of the walk. Down, move, up and wheel dispatch
// on the nearest hit, bubble from the struck instance (when the mesh is
// instanced) through the mesh and its ancestors and end at the root's
// listeners; over empty space the walk is the root alone (`mesh` null).
// Claiming is stopPropagation: a handler that stops an event keeps every
// later target, the root included, from seeing it, and the DOWN decides
// the press - a pointer whose down never reached the root stays claimed
// by the node chain for its move, up and tap too, so a mesh that drags
// itself stops its down once and an orbit control fed at the root never
// turns. Capture is per pointer to the press target, the root included: a
// drag from empty space keeps delivering to the root as it crosses meshes,
// exactly as a drag from a mesh keeps naming that mesh. Enter/leave pair
// on the struck node alone. Taps are synthesized here (DOM's click,
// Unity's IPointerClickHandler): a press that releases on the same target
// within the slop, the only pointer down for its whole press, counted up
// for repeats (DOM's detail, Unity's clickCount). @solidrt/2d's
// dispatch.ts is the same model one dimension down.
//
// Pure BY DESIGN (types only, nothing with GPU imports) so
// checks/dispatch-check.ts drives it headless with a fake pick.

import type { PointerEvent as ElementPointerEvent, PointerFeed } from "@solidrt/core"
import type { NodePointerEvent, SceneNode, ScenePointerListener } from "./node.ts"
import type { InstanceNode, Mesh } from "./mesh.ts"
import type { Hit, Scene, SceneHandlers, ViewHandle } from "./scene.ts"
import type { Vec3 } from "./math.ts"

// Finger travel from the down point, in window pixels, past which a press
// is a drag and never a tap. Core's pan and transform recognizers engage
// at the same 8, so a press is never both a tap and a drag.
const TAP_SLOP = 8
// Successive taps closer in time than this, on the same target and within
// TAP_REPEAT_SLOP of each other, count up (Unity's 0.3 s clickCount
// window, Android's double-tap timeout).
const TAP_INTERVAL_MS = 300
// Window pixels a repeat tap may land from the previous one and still
// count: a thumb repeats within about this (Hammer.js uses 10, Android
// 100 dp); 20 tolerates touch without a mouse double-click spanning
// neighbouring meshes.
const TAP_REPEAT_SLOP = 20

type HandlerName = "onPointerDown" | "onPointerMove" | "onPointerUp" | "onWheel" | "onTap"

// What a press captures, a hover tracks and a tap compares: the struck
// mesh, and the instance when the mesh is instanced.
type Target = { mesh: Mesh; instance: InstanceNode | null }

// One runtime event object serves every view (NodePointerEvent for the
// node chain, ScenePointerEvent at the root, plus the wheel and tap
// fields); the public types narrow it per handler.
type InternalEvent = {
  mesh: Mesh | null
  instance: InstanceNode | null
  currentTarget: SceneNode | Scene | ViewHandle
  point: Vec3 | null
  distance: number | null
  x: number
  y: number
  pointerId: number
  pointerType: string
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  native: ElementPointerEvent
  deltaX?: number
  deltaY?: number
  tapCount?: number
  _stopped: boolean
  stopPropagation(): void
}

// A pointer between its down and up: what it pressed (null = the root),
// whether the down reached the root, the down point for the slop, and
// whether it has been the only pointer down for its whole press.
type Press = {
  target: Target | null
  claimed: boolean
  cx: number
  cy: number
  moved: boolean
  alone: boolean
}

export type PointerInputDeps = {
  /** The root's pick: the camera ray through one of its pixels, hits
   * nearest first. */
  pick(x: number, y: number): Hit[]
  /** The root target's current size (handlersFor's layout scaling). */
  targetSize(): { width: number; height: number }
  /** The scene or view: the walk's root and the listeners' currentTarget. */
  root: Scene | ViewHandle
  /** The root's listeners, in registration order. */
  listeners: Set<ScenePointerListener>
  /** The clock for tap repeats (default performance.now; checks inject). */
  now?: () => number
}

export type PointerInput = {
  /** scene.handlers: for a leaf laid out at the target size. */
  handlers: SceneHandlers
  /** scene.handlersFor: for a leaf whose layout size differs (the
   * supersampling pattern); `layout` is read per event. */
  handlersFor(layout: () => { width: number; height: number }): SceneHandlers
}

export function makePointerInput(deps: PointerInputDeps): PointerInput {
  let now = deps.now ?? (() => performance.now())
  let presses = new Map<number, Press>()
  let hover = new Map<number, Target>()
  let lastTap: { time: number; cx: number; cy: number; target: Target | null; count: number } | null = null

  let targetOf = (hit: Hit): Target => ({ mesh: hit.mesh, instance: hit.instance ?? null })
  let sameHit = (hit: Hit, target: Target): boolean => hit.mesh === target.mesh && (hit.instance ?? null) === target.instance
  let sameTarget = (a: Target | null, b: Target | null): boolean => (a === null ? b === null : b !== null && a.mesh === b.mesh && a.instance === b.instance)
  // The node the walk starts at, and the one enter/leave fire on.
  let nodeOf = (target: Target): SceneNode => target.instance ?? target.mesh
  // The captured target's own hit, if the ray still strikes it.
  let hitOn = (target: Target | null, x: number, y: number): Hit | null => {
    if (target === null) return null
    for (let h of deps.pick(x, y)) if (sameHit(h, target)) return h
    return null
  }

  // Bubble from the struck node through its ancestors; then, when the
  // press lets it, the root's listeners (every one of them - the root is
  // the last stop, there is nothing left to claim). Returns whether the
  // root ran.
  let walk = (name: HandlerName, event: InternalEvent, toRoot: boolean): boolean => {
    for (let n: SceneNode | null = event.instance ?? event.mesh; n !== null && !event._stopped; n = n.parent) {
      // A node that left the scene mid-press has no handlers to run.
      if (n._scene === null) continue
      let handler = n[name] as ((event: InternalEvent) => void) | undefined
      if (handler) {
        event.currentTarget = n
        handler(event)
      }
    }
    if (event._stopped || !toRoot) return false
    event.currentTarget = deps.root
    for (let listener of deps.listeners) {
      let handler = listener[name] as ((event: InternalEvent) => void) | undefined
      if (handler) handler(event)
    }
    return true
  }

  // localX/localY arrive in the leaf's LAYOUT frame (the hit test undoes
  // every transform above it, design-size fits included), so a leaf laid
  // out at the target size - the built-in <Scene> leaf, a d-texture at
  // natural size - is already in scene pixels. Only a leaf deliberately
  // laid out at a DIFFERENT size (the supersampling pattern) needs the
  // ratio, and only the app knows that layout: handlersFor takes it.
  let makeHandlers = (layout: (() => { width: number; height: number }) | null): SceneHandlers => {
    let toScene = (e: ElementPointerEvent): [number, number] => {
      if (layout === null) return [e.localX, e.localY]
      let l = layout()
      let size = deps.targetSize()
      return [e.localX * (l.width > 0 ? size.width / l.width : 1), e.localY * (l.height > 0 ? size.height / l.height : 1)]
    }
    let makeEvent = (e: ElementPointerEvent, target: Target | null, x: number, y: number, hit: Hit | null): InternalEvent => {
      let event: InternalEvent = {
        mesh: target === null ? null : target.mesh,
        instance: target === null ? null : target.instance,
        currentTarget: target === null ? deps.root : nodeOf(target),
        point: hit === null ? null : hit.point,
        distance: hit === null ? null : hit.distance,
        x,
        y,
        pointerId: e.pointerId,
        pointerType: e.pointerType,
        button: e.button,
        shiftKey: e.shiftKey,
        ctrlKey: e.ctrlKey,
        altKey: e.altKey,
        metaKey: e.metaKey,
        native: e,
        _stopped: false,
        stopPropagation() {
          event._stopped = true
        },
      }
      return event
    }
    let nearest = (x: number, y: number): Hit | null => deps.pick(x, y)[0] ?? null
    // Enter/leave go to one node directly, outside the walk: the event
    // in its node view.
    let hoverEvent = (e: ElementPointerEvent, target: Target, x: number, y: number, hit: Hit | null): NodePointerEvent =>
      makeEvent(e, target, x, y, hit) as unknown as NodePointerEvent

    return {
      onPointerDown(e) {
        let [x, y] = toScene(e)
        let hit = nearest(x, y)
        // A second pointer landing ends "alone" for every press in flight.
        for (let p of presses.values()) p.alone = false
        let press: Press = { target: hit === null ? null : targetOf(hit), claimed: false, cx: e.clientX, cy: e.clientY, moved: false, alone: presses.size === 0 }
        presses.set(e.pointerId, press)
        press.claimed = !walk("onPointerDown", makeEvent(e, press.target, x, y, hit), true)
      },
      onPointerMove(e) {
        let [x, y] = toScene(e)
        let press = presses.get(e.pointerId)
        if (press) {
          if (!press.moved && Math.hypot(e.clientX - press.cx, e.clientY - press.cy) >= TAP_SLOP) press.moved = true
          walk("onPointerMove", makeEvent(e, press.target, x, y, hitOn(press.target, x, y)), !press.claimed)
          return
        }
        let hit = nearest(x, y)
        let prev = hover.get(e.pointerId) ?? null
        let changed = prev === null ? hit !== null : hit === null || !sameHit(hit, prev)
        if (changed) {
          if (prev !== null) {
            hover.delete(e.pointerId)
            let node = nodeOf(prev)
            if (node._scene !== null) node.onPointerLeave?.(hoverEvent(e, prev, x, y, null))
          }
          if (hit !== null) {
            let target = targetOf(hit)
            hover.set(e.pointerId, target)
            nodeOf(target).onPointerEnter?.(hoverEvent(e, target, x, y, hit))
          }
        }
        walk("onPointerMove", makeEvent(e, hit === null ? null : targetOf(hit), x, y, hit), true)
      },
      onPointerUp(e) {
        let [x, y] = toScene(e)
        let press = presses.get(e.pointerId)
        if (!press) {
          // A press this root never saw go down: deliver to what is under
          // it, as the element model does.
          let hit = nearest(x, y)
          walk("onPointerUp", makeEvent(e, hit === null ? null : targetOf(hit), x, y, hit), true)
          return
        }
        presses.delete(e.pointerId)
        let under = hitOn(press.target, x, y)
        walk("onPointerUp", makeEvent(e, press.target, x, y, under), !press.claimed)
        // The tap rule: no travel past the slop, alone for the whole
        // press, released over the target it pressed (empty space for the
        // root itself).
        if (press.moved || !press.alone) return
        let release = nearest(x, y)
        if (press.target === null ? release !== null : release === null || !sameHit(release, press.target)) return
        let time = now()
        let repeat = lastTap !== null && time - lastTap.time <= TAP_INTERVAL_MS && sameTarget(lastTap.target, press.target) && Math.hypot(e.clientX - lastTap.cx, e.clientY - lastTap.cy) <= TAP_REPEAT_SLOP
        let count = repeat ? lastTap!.count + 1 : 1
        lastTap = { time, cx: e.clientX, cy: e.clientY, target: press.target, count }
        let tap = makeEvent(e, press.target, x, y, release)
        tap.tapCount = count
        walk("onTap", tap, !press.claimed)
      },
      onWheel(e) {
        let [x, y] = toScene(e)
        let hit = nearest(x, y)
        let event = makeEvent(e, hit === null ? null : targetOf(hit), x, y, hit)
        event.deltaX = e.deltaX
        event.deltaY = e.deltaY
        walk("onWheel", event, true)
      },
      onPointerLeave(e) {
        let prev = hover.get(e.pointerId)
        if (prev === undefined) return
        hover.delete(e.pointerId)
        let node = nodeOf(prev)
        if (node._scene === null) return
        let [x, y] = toScene(e)
        node.onPointerLeave?.(hoverEvent(e, prev, x, y, null))
      },
    }
  }

  return { handlers: makeHandlers(null), handlersFor: makeHandlers }
}

/**
 * Feed a pointer feed (createPointerFeed) from a root's listeners: the
 * events the meshes let through - a mesh that claims its press
 * (stopPropagation on its down) keeps a camera control bound to the feed
 * out of that drag, a drag on empty space or an unclaimed mesh orbits, a
 * wheel anywhere zooms. Returns the detach. The components do this for
 * their `pointer` prop; an imperative scene or view calls it directly.
 */
export function feedPointer(root: { listen(listener: ScenePointerListener): () => void }, feed: PointerFeed): () => void {
  return root.listen({
    onPointerDown: e => feed.handlers.onPointerDown(e.native),
    onPointerMove: e => feed.handlers.onPointerMove(e.native),
    onPointerUp: e => feed.handlers.onPointerUp(e.native),
    onWheel: e => feed.handlers.onWheel(e.native),
  })
}
