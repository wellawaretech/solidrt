// The connect panel: every way to reach a dev server in one place - type an
// address, discover one on the network, scan its QR - plus the recents. Reached
// from the dev card's Connect button and takes the home SplitView's list pane,
// so a selected app's details stay up beside it in two-pane. Its column gets
// the list's treatment (same max width, centered) so opening it does not shift
// the content sideways.
//
// Starting an attempt closes the panel, because what reports on it - the dev
// card's status line - lives in the pane this one covers. That holds for
// dialing and for discovery; the QR route leaves for the camera screen instead
// and comes back here if the user cancels it.
import { Show, For } from "solid-js"
import { View, Card, Text, TextInput, space } from "@solidrt/components"
import { canDiscover, discover } from "srt:dev"
import { cameraDevices } from "@solidrt/core/camera"
import { NavButton } from "./nav"
import { BackButton } from "./back-button"
import { recentAddresses } from "./dev-connection"
import { COLUMN_MAX_WIDTH } from "./types"

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

export function ConnectPanel(props: {
  onDial: (addr: string) => void
  onScan: () => void
  onClose: () => void
}) {
  let hostDraft = ""
  let portDraft = DEFAULT_PORT
  let hasCamera = () => cameraDevices().length > 0

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
          maxWidth: COLUMN_MAX_WIDTH,
          padding: space("xl"),
        }}
      >
        <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
          <BackButton onPress={props.onClose} />
          <Text variant="heading">Connect</Text>
        </View>
        <Show when={canDiscover || hasCamera()}>
          <View layout={{ flexDirection: "row", gap: space("sm") }}>
            <Show when={canDiscover}>
              <NavButton
                variant="secondary"
                onPress={() => {
                  discover()
                  props.onClose()
                }}
              >
                Discover
              </NavButton>
            </Show>
            <Show when={hasCamera()}>
              <NavButton variant="secondary" onPress={props.onScan}>
                Scan QR
              </NavButton>
            </Show>
          </View>
        </Show>
        <Card title="Manual">
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
            <NavButton onPress={submit}>Connect</NavButton>
          </View>
        </Card>
        <Show when={recentAddresses().length > 0}>
          <Card title="Recent connections">
            <View layout={{ flexDirection: "column", gap: space("sm") }}>
              <For each={recentAddresses()}>
                {(entry) => (
                  <NavButton variant="secondary" onPress={() => props.onDial(entry)}>
                    {recentLabel(entry)}
                  </NavButton>
                )}
              </For>
            </View>
          </Card>
        </Show>
      </View>
    </View>
  )
}
