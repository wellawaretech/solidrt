// The go client's launcher: the compiled-in home screen. Lists the apps
// installed in the version store (tap to launch, delete with inline confirm)
// and manages the dev-server connection (discover, QR scan on a full-screen
// camera view, manual address entry, recents). Built from @solidrt/components;
// follows the OS dark/light preference. Bundled by `make launcher-bundle` and
// embedded via include_str! (see lattice/src/lib.rs LAUNCHER_SOURCE).
import { render, env, createSignal, createEffect, untrack, createLinearGradient } from "@solidrt/core"
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

type DevState = "idle" | "searching" | "connecting" | "connected"
type Screen = "home" | "scan" | "manual"

const STATUS_TEXT: Record<DevState, string> = {
  idle: "Not connected",
  searching: "Searching...",
  connecting: "Connecting...",
  connected: "Connected",
}

// The puzzle brand mark: the scaffold default template's segment paths (a
// 100x100 grid), each filled with its own light-to-dark linear gradient.
// Static here - the template animates them, the launcher never does.
const PUZZLE_SEGMENTS = [
  { light: "#3f5494", dark: "#162b6c", d: "M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z" },
  { light: "#547ebf", dark: "#2b5696", d: "M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z" },
  { light: "#7ea9ea", dark: "#5681c1", d: "M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z" },
  { light: "#547ebf", dark: "#2b5696", d: "M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z" },
  { light: "#7ea9ea", dark: "#5681c1", d: "M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z" },
  { light: "#3f5494", dark: "#162b6c", d: "M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z" },
  { light: "#7ea9ea", dark: "#5681c1", d: "M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z" },
]

// Renders the mark at `size`: the 100x100 segment grid in a scaled inner view
// so the layout box matches the visual size.
function PuzzleMark(props: { size: number }) {
  return (
    <view width={props.size} height={props.size} justifyContent="center" alignItems="center">
      <view width={100} height={100} scale={props.size / 100}>
        <For each={PUZZLE_SEGMENTS}>
          {(seg) => (
            <d-path
              d={seg.d}
              color={createLinearGradient(0, 0, 1, 1, [
                { offset: 0, color: seg.light },
                { offset: 1, color: seg.dark },
              ])}
            />
          )}
        </For>
      </view>
    </view>
  )
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

// The scan reticle's stroke thickness and corner radius (logical px). The
// bracket paths are inset by half the stroke so the round caps stay inside
// the reticle box; each corner turns through an arc so the bend itself is
// rounded, not just the stroke join.
const RETICLE_STROKE = 10
const RETICLE_RADIUS = 20

// Full-window camera with center cover-crop and a corner-bracket scan reticle.
// Mounted only while scanning (under <Match>), so the camera opens with the
// screen and closes when it leaves. The camera, reticle, and controls are
// absolutely positioned layers: in flow they would stack in the column and
// push each other off-center.
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

  let reticle = () => {
    let { width: w, height: h } = env.windowSize
    let s = Math.round(Math.min(w, h) * 0.55)
    let l = Math.round(s * 0.18)
    let i = RETICLE_STROKE / 2
    let r = RETICLE_RADIUS
    return {
      size: s,
      d:
        `M${i} ${l} L${i} ${i + r} A ${r} ${r} 0 0 1 ${i + r} ${i} L${l} ${i} ` +
        `M${s - l} ${i} L${s - i - r} ${i} A ${r} ${r} 0 0 1 ${s - i} ${i + r} L${s - i} ${l} ` +
        `M${s - i} ${s - l} L${s - i} ${s - i - r} A ${r} ${r} 0 0 1 ${s - i - r} ${s - i} L${s - l} ${s - i} ` +
        `M${l} ${s - i} L${i + r} ${s - i} A ${r} ${r} 0 0 1 ${i} ${s - i - r} L${i} ${s - l}`,
    }
  }

  return (
    <view flexGrow={1} position="relative">
      <d-rect color="black" />
      <Show when={cam.texture() != null && crop()}>
        {(c) => (
          <texture
            position="absolute"
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
      <view position="absolute" width="100%" height="100%" justifyContent="center" alignItems="center">
        <view width={reticle().size} height={reticle().size}>
          <d-path
            d={reticle().d}
            color="white"
            drawStyle="stroke"
            strokeWidth={RETICLE_STROKE}
            strokeCap="round"
            strokeJoin="round"
          />
        </view>
      </view>
      <view position="absolute" width="100%" height="100%">
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
                  <PuzzleMark size={144} />
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
