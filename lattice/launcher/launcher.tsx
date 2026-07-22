// The go client's launcher: the compiled-in home screen. Lists the apps
// installed in the version store (tap to launch, delete with inline confirm)
// and manages the dev-server connection (discover, QR scan on a full-screen
// camera view, manual address entry, recents). Built from @solidrt/components;
// follows the OS dark/light preference. Bundled by `make launcher-bundle` and
// embedded via include_str! (see lattice/src/lib.rs LAUNCHER_SOURCE).
import { render, env, createSignal, createEffect, untrack } from "@solidrt/core"
import { createCamera, cameraDevices, type BarcodeResult } from "@solidrt/core/camera"
import { For, Show, Switch, Match } from "solid-js"
import {
  Window,
  View,
  Text,
  Button,
  TextInput,
  ScrollView,
  Pressable,
  Icon,
  SafeArea,
  theme,
  setTheme,
  darkTheme,
  lightTheme,
  space,
} from "@solidrt/components"
import { on } from "srt:events"
import {
  available as devAvailable,
  canDiscover,
  connect,
  discover,
  stop,
  launchAddress,
} from "srt:dev"
import { available as appsAvailable, list, launch, remove } from "srt:apps"
import puzzle from "../assets/icon-puzzle.svg"

type DevState = "idle" | "searching" | "connecting" | "connected"
type Screen = "home" | "scan" | "manual"

const STATUS_TEXT: Record<DevState, string> = {
  idle: "Not connected",
  searching: "Searching...",
  connecting: "Connecting...",
  connected: "Connected",
}

const LUCIDE = (body: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"` +
  ` stroke="currentColor" stroke-width="2" stroke-linecap="round"` +
  ` stroke-linejoin="round">${body}</svg>`
const TRASH = LUCIDE(
  `<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>` +
    `<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>`,
)

// The dev server QR encodes a bare host:port; tolerate a scheme prefix and a
// trailing slash in case the encoded value ever changes.
function normalizeAddress(raw: string): string {
  return raw.trim().replace(/^(ws|http):\/\//, "").replace(/\/+$/, "")
}

// A recent entry is either a `host:port` address or a p2p ticket (which
// contains `|`). Tickets are long, so show a short "ticket <id-prefix>" label
// while still dialing the full string.
function recentLabel(entry: string): string {
  if (!entry.includes("|")) return entry
  return "ticket " + entry.split("|")[0].slice(0, 8)
}

// Full-window camera with center cover-crop and a corner-bracket scan marker.
// Mounted only while scanning (under <Match>), so the camera opens with the
// screen and closes when it leaves.
function ScanScreen(props: { onScanned: (data: string) => void; onCancel: () => void; onError: (message: string) => void }) {
  let cam = createCamera(untrack(() => ({ scan: ["qr"] as "qr"[] })))
  createEffect(() => cam.barcode(), (b?: BarcodeResult) => { if (b) props.onScanned(b.data) })
  createEffect(() => cam.error(), (e?: Error) => { if (e) props.onError(e.message) })

  // Source rect for object-fit: cover, centered, in camera pixels.
  let crop = () => {
    let cw = cam.width()
    let ch = cam.height()
    let { width: w, height: h } = env.windowSize
    if (!cw || !ch || !w || !h) return null
    let scale = Math.max(w / cw, h / ch)
    let srcW = w / scale
    let srcH = h / scale
    return { w, h, srcX: (cw - srcW) / 2, srcY: (ch - srcH) / 2, srcW, srcH }
  }

  let marker = () => {
    let { width: w, height: h } = env.windowSize
    let s = Math.round(Math.min(w, h) * 0.55)
    let l = Math.round(s * 0.18)
    return {
      size: s,
      d:
        `M0 ${l} L0 0 L${l} 0 ` +
        `M${s - l} 0 L${s} 0 L${s} ${l} ` +
        `M${s} ${s - l} L${s} ${s} L${s - l} ${s} ` +
        `M${l} ${s} L0 ${s} L0 ${s - l}`,
    }
  }

  return (
    <view flexGrow={1}>
      <d-rect color="black" />
      <Show when={cam.texture() != null && crop()}>
        {(c) => (
          <texture
            src={cam.texture()}
            w={c().w}
            h={c().h}
            srcX={c().srcX}
            srcY={c().srcY}
            srcW={c().srcW}
            srcH={c().srcH}
          />
        )}
      </Show>
      <view width="100%" height="100%" justifyContent="center" alignItems="center">
        <view width={marker().size} height={marker().size}>
          <d-path d={marker().d} color="white" drawStyle="stroke" strokeWidth={3} />
        </view>
      </view>
      <SafeArea>
        <view flexGrow={1} flexDirection="column" justifyContent="space-between" padding={space("xl")}>
          <view flexDirection="row">
            <Button variant="secondary" onPress={props.onCancel}>Cancel</Button>
          </view>
          <view alignItems="center">
            <text color="white">Scan the dev server QR code</text>
          </view>
        </view>
      </SafeArea>
    </view>
  )
}

function App() {
  let dev = devAvailable

  // Follow the OS theme; dark until it resolves.
  createEffect(
    () => env.systemTheme,
    (t) => { if (t !== "unknown") setTheme(t === "light" ? lightTheme : darkTheme) },
  )

  let [screen, setScreen] = createSignal<Screen>("home")
  let [apps, setApps] = createSignal(appsAvailable ? list() : [])
  let [confirming, setConfirming] = createSignal<string | null>(null)
  let [notice, setNotice] = createSignal<string | null>(null)

  let [state, setState] = createSignal<DevState>("idle")
  let [address, setAddress] = createSignal<string | null>(null)
  let [tunneled, setTunneled] = createSignal(false)
  let [recents, setRecents] = createSignal<string[]>([])
  if (dev) {
    on("dev", (e: { state: DevState; address: string | null; tunneled: boolean; recents?: string[] }) => {
      setState(e.state)
      setAddress(e.address)
      setTunneled(e.tunneled)
      if (e.recents) setRecents(e.recents)
    })
  }
  // Launched with a dev-server address: dial it without on-device interaction.
  // The supervisor ignores redundant connects (see go/connection.rs), so the
  // re-dial a reload-remount causes is harmless.
  if (dev && launchAddress && untrack(() => state()) === "idle") {
    connect(launchAddress)
  }

  let idle = () => state() === "idle"
  let busy = () => state() === "searching" || state() === "connecting"
  let connected = () => state() === "connected"
  let hasCamera = () => cameraDevices().length > 0

  let status = () =>
    connected()
      ? `Connected to ${address()}${tunneled() ? " (tunneled)" : ""}`
      : (notice() ?? STATUS_TEXT[state()])

  let doLaunch = (id: string) => {
    try {
      launch(id)
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e))
    }
  }
  let doRemove = (id: string) => {
    setConfirming(null)
    try {
      remove(id)
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e))
    }
    setApps(appsAvailable ? list() : [])
  }
  let dial = (addr: string) => {
    setNotice(null)
    setScreen("home")
    connect(normalizeAddress(addr))
  }

  let manualDraft = ""

  return (
    <Window title="SolidRT" layout={{ flexDirection: "column" }} style={{ backgroundColor: theme.color.background }}>
      <SafeArea>
        <Switch>
          <Match when={screen() === "scan"}>
            <ScanScreen
              onScanned={(data) => dial(data)}
              onCancel={() => setScreen("home")}
              onError={(m) => {
                setNotice(`Camera: ${m}`)
                setScreen("home")
              }}
            />
          </Match>

          <Match when={screen() === "manual"}>
            <view flexGrow={1} alignItems="center">
              <view flexDirection="column" gap={space("lg")} width="100%" maxWidth={440} padding={space("xl")} paddingTop={72}>
                <View
                  layout={{ flexDirection: "column", gap: space("lg"), padding: space("xl") }}
                  style={{ backgroundColor: theme.color.surface, borderRadius: theme.radius.lg }}
                >
                  <Text variant="title">Connect to a dev server</Text>
                  <TextInput
                    placeholder="host:port"
                    autoFocus
                    onInput={(v) => (manualDraft = v)}
                    onSubmit={(v) => { if (v.trim()) dial(v) }}
                  />
                  <view flexDirection="row" gap={space("md")}>
                    <Button onPress={() => { if (manualDraft.trim()) dial(manualDraft) }}>Connect</Button>
                    <Button variant="ghost" onPress={() => setScreen("home")}>Cancel</Button>
                  </view>
                </View>
                <Show when={recents().length > 0}>
                  <view flexDirection="column" gap={space("sm")}>
                    <Text variant="label" muted>Recent</Text>
                    <view flexDirection="row" flexWrap="wrap" gap={space("sm")}>
                      <For each={recents()}>
                        {(entry) => (
                          <Pressable
                            onPress={() => dial(entry)}
                            layout={{ paddingLeft: space("lg"), paddingRight: space("lg"), paddingTop: space("md"), paddingBottom: space("md") }}
                            style={(s) => ({
                              backgroundColor: s.hovered ? theme.color.surfaceHover : theme.color.surfaceAlt,
                              borderRadius: theme.radius.lg,
                            })}
                          >
                            <Text variant="label">{recentLabel(entry)}</Text>
                          </Pressable>
                        )}
                      </For>
                    </view>
                  </view>
                </Show>
              </view>
            </view>
          </Match>

          <Match when={screen() === "home"}>
            <view flexGrow={1} alignItems="center">
              <view flexDirection="column" width="100%" maxWidth={440} flexGrow={1} padding={space("xl")} gap={space("xl")}>
                {/* Centered mark. */}
                <view alignItems="center" paddingTop={space("xl")} gap={space("md")}>
                  <svg src={puzzle} width={144} height={144} />
                </view>

                <Show
                  when={apps().length > 0}
                  fallback={
                    <view flexGrow={1} flexDirection="column" justifyContent="center" alignItems="center" gap={space("md")}>
                      <Text variant="title">No apps installed</Text>
                      <Text muted>Connect a dev server to install apps</Text>
                    </view>
                  }
                >
                  <ScrollView layout={{ flexGrow: 1 }}>
                    <view flexDirection="column" gap={space("md")}>
                      <Text variant="label" muted>Apps</Text>
                      <For each={apps()}>
                        {(app) => (
                          <Pressable
                            onPress={() => { if (confirming() !== app.id) doLaunch(app.id) }}
                            layout={{
                              flexDirection: "row",
                              alignItems: "center",
                              padding: space("xl"),
                              gap: space("md"),
                            }}
                            style={(s) => ({
                              backgroundColor: s.hovered ? theme.color.surfaceHover : theme.color.surface,
                              borderRadius: theme.radius.lg,
                            })}
                          >
                            <view flexDirection="column" flexGrow={1} gap={2}>
                              <Text variant="title">{app.name}</Text>
                              <Text variant="caption" muted>{`${app.id} - ${app.version.slice(0, 8)}`}</Text>
                            </view>
                            <Show
                              when={confirming() === app.id}
                              fallback={
                                <Pressable
                                  onPress={() => setConfirming(app.id)}
                                  layout={{ padding: space("sm") }}
                                  style={(s) => ({
                                    backgroundColor: s.hovered ? theme.color.surfaceAlt : "transparent",
                                    borderRadius: theme.radius.sm,
                                  })}
                                >
                                  <Icon src={TRASH} size={18} color={theme.color.textMuted} />
                                </Pressable>
                              }
                            >
                              <view flexDirection="row" alignItems="center" gap={space("sm")}>
                                <Button variant="danger" onPress={() => doRemove(app.id)}>Remove</Button>
                                <Button variant="ghost" onPress={() => setConfirming(null)}>Keep</Button>
                              </view>
                            </Show>
                          </Pressable>
                        )}
                      </For>
                    </view>
                  </ScrollView>
                </Show>

                {/* Dev connection card. */}
                <Show when={dev}>
                  <View
                    layout={{ flexDirection: "column", gap: space("md"), padding: space("lg") }}
                    style={{ backgroundColor: theme.color.surface, borderRadius: theme.radius.lg }}
                  >
                    <view flexDirection="row" alignItems="center" gap={space("md")}>
                      <view width={8} height={8}>
                        <d-oval color={connected() || busy() ? theme.color.primary : theme.color.textMuted} />
                      </view>
                      <Text variant="caption" muted layout={{ flexGrow: 1 }}>{status()}</Text>
                    </view>
                    <view flexDirection="row" gap={space("sm")}>
                      <Show when={idle()}>
                        <Show when={canDiscover}>
                          <Button variant="secondary" onPress={() => discover()}>Discover</Button>
                        </Show>
                        <Show when={hasCamera()}>
                          <Button variant="secondary" onPress={() => { setNotice(null); setScreen("scan") }}>Scan QR</Button>
                        </Show>
                        <Button variant="secondary" onPress={() => setScreen("manual")}>Address</Button>
                      </Show>
                      <Show when={busy()}>
                        <Button variant="secondary" onPress={() => stop()}>Cancel</Button>
                      </Show>
                      <Show when={connected()}>
                        <Button variant="secondary" onPress={() => stop()}>Disconnect</Button>
                      </Show>
                    </view>
                  </View>
                </Show>
              </view>
            </view>
          </Match>
        </Switch>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
