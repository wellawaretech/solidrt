// The root layout: what stays on screen around every route. The matched
// screen renders at <Outlet />.
import { createLinearGradient } from "@solidrt/core"
import { SafeArea, View } from "@solidrt/components"
import { Outlet } from "@solidrt/router"

export function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  return (
    <View layout={{ flex: 1 }} style={{ backgroundColor }}>
      <SafeArea>
        <Outlet />
      </SafeArea>
    </View>
  )
}
