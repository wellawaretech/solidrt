// Pointer events carry the pointer position in three coordinate frames,
// resolved per node as the event bubbles:
// - clientX/clientY - the window frame.
// - localX/localY - the frame of the node whose handler is running, its whole
//   transform chain undone. Exact even when the pointer is not over the node:
//   after a pointer down, moves route along the frozen down path and keep
//   reporting true locals (a fast drag cannot escape the chip below).
// - parentX/parentY - the frame of the node's path parent, which is the frame
//   the node's own x/y props live in.
// The drag idiom needs no transform math in the app: take the grab offset from
// localX/localY at pointer down, place with parentX/parentY - offset during
// moves. The surface here is rotated and scaled to prove the point - the chip
// still tracks the pointer exactly. Keying the grab by pointerId keeps
// concurrent touches (each routed along its own down path) independent.
import { render, createSignal } from "@solidrt/core"

function App() {
  let [pos, setPos] = createSignal({ x: 40, y: 40 })
  let grab: { pointer: number; dx: number; dy: number } | null = null

  return (
    <window alignItems="center" justifyContent="center">
      <view width={360} height={240} rotate={0.3} scale={1.2}>
        <d-rect radius={16} color="#2a2f3a" />
        <view
          position="absolute"
          width={100}
          height={64}
          x={pos().x}
          y={pos().y}
          justifyContent="center"
          alignItems="center"
          onPointerDown={(e) => {
            if (grab) return
            grab = { pointer: e.pointerId, dx: e.localX, dy: e.localY }
          }}
          onPointerMove={(e) => {
            if (!grab || e.pointerId !== grab.pointer) return
            setPos({ x: e.parentX - grab.dx, y: e.parentY - grab.dy })
          }}
          onPointerUp={() => {
            grab = null
          }}
        >
          <d-rect radius={12} color="#3366b3" />
          <text color="white">drag me</text>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
