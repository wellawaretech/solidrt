// <view> is the layout container: a flexbox box positioned by the layout engine.
// It does NOT paint anything itself - it has no color. To make a box visible you
// place a draw primitive inside it (here an attached <rect>, which is both laid
// out and painted). Flex props (flexDirection, gap, padding, alignItems) work as
// in CSS.
import { render } from "@solidrt/core"

function App() {
  return (
    <window>
      <view flex={1} flexDirection="column" gap={12} padding={24} justifyContent="center">
        <rect height={48} radius={8} color="#3366b3" />
        <rect height={48} radius={8} color="#6699e6" />
        <rect height={48} radius={8} color="#99c2f0" />
      </view>
    </window>
  )
}

render(() => <App />)