// Enter, exit and stagger are declared on elements; the runtime plays them
// with no per-frame JS. `from` is the value a property animates in from at
// first attach, `exit` the value it animates to on removal. Both are
// per-property entries on the element's own transition spec. An exit
// reaches the whole removed subtree: every `exit` declared under the node
// that unmounts plays, and that node stays painted until the last of them
// settles, then frees. It leaves the layout at once, so a fresh panel
// mounted mid-exit takes its place while the old one fades over it. The
// rows below never unmount one by one - the panel around them does - and
// they still leave the way they arrived.
// `stagger` (ms) goes on an ANCESTOR, never on the animating elements: every
// descendant enter or exit that begins in the same frame gets index * stagger
// of extra delay, in tree order. It cascades nothing on its own - the
// descendants must declare from/exit.
//
// An enter plays once per mount. To replay it, remount the subtree: a keyed
// <Show> re-creates its child whenever `when` changes value, so bumping an
// epoch replays the whole cascade. Tap anywhere: odd taps unmount the panel
// (staggered exits), even taps mount a fresh one (staggered enters).
import { render, createSignal, Show } from "@solidrt/core"
import type { Transition } from "@solidrt/core"

const ROWS = ["Signals", "Effects", "Memos", "Stores", "Boundaries"]
// Extra delay per row before its enter or exit starts.
const STAGGER_MS = 70
// Slide in from the left, out to the right; fade both ways.
const SLIDE = { duration: 500, bounce: 0.2, from: -80, exit: 80 } satisfies Transition
const FADE = { duration: 350, curve: "ease-out", from: 0, exit: 0 } satisfies Transition

function App() {
  let [epoch, setEpoch] = createSignal(1)

  return (
    <window padding={40} gap={12} onPointerDown={() => setEpoch((e) => e + 1)}>
      <text color="#8899aa" fontSize={14}>Tap to replay</text>
      <Show when={epoch() % 2 === 1 ? epoch() : 0} keyed>
        {(_epoch) => (
          <view gap={12} transition={{ stagger: STAGGER_MS }}>
            {ROWS.map((label) => (
              <view x={0} opacity={1} transition={{ x: SLIDE, opacity: FADE }} height={48} justifyContent="center" paddingLeft={16}>
                <d-rect color="#2a3a55" radius={10} />
                <text color="#e8eef6" fontSize={18}>{label}</text>
              </view>
            ))}
          </view>
        )}
      </Show>
    </window>
  )
}

render(() => <App />)
