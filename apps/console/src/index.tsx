// Dev console: the dev servers this console can reach - the ones on this
// machine, read from the registry, plus any remote address typed into
// Connect. The app is one SplitView, shaped like a chat app: the list pane
// is the servers with their clients as the contacts, the detail pane the open
// chats as columns, as many as its width fits - a client, or a server as the
// group its clients are in (chat.tsx). A client connecting opens its chat by
// itself; a client is its machine and slot (servers.ts clientKey), so a
// restart continues the chat. Every chat keeps its own transcript (conversation.ts) with
// commands where a chat app puts its composer, and every press and its
// result as a block (blocks.tsx, commands.ts). No primary navigation for now: a
// NavShell held a second page once and comes back when there is one to hold.
// Built on @solidrt/components, so colors, type scale and spacing come from
// the shared theme (see theme.ts) and the dense policy - no styling of its
// own; the functionality is the point.
import { render, createSignal, onSettled, For, Show, Logo } from "@solidrt/core"
import {
  Button,
  Card,
  Field,
  Icon,
  Pressable,
  SafeArea,
  ScrollView,
  SplitView,
  Text,
  TextInput,
  View,
  Window,
  policy,
  setPolicy,
  space,
  theme,
  type PressState,
} from "@solidrt/components"
import { consoleTheme } from "./theme"
import {
  clientKey,
  entryLabel,
  listServers,
  parseAddress,
  probeRemote,
  serverId,
  serverLabel,
  serversDir,
  listSlots,
  type Client,
  type Server,
  type Slot,
} from "./servers"
import { ChatPane, openChat } from "./chat"
import { partyKey, type Conversation, type Party } from "./conversation"
import { COLLAPSE_ICON, EXPAND_ICON, SERVER_ICON, ClientChoice, IconButton, focusRing } from "./ui"

// Slow enough to stay out of the way, fast enough that a server coming up
// shows while you are still looking.
const POLL_MS = 2000

// The list pane while collapsed: wide enough for the expand button and
// nothing else.
const STRIP_WIDTH = 44

// The list pane's width in two-pane, matching the player's.
const LIST_WIDTH = 380

// setTheme(consoleTheme)
// Rows over air: the dashboard is a list of facts, not a touch surface.
setPolicy({ density: "dense" })

// Breathing room between the scrolling rows and the viewport's clip edge: a
// focus ring is drawn on the row's own box edge, so a row that fills the
// viewport exactly would leave the ring flush against the clip.
const LIST_GUTTER = 2

// Reading width of the list column when it is the whole screen. Two-pane
// already bounds the column with the pane, so it only applies single-pane.
const COLUMN_MAX_WIDTH = 440

// One server as a row: what it serves and where, and under it the clients
// attached to it - the typical setup is one server, so its clients are what
// the list is really for. Pressing a client opens its chat, pressing the
// server the group chat; `active` marks the server row while the group chat
// is open, `openClients` the clients whose chats are (stable keys, so a
// reconnected client keeps its mark). A port that never answered shows a
// danger mark instead of clients, so a wedged server reads from the list
// rather than only after opening it.
function ServerCard(props: {
  server: Server
  active: boolean
  openClients: string[]
  onPress: () => void
  onPick: (client: Client) => void
}) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      style={(state: PressState) => focusRing(state.focused, theme.radius.lg)}
    >
      {(state: PressState) => (
        <Card
          layout={{ flexDirection: "row", alignItems: "flex-start", gap: space("lg") }}
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
            <Show when={props.server.clients}>
              {(clients) => (
                <For
                  each={clients()}
                  keyed={(client: Client) => client.id}
                  fallback={
                    <Text variant="caption" muted>
                      No clients
                    </Text>
                  }
                >
                  {(client) => (
                    <ClientChoice
                      client={client()}
                      active={props.openClients.includes(clientKey(client()))}
                      onPress={() => props.onPick(client())}
                    />
                  )}
                </For>
              )}
            </Show>
          </View>
          <Show when={props.server.remote}>
            <Text variant="caption" muted>
              remote
            </Text>
          </Show>
          <Show when={!props.server.clients}>
            <Text color="danger">?</Text>
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
  openParties: Party[]
  failure: string | null
  collapsed: boolean
  onOpen: (server: Server) => void
  onPick: (server: Server, client: Client) => void
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
                      active={props.openParties.some(
                        (p) => p.server === serverId(server()) && p.client === null,
                      )}
                      openClients={props.openParties.flatMap((p) =>
                        p.server === serverId(server()) && p.client !== null ? [p.client] : [],
                      )}
                      onPress={() => props.onOpen(server())}
                      onPick={(client) => props.onPick(server(), client)}
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

function App() {
  let [servers, setServers] = createSignal<Server[]>([])
  // The client slots on this machine, polled with the servers: a slot fills
  // or frees without leaving a signal behind, same as a server.
  let [slots, setSlots] = createSignal<Slot[]>([])
  let [failure, setFailure] = createSignal<string | null>(null)
  // The open chats' parties, by ids rather than records: a poll replaces
  // every record, and the ids are what survive it. They also survive a
  // breakpoint crossing, which remounts both panes. Opening order, which is
  // the columns' left-to-right order; `recent` holds the same chats' keys by
  // focus, least recent first, deciding who gets a column when fewer fit
  // than are open.
  let [openParties, setOpenParties] = createSignal<Party[]>([])
  let [recent, setRecent] = createSignal<string[]>([])
  // Every chat opened this run, by party: a chat keeps its history while you
  // talk to another, and across a close - only its column goes. Created on
  // first open with its opening facts. A plain map: only the selection
  // changes reactively, and each chat's transcript is reactive on its own.
  let conversations = new Map<string, Conversation>()
  // Remote addresses typed into Connect, for this run of the console only:
  // the registry holds the local servers, and nothing holds these.
  let [remotes, setRemotes] = createSignal<string[]>([])
  let [showDetail, setShowDetail] = createSignal(false)
  // Collapsing only means anything while both panes are up: a single-pane
  // window already shows one pane at a time, so a narrowed window restores
  // the full list rather than leaving a strip with nothing beside it.
  let [collapsed, setCollapsed] = createSignal(false)
  let listCollapsed = () => collapsed() && policy.layout === "twoPane"
  // Open the chat with a client, or with the server itself (the group) when
  // no client is given. Takes the records rather than ids: a write to
  // `servers` in the same handler has not flushed for a read yet. An already
  // open chat is refocused, a closed one gets its history back from the map.
  // `quiet` is the poll opening a chat for a client that just connected: the
  // chat takes a column, but a phone is not yanked off the list for it.
  let open = (server: Server, client: Client | null = null, quiet = false) => {
    let next: Party = { server: serverId(server), client: client && clientKey(client) }
    let key = partyKey(next)
    if (!conversations.has(key)) conversations.set(key, openChat(server, client ?? undefined))
    if (!openParties().some((p) => partyKey(p) === key)) setOpenParties([...openParties(), next])
    setRecent([...recent().filter((k) => k !== key), key])
    if (!quiet) setShowDetail(true)
  }
  // Closing gives the column up; the conversation stays in the map, so the
  // list can bring the chat back with its history.
  let close = (party: Party) => {
    let key = partyKey(party)
    let remaining = openParties().filter((p) => partyKey(p) !== key)
    setOpenParties(remaining)
    setRecent(recent().filter((k) => k !== key))
    if (remaining.length === 0) setShowDetail(false)
  }
  let serverOf = (party: Party) => servers().find((s) => serverId(s) === party.server)
  // The connected client a chat talks to: the one wearing the party's stable
  // key - the newest connection when a doubled-up slot wears it twice.
  // Undefined in a group chat, and once the client has left.
  let clientOf = (party: Party) => {
    if (party.client === null) return undefined
    let wearing = serverOf(party)?.clients?.filter((c) => clientKey(c) === party.client) ?? []
    return wearing.reduce<Client | undefined>((a, b) => (a && a.id > b.id ? a : b), undefined)
  }
  let conversationOf = (party: Party) => conversations.get(partyKey(party))

  // A connection is a chat: a client key not seen before on its server opens
  // its chat from the poll. The first answer from a server only seeds, or
  // every console start would burst a column per client already sitting
  // there; a reconnect wears a seen key and opens nothing.
  let seen = new Set<string>()
  let seeded = new Set<string>()
  let noticeClients = (next: Server[]) => {
    for (let server of next) {
      if (!server.clients) continue
      let id = serverId(server)
      let fresh = !seeded.has(id)
      seeded.add(id)
      for (let client of server.clients) {
        let key = `${id}/${clientKey(client)}`
        if (seen.has(key)) continue
        seen.add(key)
        if (!fresh) open(server, client, true)
      }
    }
  }

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
      open(known)
      return null
    }
    let probed = await probeRemote(parsed.host, parsed.port)
    if (!probed.clients) return `No dev server answered at ${address}`
    setRemotes([...remotes(), address])
    setServers([...servers(), probed])
    open(probed)
    return null
  }

  let stopped = false
  let refresh = async () => {
    try {
      let [next, nextSlots] = await Promise.all([listServers(remotes()), listSlots()])
      if (stopped) return
      setServers(next)
      setSlots(nextSlots)
      noticeClients(next)
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
        <SplitView
          layout={{ flex: 1 }}
          listWidth={listCollapsed() ? STRIP_WIDTH : LIST_WIDTH}
          list={
            <ServerList
              servers={servers()}
              openParties={openParties()}
              failure={failure()}
              collapsed={listCollapsed()}
              onOpen={open}
              onPick={(server, client) => open(server, client)}
              onConnect={connect}
              onCollapse={() => setCollapsed(true)}
              onExpand={() => setCollapsed(false)}
            />
          }
          detail={
            <ChatPane
              parties={openParties()}
              recent={recent()}
              slots={slots()}
              serverOf={serverOf}
              clientOf={clientOf}
              conversationOf={conversationOf}
              onClose={close}
              onBack={() => setShowDetail(false)}
            />
          }
          showDetail={showDetail()}
        />
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
