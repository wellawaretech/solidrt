// The live connection to the dev server, shared across the player screens
// (Home's status line, Connect's recents + dial, Scan's dial). This is app-wide
// singleton state, so per Solid's guidance ("truly app-wide state is a
// module-scope signal, not Context/props") it lives here as module-scope
// signals rather than being threaded through props: the "dev" event drives the
// signals, the launch-address auto-connect fires at load, and screens read the
// exported accessors directly - each read subscribes the caller.
import { createSignal } from "@solidrt/core"
import { on } from "srt:events"
import { available as devAvailable, connect as devConnect, launchAddress } from "srt:dev"
import { normalizeAddress, type DevState } from "./types"

export let available = devAvailable

let [state, setState] = createSignal<DevState>("idle")
let [address, setAddress] = createSignal<string | null>(null)
let [tunneled, setTunneled] = createSignal(false)
let [recents, setRecents] = createSignal<string[]>([])

if (available) {
  on(
    "dev",
    (e: { state: DevState; address: string | null; tunneled: boolean; recents?: string[] }) => {
      setState(e.state)
      setAddress(e.address)
      setTunneled(e.tunneled)
      if (e.recents) setRecents(e.recents)
    },
  )
  // Launched with a dev-server address: dial it without on-device interaction.
  // The supervisor ignores redundant connects (see go/connection.rs), so a
  // re-dial from a hot-reload re-running this module is harmless.
  if (launchAddress) devConnect(launchAddress)
}

// Reactive reads: calling any of these inside a computation subscribes it.
export let connectionState = state
export let serverAddress = address
export let isTunneled = tunneled
export let recentAddresses = recents
export let isConnected = () => state() === "connected"
export let isBusy = () => state() === "searching" || state() === "connecting"
export let isIdle = () => state() === "idle"

// Dial an address (host:port or p2p ticket), tolerating a scheme/trailing slash.
export function connect(addr: string) {
  devConnect(normalizeAddress(addr))
}
