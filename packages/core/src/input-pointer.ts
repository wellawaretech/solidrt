// A pointer feed: one element's pointer events turned into gesture
// sources for an input map. The pointer is the one device that is not
// global - the tree holds UI and scene together, so a drag belongs to the
// element it started on (a slider beside a viewport must not turn the
// camera) - hence a feed per element: spread `feed.handlers` on the leaf
// (a Scene, View3d, SpriteLayer or View2d takes it as its `pointer` prop
// and spreads it on the built-in leaf) and bind its sources.
//
// The sources are gestures, not raw events, because a pointer only means
// something once a gesture is recognized: a move without a press is
// nothing, one finger dragging is `drag`, two fingers moving together is
// `pan`, spreading is `pinch`, turning is `twist`, and the wheel is its
// own thing. One merged recognizer (createTransform) serves all of them,
// so they arbitrate in the app-wide gesture arena as one - a viewport
// inside a scroller never double-handles - and they never fight each
// other over the same fingers. `mouseDelta` is the pointer-lock path:
// raw mouse motion while the pointer is locked, when the cursor does not
// move and no drag can arm.
//
// Units are device-free (the input map's contract): drag and pan travel
// in the element's own heights (a drag across the element is 1 whatever
// the window size - Three's OrbitControls convention), pinch and wheel
// in octaves (log2 of the scale change, one notch about a fifth), twist
// in turns, mouse motion in view-height-equivalents through a fixed
// reference. The focal point of a pinch or a wheel rides along as a
// fraction of the element (0..1), the anchor a zoom-about-the-cursor
// consumer maps to its own space. Screen convention: x right, y down.
//
// Gesture brackets: `drag` begins when the first pointer lands and ends
// when the last lifts (or when a second lands, which begins `pan`,
// `pinch` and `twist` until the count drops back to one); the wheel
// sends no brackets, so a consumer can tell a notch from a finger.
// Brackets fire on the press itself, before the recognizer's slop, so a
// glide stops the moment a finger lands.
//
// The element's laid-out box normalizes the travel: read at the press
// through getLayoutBox (the untransformed read, so a designSize fit or
// an ancestor transform never skews it), or from `layout` when given -
// required for a detached d-* leaf, which has no layout box (the feed
// throws rather than treat pixels as heights), and handy for a headless
// check with no tree. A zero-height box passes pixels through unscaled.
//
// Create a feed in an owned scope (a component body): the recognizer
// registers its cleanup with the owner.

import { createTransform } from "./transform"
import { getLayoutBox } from "./core"
import { pointerLocked } from "./window"
import type { PointerEvent, WheelEvent } from "./types"
import type { DeltaSink, InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"

// Octaves per wheel-delta unit: the zoom exponent both cameras used
// (0.0015 per unit) taken in log2, so a 100-unit notch zooms about a
// fifth of an octave and a fast scroll of ten notches doubles.
const WHEEL_OCTAVES = 0.0015 / Math.LN2
// Mouse travel per delta unit under pointer lock, in logical pixels: one
// unit is what a drag across the element's height means to a consumer (a
// half turn of look in the first-person control), and 1500 px per half
// turn is the ~0.002 rad/px of Three's PointerLockControls.
const MOUSE_PX_PER_UNIT = 1500

export interface PointerFeed {
  /** Spread on the element whose pointer events feed the sources. */
  handlers: {
    onPointerDown(event: PointerEvent): void
    onPointerMove(event: PointerEvent): void
    onPointerUp(event: PointerEvent): void
    onPointerLeave(event: PointerEvent): void
    onWheel(event: WheelEvent): void
  }
  /** One pointer's travel, in element heights; bracketed. */
  drag: InputSource<"vec2">
  /** Two or more pointers' focal travel, in element heights; bracketed. */
  pan: InputSource<"vec2">
  /** Span change of two pointers, in octaves (spreading positive), with
   * the focal point; bracketed. */
  pinch: InputSource<"axis">
  /** Rotation of the pointer pair, in turns, with the focal point; bracketed. */
  twist: InputSource<"axis">
  /** Wheel notches in octaves (wheel up positive), with the cursor as the
   * focal point; unbracketed. */
  wheel: InputSource<"axis">
  /** Raw mouse motion while the pointer is locked, in view-height
   * equivalents (see MOUSE_PX_PER_UNIT); unbracketed. */
  mouseDelta: InputSource<"vec2">
}

export interface PointerFeedOptions {
  /** The element's size, when the app knows it - required for a detached
   * d-* leaf, which has no layout box; default reads getLayoutBox at each
   * press. */
  layout?: () => { width: number; height: number } | null
}

type Sinks = Set<DeltaSink>

function source<K extends "axis" | "vec2">(kind: K, label: string, sinks: Sinks): InputSource<K> {
  return {
    kind,
    label,
    deltas(sink) {
      sinks.add(sink)
      return () => {
        sinks.delete(sink)
      }
    },
  }
}

export function createPointerFeed(options: PointerFeedOptions = {}): PointerFeed {
  let sinks = {
    drag: new Set<DeltaSink>(),
    pan: new Set<DeltaSink>(),
    pinch: new Set<DeltaSink>(),
    twist: new Set<DeltaSink>(),
    wheel: new Set<DeltaSink>(),
    mouseDelta: new Set<DeltaSink>(),
  }
  let begin = (s: Sinks) => s.forEach(k => k.begin())
  let end = (s: Sinks) => s.forEach(k => k.end())
  let delta = (s: Sinks, value: number | Vec2, focal?: Vec2) => s.forEach(k => k.delta(value, focal))

  // The element's box, read at each press (see the header). A detached
  // leaf has no layout box and must bring `layout`: normalizing by
  // nothing would turn a 100 px drag into 100 element heights, so that
  // is a throw (the dev validation policy), while a zero-size box is a
  // degenerate layout and passes pixels through.
  let size = (event: { currentTarget: number }): { width: number; height: number } => {
    let box = options.layout ? options.layout() : getLayoutBox({ id: event.currentTarget })
    if (box === null && !options.layout) {
      throw new Error("createPointerFeed: the element has no layout box (a detached d-* leaf, or before its first layout); create the feed with { layout: () => ({ width, height }) } for it")
    }
    return box && box.width > 0 && box.height > 0 ? { width: box.width, height: box.height } : { width: 0, height: 0 }
  }
  let box = { width: 0, height: 0 }
  let unit = () => (box.height > 0 ? box.height : 1)
  let focalOf = (x: number, y: number): Vec2 | undefined => (box.width > 0 && box.height > 0 ? [x / box.width, y / box.height] : undefined)

  let transform = createTransform({
    onTransformMove: t => {
      let h = unit()
      let travel: Vec2 = [t.dx / h, t.dy / h]
      if (t.pointers >= 2) {
        if (t.dx !== 0 || t.dy !== 0) delta(sinks.pan, travel)
        let focal = focalOf(t.x, t.y)
        if (t.scale !== 1) delta(sinks.pinch, Math.log2(t.scale), focal)
        if (t.rotation !== 0) delta(sinks.twist, t.rotation / (2 * Math.PI), focal)
      } else if (t.dx !== 0 || t.dy !== 0) {
        delta(sinks.drag, travel)
      }
    },
  })

  // Pointers down on the element, for the gesture brackets: the first
  // press opens drag, a second closes it and opens the two-pointer
  // gestures, and the count coming back down reverses that.
  let downs = new Set<number>()
  let landed = (id: number) => {
    if (downs.has(id)) return
    downs.add(id)
    if (downs.size === 1) begin(sinks.drag)
    if (downs.size === 2) {
      end(sinks.drag)
      begin(sinks.pan)
      begin(sinks.pinch)
      begin(sinks.twist)
    }
  }
  let lifted = (id: number) => {
    if (!downs.delete(id)) return
    if (downs.size === 1) {
      end(sinks.pan)
      end(sinks.pinch)
      end(sinks.twist)
      begin(sinks.drag)
    }
    if (downs.size === 0) end(sinks.drag)
  }

  return {
    handlers: {
      onPointerDown(e) {
        if (e.button != null && e.button !== 0) return
        if (downs.size === 0) box = size(e)
        landed(e.pointerId)
        transform.handlers.onPointerDown(e)
      },
      onPointerMove(e) {
        if (pointerLocked() && e.pointerType === "mouse" && (e.movementX !== 0 || e.movementY !== 0)) {
          delta(sinks.mouseDelta, [e.movementX / MOUSE_PX_PER_UNIT, e.movementY / MOUSE_PX_PER_UNIT])
        }
        transform.handlers.onPointerMove(e)
      },
      onPointerUp(e) {
        transform.handlers.onPointerUp(e)
        lifted(e.pointerId)
      },
      onPointerLeave() {},
      onWheel(e) {
        if (e.deltaY === 0) return
        box = size(e)
        delta(sinks.wheel, -e.deltaY * WHEEL_OCTAVES, focalOf(e.localX, e.localY))
      },
    },
    drag: source("vec2", "pointer drag", sinks.drag),
    pan: source("vec2", "pointer two-finger pan", sinks.pan),
    pinch: source("axis", "pointer pinch", sinks.pinch),
    twist: source("axis", "pointer twist", sinks.twist),
    wheel: source("axis", "pointer wheel", sinks.wheel),
    mouseDelta: source("vec2", "mouse motion (pointer locked)", sinks.mouseDelta),
  }
}
