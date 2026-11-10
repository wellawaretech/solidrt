// The `detached` attribute means "detached from layout": the layout engine
// ignores the element and you place it yourself with x/y. It is set once, as a
// literal written bare at the tag (never `detached={expr}`, never from a spread).
// Because a detached node does not participate in layout, moving it (animating
// x/y) does not reflow its siblings - which makes detached elements the right
// choice for overlays, badges, and anything that moves independently. They are
// the exception, not the default: screen structure (panels, cards, labels) stays
// laid out with <view>/<text>; a detached subtree is for drawn or moving content.
// The rule: a detached node can only contain other detached nodes. Everything
// under a detached element must itself be detached (here <rect detached> +
// <text detached>) - nesting a plain <view> or <text> inside a detached element
// is an error.
import { render } from "@solidrt/core"

function App() {
  return (
    <window>
      <view detached x={40} y={60}>
        <rect detached w={160} h={64} radius={12} color="#3366b3" />
        <text detached x={16} y={20} fontSize={18} color="#ffffff">Badge</text>
      </view>
    </window>
  )
}

render(() => <App />)