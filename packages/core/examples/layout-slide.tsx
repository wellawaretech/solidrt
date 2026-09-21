// The `layout` transition, the companion to `exit`: a row declaring it
// slides from the box it had to the box the next layout gives it, position
// and size, so the list never jumps when a row leaves, arrives, moves or
// grows. Nothing here animates `y` or `height` by hand and there is no
// per-frame JS: the rows keep their flex layout, and the runtime slides
// each one from the box it was painted at. Tap a row to expand it: its box
// grows on the spring, its detail text is laid out against the growing box
// every frame, and the rows below slide down with its bottom edge. Tap a
// row's "remove" mark: its exit plays where it was (the pop-out) while the
// rows below slide up past it. Tap the header to shuffle: a `<For>`
// reorder moves the surviving nodes, so every displaced row slides. Tap
// the footer to add a row back: the rows below its slot slide apart and
// the new row enters through its `from`. A reflow that lands mid-slide
// retargets, and a spring keeps its momentum. `layout` is parent-relative:
// declare it on every level that should glide.
import { render, createSignal, For, Show } from "@solidrt/core"
import type { Transition } from "@solidrt/core"

const ALL_ROWS = ["Signals", "Effects", "Memos", "Stores", "Boundaries", "Resources"]
// Rows the list starts with; the rest come back through the footer.
const INITIAL_ROWS = 4
// A row's collapsed height, and the detail text it grows to fit.
const ROW_HEIGHT = 48
const DETAIL =
  "A reactive primitive of Solid 2.0: the runtime tracks its reads and re-runs the computations that depend on it when it changes."

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
  let [expanded, setExpanded] = createSignal<string | null>(null)
  let addRow = () => {
    let next = ALL_ROWS.find((label) => !rows().includes(label))
    if (next) setRows([...rows(), next])
  }

  return (
    <window padding={40} gap={12}>
      <view height={40} justifyContent="center" onPointerDown={() => setRows(shuffled(rows()))}>
        <text color="#8899aa" fontSize={14}>Tap here to shuffle, a row to expand it</text>
      </view>
      <For each={rows()}>
        {(label) => (
          <view
            x={0}
            opacity={1}
            transition={{ layout: SLIDE, x: ARRIVE, opacity: FADE }}
            minHeight={ROW_HEIGHT}
            flexDirection="column"
            justifyContent="center"
            padding={16}
            paddingTop={12}
            paddingBottom={12}
            gap={8}
            overflow="hidden"
            clipRadius={10}
            onPointerDown={() => setExpanded(expanded() === label ? null : label)}
          >
            <d-rect color="#2a3a55" radius={10} />
            <view flexDirection="row" justifyContent="space-between">
              <text color="#e8eef6" fontSize={18}>{label}</text>
              <text
                color="#8899aa"
                fontSize={14}
                onPointerDown={(e) => {
                  e.stopPropagation()
                  setRows(rows().filter((row) => row !== label))
                }}
              >
                remove
              </text>
            </view>
            <Show when={expanded() === label}>
              <text color="#b8c4d4" fontSize={14}>{DETAIL}</text>
            </Show>
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
