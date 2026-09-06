// Pointer dispatch behind layer.handlers, shared by both layer kinds: the
// element event model one tree deeper, with the LAYER as the root of the
// walk. Down, move, up and wheel dispatch on the topmost hit sprite,
// bubble through its enclosing groups and end at the layer's listeners;
// over empty space the walk is the layer alone (`sprite` null). Claiming
// is stopPropagation: a handler that stops an event keeps every later
// target, the layer included, from seeing it, and the DOWN decides the
// press - a pointer whose down never reached the layer stays claimed by
// the sprite chain for its move, up and tap too, so a sprite that drags
// itself stops its down once and a camera attached at the root never
// pans. Capture is per pointer to the press target, the layer included: a
// drag from empty space keeps delivering to the layer as it crosses
// sprites, exactly as a drag from a sprite keeps naming that sprite.
// Enter/leave pair on the sprite alone. Taps are synthesized here (DOM's
// click, Unity's IPointerClickHandler): a press that releases on the same
// target within the slop, the only pointer down for its whole press,
// counted up for repeats (DOM's detail, Unity's clickCount).
//
// Pure BY DESIGN (types and camera.ts, nothing with GPU imports) so
// checks/dispatch-check.ts drives it headless with a fake pick.

import type { PointerEvent as ElementPointerEvent } from "@solidrt/core"
import { unprojectCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"
import type { LayerPointerListener, Sprite, SpriteGroup, SpriteHandlers, SpriteLayer, SpritePointerEvent } from "./layer.ts"
import type { RecordLayer } from "./records.ts"

// Finger travel from the down point, in window pixels, past which a press
// is a drag and never a tap. Core's pan and transform recognizers engage
// at the same 8, so a press is never both a tap and a pan.
const TAP_SLOP = 8
// Successive taps closer in time than this, on the same target and within
// TAP_REPEAT_SLOP of each other, count up (Unity's 0.3 s clickCount
// window, Android's double-tap timeout).
const TAP_INTERVAL_MS = 300
// Window pixels a repeat tap may land from the previous one and still
// count: a thumb repeats within about this (Hammer.js uses 10, Android
// 100 dp); 20 tolerates touch without a mouse double-click spanning
// neighbouring sprites.
const TAP_REPEAT_SLOP = 20

export type DispatchDeps = {
  size: () => [number, number]
  camera: () => CameraUpdate
  pick: (x: number, y: number) => Sprite[]
  /** The layer: the walk's root and the listeners' currentTarget. */
  root: SpriteLayer | RecordLayer
  /** The root's listeners, in registration order. */
  listeners: Set<LayerPointerListener>
  /** The clock for tap repeats (default performance.now; checks inject). */
  now?: () => number
}

type HandlerName = "onPointerDown" | "onPointerMove" | "onPointerUp" | "onWheel" | "onTap"

// One runtime event object serves every view (SpritePointerEvent for the
// sprite chain, LayerPointerEvent at the root, plus the wheel and tap
// fields); the public types narrow it per handler.
type InternalEvent = {
  sprite: Sprite | null
  currentTarget: Sprite | SpriteGroup | SpriteLayer | RecordLayer
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

// A pointer between its down and up: what it pressed (null = the layer),
// whether the down reached the root, the down point for the slop, and
// whether it has been the only pointer down for its whole press.
type Press = {
  target: Sprite | null
  claimed: boolean
  cx: number
  cy: number
  moved: boolean
  alone: boolean
}

export function spriteDispatch(deps: DispatchDeps): (layout: (() => { width: number; height: number }) | null) => SpriteHandlers {
  let now = deps.now ?? (() => performance.now())
  let presses = new Map<number, Press>()
  let hover = new Map<number, Sprite>()
  let lastTap: { time: number; cx: number; cy: number; target: Sprite | null; count: number } | null = null

  // Bubble from the sprite through its groups; then, when the press lets
  // it, the root's listeners (every one of them - the root is the last
  // stop, there is nothing left to claim). Returns whether the root ran.
  let walk = (name: HandlerName, event: InternalEvent, toRoot: boolean): boolean => {
    for (let n: Sprite | SpriteGroup | null = event.sprite; n !== null && !event._stopped; n = n._parent) {
      // An inert handle (removed mid-press) has no handlers to run.
      if (n.layer === null) continue
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

  return layout => {
    let toLayer = (e: ElementPointerEvent): [number, number] => {
      let x = e.localX
      let y = e.localY
      let l = layout?.()
      let [width, height] = deps.size()
      if (l && l.width > 0 && l.height > 0) {
        x *= width / l.width
        y *= height / l.height
      }
      // Undo the camera: screen -> world.
      return unprojectCamera(deps.camera(), x, y)
    }
    let makeEvent = (sprite: Sprite | null, x: number, y: number, e: ElementPointerEvent): InternalEvent => {
      let event: InternalEvent = {
        sprite,
        currentTarget: sprite ?? deps.root,
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
    let topmost = (x: number, y: number): Sprite | null => deps.pick(x, y)[0] ?? null
    // Enter/leave go to one sprite directly, outside the walk: the event
    // in its sprite view.
    let hoverEvent = (sprite: Sprite, x: number, y: number, e: ElementPointerEvent): SpritePointerEvent =>
      makeEvent(sprite, x, y, e) as unknown as SpritePointerEvent

    return {
      onPointerDown(e) {
        let [x, y] = toLayer(e)
        let hit = topmost(x, y)
        // A second pointer landing ends "alone" for every press in flight.
        for (let p of presses.values()) p.alone = false
        let press: Press = { target: hit, claimed: false, cx: e.clientX, cy: e.clientY, moved: false, alone: presses.size === 0 }
        presses.set(e.pointerId, press)
        press.claimed = !walk("onPointerDown", makeEvent(hit, x, y, e), true)
      },
      onPointerMove(e) {
        let [x, y] = toLayer(e)
        let press = presses.get(e.pointerId)
        if (press) {
          if (!press.moved && Math.hypot(e.clientX - press.cx, e.clientY - press.cy) >= TAP_SLOP) press.moved = true
          walk("onPointerMove", makeEvent(press.target, x, y, e), !press.claimed)
          return
        }
        let hit = topmost(x, y)
        let prev = hover.get(e.pointerId) ?? null
        if (prev !== hit) {
          if (prev && prev.layer) prev.onPointerLeave?.(hoverEvent(prev, x, y, e))
          if (hit) hit.onPointerEnter?.(hoverEvent(hit, x, y, e))
          if (hit) hover.set(e.pointerId, hit)
          else hover.delete(e.pointerId)
        }
        walk("onPointerMove", makeEvent(hit, x, y, e), true)
      },
      onPointerUp(e) {
        let [x, y] = toLayer(e)
        let press = presses.get(e.pointerId)
        if (!press) {
          // A press this layer never saw go down: deliver to what is
          // under it, as the element model does.
          walk("onPointerUp", makeEvent(topmost(x, y), x, y, e), true)
          return
        }
        presses.delete(e.pointerId)
        walk("onPointerUp", makeEvent(press.target, x, y, e), !press.claimed)
        // The tap rule: no travel past the slop, alone for the whole
        // press, released over the target it pressed (empty space for the
        // layer itself).
        if (press.moved || !press.alone || topmost(x, y) !== press.target) return
        let time = now()
        let repeat =
          lastTap !== null &&
          time - lastTap.time <= TAP_INTERVAL_MS &&
          lastTap.target === press.target &&
          Math.hypot(e.clientX - lastTap.cx, e.clientY - lastTap.cy) <= TAP_REPEAT_SLOP
        let count = repeat ? lastTap!.count + 1 : 1
        lastTap = { time, cx: e.clientX, cy: e.clientY, target: press.target, count }
        let tap = makeEvent(press.target, x, y, e)
        tap.tapCount = count
        walk("onTap", tap, !press.claimed)
      },
      onWheel(e) {
        let [x, y] = toLayer(e)
        let event = makeEvent(topmost(x, y), x, y, e)
        event.deltaX = e.deltaX
        event.deltaY = e.deltaY
        walk("onWheel", event, true)
      },
      onPointerLeave(e) {
        let [x, y] = toLayer(e)
        let prev = hover.get(e.pointerId)
        if (prev) {
          hover.delete(e.pointerId)
          if (prev.layer) prev.onPointerLeave?.(hoverEvent(prev, x, y, e))
        }
      },
    }
  }
}
