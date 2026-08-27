// `points` turns a line into a polyline: a flat [x0, y0, x1, y1, ...] array in
// the element's local space, so geometry that changes every frame is one
// array write - no `d` string to format and re-parse. Three uses below:
// 1. A live trace: a Float32Array rebuilt each frame in onFrame and set as
//    `points`. Typed arrays marshal like number[]; nothing is parsed.
// 2. A closed outline: `closed` strokes the segment back to the first point
//    and joins there instead of capping both ends; the join style shows at
//    the apex. A line's paint defaults to stroke; drawStyle="fill" fills the
//    polygon instead (implicitly closed), "stroke-and-fill" does both.
//    Dashing runs along the whole stroke, through the vertices.
// 3. Marching ants: `dashOffset` slides the dash pattern, so writing it every
//    frame animates the dashes. The ring is dense (6 px segments under
//    12 px dashes), which only works because the phase carries across
//    vertices; the two-point d-line goes through the same walker.
// 4. A laid-out <line points>: the points are content (like a path's `d`), so
//    the box measures from their extent and takes part in the row.
// The two-endpoint form (x1..y2, on d-line only) is unchanged; while `points`
// is set it takes precedence over the endpoints.
import { render, onFrame, createSignal } from "@solidrt/core"

const SAMPLES = 200
const TRACE_W = 560
const TRACE_H = 160
const TRIANGLE = [20, 100, 70, 20, 120, 100]
const ZIGZAG = [0, 0, 30, 24, 60, 0, 90, 24, 120, 0]
const RING = ring(70, 60, 45, 48)
const ANTS_SPEED = 40 // local units per second

function ring(cx: number, cy: number, r: number, n: number): number[] {
  let pts: number[] = []
  for (let i = 0; i < n; i++) {
    let a = (i / n) * Math.PI * 2
    pts.push(cx + Math.cos(a) * r, cy + Math.sin(a) * r)
  }
  return pts
}

// A travelling wave inside a sine envelope, sampled into x, y pairs.
function wave(t: number): Float32Array {
  let pts = new Float32Array(SAMPLES * 2)
  for (let i = 0; i < SAMPLES; i++) {
    let u = i / (SAMPLES - 1)
    pts[2 * i] = u * TRACE_W
    pts[2 * i + 1] = TRACE_H / 2 + Math.sin(u * 14 - t * 4) * Math.sin(u * Math.PI) * (TRACE_H / 2 - 8)
  }
  return pts
}

function App() {
  let [trace, setTrace] = createSignal<Float32Array>(wave(0))
  let [ants, setAnts] = createSignal(0)
  onFrame((tick) => {
    setTrace(wave(tick / 1000))
    setAnts((tick / 1000) * ANTS_SPEED)
  })

  return (
    <window padding={24} gap={20}>
      <d-rect color="#0b0f17" />

      <text fontSize={16} color="#8b949e">
        live trace: a Float32Array of {SAMPLES} points written every frame
      </text>
      <view width={TRACE_W} height={TRACE_H}>
        <d-rect radius={8} color="#151b28" />
        <d-line points={trace()} color="#3fb950" strokeWidth={3} strokeJoin="round" />
      </view>

      <text fontSize={16} color="#8b949e">
        closed (round join), open (round caps), filled, stroke-and-fill dashed
      </text>
      <view flexDirection="row" gap={20}>
        <view width={140} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line points={TRIANGLE} closed color="#e3b341" strokeWidth={8} strokeJoin="round" />
        </view>
        <view width={140} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line points={TRIANGLE} color="#e3b341" strokeWidth={8} strokeCap="round" />
        </view>
        <view width={140} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line points={TRIANGLE} drawStyle="fill" color="#a371f7" />
        </view>
        <view width={140} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line points={TRIANGLE} closed drawStyle="stroke-and-fill" onLength={12} offLength={8} color="#f85149" strokeWidth={3} />
        </view>
      </view>

      <text fontSize={16} color="#8b949e">
        marching ants: dashOffset written every frame, on a dense ring and a segment
      </text>
      <view flexDirection="row" gap={20}>
        <view width={140} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line points={RING} closed onLength={12} offLength={8} dashOffset={ants()} color="#e3b341" strokeWidth={3} />
        </view>
        <view width={300} height={120}>
          <d-rect radius={8} color="#151b28" />
          <d-line x1={20} y1={60} x2={280} y2={60} onLength={0} offLength={14} dashOffset={-ants()} color="#79c0ff" strokeWidth={6} strokeCap="round" />
        </view>
      </view>

      <view flexDirection="row" alignItems="center" gap={12}>
        <text fontSize={16} color="#8b949e">
          laid out:
        </text>
        <line points={ZIGZAG} color="#1f6feb" strokeWidth={3} strokeJoin="round" />
        <text fontSize={16} color="#8b949e">
          the box measures from the points
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
