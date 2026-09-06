// The commands as data: what a client answers (`COMMANDS`, the composer of a
// client chat) and what a server does for the group its clients are in
// (`SERVER_COMMANDS`, the composer of a group chat). The chat runs the
// pressed one (see `act` in chat.tsx). Adding a command is one entry. A
// client command's `needs` names the dev-tool query kinds the client must
// advertise (`queries` in /clients): a runtime that predates a query does
// not answer it, so the button is disabled rather than the press failing.
import type { Block, TreeRow } from "./blocks"
import { canSpawnClient, servesMe, type Client, type Server } from "./servers"
import {
  fetchGpu,
  fetchLogs,
  fetchStats,
  fetchTree,
  listDebug,
  reloadServer,
  setClientStats,
  setMute,
  setWatch,
  snapshotClient,
} from "./control"

// How many of a client's most recent log lines one Logs block keeps.
const LOG_LINES = 50

// How deep below the window a Tree block reaches, and how many rows it
// shows: enough to find the region to snapshot, not the whole app.
const TREE_DEPTH = 2
const TREE_ROWS = 60

export type Command = {
  /** The button text, and the command bubble's. A function when it reads
   * as the action about to run, which depends on the client's state. */
  label: string | ((client: Client | undefined) => string)
  /** The composer's one filled button. */
  primary?: boolean
  /** Query kinds the client must answer for this command to run. */
  needs?: string[]
  /** Runs against the chosen client. Resolves with the block to reply with,
   * or null when the check on the press says it all. */
  run: (server: Server, client: Client) => Promise<Block | null>
}

// The tree as rows, parents before children, each with its depth.
function flatten(node: any, depth: number, rows: TreeRow[]) {
  rows.push({
    id: node.id,
    depth,
    kind: String(node.kind),
    text: typeof node.text === "string" ? node.text : undefined,
    x: node.x,
    y: node.y,
    width: node.width,
    height: node.height,
    childCount: typeof node.childCount === "number" ? node.childCount : undefined,
  })
  for (let child of node.children ?? []) flatten(child, depth + 1, rows)
}

export const COMMANDS: Command[] = [
  {
    label: "Screenshot",
    primary: true,
    needs: ["tree", "snapshot"],
    run: async (server, client) => ({ kind: "shot", ...(await snapshotClient(server, client)) }),
  },
  // One block is the tail at the moment it was asked for, from the whole
  // buffer (since 0), so it holds still like any other reply.
  {
    label: "Logs",
    run: async (server, client) => {
      let got = await fetchLogs(server, client, 0)
      return { kind: "logs", lines: got.lines.slice(-LOG_LINES) }
    },
  },
  {
    label: "Stats",
    needs: ["stats"],
    run: async (server, client) => ({ kind: "stats", stats: await fetchStats(server, client) }),
  },
  // The top of the tree; a row is pressed to snapshot that node, which is
  // how the smallest node showing a change gets captured rather than the
  // whole window.
  {
    label: "Tree",
    needs: ["tree"],
    run: async (server, client) => {
      let rows: TreeRow[] = []
      flatten(await fetchTree(server, client, TREE_DEPTH), 0, rows)
      return { kind: "tree", rows: rows.slice(0, TREE_ROWS), hidden: Math.max(0, rows.length - TREE_ROWS) }
    },
  },
  {
    label: "Debug",
    needs: ["debug_list", "debug_call"],
    run: async (server, client) => ({ kind: "debug", names: await listDebug(server, client) }),
  },
  {
    label: "GPU",
    needs: ["gpu"],
    run: async (server, client) => ({ kind: "gpu", gpu: await fetchGpu(server, client) }),
  },
  // The label is the action, so it reads as the command it is about to run -
  // and as the record of what was done once it is in the transcript. Nothing
  // comes back: the check on the press is the whole answer.
  {
    label: (client) => (client?.stats ? "Overlay off" : "Overlay on"),
    run: async (server, client) => {
      await setClientStats(server, client, !client.stats)
      return null
    },
  },
]

/** The command's text for this client (or for none). */
export function commandLabel(command: Command, client: Client | undefined): string {
  return typeof command.label === "function" ? command.label(client) : command.label
}

/** Whether the client advertises every query the command needs. */
export function answers(command: Command, client: Client): boolean {
  return (command.needs ?? []).every((kind) => client.queries.includes(kind))
}

export type ServerCommand = {
  /** The button text, and the command bubble's; a function when it reads as
   * the action about to run, which depends on the server's state. */
  label: string | ((server: Server) => string)
  /** Whether it is offered for this server at all: a spawn needs a
   * toolchain, and a mute must not silence the console itself. */
  available?: (server: Server) => boolean
  /** Runs against the server, for every client it has. */
  run: (server: Server) => Promise<Block | null>
}

export const SERVER_COMMANDS: ServerCommand[] = [
  // The slots to start a client in, as a block to pick from.
  { label: "New client", available: canSpawnClient, run: async () => ({ kind: "slots" }) },
  // A build error comes back as the failure; a clean push needs no reply.
  {
    label: "Reload",
    run: async (server) => {
      await reloadServer(server)
      return null
    },
  },
  // The two holds an agent takes, for a human to take or release the same
  // way: the label is the action, the check the whole answer. Not the mute
  // on a server this console is a client of: it would mute the console, and
  // nothing but another console could lift it.
  {
    label: (server) => (server.userInputMuted ? "Unmute input" : "Mute input"),
    available: (server) => !servesMe(server),
    run: async (server) => {
      await setMute(server, !server.userInputMuted)
      return null
    },
  },
  {
    label: (server) => (server.watchPaused ? "Resume watch" : "Pause watch"),
    run: async (server) => {
      await setWatch(server, server.watchPaused)
      return null
    },
  },
]

/** The server command's text for this server. */
export function serverCommandLabel(command: ServerCommand, server: Server): string {
  return typeof command.label === "function" ? command.label(server) : command.label
}
