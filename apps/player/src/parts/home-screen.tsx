// The list-detail home screen: a header (brand mark, settings gear, QR scan),
// the installed-app list, the selected app's detail view, and the dev-server
// control surface. Wide windows show a WhatsApp-style split (list left,
// details right); narrow ones navigate between the list and a detail screen.
// The home route's layout (routes.ts): its child routes (settings, connect, an
// app's detail) each take one pane through <Outlet>. Shared state is module
// state in ./app-state and ./dev-connection.
import { createSignal, createEffect, onBack, Logo } from "@solidrt/core"
import { For, Show, createMemo } from "solid-js"
import {
  View,
  Card,
  Text,
  ScrollView,
  Pressable,
  type PressState,
  Button,
  Spinner,
  SplitView,
  Modal,
  Icon,
  theme,
  space,
  policy,
} from "@solidrt/components"
import { useRouter, useLocation, useParams, Outlet } from "@solidrt/router"
import { stop } from "srt:dev"
import {
  launch,
  remove,
  info,
  clearCache,
  type AppCacheEntry,
  type InstalledApp,
} from "srt:apps"
import * as routes from "../routes"
import { installedApps, refreshApps, notice, setNotice } from "./app-state"
import { AppIcon } from "./app-icon"
import { DetailCard, DetailRow } from "./detail-card"
import { BackButton } from "./back-button"
import { ScanButton } from "./scan-button"
import {
  COLUMN_MAX_WIDTH,
  DETAIL_MAX_WIDTH,
  LIST_GUTTER,
  STATUS_TEXT,
  TAP_TARGET,
  focusRing,
} from "./types"
import {
  available,
  connectionState,
  serverAddress,
  isTunneled,
  isConnected,
  isBusy,
  isIdle,
} from "./dev-connection"

// Where a home child route sits in the match chain: root, home, then the
// child (see routes.ts).
const HOME_CHILD_DEPTH = 2

// Lucide settings (gear) glyph for the header button that opens the settings
// screen, stroked with currentColor so the Icon component recolors it.
const GEAR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>`

// Lucide play glyph for the app rows' quick-launch button, filled (not the
// stock outline) so it still reads as a launch affordance at 20px.
const PLAY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 3 20 12 6 21 6 3"/></svg>`

// An app row's "last updated" stamp: the local time, and how long ago that was
// unless it was today. Distance is counted in calendar days (local midnights),
// so "yesterday" means yesterday's date rather than 24 hours back. An unknown
// timestamp (0, an unreadable store) formats to nothing rather than the epoch.
function formatStamp(ms: number): string {
  if (!ms) return ""
  let then = new Date(ms)
  let pad = (n: number) => String(n).padStart(2, "0")
  let time = `${pad(then.getHours())}:${pad(then.getMinutes())}`
  let midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  let days = Math.round((midnight(new Date()) - midnight(then)) / 86400000)
  // Negative days means a clock that moved backwards; read it as today.
  if (days <= 0) return time
  return days === 1 ? `${time}, yesterday` : `${time}, ${days} days ago`
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  let kb = bytes / 1024
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`
  let mb = kb / 1024
  if (mb < 1024) return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`
  return `${(mb / 1024).toFixed(2)} GB`
}

// One app row: pressing it opens the detail view (which holds launch and
// remove), and the play button launches straight from the list. That button is
// a pressable nested inside the row's: the innermost one wins the gesture
// arena, so a press on it never also opens the detail.
function AppCard(props: {
  app: InstalledApp
  active: boolean
  onPress: () => void
  onLaunch: () => void
}) {
  // Size and last-updated stamp on one line. The id earns a place in front of
  // them only when it says something the title does not: an app whose manifest
  // names no displayName is listed under its id, and the line would just
  // repeat the title. An unknown stamp drops out rather than trailing a comma.
  let subtitle = () => {
    let details = [formatSize(props.app.size), formatStamp(props.app.updated)].filter(Boolean).join(", ")
    return props.app.name === props.app.id ? details : `${props.app.id} - ${details}`
  }

  return (
    <Pressable
      focusable
      onPress={props.onPress}
      style={(s: PressState) => focusRing(s.focused, theme.radius.lg)}
    >
      {(s: PressState) => (
        <Card
          layout={{ flexDirection: "row", alignItems: "center", gap: space("lg") }}
          style={{
            backgroundColor: props.active
              ? theme.color.surfaceAlt
              : s.hovered
                ? theme.color.surfaceAlt
                : theme.color.surface,
          }}
        >
          <AppIcon app={props.app} size={40} />
          <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
            <Text variant="title">{props.app.name}</Text>
            <Text variant="body" muted>
              {subtitle()}
            </Text>
          </View>
          <Pressable
            onPress={props.onLaunch}
            layout={{
              width: TAP_TARGET,
              height: TAP_TARGET,
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            {(ps: PressState) => (
              <Icon
                src={PLAY_SVG}
                size={20}
                color={ps.pressed || ps.hovered ? theme.color.text : theme.color.primary}
              />
            )}
          </Pressable>
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

// The selected app's detail view: identity, storage usage, stored versions and
// the data sandbox's files, with launch and remove. Its back arrow clears the
// selection in both layouts - single-pane returns to the list, two-pane empties
// the detail pane back to the placeholder. Single-pane centers the max-width
// column the way the list and the settings panel do, so crossing the breakpoint
// does not shift the content sideways; two-pane leaves it against the split's
// hairline.
function AppDetail(props: {
  app: InstalledApp
  onLaunch: () => void
  onRemove: () => void
  onBack: () => void
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
  // The confirm dialog is the top of the back stack while it is open, so it
  // takes the next back press for itself; otherwise the event travels on to the
  // detail's own step (the list) and eventually to App's.
  onBack((e) => {
    if (confirming()) {
      e.preventDefault()
      setConfirming(false)
    }
  })
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
        layout={{ flexGrow: 1, alignItems: policy.layout === "twoPane" ? "flex-start" : "center" }}
      >
        <View
          layout={{
            flexDirection: "column",
            gap: space("lg"),
            padding: space("xl"),
            width: "100%",
            maxWidth: DETAIL_MAX_WIDTH,
          }}
        >
          <View layout={{ flexDirection: "row", alignItems: "center", gap: space("lg") }}>
            <BackButton onPress={props.onBack} />
            <AppIcon app={props.app} size={56} />
            <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
              <Text variant="heading">{props.app.name}</Text>
              <Text variant="body" muted>
                {props.app.id}
              </Text>
            </View>
          </View>
          <View layout={{ flexDirection: "row", gap: space("md") }}>
            <Button layout={{ flexGrow: 1 }} onPress={() => props.onLaunch()}>
              Launch
            </Button>
            <Button layout={{ flexGrow: 1 }} variant="secondary" onPress={() => setConfirming(true)}>
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
                    <Button layout={{ flexGrow: 1 }} variant="ghost" onPress={() => setConfirming(false)}>
                      Cancel
                    </Button>
                    <Button layout={{ flexGrow: 1 }} variant="danger" onPress={() => props.onRemove()}>
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
                        value={`${v.solidrtVersion}, ${formatSize(v.size)}`}
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
      </View>
    </ScrollView>
  )
}

function doLaunch(id: string) {
  try {
    launch(id)
  } catch (e) {
    setNotice(e instanceof Error ? e.message : String(e))
  }
}

function doRemove(id: string) {
  try {
    remove(id)
  } catch (e) {
    setNotice(e instanceof Error ? e.message : String(e))
  }
  refreshApps()
}

// What the detail pane shows for an id the store does not have: removed,
// replaced by a dev push, or a link naming an app that was never installed.
function MissingApp(props: { id: string; onBack: () => void }) {
  return (
    <View
      layout={{ flexGrow: 1, alignItems: policy.layout === "twoPane" ? "flex-start" : "center" }}
    >
      <View
        layout={{
          flexDirection: "column",
          gap: space("lg"),
          padding: space("xl"),
          width: "100%",
          maxWidth: DETAIL_MAX_WIDTH,
        }}
      >
        <View layout={{ flexDirection: "row", alignItems: "center", gap: space("lg") }}>
          <BackButton onPress={props.onBack} />
          <Text variant="heading">Not installed</Text>
        </View>
        <Text variant="body" muted>
          {props.id}
        </Text>
      </View>
    </View>
  )
}

// The /app/$id route: the app's detail in the home split view's detail pane.
// The id is the route's param (validated in routes.ts); whether the store has
// it is checked here. Back pops to whatever opened it: the list, or a link's
// empty stack, where the platform default takes over.
export function AppDetailRoute() {
  let router = useRouter()
  let params = useParams(routes.app)
  let app = createMemo(() => installedApps().find((a) => a.id === params().id) ?? null)
  return (
    <Show when={app()} fallback={<MissingApp id={params().id} onBack={() => router.back()} />}>
      {(a) => (
        <AppDetail
          app={a()}
          onLaunch={() => doLaunch(a().id)}
          onRemove={() => {
            doRemove(a().id)
            void router.back()
          }}
          onBack={() => router.back()}
        />
      )}
    </Show>
  )
}

// The installed-app list: one AppCard per app, scrolling. Selection state is
// owned by App and threaded in, so crossing the layout breakpoint keeps it.
function AppList(props: {
  apps: InstalledApp[]
  selectedId: string | null
  twoPane: boolean
  onSelect: (id: string) => void
  onLaunch: (id: string) => void
}) {
  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View layout={{ flexDirection: "column", gap: space("md"), padding: LIST_GUTTER }}>
        <For each={props.apps}>
          {(app) => (
            <AppCard
              app={app}
              active={props.twoPane && props.selectedId === app.id}
              onPress={() => props.onSelect(app.id)}
              onLaunch={() => props.onLaunch(app.id)}
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
// and the one action the current state affords. Idle offers Connect, which opens
// the connect panel - the ways to connect (address, discovery, QR) live there
// rather than on this card, which sits under the app list and should read as a
// status strip, not a toolbar. Connection state is owned by App; opening the
// panel comes back as a callback.
function DevCard(props: {
  status: string
  idle: boolean
  busy: boolean
  connected: boolean
  onConnect: () => void
}) {
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
          <Button layout={{ flexGrow: 1 }} variant="secondary" onPress={props.onConnect}>
            Connect
          </Button>
        </Show>
        <Show when={props.busy}>
          <Button layout={{ flexGrow: 1 }} variant="secondary" onPress={() => stop()}>
            Cancel
          </Button>
        </Show>
        <Show when={props.connected}>
          <Button layout={{ flexGrow: 1 }} variant="secondary" onPress={() => stop()}>
            Disconnect
          </Button>
        </Show>
      </View>
    </Card>
  )
}

// List-detail home, the / route's layout: SplitView shows the app list beside
// the selected app's details when the layout policy is two-pane, and
// navigates between the list and a detail screen when single-pane. The list
// chrome (mark size, centering) forks on the layout, per the SplitView
// contract. The child route on top (settings, connect, an app's detail; see
// routes.ts) takes one pane through <Outlet> so the other keeps its content:
// settings and an app's detail take the detail pane over the placeholder,
// connect takes the list pane. Single-pane has one pane to give, so a child
// reads as a screen there - which is why connect forces the list pane
// forward. Owns the launch/remove notice through ./app-state; the dev-server
// connection is app-wide module state, read directly from ./dev-connection,
// and drives the status line.
export function HomeScreen() {
  let router = useRouter()
  let location = useLocation()

  let twoPane = () => policy.layout === "twoPane"
  // The child route on top, or null at home itself.
  let pane = () => location()?.matches[HOME_CHILD_DEPTH]?.route ?? null
  let detailUp = () => pane() === routes.settings || pane() === routes.app
  // The app whose detail is up, for the list's highlight; a stale id (removed
  // app, replaced store) still highlights nothing, since the list lacks it.
  let selectedId = createMemo(() => {
    let m = location()?.matches[HOME_CHILD_DEPTH]
    return m && m.route === routes.app ? (m.params as { id: string }).id : null
  })

  let status = () =>
    isConnected()
      ? `Connected to ${serverAddress()}${isTunneled() ? " (tunneled)" : ""}`
      : (notice() ?? STATUS_TEXT[connectionState()])

  return (
    <SplitView
      layout={{ flexGrow: 1 }}
      listWidth={380}
      // Single-pane shows whichever pane this picks, and a child the user
      // just opened has to be the one on screen: settings and an app's
      // detail pull the detail forward, connect pulls the list forward.
      showDetail={detailUp()}
      list={
        <Show when={pane() !== routes.connect} fallback={<Outlet />}>
          <View layout={{ flexGrow: 1, flexDirection: "column", alignItems: "center" }}>
            <View
              layout={{
                flexDirection: "column",
                flexGrow: 1,
                width: "100%",
                maxWidth: twoPane() ? undefined : COLUMN_MAX_WIDTH,
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
                  <Logo size={40} />
                  <Text variant="heading">SolidRT</Text>
                </View>
                <View layout={{ flexDirection: "row", alignItems: "center" }}>
                  <Pressable
                    focusable
                    onPress={() => router.navigate(routes.settings)}
                    layout={{
                      width: TAP_TARGET,
                      height: TAP_TARGET,
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                    style={(s: PressState) => ({
                      backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
                      borderRadius: theme.radius.md,
                      ...focusRing(s.focused),
                    })}
                  >
                    <Icon src={GEAR_SVG} size={22} />
                  </Pressable>
                  {/* The scan icon is a connect shortcut, so it hides while
                      connected; the dev card's Disconnect is the affordance
                      then. No camera pre-check: enumerating cameras starts the
                      platform's capture backend, and doing that on every
                      player start is what a wedged backend turns into a dead
                      client (Raspberry Pi; see sdl_utils::camera_subsystem_init).
                      The subsystem starts when the scan screen opens a camera,
                      and a machine without one gets the scan screen's error
                      notice. */}
                  <Show when={available && !isConnected()}>
                    <ScanButton onPress={() => router.navigate(routes.scan)} />
                  </Show>
                </View>
              </View>
              <Show when={installedApps().length > 0} fallback={<NoApps />}>
                <AppList
                  apps={installedApps()}
                  selectedId={selectedId()}
                  twoPane={twoPane()}
                  onSelect={(id) => {
                    // Two-pane keeps the list interactive while settings or
                    // another app's detail holds the detail pane; picking an
                    // app replaces that entry rather than stacking on it, so
                    // back still returns to the plain list.
                    void router.navigate({ route: routes.app, params: { id } }, { replace: pane() != null })
                  }}
                  onLaunch={(id) => doLaunch(id)}
                />
              </Show>
              <Show when={available}>
                <DevCard
                  status={status()}
                  idle={isIdle()}
                  busy={isBusy()}
                  connected={isConnected()}
                  onConnect={() => router.navigate(routes.connect)}
                />
              </Show>
            </View>
          </View>
        </Show>
      }
      detail={
        <Show
          when={detailUp()}
          fallback={
            <View
              layout={{
                flexGrow: 1,
                justifyContent: "center",
                alignItems: "center",
                gap: space("lg"),
              }}
            >
              <Logo size={360} />
            </View>
          }
        >
          <Outlet />
        </Show>
      }
    />
  )
}
