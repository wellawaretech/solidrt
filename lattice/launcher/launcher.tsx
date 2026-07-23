// The go client's launcher: the compiled-in home screen. Lists the apps
// installed in the version store (tap to launch, info button for a detail
// view with remove) and manages the dev-server connection (discover, QR scan
// on a full-screen camera view, manual address entry, recents). Built from
// @solidrt/components; follows the OS dark/light preference and the layout
// policy: wide windows show a WhatsApp-style split (list left, selected app's
// details right), narrow ones navigate between two screens. Bundled by
// `make launcher-bundle` and embedded via include_str! (see lattice/src/lib.rs
// LAUNCHER_SOURCE).
import {
  render,
  env,
  createSignal,
  createEffect,
  untrack,
  createLinearGradient,
} from "@solidrt/core"
import { createCamera, cameraDevices, type BarcodeResult } from "@solidrt/core/camera"
import { For, Show, Switch, Match, createMemo } from "solid-js"
import {
  Window,
  View,
  Card,
  Text,
  Button,
  TextInput,
  ScrollView,
  Pressable,
  type PressState,
  SafeArea,
  Spinner,
  SplitView,
  Modal,
  Icon,
  SegmentedControl,
  theme,
  setTheme,
  darkTheme,
  lightTheme,
  policy,
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
import {
  available as appsAvailable,
  list,
  info,
  launch,
  remove,
  clearCache,
  version as buildVersion,
  profile as buildProfile,
  platform as buildPlatform,
  type AppCacheEntry,
  type InstalledApp,
} from "srt:apps"

type DevState = "idle" | "searching" | "connecting" | "connected"
type Screen = "home" | "scan" | "manual" | "settings"
type ThemeMode = "system" | "light" | "dark"

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
  {
    light: "#3f5494",
    dark: "#162b6c",
    d: "M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z",
  },
  {
    light: "#547ebf",
    dark: "#2b5696",
    d: "M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z",
  },
  {
    light: "#7ea9ea",
    dark: "#5681c1",
    d: "M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z",
  },
  {
    light: "#547ebf",
    dark: "#2b5696",
    d: "M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z",
  },
  {
    light: "#7ea9ea",
    dark: "#5681c1",
    d: "M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z",
  },
  {
    light: "#3f5494",
    dark: "#162b6c",
    d: "M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z",
  },
  {
    light: "#7ea9ea",
    dark: "#5681c1",
    d: "M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z",
  },
]

// Lucide settings (gear) glyph for the header button that opens the settings
// screen, stroked with currentColor so the Icon component recolors it.
const GEAR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>`

// Renders the mark at `size`: the segment paths are authored in a 100x100 space
// and scaled to `size` from the TOP-LEFT origin, so they map exactly onto the
// box. (A center origin only fills a 100px box; scaling an oversized inner box
// instead overflows and corrupts the surrounding flex layout.) The d-paths are
// detached, so they add no layout footprint of their own.
function PuzzleMark(props: { size: number }) {
  return (
    <View
      layout={{ width: props.size, height: props.size }}
      style={{ scale: props.size / 100, originX: 0, originY: 0 }}
    >
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
    </View>
  )
}

// The dev server QR encodes a bare host:port; tolerate a scheme prefix and a
// trailing slash in case the encoded value ever changes.
function normalizeAddress(raw: string): string {
  return raw
    .trim()
    .replace(/^(ws|http):\/\//, "")
    .replace(/\/+$/, "")
}

// A recent entry is either a `host:port` address or a p2p ticket (which
// contains `|`). Tickets are long, so show a short "ticket <id-prefix>" label
// while still dialing the full string.
function recentLabel(entry: string): string {
  if (!entry.includes("|")) return entry
  return "ticket " + entry.split("|")[0].slice(0, 8)
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  let kb = bytes / 1024
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`
  let mb = kb / 1024
  if (mb < 1024) return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`
  return `${(mb / 1024).toFixed(2)} GB`
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
function ScanScreen(props: {
  onScanned: (data: string) => void
  onCancel: () => void
  onError: (message: string) => void
}) {
  let cam = createCamera(untrack(() => ({ scan: ["qr"] as "qr"[] })))
  createEffect(
    () => cam.barcode(),
    (b?: BarcodeResult) => {
      if (b) props.onScanned(b.data)
    },
  )
  createEffect(
    () => cam.error(),
    (e?: Error) => {
      if (e) props.onError(e.message)
    },
  )

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
    <View layout={{ flexGrow: 1, position: "relative" }} style={{ backgroundColor: "black" }}>
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
      <View
        layout={{
          position: "absolute",
          width: "100%",
          height: "100%",
          justifyContent: "center",
          alignItems: "center",
        }}
      >
        <View layout={{ width: reticle().size, height: reticle().size }}>
          <d-path
            d={reticle().d}
            color="white"
            drawStyle="stroke"
            strokeWidth={RETICLE_STROKE}
            strokeCap="round"
            strokeJoin="round"
          />
        </View>
      </View>
      <View layout={{ position: "absolute", width: "100%", height: "100%" }}>
        <SafeArea>
          <View layout={{ flexGrow: 1, padding: space("xl") }}>
            <View layout={{ flexDirection: "row" }}>
              <Button variant="secondary" size="md" onPress={props.onCancel}>
                Cancel
              </Button>
            </View>
          </View>
        </SafeArea>
      </View>
    </View>
  )
}

// One app row: pressing it opens the detail view (which holds launch and
// remove).
function AppCard(props: { app: InstalledApp; active: boolean; onPress: () => void }) {
  return (
    <Pressable onPress={props.onPress}>
      {(s: PressState) => (
        <Card
          layout={{ gap: 2 }}
          style={{
            backgroundColor: props.active
              ? theme.color.surfaceAlt
              : s.hovered
                ? theme.color.surfaceHover
                : theme.color.surface,
          }}
        >
          <Text variant="title">{props.app.name}</Text>
          <Text variant="body" muted>{`${props.app.id} - ${props.app.version.slice(0, 8)}`}</Text>
        </Card>
      )}
    </Pressable>
  )
}

// The detail view's cache aggregation: entries grouped by a key (content
// type, domain), largest first.
function groupCache(entries: AppCacheEntry[], key: (e: AppCacheEntry) => string) {
  let groups = new Map<string, { key: string; count: number; size: number }>()
  for (let e of entries) {
    let k = key(e)
    let g = groups.get(k)
    if (!g) groups.set(k, (g = { key: k, count: 0, size: 0 }))
    g.count += 1
    g.size += e.size
  }
  return [...groups.values()].sort((a, b) => b.size - a.size)
}

function cacheDomain(url: string): string {
  let m = /^[a-z][a-z0-9+.-]*:\/\/([^/]+)/i.exec(url)
  return m?.[1] ?? "unknown"
}

// "3 files, 1.2 MB" - the uniform count + size format the detail rows use.
function amount(count: number, size: number): string {
  return `${count} file${count === 1 ? "" : "s"}, ${formatSize(size)}`
}

function DetailRow(props: { label: string; value: string; mutedValue?: boolean }) {
  return (
    <View layout={{ flexDirection: "row", justifyContent: "space-between", gap: space("md") }}>
      <Text variant="body" muted>
        {props.label}
      </Text>
      <Text variant="body" muted={props.mutedValue}>
        {props.value}
      </Text>
    </View>
  )
}

function DetailCard(props: { title: string; children?: any }) {
  return (
    <Card layout={{ gap: space("md"), padding: space("lg") }}>
      <Text variant="title" muted>
        {props.title}
      </Text>
      {props.children}
    </Card>
  )
}

// The selected app's detail view: identity, storage usage, stored versions and
// the data sandbox's files, with launch and remove. Shared between the split
// view's right pane (no onBack) and the narrow layout's detail screen (with
// onBack).
function AppDetail(props: {
  app: InstalledApp
  onLaunch: () => void
  onRemove: () => void
  onBack?: () => void
}) {
  let [confirming, setConfirming] = createSignal(false)
  // Selecting another app in the split view reuses this component; reset the
  // pending confirm so it never carries over to the newly selected app.
  createEffect(
    () => props.app.id,
    () => {
      setConfirming(false)
    },
  )
  // Usage details, re-read per app and after a cache clear (the bump signal
  // is the only local mutation that changes what info() reports). Null when
  // the store entry vanished mid-view (e.g. replaced by a dev push);
  // identity and actions still work.
  let [detailsGen, setDetailsGen] = createSignal(0)
  let details = createMemo(() => {
    detailsGen()
    try {
      return info(props.app.id)
    } catch {
      return null
    }
  })

  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View
        layout={{
          flexDirection: "column",
          gap: space("lg"),
          padding: space("xl"),
          width: "100%",
          maxWidth: 520,
        }}
      >
        <Show when={props.onBack}>
          <View layout={{ flexDirection: "row" }}>
            <Button variant="ghost" size="sm" onPress={() => props.onBack?.()}>
              Back
            </Button>
          </View>
        </Show>
        <View layout={{ flexDirection: "column", gap: 2 }}>
          <Text variant="heading">{props.app.name}</Text>
          <Text variant="body" muted>
            {props.app.id}
          </Text>
        </View>
        <View layout={{ flexDirection: "row", gap: space("md") }}>
          <Button onPress={() => props.onLaunch()}>Launch</Button>
          <Button variant="secondary" onPress={() => setConfirming(true)}>
            Remove
          </Button>
        </View>
        <Show when={confirming()}>
          <Modal onClose={() => setConfirming(false)}>
            <View layout={{ width: "100%", maxWidth: 380, padding: space("xl") }}>
              <Card layout={{ gap: space("lg") }}>
                <View layout={{ flexDirection: "column", gap: space("sm") }}>
                  <Text variant="title">Remove {props.app.name}?</Text>
                  <Text variant="body" muted>
                    This deletes the app and its stored data. This cannot be undone.
                  </Text>
                </View>
                <View layout={{ flexDirection: "row", gap: space("md") }}>
                  <Button variant="ghost" onPress={() => setConfirming(false)}>
                    Cancel
                  </Button>
                  <Button variant="danger" onPress={() => props.onRemove()}>
                    Remove
                  </Button>
                </View>
              </Card>
            </View>
          </Modal>
        </Show>
        <Show when={details()}>
          {(d) => (
            <>
              <DetailCard title="Storage">
                <DetailRow label="App" value={formatSize(d().installSize)} />
                <DetailRow
                  label="Files"
                  value={amount(
                    d().files.length,
                    d().files.reduce((sum, f) => sum + f.size, 0),
                  )}
                />
                <DetailRow label="Data" value={amount(d().data.length, d().dataSize)} />
                <DetailRow label="Cache" value={amount(d().cache.length, d().cacheSize)} />
              </DetailCard>
              <DetailCard title="Versions">
                <For each={d().versions}>
                  {(v) => (
                    <DetailRow
                      label={v.id.slice(0, 12) + (v.current ? " (current)" : "")}
                      value={formatSize(v.size)}
                      mutedValue={!v.current}
                    />
                  )}
                </For>
              </DetailCard>
              <DetailCard title="Files">
                <For each={d().files}>
                  {(f) => <DetailRow label={f.path} value={formatSize(f.size)} />}
                </For>
              </DetailCard>
              <DetailCard title="Data">
                <Show
                  when={d().data.length > 0}
                  fallback={
                    <Text variant="body" muted>
                      Empty
                    </Text>
                  }
                >
                  <For each={d().data}>
                    {(f) => <DetailRow label={f.path} value={formatSize(f.size)} />}
                  </For>
                </Show>
              </DetailCard>
              <DetailCard title="Cache">
                <Show
                  when={d().cache.length > 0}
                  fallback={
                    <Text variant="body" muted>
                      Empty
                    </Text>
                  }
                >
                  <Text variant="body">By type</Text>
                  <For each={groupCache(d().cache, (e) => e.type ?? "unknown")}>
                    {(g) => <DetailRow label={g.key} value={amount(g.count, g.size)} />}
                  </For>
                  <Text variant="body">By domain</Text>
                  <For each={groupCache(d().cache, (e) => cacheDomain(e.url))}>
                    {(g) => <DetailRow label={g.key} value={amount(g.count, g.size)} />}
                  </For>
                </Show>
              </DetailCard>
                <Show when={d().cache.length > 0}>
                  <Button
                    variant="danger"
                    onPress={() => {
                      clearCache(props.app.id)
                      setDetailsGen((n) => n + 1)
                    }}
                  >
                    Clear cache
                  </Button>
                </Show>
            </>
          )}
        </Show>
      </View>
    </ScrollView>
  )
}

// The installed-app list: one AppCard per app, scrolling. Selection state is
// owned by App and threaded in, so crossing the layout breakpoint keeps it.
function AppList(props: {
  apps: InstalledApp[]
  selectedId: string | null
  twoPane: boolean
  onSelect: (id: string) => void
}) {
  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View layout={{ flexDirection: "column", gap: space("md") }}>
        <For each={props.apps}>
          {(app) => (
            <AppCard
              app={app}
              active={props.twoPane && props.selectedId === app.id}
              onPress={() => props.onSelect(app.id)}
            />
          )}
        </For>
      </View>
    </ScrollView>
  )
}

// Empty state for the list, shown when nothing is installed yet.
function NoApps() {
  return (
    <View
      layout={{
        flexGrow: 1,
        flexDirection: "column",
        justifyContent: "center",
        alignItems: "center",
        gap: space("md"),
      }}
    >
      <Text variant="title">No apps installed</Text>
      <Text muted>Connect a dev server to install apps</Text>
    </View>
  )
}

// The dev-server control surface: a status line (dot, or spinner while working)
// and the connect actions for the current state. Connection state is owned by
// App; scan/manual navigation come back as callbacks.
function DevCard(props: {
  status: string
  idle: boolean
  busy: boolean
  connected: boolean
  onScan: () => void
  onManual: () => void
}) {
  let hasCamera = () => cameraDevices().length > 0
  return (
    <Card layout={{ gap: space("md"), padding: space("lg") }}>
      <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
        <Show
          when={props.busy}
          fallback={
            <View layout={{ width: 8, height: 8 }}>
              <d-oval color={props.connected ? theme.color.primary : theme.color.textMuted} />
            </View>
          }
        >
          <Spinner size={14} thickness={2} />
        </Show>
        <Text variant="body" muted layout={{ flexGrow: 1 }}>
          {props.status}
        </Text>
      </View>
      <View layout={{ flexDirection: "row", gap: space("sm") }}>
        <Show when={props.idle}>
          <Show when={canDiscover}>
            <Button variant="secondary" onPress={() => discover()}>
              Discover
            </Button>
          </Show>
          <Show when={hasCamera()}>
            <Button variant="secondary" onPress={props.onScan}>
              Scan QR
            </Button>
          </Show>
          <Button variant="secondary" onPress={props.onManual}>
            Address
          </Button>
        </Show>
        <Show when={props.busy}>
          <Button variant="secondary" onPress={() => stop()}>
            Cancel
          </Button>
        </Show>
        <Show when={props.connected}>
          <Button variant="secondary" onPress={() => stop()}>
            Disconnect
          </Button>
        </Show>
      </View>
    </Card>
  )
}

// One capability name as a filled chip, for the About block's list.
function CapabilityChip(props: { name: string }) {
  return (
    <View
      layout={{
        paddingLeft: space("md"),
        paddingRight: space("md"),
        paddingTop: space("sm"),
        paddingBottom: space("sm"),
      }}
      style={{ backgroundColor: theme.color.surfaceAlt, borderRadius: theme.radius.sm }}
    >
      <Text variant="body" muted>
        {props.name}
      </Text>
    </View>
  )
}

// The settings screen: theme mode and the runtime's build identity. Reached
// from the header gear; a Back button returns home in both layouts.
function SettingsScreen(props: {
  mode: ThemeMode
  onMode: (mode: ThemeMode) => void
  onBack: () => void
}) {
  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View layout={{ flexGrow: 1, alignItems: "center" }}>
        <View
          layout={{
            flexDirection: "column",
            gap: space("lg"),
            width: "100%",
            maxWidth: 440,
            padding: space("xl"),
          }}
        >
          <View layout={{ flexDirection: "row" }}>
            <Button variant="ghost" size="sm" onPress={props.onBack}>
              Back
            </Button>
          </View>
          <Text variant="heading">Settings</Text>
          <DetailCard title="Appearance">
            <SegmentedControl
              options={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
              value={props.mode}
              onChange={(v) => props.onMode(v as ThemeMode)}
            />
          </DetailCard>
          <DetailCard title="About">
            <DetailRow label="Build version" value={buildVersion} />
            <DetailRow label="Profile" value={buildProfile} />
            <DetailRow label="Flux version" value={Flux.version} />
            <DetailRow label="Platform" value={buildPlatform} />
          </DetailCard>
          <DetailCard title="Capabilities">
            <View layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("sm") }}>
              <For each={Flux.capabilities}>{(name) => <CapabilityChip name={name} />}</For>
            </View>
          </DetailCard>
        </View>
      </View>
    </ScrollView>
  )
}

function App() {
  let dev = devAvailable

  // Theme mode: "system" follows the OS preference (settable back to, unlike a
  // one-way toggle), "light"/"dark" pin it. Effective dark is dark until the OS
  // preference resolves.
  let [themeMode, setThemeMode] = createSignal<ThemeMode>("system")
  let dark = () => {
    let mode = themeMode()
    if (mode === "system") return env.systemTheme !== "light"
    return mode === "dark"
  }
  createEffect(
    () => dark(),
    (d) => setTheme(d ? darkTheme : lightTheme),
  )

  let [screen, setScreen] = createSignal<Screen>("home")
  let [apps, setApps] = createSignal(appsAvailable ? list() : [])
  let [selectedId, setSelectedId] = createSignal<string | null>(null)
  let [notice, setNotice] = createSignal<string | null>(null)

  let [state, setState] = createSignal<DevState>("idle")
  let [address, setAddress] = createSignal<string | null>(null)
  let [tunneled, setTunneled] = createSignal(false)
  let [recents, setRecents] = createSignal<string[]>([])
  if (dev) {
    on(
      "dev",
      (e: { state: DevState; address: string | null; tunneled: boolean; recents?: string[] }) => {
        setState(e.state)
        setAddress(e.address)
        setTunneled(e.tunneled)
        if (e.recents) setRecents(e.recents)
      },
    )
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
  let twoPane = () => policy.layout === "twoPane"
  // A stale selection (removed app, replaced store) resolves to null, which
  // reads as "nothing selected" in both layouts.
  let selectedApp = () => apps().find((a) => a.id === selectedId()) ?? null

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
    try {
      remove(id)
    } catch (e) {
      setNotice(e instanceof Error ? e.message : String(e))
    }
    setSelectedId(null)
    setApps(appsAvailable ? list() : [])
  }
  let dial = (addr: string) => {
    setNotice(null)
    setScreen("home")
    connect(normalizeAddress(addr))
  }

  let manualDraft = ""

  return (
    <Window
      title="SolidRT"
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
    >
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
            <View layout={{ flexGrow: 1, alignItems: "center" }}>
              <View
                layout={{
                  flexDirection: "column",
                  gap: space("lg"),
                  width: "100%",
                  maxWidth: 440,
                  padding: space("xl"),
                  paddingTop: 72,
                }}
              >
                <Card>
                  <Text variant="title">Connect to a dev server</Text>
                  <TextInput
                    placeholder="host:port"
                    autoFocus
                    onInput={(v) => (manualDraft = v)}
                    onSubmit={(v) => {
                      if (v.trim()) dial(v)
                    }}
                  />
                  <View layout={{ flexDirection: "row", gap: space("md") }}>
                    <Button
                      onPress={() => {
                        if (manualDraft.trim()) dial(manualDraft)
                      }}
                    >
                      Connect
                    </Button>
                    <Button variant="ghost" onPress={() => setScreen("home")}>
                      Cancel
                    </Button>
                  </View>
                </Card>
                <Show when={recents().length > 0}>
                  <View layout={{ flexDirection: "column", gap: space("sm") }}>
                    <Text variant="body" muted>
                      Recent
                    </Text>
                    <View layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("sm") }}>
                      <For each={recents()}>
                        {(entry) => (
                          <Pressable
                            onPress={() => dial(entry)}
                            layout={{
                              paddingLeft: space("lg"),
                              paddingRight: space("lg"),
                              paddingTop: space("md"),
                              paddingBottom: space("md"),
                            }}
                            style={(s) => ({
                              backgroundColor: s.hovered
                                ? theme.color.surfaceHover
                                : theme.color.surfaceAlt,
                              borderRadius: theme.radius.lg,
                            })}
                          >
                            <Text variant="body">{recentLabel(entry)}</Text>
                          </Pressable>
                        )}
                      </For>
                    </View>
                  </View>
                </Show>
              </View>
            </View>
          </Match>

          <Match when={screen() === "settings"}>
            <SettingsScreen
              mode={themeMode()}
              onMode={setThemeMode}
              onBack={() => setScreen("home")}
            />
          </Match>

          {/* List-detail home: SplitView shows the app list beside the
              selected app's details when the layout policy is two-pane, and
              navigates between the list and a detail screen when single-pane.
              The list chrome (mark size, centering) and the detail's Back
              affordance fork on the layout, per the SplitView contract. */}
          <Match when={screen() === "home"}>
            <SplitView
              layout={{ flexGrow: 1 }}
              listWidth={380}
              showDetail={selectedApp() != null}
              list={
                <View layout={{ flexGrow: 1, flexDirection: "column", alignItems: "center" }}>
                  <View
                    layout={{
                      flexDirection: "column",
                      flexGrow: 1,
                      width: "100%",
                      maxWidth: twoPane() ? undefined : 440,
                      padding: space("xl"),
                      gap: space("xl"),
                    }}
                  >
                    <View
                      layout={{
                        flexDirection: "row",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
                        <PuzzleMark size={40} />
                        <Text variant="heading">SolidRT</Text>
                      </View>
                      <Pressable
                        onPress={() => setScreen("settings")}
                        layout={{ padding: space("sm") }}
                        style={(s: PressState) => ({
                          backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
                          borderRadius: theme.radius.md,
                        })}
                      >
                        <Icon src={GEAR_SVG} size={22} />
                      </Pressable>
                    </View>
                    <Show when={apps().length > 0} fallback={<NoApps />}>
                      <AppList
                        apps={apps()}
                        selectedId={selectedId()}
                        twoPane={twoPane()}
                        onSelect={(id) => setSelectedId(id)}
                      />
                    </Show>
                    <Show when={dev}>
                      <DevCard
                        status={status()}
                        idle={idle()}
                        busy={busy()}
                        connected={connected()}
                        onScan={() => {
                          setNotice(null)
                          setScreen("scan")
                        }}
                        onManual={() => setScreen("manual")}
                      />
                    </Show>
                  </View>
                </View>
              }
              detail={
                <Show
                  when={selectedApp()}
                  fallback={
                    <View
                      layout={{
                        flexGrow: 1,
                        justifyContent: "center",
                        alignItems: "center",
                        gap: space("lg"),
                      }}
                    >
                      <PuzzleMark size={360} />
                    </View>
                  }
                >
                  {(app) => (
                    <AppDetail
                      app={app()}
                      onLaunch={() => doLaunch(app().id)}
                      onRemove={() => doRemove(app().id)}
                      onBack={twoPane() ? undefined : () => setSelectedId(null)}
                    />
                  )}
                </Show>
              }
            />
          </Match>
        </Switch>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
