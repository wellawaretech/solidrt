// The root layout: what stays on screen around every route. The matched
// screen renders at <Outlet />.
import { createLinearGradient, safeArea } from "@solidrt/core"
import { Outlet } from "@solidrt/router"

export function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  return (
    <>
      <d-rect color={backgroundColor} />
      <view flex={1} paddingTop={safeArea().top} paddingBottom={safeArea().bottom}>
        <Outlet />
      </view>
    </>
  )
}
