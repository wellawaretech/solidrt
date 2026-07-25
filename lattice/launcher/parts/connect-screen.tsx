// The manual connect screen: a host + port entry form plus recent addresses.
// Scanning a QR feeds the same dial path; this is the type-it-in alternative.
import { Show, For } from "solid-js"
import { View, Card, Text, Button, TextInput, space } from "@solidrt/components"
import { recentAddresses } from "./dev-connection"

// The dev server's default port (the CLI's DEV_PORT, 0x8844), pre-filled so the
// common case is host-only entry.
const DEFAULT_PORT = "34884"

// A recent entry is either a `host:port` address or a p2p ticket (which
// contains `|`). Tickets are long, so show a short "ticket <id-prefix>" label
// while still dialing the full string.
function recentLabel(entry: string): string {
  if (!entry.includes("|")) return entry
  return "ticket " + entry.split("|")[0]!.slice(0, 8)
}

export function ConnectScreen(props: {
  onDial: (addr: string) => void
  onCancel: () => void
}) {
  let hostDraft = ""
  let portDraft = DEFAULT_PORT

  // Dial host:port; a blank port falls back to whatever the host holds (so a
  // pasted host:port still works). Host is required.
  let submit = () => {
    let host = hostDraft.trim()
    if (!host) return
    let port = portDraft.trim()
    props.onDial(port ? `${host}:${port}` : host)
  }

  return (
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
          <View layout={{ flexDirection: "row", gap: space("md") }}>
            <TextInput
              layout={{ flexGrow: 1 }}
              placeholder="IP address"
              onInput={(v) => (hostDraft = v)}
              onSubmit={submit}
            />
            <TextInput
              layout={{ width: 96 }}
              placeholder="port"
              defaultValue={DEFAULT_PORT}
              onInput={(v) => (portDraft = v)}
              onSubmit={submit}
            />
          </View>
          <View layout={{ flexDirection: "row", gap: space("md") }}>
            <Button onPress={submit}>Connect</Button>
            <Button variant="ghost" onPress={props.onCancel}>
              Cancel
            </Button>
          </View>
        </Card>
        <Show when={recentAddresses().length > 0}>
          <View layout={{ flexDirection: "column", gap: space("sm") }}>
            <Text variant="body" muted>
              Recent
            </Text>
            <View layout={{ flexDirection: "column", gap: space("sm") }}>
              <For each={recentAddresses()}>
                {(entry) => (
                  <Button variant="secondary" onPress={() => props.onDial(entry)}>
                    {recentLabel(entry)}
                  </Button>
                )}
              </For>
            </View>
          </View>
        </Show>
      </View>
    </View>
  )
}
