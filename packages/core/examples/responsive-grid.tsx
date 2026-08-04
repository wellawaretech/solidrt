// Responsive layout: the SAME app on a phone, tablet, or desktop. The window is
// host-sized and resizable, so drive the layout from the live window instead of
// hardcoding pixels. Here the column count comes from
// `capabilities.windowSizeClass` (Material 3 width breakpoints: compact <600,
// medium 600-840, expanded >=840). Resize the window and the grid reflows.
//
// `capabilities` and `env` are plain objects with reactive GETTERS, not
// functions - read `capabilities.windowSizeClass` (no call). Reading it inside
// JSX tracks, so the memo below re-runs on every resize. `windowSize()` IS a
// function (call it) - we read its width to size each card exactly.
//
// This is the REFLOW answer, for layouts that genuinely rearrange across form
// factors. For content with fixed internal geometry (diagrams, slides,
// dashboards, game boards) do not branch on window size at all: author one
// design space and let `viewBox` scale it to fit - see view-viewbox.tsx.
import { render, capabilities, windowSize, createMemo, For } from "@solidrt/core"

const GAP = 16
const PAD = 24
const COLORS = ["#1f6feb", "#3fb950", "#db6d28", "#a371f7", "#e3b341", "#f778ba", "#2dd4bf", "#f85149"]

function App() {
  // Column count from the size class; card width derived so N fit per row.
  let cols = createMemo(() => (capabilities.windowSizeClass === "expanded" ? 3 : capabilities.windowSizeClass === "medium" ? 2 : 1))
  let cardWidth = createMemo(() => (windowSize().width - PAD * 2 - GAP * (cols() - 1)) / cols())

  return (
    <window>
      <d-rect color="#0b0f17" />
      <view flex={1} flexDirection="column" gap={GAP} padding={PAD}>
        <text color="#e6e6e6" fontSize={18}>{capabilities.windowSizeClass} - {cols()} column{cols() === 1 ? "" : "s"}</text>
        <view flexDirection="row" flexWrap="wrap" gap={GAP}>
          <For each={COLORS}>
            {(c) => (
              <view width={cardWidth()} height={96} alignItems="center" justifyContent="center">
                <d-rect color={c} radius={12} />
                <text color="#0b0f17" fontSize={16} fontWeight={700}>card</text>
              </view>
            )}
          </For>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
