// onLayout(fn) fires after layout is computed but before paint - the one point
// where measured geometry is available via getBoundingBox (a window-relative
// snapshot, valid only here or in an event handler). Its canonical use is to
// connect or annotate laid-out elements with detached drawing: let the layout
// engine place the boxes, read their boxes, then draw a d-path between them.
//
// Two rules make it safe:
//  1. Write the result to something that does NOT affect layout - a d-path's `d`,
//     a detached node's position. Writing a layout-affecting prop here forces an
//     extra layout pass every frame.
//  2. Call flush() after the write so it lands before the display list is built
//     (onLayout runs after the frame's normal flush).
import { render, onLayout, getBoundingBox, createSignal, flush } from "@solidrt/core"

function App() {
  let boxA!: { id: number }
  let boxB!: { id: number }
  let [d, setD] = createSignal("")

  onLayout(() => {
    let a = getBoundingBox(boxA)
    let b = getBoundingBox(boxB)
    if (!a || !b) return
    let ax = a.x + a.width / 2, ay = a.y + a.height / 2
    let bx = b.x + b.width / 2, by = b.y + b.height / 2
    setD(`M ${ax} ${ay} L ${bx} ${by}`)
    flush()
  })

  return (
    <window flexDirection="row" justifyContent="space-between" alignItems="center" padding={48}>
      <rect ref={n => (boxA = n)} width={80} height={80} radius={8} color="#3366b3" />
      <rect ref={n => (boxB = n)} width={80} height={80} radius={8} color="#6699e6" />
      <d-path d={d()} color="#e0245e" drawStyle="stroke" strokeWidth={3} />
    </window>
  )
}

render(() => <App />)