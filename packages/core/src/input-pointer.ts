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
// Chords: every source is also callable with a modifier spec, and
// `pointer.drag("Ctrl")` is the variant that fires only when the event
// opening the gesture carries Ctrl - and then instead of the bare `drag`,
// as the keyboard's most specific spec wins a key down (input-chord.ts,
// the one grammar for both devices). That is the every-viewport default,
// plain drag orbits and Ctrl-drag pans, as one binding each. A chord is
// resolved once per bracket, at the event that opens it (the down that
// starts a drag, the second down that starts pan/pinch/twist, the up
// that hands the gesture back to drag), and held for the bracket, so a
// modifier tapped mid-drag cannot split one gesture across two actions
// (Three decides at mousedown; Unreal's continuous chord splits). The
// unbracketed wheel and mouseDelta resolve per event. Touch carries no
// modifiers, so a chord binding is the desktop path and the bare one
// stays for fingers.
//
// Buttons: a bracketed spec may end in the button that opens it, as a
// key spec ends in its key - `pointer.drag("Right")`, `drag("Shift+Middle")`
// (Left, Middle, Right; bare means Left) - so Three's right-drag pan is
// a binding too. A button is a discriminator, not a modifier: a right
// drag feeds only Right specs, and the chord rule picks among those. One
// button per pointer: a second button pressed while the first is held
// joins nothing, and only the held button's release closes the gesture.
// The wheel and mouseDelta have no button; a button word on them throws.
//
// Discrete gestures are button sources that pulse (pressed for one task,
// the shape of the `tap()` interaction): `swipe("Left")` at the lift of
// a one-finger drag that qualifies (swipe.ts: distance, speed, within 30
// degrees of an axis; the direction is the spec's trailing word, as a
// drag's button is, and a swipe never has a button word), `longPress`
// at the hold timer (long-press.ts) and `doubleTap` on the second tap's
// down (double-tap.ts). They ride the feed's own events - the feed is
// ONE arena claimant, its transform, so they never steal - and take the
// chord of the down that opened them. Bound to an AXIS action they
// nudge 1 with the gesture's focal point (the tap's or hold's position,
// a fraction of the element) and contribute no rate (input-map.ts skips
// a pulse's press when it combines an axis's rates), which is how a
// control learns WHERE a double tap landed (the orbit camera's `focus`,
// the 2d camera's octave); a swipe has no focal.
// The drag's end bracket carries the release velocity the transform
// measured, in element heights per second, so a camera flings from the
// gesture's speed.
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

import { createSignal } from "@solidjs/signals"
import { createTransform } from "./transform"
import { getLayoutBox } from "./core"
import { pointerLocked } from "./window"
import type { PointerEvent, WheelEvent } from "./types"
import type { ActionKind, DeltaSink, InputDevice, InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"
import { chordName, eventModifiers, mostSpecific, parseModifiers } from "./input-chord"
import type { Modified, Modifier } from "./input-chord"
import { createVelocityTracker } from "./velocity"
import type { Velocity } from "./velocity"
import { classifySwipe, SWIPE_DIRECTIONS } from "./swipe"
import type { SwipeDirection } from "./swipe"
import { createHoldTimer } from "./long-press"
import { createTapSequence } from "./double-tap"

// Source ids (input-id.ts): `pointer:<gesture>` for the bare source and
// `pointer:<gesture>:<spec>` for a chord or button variant, the spec in
// its canonical spelling ("Shift+Ctrl+Right").
const DEVICE = "pointer"

// Octaves per wheel-delta unit: the zoom exponent both cameras used
// (0.0015 per unit) taken in log2, so a 100-unit notch zooms about a
// fifth of an octave and a fast scroll of ten notches doubles.
const WHEEL_OCTAVES = 0.0015 / Math.LN2
// Mouse travel per delta unit under pointer lock, in logical pixels: one
// unit is what a drag across the element's height means to a consumer (a
// half turn of look in the first-person control), and 1500 px per half
// turn is the ~0.002 rad/px of Three's PointerLockControls.
const MOUSE_PX_PER_UNIT = 1500

/**
 * A pointer gesture as a source. Bound as is, it fires on the primary
 * button under any modifiers; called with a spec ("Ctrl", "Shift+Alt",
 * "Right", "Shift+Middle") it is the variant that fires only when the
 * event opening the gesture carries those modifiers and that button, and
 * then instead of the bare one: the most specific bound chord wins. The
 * same spec returns the same source, so unbind by identity works.
 */
export type PointerSource<K extends ActionKind> = InputSource<K> & ((chord: string) => InputSource<K>)

export interface PointerFeed extends InputDevice {
  readonly name: "pointer"
  /** Spread on the element whose pointer events feed the sources. */
  handlers: {
    onPointerDown(event: PointerEvent): void
    onPointerMove(event: PointerEvent): void
    onPointerUp(event: PointerEvent): void
    onPointerLeave(event: PointerEvent): void
    onWheel(event: WheelEvent): void
  }
  /** One pointer's travel, in element heights; bracketed. */
  drag: PointerSource<"vec2">
  /** Two or more pointers' focal travel, in element heights; bracketed. */
  pan: PointerSource<"vec2">
  /** Span change of two pointers, in octaves (spreading positive), with
   * the focal point; bracketed. */
  pinch: PointerSource<"axis">
  /** Rotation of the pointer pair, in turns, with the focal point; bracketed. */
  twist: PointerSource<"axis">
  /** Wheel notches in octaves (wheel up positive), with the cursor as the
   * focal point; unbracketed. */
  wheel: PointerSource<"axis">
  /** Raw mouse motion while the pointer is locked, in view-height
   * equivalents (see MOUSE_PX_PER_UNIT); unbracketed. */
  mouseDelta: PointerSource<"vec2">
  /** A one-finger swipe in the spec's direction ("Left", "Ctrl+Right"):
   * a button pulsing once at the lift. */
  swipe(spec: string): InputSource<"button">
  /** A pointer held still for the long-press hold: a button pulsing once
   * at the timer. */
  longPress: PointerSource<"button">
  /** Two quick taps: a button pulsing once on the second down. */
  doubleTap: PointerSource<"button">
  /** The source of an id's spec ("drag", "drag:Ctrl+Right", "wheel",
   * "swipe:Left", "longPress:Ctrl"). */
  resolve(spec: string): InputSource
  /** For a rebind: `found` gets the next gesture of `kind` the pointer
   * opens on the element (a drag, pan or locked mouse motion for a vec2;
   * a pinch, twist or wheel notch for an axis; a swipe, long-press or
   * double-tap for a button), as the variant of the chord and button or
   * direction that opened it. Returns the stop. */
  listen(kind: ActionKind, found: (source: InputSource) => void): () => void
}

export interface PointerFeedOptions {
  /** The element's size, when the app knows it - required for a detached
   * d-* leaf, which has no layout box; default reads getLayoutBox at each
   * press. */
  layout?: () => { width: number; height: number } | null
}

// Button names a bracketed spec may end in, to the event's button index.
const BUTTONS: Record<string, number> = { Left: 0, Middle: 1, Right: 2 }
const PRIMARY = 0

// One gesture's sinks, bucketed by spec (the bare source is the empty
// chord on the primary button). A bracket opens the buckets of the
// button that opened it whose modifiers the opening event carries,
// narrowed to the most specific among those with a sink bound, and
// delivers to them until it closes; an unbracketed send resolves the
// same way per event. Sinks are live sets, as before chords: a consumer
// bound mid-gesture joins it.
type Bucket = { mods: Modifier[]; button: number; sinks: Set<DeltaSink>; source: InputSource }

function gesture<K extends "axis" | "vec2">(kind: K, id: string, label: string, buttons: boolean) {
  let buckets = new Map<string, Bucket>()
  let open: Set<DeltaSink>[] = []
  // Rebind listeners: told the variant each bracket or send opens.
  let probes = new Set<(source: InputSource) => void>()
  let variant = (mods: Modifier[], button: number): InputSource<K> => {
    let name = [chordName(mods), button === PRIMARY ? "" : Object.keys(BUTTONS)[button]!].filter(Boolean).join("+")
    let bucket = buckets.get(name)
    if (!bucket) {
      let sinks = new Set<DeltaSink>()
      let source: InputSource<K> = {
        kind,
        label: name ? `${label} (${name})` : label,
        id: name ? `${DEVICE}:${id}:${name}` : `${DEVICE}:${id}`,
        device: DEVICE,
        deltas(sink) {
          sinks.add(sink)
          return () => {
            sinks.delete(sink)
          }
        },
      }
      bucket = { mods, button, sinks, source }
      buckets.set(name, bucket)
    }
    return bucket.source as InputSource<K>
  }
  let bare = variant([], PRIMARY)
  let spec = (text: string): InputSource<K> => {
    if (typeof text !== "string" || text.length === 0) throw new Error(`${label}: expected a spec such as "Ctrl", "Shift+Alt"${buttons ? ', "Right" or "Shift+Middle"' : ""}, got ${String(text)}`)
    let parts = text.split("+")
    let button = PRIMARY
    if (parts[parts.length - 1]! in BUTTONS) {
      if (!buttons) throw new Error(`${label}: has no button, but "${text}" names one`)
      button = BUTTONS[parts.pop()!]!
    }
    return variant(parseModifiers(label, text, parts), button)
  }
  let source: PointerSource<K> = Object.assign(spec, { kind, label: bare.label, id: bare.id, device: bare.device, deltas: bare.deltas! })
  let resolve = (event: Modified, button: number): Set<DeltaSink>[] =>
    mostSpecific(
      [...buckets.values()].filter(b => b.sinks.size > 0 && b.button === button),
      b => b.mods,
      event,
    ).map(b => b.sinks)
  let probe = (event: Modified, button: number) => {
    if (probes.size === 0) return
    let found = variant(eventModifiers(event), button)
    probes.forEach(p => p(found))
  }
  return {
    source,
    spec,
    listen(found: (source: InputSource) => void) {
      probes.add(found)
      return () => {
        probes.delete(found)
      }
    },
    begin(event: Modified, button: number) {
      probe(event, button)
      open = resolve(event, button)
      for (let sinks of open) sinks.forEach(k => k.begin())
    },
    delta(value: number | Vec2, focal?: Vec2) {
      for (let sinks of open) sinks.forEach(k => k.delta(value, focal))
    },
    end(velocity?: number | Vec2) {
      for (let sinks of open) sinks.forEach(k => k.end(velocity))
      open = []
    },
    send(event: Modified, value: number | Vec2, focal?: Vec2) {
      probe(event, PRIMARY)
      for (let sinks of resolve(event, PRIMARY)) sinks.forEach(k => k.delta(value, focal))
    },
  }
}

// A discrete gesture's sources, bucketed by spec like a delta gesture's:
// the chord plus, when the gesture takes one, a trailing word from
// `words` (a swipe's direction; then the bare spec has no meaning and
// `spec` demands the word). A fire resolves the buckets of the word whose
// chord the event carries, narrowed to the most specific, and pulses each:
// pressed now, released on the next task, so a map's onPress sees the
// edge (input-processors.ts `tap`). A created variant counts as bound.
type PulseBucket = { mods: Modifier[]; word: string | null; source: InputSource<"button">; set: (pressed: boolean) => void; sinks: Set<DeltaSink> }

function pulses(id: string, label: string, words: readonly string[] | null) {
  let buckets = new Map<string, PulseBucket>()
  let probes = new Set<(source: InputSource) => void>()
  let variant = (mods: Modifier[], word: string | null): InputSource<"button"> => {
    let name = [chordName(mods), word ?? ""].filter(Boolean).join("+")
    let bucket = buckets.get(name)
    if (!bucket) {
      // ownedWrite: a fire lands inside a pointer handler or a timer.
      let [pressed, set] = createSignal(false, { ownedWrite: true })
      let sinks = new Set<DeltaSink>()
      let source: InputSource<"button"> = {
        kind: "button",
        label: name ? `${label} (${name})` : label,
        id: name ? `${DEVICE}:${id}:${name}` : `${DEVICE}:${id}`,
        device: DEVICE,
        rate: pressed,
        deltas(sink) {
          sinks.add(sink)
          return () => {
            sinks.delete(sink)
          }
        },
      }
      bucket = { mods, word, source, set, sinks }
      buckets.set(name, bucket)
    }
    return bucket.source
  }
  let spec = (text: string): InputSource<"button"> => {
    if (typeof text !== "string" || text.length === 0) throw new Error(`${label}: expected a spec such as "Ctrl"${words ? `, "${words[0]}" or "Shift+${words[0]}"` : ' or "Shift+Alt"'}, got ${String(text)}`)
    let parts = text.split("+")
    let word: string | null = null
    if (words && words.includes(parts[parts.length - 1]!)) word = parts.pop()!
    if (words && word === null) throw new Error(`${label}: "${text}" must end in a direction (${words.join(", ")})`)
    return variant(parseModifiers(label, text, parts), word)
  }
  let bare = words ? null : variant([], null)
  return {
    /** The bare source (null for a gesture that needs its word). */
    bare,
    spec,
    listen(found: (source: InputSource) => void) {
      probes.add(found)
      return () => {
        probes.delete(found)
      }
    },
    fire(event: Modified, word: string | null, focal?: Vec2) {
      if (probes.size > 0) {
        let found = variant(eventModifiers(event), word)
        probes.forEach(p => p(found))
      }
      let hit = mostSpecific(
        [...buckets.values()].filter(b => b.word === word),
        b => b.mods,
        event,
      )
      for (let b of hit) {
        b.set(true)
        setTimeout(() => b.set(false), 0)
        b.sinks.forEach(k => k.delta(1, focal))
      }
    },
  }
}

export function createPointerFeed(options: PointerFeedOptions = {}): PointerFeed {
  let drag = gesture("vec2", "drag", "pointer drag", true)
  let pan = gesture("vec2", "pan", "pointer two-finger pan", true)
  let pinch = gesture("axis", "pinch", "pointer pinch", true)
  let twist = gesture("axis", "twist", "pointer twist", true)
  let wheel = gesture("axis", "wheel", "pointer wheel", false)
  let mouseDelta = gesture("vec2", "mouseDelta", "mouse motion (pointer locked)", false)
  let gestures = { drag, pan, pinch, twist, wheel, mouseDelta }
  let swipe = pulses("swipe", "pointer swipe", SWIPE_DIRECTIONS)
  let longPress = pulses("longPress", "pointer long press", null)
  let doubleTap = pulses("doubleTap", "pointer double tap", null)
  let discrete = { swipe, longPress, doubleTap }
  let callable = (p: typeof longPress): PointerSource<"button"> => Object.assign(p.spec, { kind: "button" as const, label: p.bare!.label, id: p.bare!.id, device: p.bare!.device, rate: p.bare!.rate!, deltas: p.bare!.deltas! })

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

  // The release velocity of the gesture that just ended, for the drag's
  // end bracket (the transform's up runs before `lifted`).
  let released: Velocity | null = null
  let transform = createTransform({
    buttons: "any",
    onTransformEnd: v => {
      released = v.vx !== 0 || v.vy !== 0 ? v : null
    },
    onTransformMove: t => {
      let h = unit()
      let travel: Vec2 = [t.dx / h, t.dy / h]
      if (t.pointers >= 2) {
        if (t.dx !== 0 || t.dy !== 0) pan.delta(travel)
        let focal = focalOf(t.x, t.y)
        if (t.scale !== 1) pinch.delta(Math.log2(t.scale), focal)
        if (t.rotation !== 0) twist.delta(t.rotation / (2 * Math.PI), focal)
      } else if (t.dx !== 0 || t.dy !== 0) {
        drag.delta(travel)
      }
    },
  })

  // Pointers down on the element with the button that pressed them, for
  // the gesture brackets: the first press opens drag, a second closes it
  // and opens the two-pointer gestures, and the count coming back down
  // reverses that. The event that opens a bracket resolves its chord,
  // and the pointer it opens for supplies the button (the drag that
  // resumes when a second pointer lifts belongs to the one still down).
  let downs = new Map<number, number>()
  let landed = (e: PointerEvent, button: number) => {
    downs.set(e.pointerId, button)
    if (downs.size === 1) drag.begin(e, button)
    if (downs.size === 2) {
      drag.end()
      pan.begin(e, button)
      pinch.begin(e, button)
      twist.begin(e, button)
    }
  }
  let lifted = (e: PointerEvent) => {
    downs.delete(e.pointerId)
    if (downs.size === 1) {
      pan.end()
      pinch.end()
      twist.end()
      drag.begin(e, [...downs.values()][0]!)
    }
    if (downs.size === 0) {
      let h = unit()
      drag.end(released ? [released.vx / h, released.vy / h] : undefined)
      released = null
    }
  }

  // The discrete gestures over the primary button's first pointer: the
  // finger's window-px velocity and the opening down for the swipe (a
  // second finger disqualifies it), the hold timer, the tap sequence.
  let finger = createVelocityTracker()
  let opening: { id: number; x: number; y: number; mods: Modified; alone: boolean } | null = null
  // The primary pointer's last down, as a focal: where a double tap or
  // a long press landed (both fire from that down's own sequence).
  let downFocal: Vec2 | undefined
  let hold = createHoldTimer({ onFire: (_id, _at, mods) => longPress.fire(mods, null, downFocal) })
  let taps = createTapSequence({ onDouble: (_id, _at, mods) => doubleTap.fire(mods, null, downFocal) })
  let mods = (e: PointerEvent): Modified => ({ shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, altKey: e.altKey, metaKey: e.metaKey })

  return {
    handlers: {
      onPointerDown(e) {
        if (downs.has(e.pointerId)) return
        if (downs.size === 0) box = size(e)
        let button = e.button ?? PRIMARY
        landed(e, button)
        transform.handlers.onPointerDown(e)
        if (button === PRIMARY) {
          downFocal = focalOf(e.localX, e.localY)
          hold.down(e)
          taps.down(e)
          if (downs.size === 1) {
            opening = { id: e.pointerId, x: e.clientX, y: e.clientY, mods: mods(e), alone: true }
            finger.reset()
            finger.push(e.clientX, e.clientY)
          } else if (opening) opening.alone = false
        }
      },
      onPointerMove(e) {
        if (pointerLocked() && e.pointerType === "mouse" && (e.movementX !== 0 || e.movementY !== 0)) {
          mouseDelta.send(e, [e.movementX / MOUSE_PX_PER_UNIT, e.movementY / MOUSE_PX_PER_UNIT])
        }
        transform.handlers.onPointerMove(e)
        hold.move(e)
        taps.move(e)
        if (opening && opening.id === e.pointerId) finger.push(e.clientX, e.clientY)
      },
      onPointerUp(e) {
        if (downs.get(e.pointerId) !== (e.button ?? PRIMARY)) return
        transform.handlers.onPointerUp(e)
        lifted(e)
        hold.up(e)
        taps.up(e)
        if (opening && opening.id === e.pointerId) {
          if (opening.alone && downs.size === 0) {
            let direction: SwipeDirection | null = classifySwipe(finger.velocity(), { dx: e.clientX - opening.x, dy: e.clientY - opening.y })
            if (direction) swipe.fire(opening.mods, direction)
          }
          opening = null
        }
      },
      onPointerLeave() {},
      onWheel(e) {
        if (e.deltaY === 0) return
        box = size(e)
        wheel.send(e, -e.deltaY * WHEEL_OCTAVES, focalOf(e.localX, e.localY))
      },
    },
    name: DEVICE,
    drag: drag.source,
    pan: pan.source,
    pinch: pinch.source,
    twist: twist.source,
    wheel: wheel.source,
    mouseDelta: mouseDelta.source,
    swipe: swipe.spec,
    longPress: callable(longPress),
    doubleTap: callable(doubleTap),
    resolve(spec) {
      let colon = spec.indexOf(":")
      let name = colon < 0 ? spec : spec.slice(0, colon)
      let rest = colon < 0 ? null : spec.slice(colon + 1)
      let g = gestures[name as keyof typeof gestures]
      if (g) return rest === null ? g.source : g.spec(rest)
      let p = discrete[name as keyof typeof discrete]
      if (p) {
        if (rest === null) {
          if (!p.bare) throw new Error(`pointer.resolve: "${spec}" needs a direction (swipe:Left)`)
          return p.bare
        }
        return p.spec(rest)
      }
      throw new Error(`pointer.resolve: unknown gesture "${spec}" (${[...Object.keys(gestures), ...Object.keys(discrete)].join(", ")}, each with an optional :<chord>; swipe with its direction)`)
    },
    listen(kind, found) {
      if (kind !== "button" && kind !== "axis" && kind !== "vec2") throw new Error(`pointer.listen: expected a kind, got ${String(kind)}`)
      if (typeof found !== "function") throw new Error("pointer.listen: expects a function")
      let stops = kind === "button"
        ? Object.values(discrete).map(p => p.listen(found))
        : Object.values(gestures)
            .filter(g => g.source.kind === kind)
            .map(g => g.listen(found))
      return () => {
        for (let stop of stops) stop()
      }
    },
  }
}
