// Window state is exposed as reactive accessors - read them in JSX and the UI
// updates when the window changes. Prefer these over the onResize event callback:
// they are reactive, sticky (the first read sees the current value), and live for
// the whole app.
//
//   windowSize()   -> { width, height }
//   safeArea()     -> { top, left, right, bottom } inset distances from each edge
//   displayScale() -> device pixel ratio
//
// safeArea is mostly an interactivity boundary: the platform may intercept
// touches/gestures inside the insets (system bars, notch, home indicator), so do
// not place interactive content there. For plain drawing it is usually fine to
// extend into the left/right insets (full-bleed backgrounds); top and bottom are
// what matter, since content there - text especially - can be covered. So pad the
// top/bottom by the inset and let the background fill the whole window.
import { render, windowSize, safeArea } from "@solidrt/core"

function App() {
  return (
    <window>
      <d-rect color="#101418" />
      <view flex={1} flexDirection="column" gap={8} paddingTop={safeArea().top} paddingBottom={safeArea().bottom}>
        <text fontSize={18} color="#e6e6e6">{windowSize().width} x {windowSize().height}</text>
      </view>
    </window>
  )
}

render(() => <App />)