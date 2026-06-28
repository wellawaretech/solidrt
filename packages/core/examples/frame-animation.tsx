// onFrame(fn) is the per-frame animation hook. tick is runtime-paced time in ms
// (smooth even when frame times jitter), frame is the present count, rate is the
// refresh rate in Hz. Drive a signal from the tick and read it in JSX - the graph
// repaints each frame. A pending onFrame is a standing request for the next
// frame, so the loop runs only while something is animating and stops when the
// callback is cleaned up (automatic within a reactive scope).
//
// Animate a transform (rotate / scale / x / y on a <view>), not a layout prop:
// transforms are applied at paint time, while animating width/margin/etc would
// re-run layout every frame.
import { render, onFrame, createSignal } from "@solidrt/core"

function App() {
  let [angle, setAngle] = createSignal(0)
  onFrame((tick) => setAngle(tick / 1000)) // radians; ~1 turn every 6.3s

  return (
    <window alignItems="center" justifyContent="center">
      <view width={120} height={120} rotate={angle()}>
        <rect width={120} height={120} radius={16} color="#3366b3" />
      </view>
    </window>
  )
}

render(() => <App />)
