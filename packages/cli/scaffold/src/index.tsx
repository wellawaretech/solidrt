import { render } from "@solidrt/core"

function Icon() {
  return (
    <view width={100} height={100} scale={3}>
      <d-path d="M50 50L0 50L50 0Z" color="#1a3380" />
      <d-path d="M50 50L50 100L0 50Z" color="#3366b3" />
      <d-path d="M50 25L50 0L75 25Z" color="#6699e6" />
      <d-path d="M50 25L75 25L75 50L50 50Z" color="#3366b3" />
      <d-path d="M50 50L75 50L50 75Z" color="#6699e6" />
      <d-path d="M75 50L75 75L50 100L50 75Z" color="#1a3380" />
      <d-path d="M100 50L75 75L75 25Z" color="#6699e6" />
    </view>
  )
}

function App() {
  return (
    <window alignItems="center" justifyContent="center">
      <Icon />
    </window>
  )
}

render(() => <App />)