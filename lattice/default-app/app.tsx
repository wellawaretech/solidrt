import { render } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"
import { Logo } from "./logo"

// The dev-server control surface installed by the runtime (srt.devServer). It
// is absent in non-go / record builds, so everything below guards on it.
declare const srt: any

type DevState = "idle" | "searching" | "scanning" | "connecting" | "connected"

const LOOPBACK = "127.0.0.1:15194"

const STATUS_TEXT: Record<DevState, string> = {
  idle: "not connected",
  searching: "searching...",
  scanning: "scanning...",
  connecting: "connecting...",
  connected: "connected",
}

function Button(props: { label: string; color: string; onTap: () => void }) {
  return (
    <view
      onPointerDown={props.onTap}
      paddingLeft={18}
      paddingRight={18}
      paddingTop={10}
      paddingBottom={10}
      justifyContent="center"
      alignItems="center"
    >
      <d-rect color={props.color} radius={8} />
      <text color="white">{props.label}</text>
    </view>
  )
}

function App() {
  let dev = typeof srt !== "undefined" ? srt.devServer : undefined
  let caps = dev?.capabilities ?? { connect: false, discover: false, scanQr: false }
  let isAndroid = dev?.platform === "android"

  let [state, setState] = createSignal<DevState>("idle")
  if (dev) srt.on("devServer", (e: { state: DevState }) => setState(e.state))

  let busy = () => state() !== "idle" && state() !== "connected"

  return (
    <window title="solidrt-go">
      <d-rect color="#111" />
      <view
        flexGrow={1}
        justifyContent="center"
        alignItems="center"
        flexDirection="column-reverse"
        gap={40}
      >
        <view flexDirection="column" alignItems="center" gap={16}>
          <text color="lightgrey">{STATUS_TEXT[state()]}</text>
          <view flexDirection="row" gap={12}>
            {caps.discover && <Button label="Discover" color="#3366b3" onTap={() => dev.discover()} />}
            {isAndroid && <Button label="Connect (adb)" color="#3366b3" onTap={() => dev.connect(LOOPBACK)} />}
            {busy() && <Button label="Cancel" color="#555" onTap={() => dev.stop()} />}
          </view>
        </view>
        <Logo />
      </view>
    </window>
  )
}

render(() => <App />)