// The detail pane: the open chats as columns, each with one party - a
// client, or a server as the group its clients are in. As many columns fit
// as the pane's width allows (ChatPane), so a wide window or a collapsed
// list shows several side by side and a phone shows one.
// Commands sit where a chat app puts its composer,
// and every press and its result is a block in the transcript, oldest at the
// top; nothing is ever replaced, so a second screenshot is a second block
// rather than a card that changed under you. The transcript itself is the
// Conversation App keeps per party (conversation.ts), so a chat keeps its
// history while you talk to another. The blocks' look lives in blocks.tsx
// and the client commands in commands.ts; this file is the chat itself: who
// it is with, what was said, and what the party answers.
import { createEffect, createMemo, createSignal, For, Show, getBoundingBox, onLayout, pct } from "@solidrt/core"
import {
  Button,
  Divider,
  Image,
  Modal,
  Pressable,
  SegmentedControl,
  Text,
  View,
  policy,
  space,
} from "@solidrt/components"
import { ChatView } from "./chat-view"
import {
  clientFacts,
  clientLabel,
  entryLabel,
  serverLabel,
  serverWhere,
  spawnClient,
  type Client,
  type Server,
  type Slot,
} from "./servers"
import { callDebug, readTexture, setClock, snapshotClient, snapshotNode, stepFrames, type Shot } from "./control"
import { renderBlock, type Block, type BlockContext } from "./blocks"
import {
  COMMANDS,
  SERVER_COMMANDS,
  answers,
  commandLabel,
  serverCommandLabel,
  type Command,
  type ServerCommand,
} from "./commands"
import { createConversation, partyKey, type Conversation, type Party } from "./conversation"
import { BACK_ICON, CHECK_ICON, CROSS_ICON, IconButton } from "./ui"

// How much of the window a blown-up screenshot takes: short of the edges, so
// the dimmed backdrop still reads as something to press to get out.
const ZOOM_PCT = 92

// The transport a client chat's header carries when the client answers the
// clock query: its time scale as one control, and the steps a paused client
// can be walked forward by. The scale is state - the strip shows it live and
// changing it says nothing in the transcript - while a step is a command: it
// goes in as a press and comes back as a screenshot of the frame it stepped
// to, which is what a step is for.
const SPEEDS = [
  { value: 0, label: "Paused" },
  { value: 0.5, label: "0.5x" },
  { value: 1, label: "1x" },
  { value: 2, label: "2x" },
]
const STEPS = [1, 10]

// What a group chat opens with: the facts the server's list row could not
// fit.
function serverIntro(server: Server): string[] {
  return [
    `${server.mode} ${server.key || "unknown"}`,
    server.entry || "Not answering",
    serverWhere(server),
  ]
}

/** A new chat with a client, or with the server itself when none is given:
 * named after the party, opening with the facts its list row could not fit.
 * What was known when the chat opened, not a live view of it. */
export function openChat(server: Server, client: Client | undefined): Conversation {
  if (client) return createConversation(clientLabel(client), [{ kind: "text", lines: clientFacts(client) }])
  let blocks: Block[] = [{ kind: "text", lines: serverIntro(server) }]
  if (!server.clients) blocks.push({ kind: "text", lines: ["Not answering on its port"], tone: "danger" })
  return createConversation(serverLabel(server), blocks)
}

/** Everything a chat is about, resolved by App from the selection: the party,
 * its server's live record and the conversation kept for it. */
export type ChatTarget = { party: Party; server: Server; conversation: Conversation }

function Chat(props: {
  target: ChatTarget
  /** The client's live record, or undefined in a group chat and once a
   * client has left. */
  client: Client | undefined
  slots: Slot[]
  onClose: () => void
  onBack: () => void
}) {
  // The screenshot blown up over the window, or null. Starts null because a
  // portal cannot mount during the app's initial render, which is what Modal
  // is built on.
  let [zoomed, setZoomed] = createSignal<Shot | null>(null)
  let say = (block: Block) => props.target.conversation.say(block)
  let settle = (id: number, ok: boolean) => props.target.conversation.settle(id, ok)
  let mark = (id: number) => {
    let ok = props.target.conversation.outcome(id)
    return ok === undefined ? null : ok ? CHECK_ICON : CROSS_ICON
  }
  let group = () => props.target.party.client === null
  // A client chat whose client has left: the history stays, the commands
  // do not.
  let gone = () => !group() && !props.client

  // A press: the command as your turn, marked in place once it settles, and
  // a reply only when there is something to look at - `null` back from `run`
  // means the mark said it all. A failure marks the press and explains
  // itself; the transcript is the only place this pane reports anything.
  let act = async (label: string, run: () => Promise<Block | null>) => {
    let id = say({ kind: "command", text: label })
    try {
      let reply = await run()
      settle(id, true)
      if (reply) say(reply)
    } catch (e) {
      settle(id, false)
      say({ kind: "text", lines: [String(e)], tone: "danger" })
    }
  }
  // Against the client, which must still be there: a chat outlives its
  // client, its commands do not.
  let withClient = (label: string, run: (client: Client) => Promise<Block | null>) =>
    act(label, async () => {
      let target = props.client
      if (!target) throw new Error("No longer connected")
      return run(target)
    })
  let runClient = (command: Command) =>
    withClient(commandLabel(command, props.client), (client) => command.run(props.target.server, client))
  let runServer = (command: ServerCommand) =>
    act(serverCommandLabel(command, props.target.server), () => command.run(props.target.server))
  // Whether the client answers the command: one that left, or whose runtime
  // predates a query the command needs, leaves the button disabled.
  let can = (command: Command) => props.client !== undefined && answers(command, props.client)

  // The client's time scale as the strip shows it: the client's own answer
  // to the last change, else what the server reports - which is what a poll,
  // a reload (which resets it) or a switch to another chat brings in. Through
  // a memo so a poll that reports the same value leaves a fresh answer alone.
  let [scale, setScale] = createSignal(1)
  let reported = createMemo(() => props.client?.timeScale ?? 1)
  createEffect(
    () => reported(),
    (value) => {
      setScale(value)
    },
  )
  let hasClock = () => props.client !== undefined && props.client.queries.includes("clock")
  let changeScale = async (value: number) => {
    let target = props.client
    if (!target) return
    try {
      setScale((await setClock(props.target.server, target, value)).scale)
    } catch (e) {
      say({ kind: "text", lines: [String(e)], tone: "danger" })
    }
  }
  let step = (n: number) =>
    withClient(`Step ${n}`, async (client) => {
      await stepFrames(props.target.server, client, n)
      return { kind: "shot", ...(await snapshotClient(props.target.server, client)) }
    })

  // What a block may ask of this chat: live data through accessors, so a
  // renderer reads it inside its JSX per poll rather than once at render,
  // and the follow-up commands a press in a block starts, each a turn of
  // its own.
  let ctx: BlockContext = {
    slots: () => props.slots,
    mark,
    start: (slot) =>
      act(`Slot ${slot}`, async () => {
        let started = await spawnClient(props.target.server, slot)
        return { kind: "text", lines: [`Started client ${slot} (pid ${started.pid ?? "unknown"})`] }
      }),
    zoom: (shot) => setZoomed(shot),
    snapshot: (nodeId, label) =>
      withClient(`Snapshot ${label}`, async (client) => ({
        kind: "shot",
        ...(await snapshotNode(props.target.server, client, nodeId)),
      })),
    call: (name) =>
      withClient(`Debug ${name}`, async (client) => ({
        kind: "json",
        value: await callDebug(props.target.server, client, name),
      })),
    texture: (id) =>
      withClient(`Texture ${id}`, async (client) => ({
        kind: "shot",
        ...(await readTexture(props.target.server, client, id)),
      })),
  }

  return (
    <View layout={{ flexDirection: "column", flexGrow: 1 }}>
      {/* Who the chat is with: a client and the server it is on, or the
          server and what it serves - and, for a client with a clock, its
          transport. The row wraps, so a narrow window drops the transport
          under the title instead of squeezing it. */}
      <View
        layout={{
          flexDirection: "row",
          alignItems: "center",
          flexWrap: "wrap",
          gap: space("md"),
          padding: space("lg"),
        }}
      >
        <Show when={policy.layout === "singlePane"}>
          <IconButton icon={BACK_ICON} onPress={props.onBack} />
        </Show>
        <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
          <Text variant="heading">{props.target.conversation.title}</Text>
          <Show
            when={gone()}
            fallback={
              <Text variant="caption" muted>
                {group() ? entryLabel(props.target.server) : serverLabel(props.target.server)}
              </Text>
            }
          >
            <Text variant="caption" color="danger">
              No longer connected
            </Text>
          </Show>
        </View>
        <Show when={hasClock()}>
          <View layout={{ flexDirection: "row", alignItems: "center", gap: space("sm") }}>
            <SegmentedControl
              options={SPEEDS}
              value={scale()}
              onChange={(value) => changeScale(value as number)}
            />
            <For each={STEPS}>
              {(n: number) => (
                <Button size="sm" variant="secondary" disabled={scale() !== 0} onPress={() => step(n)}>
                  {`Step ${n}`}
                </Button>
              )}
            </For>
          </View>
        </Show>
        {/* Closing gives its column up; the conversation stays in App's map,
            so reopening from the list brings the history back. */}
        <IconButton icon={CROSS_ICON} onPress={props.onClose} />
      </View>
      <Divider />
      {/* Oldest at the top, newest at the bottom; ChatView opens at the end,
          follows growth, and rests a short transcript against the composer. */}
      <ChatView layout={{ flexGrow: 1, flexBasis: 0 }}>
        <View layout={{ flexDirection: "column", gap: space("md"), padding: space("lg") }}>
          <For each={props.target.conversation.blocks()}>{(entry) => renderBlock(entry, ctx)}</For>
        </View>
      </ChatView>
      {/* Where a chat app puts its composer: what this party answers, rather
          than anything to type - the client commands, or for the group what
          the server does for it. */}
      <View
        layout={{
          flexDirection: "row",
          flexWrap: "wrap",
          gap: space("sm"),
          padding: space("lg"),
        }}
      >
        <Show
          when={group()}
          fallback={
            <For each={COMMANDS}>
              {(command: Command) => (
                <Button
                  size="sm"
                  variant={command.primary ? "primary" : "secondary"}
                  disabled={!can(command)}
                  onPress={() => runClient(command)}
                >
                  {commandLabel(command, props.client)}
                </Button>
              )}
            </For>
          }
        >
          <For each={SERVER_COMMANDS}>
            {(command: ServerCommand) => (
              <Show when={!command.available || command.available(props.target.server)}>
                <Button size="sm" variant="secondary" onPress={() => runServer(command)}>
                  {serverCommandLabel(command, props.target.server)}
                </Button>
              </Show>
            )}
          </For>
        </Show>
      </View>
      {/* The blown-up shot: a box just inside the window with the picture
          contained in it, so a wide capture and a tall one both fit whole.
          Pressing anywhere closes - the backdrop through Modal's own dismiss,
          the picture and the letterboxing beside it through this Pressable,
          which is what a reader expects of something that popped open. */}
      <Show when={zoomed()}>
        {(shot) => (
          <Modal onClose={() => setZoomed(null)}>
            <Pressable
              onPress={() => setZoomed(null)}
              layout={{
                width: pct(ZOOM_PCT),
                height: pct(ZOOM_PCT),
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Image
                src={shot().png}
                fit="contain"
                layout={{ width: pct(100), height: pct(100) }}
              />
            </Pressable>
          </Modal>
        )}
      </Show>
    </View>
  )
}

// The narrowest a chat column is let get: below this the composer buttons
// stack into a tower, so fewer chats fit instead of thinner ones.
const CHAT_MIN_WIDTH = 360

/** The detail pane: the open chats as columns, as many as the pane's width
 * fits at CHAT_MIN_WIDTH each - so collapsing the list buys a column, and a
 * narrow window is simply the one-column case. When more chats are open than
 * fit, the most recently focused ones get the columns; the rest keep their
 * conversations and wait in the list. */
export function ChatPane(props: {
  /** Every open chat, in opening order - the columns' order. */
  parties: Party[]
  /** The same chats' keys, least recently focused first: who gets a column
   * when not all fit. */
  recent: string[]
  slots: Slot[]
  serverOf: (party: Party) => Server | undefined
  clientOf: (party: Party) => Client | undefined
  conversationOf: (party: Party) => Conversation | undefined
  onClose: (party: Party) => void
  onBack: () => void
}) {
  // The pane's width, measured each layout, is what decides the column count.
  let outer: { id: number } | undefined
  let [width, setWidth] = createSignal(0)
  onLayout(() => {
    let b = outer && getBoundingBox(outer)
    if (b && b.width !== width()) setWidth(b.width)
  })
  let capacity = () => Math.max(1, Math.floor(width() / CHAT_MIN_WIDTH))
  let visible = createMemo(() => {
    let keys = props.recent.slice(-capacity())
    return props.parties.filter((party) => keys.includes(partyKey(party)))
  })
  return (
    <View ref={(n) => (outer = n)} layout={{ flexDirection: "row", flexGrow: 1 }}>
      <For
        each={visible()}
        keyed={partyKey}
        fallback={
          <View layout={{ flexGrow: 1, alignItems: "center", justifyContent: "center" }}>
            <Text muted>Pick a client.</Text>
          </View>
        }
      >
        {(party) => (
          <Show when={props.serverOf(party())}>
            {(server) => (
              <View layout={{ flexDirection: "row", flexGrow: 1, flexBasis: 0 }}>
                <Show when={visible().indexOf(party()) > 0}>
                  <Divider orientation="vertical" />
                </Show>
                <Chat
                  target={{
                    party: party(),
                    server: server(),
                    conversation: props.conversationOf(party())!,
                  }}
                  client={props.clientOf(party())}
                  slots={props.slots}
                  onClose={() => props.onClose(party())}
                  onBack={props.onBack}
                />
              </View>
            )}
          </Show>
        )}
      </For>
    </View>
  )
}
