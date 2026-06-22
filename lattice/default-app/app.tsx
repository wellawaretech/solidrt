import { render } from "@solidrt/core"
import { createCamera, cameraDevices, type BarcodeResult } from "@solidrt/core/camera"
import { createEffect, createSignal, untrack } from "@solidjs/signals"
import { For, Show } from "solid-js"
import { on } from "srt:events"
import { available as devAvailable, canDiscover, connect, discover, stop, recents as initialRecents, launchAddress } from "srt:dev"
import { Logo } from "./logo"

type DevState = "idle" | "searching" | "connecting" | "connected"

const STATUS_TEXT: Record<DevState, string> = {
  idle: "not connected",
  searching: "searching...",
  connecting: "connecting...",
  connected: "connected",
}

// The dev server QR encodes a bare host:port; tolerate a scheme prefix and a
// trailing slash in case the encoded value ever changes.
function normalizeAddress(raw: string): string {
  return raw.trim().replace(/^(ws|http):\/\//, "").replace(/\/+$/, "")
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

// Viewfinder over createCamera: mounts only while scanning (under <Show>), so
// the camera opens when scanning starts and closes when it stops.
function CameraView(props: {
  width?: number
  scan?: "qr"[]
  onBarcode?: (result: BarcodeResult) => void
  onError?: (error: Error) => void
}) {
  let cam = createCamera(untrack(() => ({ width: props.width, scan: props.scan })))
  createEffect(() => cam.barcode(), (b) => { if (b) props.onBarcode?.(b) })
  createEffect(() => cam.error(), (e) => { if (e) props.onError?.(e) })
  return <texture src={cam.texture()} width={props.width} />
}

function App() {
  let dev = devAvailable
  let hasCamera = () => cameraDevices().length > 0

  let [state, setState] = createSignal<DevState>("idle")
  let [address, setAddress] = createSignal<string | null>(null)
  let [recents, setRecents] = createSignal<string[]>(initialRecents)
  // QR pairing is app-local: a camera scan view that feeds connect() with the
  // decoded address (the supervisor only ever sees a plain Connect).
  let [scanning, setScanning] = createSignal(false)
  let [scanError, setScanError] = createSignal<string | null>(null)

  if (dev) {
    on("dev", (e: { state: DevState; address: string | null; recents?: string[] }) => {
      setState(e.state)
      setAddress(e.address)
      if (e.recents) {
        setRecents(e.recents)
        console.log("got recents", e.recents)
      }
    })
  }

  // Launched with a dev-server address (srt client --android delivers it as an
  // intent extra -> argv): connect immediately so no on-device interaction is
  // needed (e.g. on a TV with no manual entry).
  if (dev && launchAddress) {
    connect(launchAddress)
  }

  let idle = () => state() === "idle"
  let busy = () => state() === "searching" || state() === "connecting"
  let connected = () => state() === "connected"

  let status = () =>
    scanning()
      ? "scan the dev server QR code"
      : connected()
        ? `connected to ${address()}`
        : (scanError() ?? STATUS_TEXT[state()])

  let startScan = () => {
    setScanError(null)
    setScanning(true)
  }

  let onScanned = (data: string) => {
    setScanning(false)
    connect(normalizeAddress(data))
  }

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
          <Show when={scanning()}>
            <CameraView
              width={280}
              scan={["qr"]}
              onBarcode={(r) => onScanned(r.data)}
              onError={(e) => {
                setScanError(`camera: ${e.message}`)
                setScanning(false)
              }}
            />
          </Show>

          <text color="lightgrey">{status()}</text>

          <view flexDirection="row" gap={12}>
            {idle() && !scanning() && canDiscover && (
              <Button label="Discover" color="#3366b3" onTap={() => discover()} />
            )}
            {idle() && !scanning() && dev && hasCamera() && (
              <Button label="Scan QR" color="#3366b3" onTap={startScan} />
            )}
            {idle() && !scanning() && launchAddress && (
              <Button label="Connect" color="#3366b3" onTap={() => connect(launchAddress)} />
            )}
            {scanning() && <Button label="Cancel" color="#555" onTap={() => setScanning(false)} />}
            {busy() && <Button label="Cancel" color="#555" onTap={() => stop()} />}
            {connected() && <Button label="Disconnect" color="#555" onTap={() => stop()} />}
          </view>

          <Show when={idle() && !scanning() && recents().length > 0}>
            <view flexDirection="column" alignItems="center" gap={8}>
              <text color="grey">recent</text>
              <For each={recents()}>
                {(addr) => <Button label={addr} color="#333" onTap={() => connect(addr)} />}
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