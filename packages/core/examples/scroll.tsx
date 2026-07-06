// createScroll is a HEADLESS scroll primitive: it owns only the geometry - the
// clamped offset, re-clamped in onLayout against the measured content-vs-viewport
// overflow - and nothing with a UI opinion. Input handling, momentum, and
// scrollbars are policy you supply. (@solidrt/components ScrollView is one such
// skin built on top of this.)
//
// The shape it expects:
//  - a VIEWPORT node: the clipping box, overflow="hidden".
//  - a CONTENT node inside it: an inner wrapper that takes the children's natural
//    size (flexShrink={0} so it can exceed the viewport).
// Capture both with refs, pass their accessors to createScroll, then apply the
// returned offset to the viewport's scrollX/scrollY. createScroll does no input,
// so wire an event (here onWheel) to scroll.scrollBy - positive dy moves content
// up. scrollTo(x, y) jumps to an absolute, clamped offset.
import { render, For, createScroll } from "@solidrt/core"
import type { WheelEvent } from "@solidrt/core"

function App() {
  let viewport: { id: number } | undefined
  let content: { id: number } | undefined

  // Default axis is "vertical"; pass { axis: "horizontal" } or "both" for others.
  let scroll = createScroll(() => viewport, () => content)

  let onWheel = (e: WheelEvent) => scroll.scrollBy(e.deltaX, e.deltaY)

  let rows = Array.from({ length: 30 }, (_, i) => i)

  return (
    <window alignItems="center" justifyContent="center">
      <view
        ref={(n: { id: number }) => (viewport = n)}
        width={220}
        height={280}
        overflow="hidden"
        clipRadius={12}
        scrollY={scroll.offset().y}
        onWheel={onWheel}
      >
        <d-rect color="#1a2233" radius={12} />
        <view ref={(n: { id: number }) => (content = n)} flexShrink={0} padding={12} gap={8}>
          <For each={rows}>
            {(i) => (
              <view height={40} justifyContent="center" paddingLeft={12}>
                <rect height={40} radius={6} color={i % 2 ? "#2a3f5f" : "#3366b3"} />
                <text color="#ffffff">Row {i}</text>
              </view>
            )}
          </For>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
