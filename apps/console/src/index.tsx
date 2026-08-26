// Dev console, stage 1: the dev servers running on this machine, read from
// the registry. Built on @solidrt/components, so colors, type scale and
// spacing come from the shared theme (see theme.ts) and the dense policy -
// no styling of its own; the functionality is the point.
import { render, createSignal, onSettled, For, Show } from "@solidrt/core"
import {
  Button,
  Card,
  Divider,
  ScrollView,
  Text,
  View,
  Window,
  setPolicy,
  space,
  theme,
} from "@solidrt/components"
import { consoleTheme } from "./theme"
import {
  canSpawnClient,
  clientFacts,
  clientLabel,
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

// setTheme(consoleTheme)
// Rows over air: the dashboard is a list of facts, not a touch surface.
setPolicy({ density: "dense" })

// setPolicy({ density: "dense", textWeightDelta: 0 })

function ServerRow(props: { server: Server }) {
  // What the last "Start client" press came to, until the next press.
  let [note, setNote] = createSignal<string | null>(null)
  let start = async () => {
    try {
      let started = await spawnClient(props.server)
      setNote(`Started client ${started.client} (pid ${started.pid ?? "unknown"})`)
    } catch (e) {
      setNote(String(e))
    }
  }
  return (
    <Card title={serverLabel(props.server)} layout={{ gap: space("sm") }}>
      <Text>{`${props.server.mode} ${props.server.key}`}</Text>
      <Text>{props.server.entry}</Text>
      <Text>
        {`pid ${props.server.pid} on ${props.server.address}:${props.server.port}, up ${uptime(props.server)}`}
      </Text>
      <Show
        when={props.server.clients}
        fallback={<Text color="danger">Not answering on its port</Text>}
      >
        {(clients) => (
          <For
            each={clients()}
            keyed={(client: Client) => client.id}
            fallback={<Text muted>No clients connected</Text>}
          >
            {(client) => (
              <View layout={{ flexDirection: "column", gap: space("sm") }}>
                <Divider />
                <Text>{clientLabel(client())}</Text>
                <For each={clientFacts(client())}>{(line) => <Text>{line}</Text>}</For>
              </View>
            )}
          </For>
        )}
      </Show>
      <Show when={canSpawnClient()}>
        <View layout={{ flexDirection: "row", gap: space("md"), alignItems: "center" }}>
          <Button size="sm" onPress={start}>
            Start client
          </Button>
          <Show when={note()}>{(text) => <Text>{text()}</Text>}</Show>
        </View>
      </Show>
    </Card>
  )
}

function App() {
  let [servers, setServers] = createSignal<Server[]>([])
  let [failure, setFailure] = createSignal<string | null>(null)

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
      <View
        layout={{ flexDirection: "column", flexGrow: 1, padding: space("xl"), gap: space("lg") }}
      >
        <Text variant="heading">Dev servers</Text>
        <Show when={failure()}>{(message) => <Text color="danger">{message()}</Text>}</Show>
        <ScrollView layout={{ flexGrow: 1, flexBasis: 0 }}>
          <View layout={{ flexDirection: "column", gap: space("lg") }}>
            <For
              each={servers()}
              keyed={(server: Server) => server.port}
              fallback={<Text muted>No dev servers running.</Text>}
            >
              {(server) => <ServerRow server={server()} />}
            </For>
          </View>
        </ScrollView>
      </View>
    </Window>
  )
}

render(() => <App />)
