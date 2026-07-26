// The list-detail home screen: a header (brand mark + settings gear), the
// installed-app list, the selected app's detail view, and the dev-server
// control surface. Wide windows show a WhatsApp-style split (list left,
// details right); narrow ones navigate between the list and a detail screen.
// All connection/selection state is owned by App and threaded in as props.
import { createSignal, createEffect, onBack } from "@solidrt/core"
import { For, Show, createMemo } from "solid-js"
import {
  View,
  Card,
  Text,
  Button,
  ScrollView,
  Pressable,
  type PressState,
  Spinner,
  SplitView,
  Modal,
  Icon,
  theme,
  space,
  policy,
} from "@solidrt/components"
import { canDiscover, discover, stop } from "srt:dev"
import {
  available as appsAvailable,
  list,
  launch,
  remove,
  info,
  clearCache,
  type AppCacheEntry,
  type InstalledApp,
} from "srt:apps"
import { cameraDevices } from "@solidrt/core/camera"
import { PuzzleMark } from "./puzzle"
import { DetailCard, DetailRow } from "./detail-card"
import { BackButton } from "./back-button"
import { COLUMN_MAX_WIDTH, DETAIL_MAX_WIDTH, STATUS_TEXT, TAP_TARGET } from "./types"
import {
  available,
  connectionState,
  serverAddress,
  isTunneled,
  isConnected,
  isBusy,
  isIdle,
} from "./dev-connection"

// Lucide settings (gear) glyph for the header button that opens the settings
// screen, stroked with currentColor so the Icon component recolors it.
const GEAR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z"/><circle cx="12" cy="12" r="3"/></svg>`

// Lucide play glyph for the app rows' quick-launch button, filled (not the
// stock outline) so it still reads as a launch affordance at 20px.
const PLAY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 3 20 12 6 21 6 3"/></svg>`

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
  return (
    <Pressable onPress={props.onPress}>
      {(s: PressState) => (
        <Card
          layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}
          style={{
            backgroundColor: props.active
              ? theme.color.surfaceAlt
              : s.hovered
                ? theme.color.surfaceHover
                : theme.color.surface,
          }}
        >
          <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
            <Text variant="title">{props.app.name}</Text>
            <Text variant="body" muted>{`${props.app.id} - ${props.app.version.slice(0, 8)}`}</Text>
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
                color={ps.pressed || ps.hovered ? theme.color.primaryHover : theme.color.primary}
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
// the data sandbox's files, with launch and remove. Shared between the split
// view's right pane (no onBack) and the narrow layout's detail screen (with
// onBack). Single-pane centers the max-width column the way the list and the
// settings screen do, so crossing the breakpoint does not shift the content
// sideways; two-pane leaves it against the split's hairline.
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
          <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
            <Show when={props.onBack}>
              <BackButton onPress={() => props.onBack?.()} />
            </Show>
            <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
              <Text variant="heading">{props.app.name}</Text>
              <Text variant="body" muted>
                {props.app.id}
              </Text>
            </View>
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
      <View layout={{ flexDirection: "column", gap: space("md") }}>
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

// List-detail home: SplitView shows the app list beside the selected app's
// details when the layout policy is two-pane, and navigates between the list
// and a detail screen when single-pane. The list chrome (mark size, centering)
// and the detail's Back affordance fork on the layout, per the SplitView
// contract. Owns the app list and the launch/remove notice; the selection and
// notice are lifted to App (passed as values) so they survive visits to the
// other screens. The dev-server connection is app-wide module state, read
// directly from ./dev-connection, and drives the status line.
export function HomeScreen(props: {
  selectedId: string | null
  setSelectedId: (id: string | null) => void
  notice: string | null
  setNotice: (message: string | null) => void
  onScan: () => void
  onManual: () => void
  onSettings: () => void
}) {
  let [apps, setApps] = createSignal(appsAvailable ? list() : [])

  let twoPane = () => policy.layout === "twoPane"
  // A stale selection (removed app, replaced store) resolves to null, which
  // reads as "nothing selected" in both layouts.
  let selectedApp = () => apps().find((a) => a.id === props.selectedId) ?? null

  let status = () =>
    isConnected()
      ? `Connected to ${serverAddress()}${isTunneled() ? " (tunneled)" : ""}`
      : (props.notice ?? STATUS_TEXT[connectionState()])

  let doLaunch = (id: string) => {
    try {
      launch(id)
    } catch (e) {
      props.setNotice(e instanceof Error ? e.message : String(e))
    }
  }
  let doRemove = (id: string) => {
    try {
      remove(id)
    } catch (e) {
      props.setNotice(e instanceof Error ? e.message : String(e))
    }
    props.setSelectedId(null)
    setApps(appsAvailable ? list() : [])
  }

  // Back clears a narrow-layout detail selection before the launcher root's
  // default action (exit) runs. This handler is only registered while the home
  // screen is mounted, so it never fires from a sub-screen; App owns the
  // sub-screen -> home pop.
  onBack((e) => {
    if (!twoPane() && selectedApp() != null) {
      e.preventDefault()
      props.setSelectedId(null)
    }
  })

  return (
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
                <PuzzleMark size={40} />
                <Text variant="heading">SolidRT</Text>
              </View>
              <Pressable
                onPress={props.onSettings}
                layout={{
                  width: TAP_TARGET,
                  height: TAP_TARGET,
                  alignItems: "center",
                  justifyContent: "center",
                }}
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
                selectedId={props.selectedId}
                twoPane={twoPane()}
                onSelect={(id) => props.setSelectedId(id)}
                onLaunch={(id) => doLaunch(id)}
              />
            </Show>
            <Show when={available}>
              <DevCard
                status={status()}
                idle={isIdle()}
                busy={isBusy()}
                connected={isConnected()}
                onScan={props.onScan}
                onManual={props.onManual}
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
              onBack={twoPane() ? undefined : () => props.setSelectedId(null)}
            />
          )}
        </Show>
      }
    />
  )
}
