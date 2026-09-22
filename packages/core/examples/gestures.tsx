// Discrete gestures test bed: the recognizer family beyond press and pan,
// each on its own target with its state on screen (and in the log):
//   - swipe: a card that follows the finger and reports the direction it
//     left in (createSwipe), snapping back when the drag was too slow;
//   - long-press: a tile that arms on a hold and can then be dragged
//     (createLongPress, the reorder idiom), while a plain drag moves it
//     not at all;
//   - double-tap beside a single tap: the tap fires late only where a
//     double-tap is registered (the arena's pend/defer relation), at once
//     on the plain target;
//   - the pointer feed: an input map binding `dodge` to swipe("Left"),
//     `charge` to longPress and `zoomIn` to doubleTap on one viewport, so
//     the same gestures reach a game through the map.
// A pan target prints the release velocity core measures at every lift.
import { arena, createDoubleTap, createInputMap, createLongPress, createPan, createPointerFeed, createSignal, createSwipe, render } from "@solidrt/core"
import type { PointerEvent, PointerPoint } from "@solidrt/core"

const TILE = 120
const CARD_WIDTH = 240
const CARD_HEIGHT = 64

function Swipeable() {
  let [x, setX] = createSignal(0)
  let [dragging, setDragging] = createSignal(false)
  let [status, setStatus] = createSignal("swipe me")
  let swipe = createSwipe({
    directions: ["Left", "Right"],
    onSwipeStart: () => setDragging(true),
    onSwipeMove: dx => setX(x() + dx),
    onSwipe: (direction, v) => {
      setStatus(`swiped ${direction} at ${Math.round(v.vx)} px/s`)
      console.log(`swipe ${direction} ${Math.round(v.vx)}`)
    },
    onSwipeEnd: () => {
      setDragging(false)
      setX(0)
    },
  })
  return (
    <view width={CARD_WIDTH} height={CARD_HEIGHT} overflow="hidden" {...swipe.handlers}>
      <d-rect color="#243044" radius={8} />
      <view width={CARD_WIDTH} height={CARD_HEIGHT} x={x()} transition={dragging() ? null : { x: { duration: 250 } }} alignItems="center" justifyContent="center">
        <d-rect color="#3b5f8a" radius={8} />
        <d-text color="#ffffff">{status()}</d-text>
      </view>
    </view>
  )
}

function LongPressable() {
  let [pos, setPos] = createSignal({ x: 0, y: 0 })
  let [held, setHeld] = createSignal(false)
  let [status, setStatus] = createSignal("hold me")
  let lp = createLongPress({
    onLongPress: () => {
      setHeld(true)
      setStatus("held: now drag")
      console.log("long-press")
    },
    onLongPressMove: (dx, dy) => setPos({ x: pos().x + dx, y: pos().y + dy }),
    onLongPressEnd: () => {
      setHeld(false)
      setStatus(`dropped at ${Math.round(pos().x)},${Math.round(pos().y)}`)
      console.log(`long-press end ${Math.round(pos().x)},${Math.round(pos().y)}`)
    },
  })
  return (
    <view width={TILE} height={TILE} x={pos().x} y={pos().y} alignItems="center" justifyContent="center" {...lp.handlers}>
      <d-rect color={held() ? "#d9822b" : "#8a5f3b"} radius={12} />
      <d-text color="#ffffff">{status()}</d-text>
    </view>
  )
}

// A press the way components' createPress does it: claim on the down,
// fire on the up, deferred while a double-tap is pending on the pointer.
function pressable(onPress: () => void) {
  let active: number | null = null
  let owner = {
    cancel() {
      active = null
    },
  }
  return {
    onPointerDown(e: PointerEvent) {
      if (arena.claim(e.pointerId, owner)) active = e.pointerId
    },
    onPointerUp(e: PointerEvent) {
      if (active !== e.pointerId) return
      active = null
      arena.release(e.pointerId, owner)
      if (!arena.defer(e.pointerId, onPress)) onPress()
    },
  }
}

function Tappable(props: { double: boolean }) {
  let [status, setStatus] = createSignal(props.double ? "tap or double-tap" : "tap")
  let taps = 0
  let press = pressable(() => {
    taps++
    setStatus(`tap ${taps}`)
    console.log(`${props.double ? "double-target" : "plain-target"} tap ${taps}`)
  })
  let dt = createDoubleTap({
    onDoubleTap: (at: PointerPoint) => {
      setStatus(`double-tap at ${Math.round(at.localX)},${Math.round(at.localY)}`)
      console.log("double-tap")
    },
  })
  let down = (e: PointerEvent) => {
    if (props.double) dt.handlers.onPointerDown(e)
    press.onPointerDown(e)
  }
  let up = (e: PointerEvent) => {
    if (props.double) dt.handlers.onPointerUp(e)
    press.onPointerUp(e)
  }
  return (
    <view width={TILE} height={TILE} alignItems="center" justifyContent="center" onPointerDown={down} onPointerMove={props.double ? dt.handlers.onPointerMove : undefined} onPointerUp={up}>
      <d-rect color={props.double ? "#3b8a5f" : "#5f8a3b"} radius={12} />
      <d-text color="#ffffff">{status()}</d-text>
    </view>
  )
}

function Pannable() {
  let [status, setStatus] = createSignal("drag and let go")
  let pan = createPan({
    onPanEnd: v => {
      setStatus(`lift at ${Math.round(v.vx)},${Math.round(v.vy)} px/s`)
      console.log(`pan end ${Math.round(v.vx)},${Math.round(v.vy)}`)
    },
  })
  return (
    <view width={CARD_WIDTH} height={CARD_HEIGHT} alignItems="center" justifyContent="center" {...pan.handlers}>
      <d-rect color="#44304a" radius={8} />
      <d-text color="#ffffff">{status()}</d-text>
    </view>
  )
}

function Viewport() {
  let pointer = createPointerFeed()
  let input = createInputMap({ dodge: "button", charge: "button", zoomIn: "button", look: "vec2" })
  input.bind("dodge", pointer.swipe("Left"))
  input.bind("charge", pointer.longPress)
  input.bind("zoomIn", pointer.doubleTap)
  input.bind("look", pointer.drag)
  let [status, setStatus] = createSignal("feed: swipe left, hold, double-tap")
  input.onPress("dodge", () => {
    setStatus("feed: dodge")
    console.log("feed dodge")
  })
  input.onPress("charge", () => {
    setStatus("feed: charge")
    console.log("feed charge")
  })
  input.onPress("zoomIn", () => {
    setStatus("feed: zoomIn")
    console.log("feed zoomIn")
  })
  input.onGesture("look", {
    end: v => {
      if (v) {
        setStatus(`feed: look end ${v[0].toFixed(2)},${v[1].toFixed(2)} heights/s`)
        console.log(`feed look end ${v[0].toFixed(2)},${v[1].toFixed(2)}`)
      }
    },
  })
  return (
    <view width={CARD_WIDTH} height={TILE} alignItems="center" justifyContent="center" {...pointer.handlers}>
      <d-rect color="#2b4a5a" radius={12} />
      <d-text color="#ffffff">{status()}</d-text>
    </view>
  )
}

function App() {
  return (
    <window flexDirection="column" gap={24} padding={24}>
      <d-rect color="#101418" />
      <view flexDirection="row" gap={24}>
        <Swipeable />
        <Pannable />
      </view>
      <view flexDirection="row" gap={24}>
        <LongPressable />
        <Tappable double={true} />
        <Tappable double={false} />
      </view>
      <Viewport />
    </window>
  )
}

render(() => <App />)
