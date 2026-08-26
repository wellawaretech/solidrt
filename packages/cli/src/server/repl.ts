import { isTTY, on, write } from "flux:tty"
import type { ServerWebSocket } from "flux:http"
import { state } from "./state"
import { rebuildAndBroadcast, showBuildFailure } from "./rebuild"
import { clientList, loadEntry, setStats, setUserInputMuted, setWatchActive } from "./control"

// The repl on the server's own terminal: the same actions the control API
// offers a coding agent, typed by hand. Cooked-mode lines from flux:tty (the
// terminal does the editing; history and completion are a later stage). Only
// with a terminal on stdin: a server started by a supervisor, the console or
// a background `&` has none, runs without a prompt, and stops on a signal.
// Server log lines interleave with the prompt line; accepted for now.

const PROMPT = "srt> "
const HELP = "Commands: load <file>, reload [id...], stop [id...], list, stats [on|off], watch on|off, mute on|off, quit, help"

// The clients named by a list of ids ("0 2", as `list` prints them), or
// every client for no ids. Unknown ids are reported and skipped.
function targets(args: string): ServerWebSocket[] {
  let entries = [...state.clients.entries()]
  if (!args) return entries.map(([ws]) => ws)
  let found: ServerWebSocket[] = []
  for (let token of args.split(/\s+/)) {
    let id = parseInt(token, 10)
    let entry = entries.find(([, info]) => info.id === id)
    if (!entry) {
      console.log(`No client with id ${token}`)
      continue
    }
    found.push(entry[0])
  }
  return found
}

function describe(args: string, count: number): string {
  return args ? `client(s) ${args.split(/\s+/).join(", ")}` : `all ${count} client(s)`
}

async function cmdReload(args: string) {
  let error = await rebuildAndBroadcast()
  if (error) {
    console.error(error)
    showBuildFailure()
    return
  }
  if (args) {
    // rebuildAndBroadcast pushed to everyone; a selective reload re-sends
    // the latched bundle to the named clients only, which is what it meant
    // before the push existed. Kept for the one case it still serves: a
    // client that missed the broadcast.
    for (let ws of targets(args)) if (state.currentReload) ws.send(state.currentReload)
  }
  console.log(`[cli] Sent reload to ${describe(args, state.clients.size)}`)
}

function cmdStop(args: string) {
  let text = JSON.stringify({ type: "stop" })
  let list = targets(args)
  for (let ws of list) ws.send(text)
  if (list.length) console.log(`[cli] Sent stop to ${describe(args, list.length)}`)
}

async function cmdLoad(file: string) {
  if (!file) {
    console.log("Usage: load <file>")
    return
  }
  let result = await loadEntry(file)
  if ("error" in result) {
    console.error(result.error)
    if (result.status === 502) showBuildFailure()
    return
  }
  console.log(`[cli] Loaded ${result.entry}`)
}

function cmdList() {
  let clients = clientList(true)
  if (clients.length === 0) {
    console.log("No connected clients")
    return
  }
  console.log(`${clients.length} connected client(s):`)
  for (let c of clients) {
    console.log(`  ${c.id}: ${c.address ?? "unknown"} [${c.platform}, ${c.version}]`)
  }
}

// "on" | "off" | "" (toggle) for the three switches; anything else is a
// usage error, reported with the command's usage line.
function toggle(args: string, current: boolean, usage: string): boolean | undefined {
  if (args === "on") return true
  if (args === "off") return false
  if (!args) return !current
  console.log(usage)
  return undefined
}

function cmdStats(args: string) {
  let on = toggle(args, state.stats, "Usage: stats [on|off]")
  if (on === undefined) return
  setStats(on)
  console.log(`[cli] Stats overlay ${on ? "on" : "off"}`)
}

function cmdWatch(args: string) {
  let on = toggle(args, !state.watchPaused, "Usage: watch on|off")
  if (on === undefined) return
  setWatchActive(on)
}

function cmdMute(args: string) {
  let on = toggle(args, state.userInputMuted, "Usage: mute on|off")
  if (on === undefined) return
  setUserInputMuted(on)
}

// The word and the rest of the line, trimmed.
function split(line: string): [string, string] {
  let cmd = line.trim()
  let i = cmd.indexOf(" ")
  return i < 0 ? [cmd, ""] : [cmd.slice(0, i), cmd.slice(i + 1).trim()]
}

async function dispatch(line: string, quit: () => void): Promise<void> {
  let [cmd, args] = split(line)
  switch (cmd) {
    case "":
      return
    case "reload":
      return cmdReload(args)
    case "stop":
      return cmdStop(args)
    case "load":
      return cmdLoad(args)
    case "list":
      return cmdList()
    case "stats":
      return cmdStats(args)
    case "watch":
      return cmdWatch(args)
    case "mute":
      return cmdMute(args)
    case "quit":
    case "exit":
      return quit()
    case "help":
      console.log(HELP)
      return
    default:
      console.log(`Unknown command: ${cmd}`)
  }
}

// Start the repl when a terminal is attached; `quit` ends the server (the
// quit command, or stdin closing on Ctrl-D). Returns the function that
// detaches it, for the server's shutdown.
export function startRepl(quit: () => void): () => void {
  if (!isTTY) {
    console.log("[cli] No terminal on stdin, running without the repl")
    return () => {}
  }
  // Commands run one at a time: a line typed while one runs waits for it, so
  // its output and the next prompt keep their order. No prompt once the
  // server is shutting down (quit, or a signal mid-command).
  let attached = true
  let prompt = () => {
    if (attached) write(PROMPT)
  }
  let chain = Promise.resolve()
  let offLine = on("line", (line) => {
    chain = chain.then(() => dispatch(line, quit)).then(prompt, (e) => {
      console.error(`[cli] ${String(e)}`)
      prompt()
    })
  })
  let offClose = on("close", quit)
  prompt()
  return () => {
    attached = false
    offLine()
    offClose()
  }
}
