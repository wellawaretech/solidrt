// Dev console: the dev servers this console can reach - the ones on this
// machine, read from the registry, plus any remote address typed into
// Connect. A NavShell holds the pages; the servers page is a SplitView whose
// list pane is the servers and whose detail pane is the selected server's
// clients. Built on @solidrt/components, so colors, type scale and spacing
// come from the shared theme (see theme.ts) and the dense policy - no styling
// of its own; the functionality is the point.
import { render, createSignal, onSettled, For, Show, Logo } from "@solidrt/core"
import {
  Badge,
  Button,
  Card,
  Field,
  Icon,
  Item,
  NavShell,
  Pressable,
  SafeArea,
  ScrollView,
  SplitView,
  Text,
  TextInput,
  View,
  Window,
  defaultPolicyResolver,
  policy,
  setPolicy,
  setPolicyResolver,
  space,
  theme,
  type PressState,
  type StyleProps,
} from "@solidrt/components"
import { consoleTheme } from "./theme"
import {
  canSpawnClient,
  clientFacts,
  clientLabel,
  entryLabel,
  listServers,
  parseAddress,
  probeRemote,
  serverId,
  serverLabel,
  serverWhere,
  serversDir,
  spawnClient,
  type Client,
  type Server,
} from "./servers"

// Slow enough to stay out of the way, fast enough that a server coming up
// shows while you are still looking.
const POLL_MS = 2000

const LUCIDE = (body: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"` +
  ` stroke="currentColor" stroke-width="2" stroke-linecap="round"` +
  ` stroke-linejoin="round">${body}</svg>`

const SERVER_ICON = LUCIDE(
  `<rect x="2" y="3" width="20" height="8" rx="2"/><rect x="2" y="13" width="20" height="8" rx="2"/>` +
    `<path d="M6 7h.01M6 17h.01"/>`,
)
const CLIENT_ICON = LUCIDE(
  `<rect x="2" y="4" width="20" height="13" rx="2"/><path d="M8 21h8M12 17v4"/>`,
)
const BACK_ICON = LUCIDE(`<path d="m12 19-7-7 7-7"/><path d="M19 12h-14"/>`)
const COLLAPSE_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m16 15-3-3 3-3"/>`,
)
const EXPAND_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m14 9 3 3-3 3"/>`,
)

// The list pane while collapsed: wide enough for the expand button and
// nothing else.
const STRIP_WIDTH = 44

// The list pane's width in two-pane, matching the launcher's.
const LIST_WIDTH = 380

// setTheme(consoleTheme)
// Rows over air: the dashboard is a list of facts, not a touch surface.
setPolicy({ density: "dense" })

// A side nav strip is only affordable when there is a side to spare, so the
// navigation follows the pane count instead of its own breakpoint: a narrow
// rail once the window shows both panes, bottom tabs while it shows one. See
// okf/backlog/shell-layout-fixes.md - the resolver is where an app expresses
// this until the default agrees.
setPolicyResolver((caps) => {
  let base = defaultPolicyResolver(caps)
  return { ...base, navigation: base.layout === "twoPane" ? "rail" : "bottomTabs" }
})

const NAV = [
  { value: "servers", label: "Servers", icon: <Icon src={SERVER_ICON} /> },
  { value: "clients", label: "Clients", icon: <Icon src={CLIENT_ICON} /> },
]

// Edge of an icon button's press box: the glyphs are small, so these boxes are
// sized rather than padded. Not density-scaled - a finger is the same size at
// every density.
const TAP_TARGET = 44

// Breathing room between the scrolling rows and the viewport's clip edge: a
// focus ring is drawn on the row's own box edge, so a row that fills the
// viewport exactly would leave the ring flush against the clip.
const LIST_GUTTER = 2

// Reading width of the list column when it is the whole screen. Two-pane
// already bounds the column with the pane, so it only applies single-pane.
const COLUMN_MAX_WIDTH = 440

// The focus-navigation ring for this app's own pressables (Button draws its
// own), spread into a style. Text-colored so it stays visible on any fill.
function focusRing(focused: boolean, radius?: number): StyleProps {
  if (!focused || !policy.focusRing) return {}
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md,
  }
}

function IconButton(props: { icon: string; onPress: () => void }) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      layout={{
        width: TAP_TARGET,
        height: TAP_TARGET,
        alignItems: "center",
        justifyContent: "center",
      }}
      style={(state: PressState) => ({
        backgroundColor: state.hovered ? theme.color.overlayHover : "transparent",
        borderRadius: theme.radius.md,
        ...focusRing(state.focused),
      })}
    >
      <Icon src={props.icon} size={22} />
    </Pressable>
  )
}

// The count a server's row carries: a number once the port answered, nothing
// while it has not.
function clientCount(server: Server): number | null {
  return server.clients ? server.clients.length : null
}

// One server as a row: what it serves and where, with its client count. A port
// that never answered shows a danger mark instead of a count, so a wedged
// server reads from the list rather than only after opening it.
function ServerCard(props: { server: Server; active: boolean; onPress: () => void }) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      style={(state: PressState) => focusRing(state.focused, theme.radius.lg)}
    >
      {(state: PressState) => (
        <Card
          layout={{ flexDirection: "row", alignItems: "center", gap: space("lg") }}
          style={{
            backgroundColor:
              props.active || state.hovered ? theme.color.surfaceAlt : theme.color.surface,
          }}
        >
          <Icon src={SERVER_ICON} size={24} color={theme.color.textMuted} />
          <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
            <Text variant="title">{serverLabel(props.server)}</Text>
            <Text variant="body" muted>
              {entryLabel(props.server)}
            </Text>
          </View>
          <Show when={props.server.remote}>
            <Text variant="caption" muted>
              remote
            </Text>
          </Show>
          <Show
            when={clientCount(props.server)}
            fallback={
              <Show when={props.server.clients} fallback={<Text color="danger">?</Text>}>
                <Text muted>0</Text>
              </Show>
            }
          >
            {(count) => <Badge>{count()}</Badge>}
          </Show>
        </Card>
      )}
    </Pressable>
  )
}

// The first thing in the list pane: a server this machine's registry cannot
// know about, reached by typing its address. Always up rather than hidden
// behind a disclosure - it is one of the two ways a server gets into the
// list, so it reads as part of the list's frame. Held for the session only:
// nothing is remembered across a restart yet.
function ConnectRemote(props: { onConnect: (address: string) => Promise<string | null> }) {
  let [address, setAddress] = createSignal("")
  let [error, setError] = createSignal<string | null>(null)
  // Button treats a pending promise as the action running, so the field stays
  // put and un-pressable until the probe comes back.
  let connect = async () => {
    let failure = await props.onConnect(address())
    if (failure) {
      setError(failure)
      return
    }
    setAddress("")
    setError(null)
  }
  return (
    <Card layout={{ flexDirection: "column", gap: space("md") }}>
      <Field error={error() ?? undefined}>
        <TextInput
          value={address()}
          onInput={(value) => {
            setAddress(value)
            setError(null)
          }}
          onSubmit={connect}
          placeholder="host:port"
          hints={{ capitalize: "none", autocorrect: false }}
        />
      </Field>
      <Button size="sm" onPress={connect}>
        Connect remote server
      </Button>
    </Card>
  )
}

function NoServers() {
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
      <Text variant="title">No dev servers</Text>
      <Text muted>Start one with srt run</Text>
    </View>
  )
}

function ServerList(props: {
  servers: Server[]
  selected: string | undefined
  failure: string | null
  collapsed: boolean
  onOpen: (id: string) => void
  onConnect: (address: string) => Promise<string | null>
  onCollapse: () => void
  onExpand: () => void
}) {
  return (
    <View layout={{ flexDirection: "row", flexGrow: 1 }}>
      <View
        layout={{
          display: props.collapsed ? "none" : "flex",
          flexDirection: "column",
          flexGrow: 1,
          flexBasis: 0,
          alignItems: "center",
        }}
      >
        <View
          layout={{
            flexDirection: "column",
            flexGrow: 1,
            width: "100%",
            maxWidth: policy.layout === "twoPane" ? undefined : COLUMN_MAX_WIDTH,
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
              <Text variant="heading">Console</Text>
            </View>
            <Show when={policy.layout === "twoPane"}>
              <IconButton icon={COLLAPSE_ICON} onPress={props.onCollapse} />
            </Show>
          </View>
          <ConnectRemote onConnect={props.onConnect} />
          <Show when={props.failure}>{(message) => <Text color="danger">{message()}</Text>}</Show>
          <Show when={props.servers.length > 0} fallback={<NoServers />}>
            <ScrollView layout={{ flexGrow: 1 }}>
              <View
                layout={{ flexDirection: "column", gap: space("md"), padding: LIST_GUTTER }}
              >
                <For each={props.servers} keyed={(server: Server) => serverId(server)}>
                  {(server) => (
                    <ServerCard
                      server={server()}
                      active={serverId(server()) === props.selected}
                      onPress={() => props.onOpen(serverId(server()))}
                    />
                  )}
                </For>
              </View>
            </ScrollView>
          </Show>
        </View>
      </View>
      <Show when={props.collapsed}>
        <View
          layout={{
            flexDirection: "column",
            alignItems: "center",
            flexGrow: 1,
            paddingTop: space("lg"),
          }}
        >
          <IconButton icon={EXPAND_ICON} onPress={props.onExpand} />
        </View>
      </Show>
    </View>
  )
}

function ClientCard(props: { client: Client }) {
  return (
    <View layout={{ flexDirection: "column", gap: space("sm") }}>
      <Text>{clientLabel(props.client)}</Text>
      <For each={clientFacts(props.client)}>{(line) => <Text muted>{line}</Text>}</For>
    </View>
  )
}

function ServerDetail(props: { server: Server | undefined; onBack: () => void }) {
  // What the last "Start client" press came to, until the next press.
  let [note, setNote] = createSignal<string | null>(null)
  let start = async () => {
    if (!props.server) return
    try {
      let started = await spawnClient(props.server)
      setNote(`Started client ${started.client} (pid ${started.pid ?? "unknown"})`)
    } catch (e) {
      setNote(String(e))
    }
  }

  return (
    <Show
      when={props.server}
      fallback={
        <View layout={{ flexGrow: 1, alignItems: "center", justifyContent: "center" }}>
          <Text muted>Pick a server.</Text>
        </View>
      }
    >
      {(server) => (
        <View layout={{ flexDirection: "column", flexGrow: 1 }}>
          <View
            layout={{
              flexDirection: "row",
              alignItems: "center",
              gap: space("md"),
              padding: space("lg"),
            }}
          >
            <Show when={policy.layout === "singlePane"}>
              <IconButton icon={BACK_ICON} onPress={props.onBack} />
            </Show>
            <Text variant="heading">{serverLabel(server())}</Text>
          </View>
          <ScrollView layout={{ flexGrow: 1, flexBasis: 0 }}>
            <View layout={{ flexDirection: "column", gap: space("lg"), padding: space("lg") }}>
              <View layout={{ flexDirection: "column", gap: space("sm") }}>
                <Text>{`${server().mode} ${server().key || "unknown"}`}</Text>
                <Text muted>{server().entry || "Not answering"}</Text>
                <Text muted>{serverWhere(server())}</Text>
              </View>
              <Show
                when={server().clients}
                fallback={<Text color="danger">Not answering on its port</Text>}
              >
                {(clients) => (
                  <View layout={{ flexDirection: "column", gap: space("md") }}>
                    <Text variant="title">Clients</Text>
                    <For
                      each={clients()}
                      keyed={(client: Client) => client.id}
                      fallback={<Text muted>No clients connected</Text>}
                    >
                      {(client) => <ClientCard client={client()} />}
                    </For>
                  </View>
                )}
              </Show>
              <Show when={canSpawnClient()}>
                <View layout={{ flexDirection: "row", gap: space("md"), alignItems: "center" }}>
                  <Button size="sm" onPress={start}>
                    Start client
                  </Button>
                  <Show when={note()}>{(text) => <Text muted>{text()}</Text>}</Show>
                </View>
              </Show>
            </View>
          </ScrollView>
        </View>
      )}
    </Show>
  )
}

// Every client on the machine, with the server it is attached to, for the
// times the question is "where is that client" rather than "what is running".
function AllClients(props: { servers: Server[] }) {
  let rows = () =>
    props.servers.flatMap((server) =>
      (server.clients ?? []).map((client) => ({ server, client })),
    )
  return (
    <View layout={{ flexDirection: "column", flexGrow: 1 }}>
      <View layout={{ padding: space("lg") }}>
        <Text variant="heading">Clients</Text>
      </View>
      <ScrollView layout={{ flexGrow: 1, flexBasis: 0 }}>
        <View layout={{ flexDirection: "column", padding: space("sm") }}>
          <For
            each={rows()}
            fallback={
              <View layout={{ padding: space("lg") }}>
                <Text muted>No clients connected to any server.</Text>
              </View>
            }
          >
            {(row) => (
              <Item
                label={clientLabel(row.client)}
                description={`${serverLabel(row.server)} - ${clientFacts(row.client)[0] ?? ""}`}
              />
            )}
          </For>
        </View>
      </ScrollView>
    </View>
  )
}

function App() {
  let [servers, setServers] = createSignal<Server[]>([])
  let [failure, setFailure] = createSignal<string | null>(null)
  let [page, setPage] = createSignal("servers")
  // Selection is the port, not the record: a poll replaces every record, and
  // the port is what survives it. It also survives a breakpoint crossing,
  // which remounts both panes.
  let [selected, setSelected] = createSignal<string | undefined>(undefined)
  // Remote addresses typed into Connect, for this run of the console only:
  // the registry holds the local servers, and nothing holds these.
  let [remotes, setRemotes] = createSignal<string[]>([])
  let [showDetail, setShowDetail] = createSignal(false)
  // Collapsing only means anything while both panes are up: a single-pane
  // window already shows one pane at a time, so a narrowed window restores
  // the full list rather than leaving a strip with nothing beside it.
  let [collapsed, setCollapsed] = createSignal(false)
  let listCollapsed = () => collapsed() && policy.layout === "twoPane"
  let open = (id: string) => {
    setSelected(id)
    setShowDetail(true)
  }
  let server = () => servers().find((s) => serverId(s) === selected())

  // Connect: validate, then ask the address itself. A silent address is a
  // typo or an unreachable machine, so it is reported rather than kept - the
  // list is not the place to keep guesses. Resolves with the message to show,
  // or null when the row is up.
  let connect = async (input: string): Promise<string | null> => {
    let parsed = parseAddress(input)
    if (!parsed) return `Needs host:port (got "${input.trim()}")`
    let address = `${parsed.host}:${parsed.port}`
    let known = servers().find((s) => serverId(s) === address)
    if (known) {
      open(address)
      return null
    }
    let probed = await probeRemote(parsed.host, parsed.port)
    if (!probed.clients) return `No dev server answered at ${address}`
    setRemotes([...remotes(), address])
    setServers([...servers(), probed])
    open(address)
    return null
  }

  let stopped = false
  let refresh = async () => {
    try {
      let next = await listServers(remotes())
      if (stopped) return
      setServers(next)
      setFailure(null)
    } catch (e) {
      if (!stopped) setFailure(String(e))
    }
  }

  // Poll rather than subscribe: the registry is a folder, and a server
  // appearing or dying leaves no signal behind.
  onSettled(() => {
    if (!serversDir())
      setFailure(
        "The runtime reports no home directory, so there is no local registry to read. Connect a remote server by address.",
      )
    refresh()
    let timer = setInterval(refresh, POLL_MS)
    return () => {
      stopped = true
      clearInterval(timer)
    }
  })

  return (
    <Window title="SolidRT console" style={{ backgroundColor: theme.color.background }}>
      <SafeArea>
        <NavShell
          items={NAV}
          value={page()}
          onChange={(value) => setPage(value as string)}
          layout={{ flex: 1 }}
        >
          <Show when={page() === "servers"} fallback={<AllClients servers={servers()} />}>
            <SplitView
              layout={{ flex: 1 }}
              listWidth={listCollapsed() ? STRIP_WIDTH : LIST_WIDTH}
              list={
                <ServerList
                  servers={servers()}
                  selected={selected()}
                  failure={failure()}
                  collapsed={listCollapsed()}
                  onOpen={open}
                  onConnect={connect}
                  onCollapse={() => setCollapsed(true)}
                  onExpand={() => setCollapsed(false)}
                />
              }
              detail={<ServerDetail server={server()} onBack={() => setShowDetail(false)} />}
              showDetail={showDetail()}
            />
          </Show>
        </NavShell>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
