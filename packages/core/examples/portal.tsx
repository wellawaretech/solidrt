// createPortal relocates an already-built node to the window root (or a given
// mount), so content declared deep in the tree can escape the layout, clipping,
// and stacking of its surroundings - the primitive behind overlays like modals,
// menus, and tooltips. It moves one concrete node and removes it again when the
// surrounding reactive scope disposes.
//
// Key points:
//  - A JSX element's runtime value IS the built node, so pass the JSX straight to
//    createPortal. It returns void, so `return createPortal(...)` renders nothing
//    in place while the content lives at the mount.
//  - The default mount is the window's flex root, so the portaled node must be
//    position="absolute" - otherwise it takes flow space and displaces content.
//  - Pass a second argument (a node captured from a ref) to mount elsewhere.
import { render, createPortal } from "@solidrt/core"

// Declared inside the clipped card below, but drawn at the window root, on top of
// everything and outside the card's overflow clip.
function Banner() {
  return createPortal(
    <view position="absolute" top={0} left={0} right={0} padding={12}>
      <d-rect color="#e0245e" />
      <text color="#ffffff">Portaled banner - escaped the card</text>
    </view>
  )
}

function App() {
  return (
    <window padding={40}>
      <view flex={1} overflow="hidden" alignItems="center" justifyContent="center">
        <rect width={140} height={140} radius={8} color="#3366b3" />
        <Banner />
      </view>
    </window>
  )
}

render(() => <App />)