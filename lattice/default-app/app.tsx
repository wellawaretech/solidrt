import { render } from "@solidrt/core"
import { createMemo, createSignal } from "@solidjs/signals"
import { For, Show } from "solid-js"
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
  let [address, setAddress] = createSignal<string | null>(null)
  let [recents, setRecents] = createSignal<string[]>(dev?.recents ?? [])

  if (dev) {
    srt.on("devServer", (e: { state: DevState; address: string | null; recents?: string[] }) => {
      setState(e.state)
      setAddress(e.address)
      if (e.recents) {
        setRecents(e.recents)
        console.log("got recents", e.recents)
      }
    })
  }

  let idle = () => state() === "idle"
  let busy = () => state() === "searching" || state() === "scanning" || state() === "connecting"
  let connected = () => state() === "connected"

  let status = () => (connected() ? `connected to ${address()}` : STATUS_TEXT[state()])

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
          <text color="lightgrey">{status()}</text>

          <view flexDirection="row" gap={12}>
            {idle() && caps.discover && (
              <Button label="Discover" color="#3366b3" onTap={() => dev.discover()} />
            )}
            {idle() && caps.scanQr && (
              <Button label="Scan QR" color="#3366b3" onTap={() => dev.scanQr()} />
            )}
            {idle() && isAndroid && (
              <Button label="Connect (adb)" color="#3366b3" onTap={() => dev.connect(LOOPBACK)} />
            )}
            {busy() && <Button label="Cancel" color="#555" onTap={() => dev.stop()} />}
            {connected() && <Button label="Disconnect" color="#555" onTap={() => dev.stop()} />}
          </view>

          <Show when={idle() && recents().length > 0}>
            <view flexDirection="column" alignItems="center" gap={8}>
              <text color="grey">recent</text>
              <For each={recents()}>
                {(addr) => <Button label={addr} color="#333" onTap={() => dev.connect(addr)} />}
              </For>
            </view>
          </Show>
        </view>
        <Logo />
      </view>
    </window>
  )
}

render(() => <App />)
