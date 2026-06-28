// The d- prefix means "detached from layout": the layout engine ignores it and
// you place it yourself with x/y. Because a detached node does not participate in
// layout, moving it (animating x/y) does not reflow its siblings - which makes
// d- elements the right choice for overlays, badges, and anything that moves
// independently. The rule: a detached node can only contain other detached
// nodes. Everything under a d- element must itself be a d- element (here d-rect +
// d-text) - nesting a plain <view> or <text> inside a d- element is an error.
import { render } from "@solidrt/core"

function App() {
  return (
    <window>
      <d-view x={40} y={60}>
        <d-rect w={160} h={64} radius={12} color="#3366b3" />
        <d-text x={16} y={20} fontSize={18} color="#ffffff">Badge</d-text>
      </d-view>
    </window>
  )
}

render(() => <App />)