import { Logo } from "@solidrt/core"

export function Home() {
  return (
    <view flex={1} gap={20} alignItems="center" justifyContent="center">
      <Logo size={300} animation="loop" />
      <text fontSize={40} color="#ccc">The Solid Runtime</text>
    </view>
  )
}
