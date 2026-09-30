// The catch-all: a link that matches no other route lands here.
import { useLocation } from "@solidrt/router"

export function NotFound() {
  let location = useLocation()

  return (
    <view flex={1} alignItems="center" justifyContent="center">
      <text fontSize={24} color="#ccc">{"Nothing at " + location()?.path}</text>
    </view>
  )
}
