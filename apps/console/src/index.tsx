// Dev console: the dev servers running on this machine, read from the
// registry. A NavShell holds the pages; the servers page is a SplitView whose
// list pane is the servers and whose detail pane is the selected server's
// clients. Built on @solidrt/components, so colors, type scale and spacing
// come from the shared theme (see theme.ts) and the dense policy - no styling
// of its own; the functionality is the point.
import { render, createSignal, onSettled, For, Show, Logo } from "@solidrt/core"
import {
  Badge,
  Button,
  Divider,
  Icon,
  Item,
  NavShell,
  Pressable,
  SafeArea,
  ScrollView,
  SplitView,
  Text,
  View,
  Window,
  defaultPolicyResolver,
  policy,
  setPolicy,
  setPolicyResolver,
  space,
  theme,
} from "@solidrt/components"
import { consoleTheme } from "./theme"
import {
  canSpawnClient,
  clientFacts,
  clientLabel,
  entryLabel,
  listServers,
  serverLabel,
  serversDir,
  spawnClient,
  uptime,
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
const COLLAPSE_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m16 15-3-3 3-3"/>`,
)
const EXPAND_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m14 9 3 3-3 3"/>`,
)

// The list pane while collapsed: wide enough for the expand button and
// nothing else.
const STRIP_WIDTH = 44

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

function IconButton(props: { icon: string; onPress: () => void }) {
  return (
    <Pressable
      onPress={props.onPress}
      layout={{ padding: space("sm") }}
      style={(state) => ({
        backgroundColor: state.hovered ? theme.color.overlayHover : "transparent",
        borderRadius: theme.radius.sm,
      })}
    >
      <Icon src={props.icon} size={18} color={theme.color.textMuted} />
    </Pressable>
  )
}

// The count a server's row carries: a number once the port answered, nothing
// while it has not.
function clientCount(server: Server): number | null {
  return server.clients ? server.clients.length : null
}

function ClientCard(props: { client: Client }) {
  return (
    <View layout={{ flexDirection: "column", gap: space("sm") }}>
      <Text>{clientLabel(props.client)}</Text>
      <For each={clientFacts(props.client)}>{(line) => <Text muted>{line}</Text>}</For>
    </View>
  )
}

function ServerList(props: {
  servers: Server[]
  selected: number | undefined
  failure: string | null
  collapsed: boolean
  onOpen: (port: number) => void
  onCollapse: () => void
  onExpand: () => void
}) {
  return (
    <View layout={{ flexDirection: "row", flexGrow: 1 }}>
      <Show
        when={!props.collapsed}
        fallback={
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
        }
      >
        <View layout={{ flexDirection: "column", flexGrow: 1, flexBasis: 0 }}>
          <View
            layout={{
              flexDirection: "row",
              alignItems: "center",
              gap: space("md"),
              padding: space("lg"),
            }}
          >
            <Logo size={22} />
            <Text variant="heading">Console</Text>
            <View layout={{ flexGrow: 1 }} />
            <Show when={policy.layout === "twoPane"}>
              <IconButton icon={COLLAPSE_ICON} onPress={props.onCollapse} />
            </Show>
          </View>
          <Show when={props.failure}>
            {(message) => (
              <View layout={{ paddingLeft: space("lg"), paddingRight: space("lg") }}>
                <Text color="danger">{message()}</Text>
              </View>
            )}
          </Show>
          <ScrollView layout={{ flexGrow: 1, flexBasis: 0 }}>
            <View layout={{ flexDirection: "column", padding: space("sm") }}>
              <For
                each={props.servers}
                keyed={(server: Server) => server.port}
                fallback={
                  <View layout={{ padding: space("lg") }}>
                    <Text muted>No dev servers running.</Text>
                  </View>
                }
              >
                {(server) => (
                  <Item
                    label={serverLabel(server())}
                    description={entryLabel(server())}
                    selected={server().port === props.selected}
                    onPress={() => props.onOpen(server().port)}
                    endContent={
                      <Show
                        when={clientCount(server())}
                        fallback={
                          <Show when={server().clients} fallback={<Text color="danger">?</Text>}>
                            <Text muted>0</Text>
                          </Show>
                        }
                      >
                        {(count) => <Badge>{count()}</Badge>}
                      </Show>
                    }
                  />
                )}
              </For>
            </View>
          </ScrollView>
        </View>
      </Show>
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
              <Button variant="ghost" size="sm" onPress={props.onBack}>
                Back
              </Button>
            </Show>
            <Text variant="heading">{serverLabel(server())}</Text>
          </View>
          <ScrollView layout={{ flexGrow: 1, flexBasis: 0 }}>
            <View layout={{ flexDirection: "column", gap: space("lg"), padding: space("lg") }}>
              <View layout={{ flexDirection: "column", gap: space("sm") }}>
                <Text>{`${server().mode} ${server().key}`}</Text>
                <Text muted>{server().entry}</Text>
                <Text muted>
                  {`pid ${server().pid} on ${server().address}:${server().port}, up ${uptime(server())}`}
                </Text>
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
  let [selected, setSelected] = createSignal<number | undefined>(undefined)
  let [showDetail, setShowDetail] = createSignal(false)
  // Collapsing only means anything while both panes are up: a single-pane
  // window already shows one pane at a time, so a narrowed window restores
  // the full list rather than leaving a strip with nothing beside it.
  let [collapsed, setCollapsed] = createSignal(false)
  let listCollapsed = () => collapsed() && policy.layout === "twoPane"
  let open = (port: number) => {
    setSelected(port)
    setShowDetail(true)
  }
  let server = () => servers().find((s) => s.port === selected())

  let stopped = false
  let refresh = async () => {
    try {
      let next = await listServers()
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
      setFailure("The runtime reports no home directory, so the server registry cannot be found.")
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
              listWidth={listCollapsed() ? STRIP_WIDTH : undefined}
              list={
                <ServerList
                  servers={servers()}
                  selected={selected()}
                  failure={failure()}
                  collapsed={listCollapsed()}
                  onOpen={open}
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
