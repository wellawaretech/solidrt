// The `layout` transition, the companion to `exit`: a row declaring it
// slides from the box it had to the box the next layout gives it, so the
// list never jumps when a row leaves, arrives or moves. Nothing here
// animates `y` by hand and there is no per-frame JS: the rows keep their
// flex layout, and the runtime slides each one from where it was painted.
// Tap a row to remove it - its exit plays where it was (the pop-out) while
// the rows below slide up past it. Tap the header to shuffle: a `<For>`
// reorder moves the surviving nodes, so every displaced row slides. Tap
// the footer to add a row back: the rows below its slot slide apart and
// the new row enters through its `from`. A reflow that lands mid-slide
// retargets, and a spring keeps its momentum. `layout` is parent-relative
// (declare it on every level that should glide) and position only: a size
// change snaps.
import { render, createSignal, For } from "@solidrt/core"
import type { Transition } from "@solidrt/core"

const ALL_ROWS = ["Signals", "Effects", "Memos", "Stores", "Boundaries", "Resources"]
// Rows the list starts with; the rest come back through the footer.
const INITIAL_ROWS = 4
// Every reflow slides on this spring.
const SLIDE = { duration: 400, bounce: 0.15 } satisfies Transition
// Enter from the left, leave to the right, each direction on its own curve.
const ARRIVE = {
  duration: 300,
  curve: "ease-out",
  from: -80,
  exit: { value: 80, curve: "ease-in", duration: 200 },
} satisfies Transition
const FADE = {
  duration: 300,
  curve: "ease-out",
  from: 0,
  exit: { value: 0, curve: "ease-in", duration: 200 },
} satisfies Transition

function shuffled(items: string[]): string[] {
  let pool = items.slice()
  let out: string[] = []
  while (pool.length > 0) out.push(...pool.splice(Math.floor(Math.random() * pool.length), 1))
  return out
}

function App() {
  let [rows, setRows] = createSignal(ALL_ROWS.slice(0, INITIAL_ROWS))
  let addRow = () => {
    let next = ALL_ROWS.find((label) => !rows().includes(label))
    if (next) setRows([...rows(), next])
  }

  return (
    <window padding={40} gap={12}>
      <view height={40} justifyContent="center" onPointerDown={() => setRows(shuffled(rows()))}>
        <text color="#8899aa" fontSize={14}>Tap here to shuffle, a row to remove it</text>
      </view>
      <For each={rows()}>
        {(label) => (
          <view
            x={0}
            opacity={1}
            transition={{ layout: SLIDE, x: ARRIVE, opacity: FADE }}
            height={48}
            justifyContent="center"
            paddingLeft={16}
            onPointerDown={() => setRows(rows().filter((row) => row !== label))}
          >
            <d-rect color="#2a3a55" radius={10} />
            <text color="#e8eef6" fontSize={18}>{label}</text>
          </view>
        )}
      </For>
      <view height={40} justifyContent="center" transition={{ layout: SLIDE }} onPointerDown={addRow}>
        <text color="#8899aa" fontSize={14}>Tap here to add a row back</text>
      </view>
    </window>
  )
}

render(() => <App />)
