import { createInterface } from "node:readline"
import { resolve, dirname } from "path"
import { readdirSync } from "node:fs"
import { state, print, printErr, shutdown } from "./util"
import { buildReload, getClients, sendReload, sendStop, sendStats, sendWatch, showBuildFailure } from "./dev-server"
import { bundle, bundleMaps } from "./bundler"
import { buildManifest } from "./project"
import { startWatcher, stopWatcher } from "./watcher"

// Resolve repl client indexes ("0 2") against the server's client list,
// printing a complaint for each invalid token. The list order is connect
// order, matching what `list` shows.
async function indexesToIds(args: string): Promise<number[]> {
  let clients = await getClients()
  let ids: number[] = []
  for (let token of args.split(/\s+/)) {
    let idx = parseInt(token, 10)
    if (isNaN(idx) || idx < 0 || idx >= clients.length) {
      print(`Invalid client index: ${token}`)
      continue
    }
    ids.push(clients[idx]!.id)
  }
  return ids
}

async function cmdStop(args: string) {
  if (!args) {
    stopWatcher()
    state.currentCode = null
    state.currentMaps = null
    state.currentManifest = null
    state.source = undefined
    await sendStop()
    print("[cli] Sent stop to all clients")
    return
  }
  let ids = await indexesToIds(args)
  if (ids.length) {
    await sendStop(ids)
    print(`[cli] Sent stop to client(s) ${ids.join(", ")}`)
  }
}

async function cmdReload(args: string) {
  if (state.source && state.source.endsWith(".tsx")) {
    let result = await bundle(state.source)
    if (!result) {
      printErr("[cli] Build failed, reload aborted")
      await showBuildFailure()
      return
    }
    state.currentCode = result.code
    state.currentMaps = bundleMaps(result)
    state.currentManifest = result.manifest
  }
  let msg = buildReload({ code: state.currentCode, manifest: state.currentManifest })
  if (!args) {
    await sendReload(msg, { latch: true, maps: state.currentMaps })
    print("[cli] Sent reload to all clients")
    return
  }
  let ids = await indexesToIds(args)
  if (ids.length) {
    await sendReload(msg, { clients: ids, maps: state.currentMaps })
    print(`[cli] Sent reload to client(s) ${ids.join(", ")}`)
  }
}

async function cmdStats(args: string) {
  if (args === "on") {
    state.stats = true
  } else if (args === "off") {
    state.stats = false
  } else if (!args) {
    state.stats = !state.stats
  } else {
    print("Usage: stats [on|off]")
    return
  }
  await sendStats(state.stats)
  print(`[cli] Stats overlay ${state.stats ? "on" : "off"}`)
}

async function cmdList() {
  let clients = await getClients()
  if (clients.length === 0) {
    print("No connected clients")
    return
  }
  print(`${clients.length} connected client(s):`)
  let i = 0
  for (let c of clients) {
    print(`  ${i++}: ${c.address ?? "unknown"} [${c.platform}, ${c.version}]`)
  }
}

async function cmdLoad(file: string) {
  if (!file) {
    print("Usage: load <file.tsx|.srt.js|.srt.bin>")
    return
  }
  let path = resolve(file)
  // Same rule as /__control__/load (control.ts): a server run serves the
  // project it started in, and an entry outside the project root cannot
  // resolve the project's dependencies anyway.
  // Windows paths are case-insensitive and the same drive shows up as both
  // `c:` and `C:` (an editor-spawned bridge keeps its parent's spelling), so a
  // drive-letter path folds case; a POSIX path stays exact.
  let norm = (p: string) => {
    let s = p.replace(/\\/g, "/")
    return /^[a-zA-Z]:\//.test(s) ? s.toLowerCase() : s
  }
  let root = norm(state.projectDir).replace(/\/+$/, "") + "/"
  if (!norm(path).startsWith(root)) {
    printErr(`[cli] Entry is outside the project root: ${path} is not under ${state.projectDir}. Restart srt in that project to work on it.`)
    return
  }
  if (file.endsWith(".tsx")) {
    let result = await bundle(path)
    if (!result) {
      printErr("[cli] Build failed")
      return
    }
    state.currentCode = result.code
    state.currentMaps = bundleMaps(result)
    state.currentManifest = result.manifest
  } else if (file.endsWith(".srt.js")) {
    state.currentCode = await Bun.file(path).text()
    state.currentMaps = null
    state.currentManifest = buildManifest(state.currentCode, path)
  } else if (file.endsWith(".srt.bin")) {
    let bytes = await Bun.file(path).arrayBuffer()
    // One-shot: bytecode loads are pushed but not latched for late joiners.
    await sendReload(buildReload({ bytecode: Buffer.from(bytes).toString("base64") }))
    print(`[cli] Loaded ${file} (bytecode, ${bytes.byteLength} bytes)`)
    return
  } else {
    print("Unsupported file type. Use .tsx, .srt.js, or .srt.bin")
    return
  }
  state.source = path
  state.sourceDir = dirname(path)
  startWatcher()
  // The load also moves the server's file-serving root to the new source dir
  // and its rebuild entry to the new file (for a later MCP reload). The
  // project root - and with it the /assets/ root - is fixed for the life of
  // the run.
  await sendReload(buildReload({ code: state.currentCode, manifest: state.currentManifest }), {
    latch: true,
    sourceDir: state.sourceDir,
    entry: file.endsWith(".tsx") ? path : undefined,
    maps: state.currentMaps,
  })
  print(`[cli] Loaded ${file}`)
}

let COMMANDS = ["load ", "stop", "reload", "list", "stats", "watch ", "quit", "exit", "help"]
let LOAD_EXTENSIONS = [".tsx", ".srt.js", ".srt.bin"]

function completer(line: string): [string[], string] {
  if (line.startsWith("load ")) {
    let partial = line.slice(5)
    let dir = partial.includes("/") ? partial.slice(0, partial.lastIndexOf("/") + 1) : ""
    let prefix = partial.slice(dir.length)
    let absDir = resolve(dir || ".")
    try {
      let entries = readdirSync(absDir, { withFileTypes: true })
      let matches: string[] = []
      for (let entry of entries) {
        if (!entry.name.startsWith(prefix)) continue
        if (entry.isDirectory()) {
          matches.push(entry.name + "/")
        } else if (LOAD_EXTENSIONS.some((ext) => entry.name.endsWith(ext))) {
          matches.push(entry.name)
        }
      }
      return [matches, prefix]
    } catch {
      return [[], line]
    }
  }
  let matches = COMMANDS.filter((c) => c.startsWith(line))
  return [matches, line]
}

// Run a repl command, reporting a failed server round-trip instead of leaving
// an unhandled rejection (e.g. the server process died mid-command).
function guard(p: Promise<void>) {
  p.catch((e) => printErr(`[cli] ${String(e)}`))
}

export function startRepl() {
  // Without a terminal there is nobody to prompt, and stdin is at EOF from the
  // start: readline would fire `close` immediately and shutdown() would tear
  // down the server, the client and the registry record about a second after
  // boot. A backgrounded or supervisor-launched srt therefore runs with no
  // repl at all, kept alive by the server process and the watcher, and stopped
  // with a signal. See okf/backlog/srt-run-exits-on-stdin-eof.md.
  if (!process.stdin.isTTY) {
    print("[cli] No terminal on stdin, running without the repl")
    return
  }

  state.rl = createInterface({ input: process.stdin, output: process.stdout, completer })
  state.rl.setPrompt("srt> ")

  state.rl.on("close", shutdown)

  state.rl.on("line", (line) => {
    let cmd = line.trim()
    if (cmd === "stop" || cmd.startsWith("stop ")) {
      guard(cmdStop(cmd.slice(5).trim()))
    } else if (cmd === "reload" || cmd.startsWith("reload ")) {
      guard(cmdReload(cmd.slice(7).trim()))
    } else if (cmd.startsWith("load ")) {
      guard(cmdLoad(cmd.slice(5).trim()))
    } else if (cmd === "list") {
      guard(cmdList())
    } else if (cmd === "stats" || cmd.startsWith("stats ")) {
      guard(cmdStats(cmd.slice(6).trim()))
    } else if (cmd === "watch on" || cmd === "watch off") {
      // Manual override for the agent-latched auto-reload pause (an agent
      // that died mid-edit leaves it off; its reload normally restores it).
      let enabled = cmd === "watch on"
      guard(sendWatch(enabled).then(() => print(`[cli] Auto-reload on change ${enabled ? "on" : "off"}`)))
    } else if (cmd === "quit" || cmd === "exit") {
      shutdown()
    } else if (cmd.startsWith("!")) {
      let shell = cmd.slice(1)
      if (shell) {
        Bun.$`${{ raw: shell }}`.quiet().then(
          (r) => {
            if (r.stdout.length) print(r.text())
          },
          (e) => {
            printErr(e.stderr.toString())
          },
        )
      }
    } else if (cmd === "help") {
      print("Commands: load, stop, reload, list, stats, watch on|off, !<cmd>, quit, help")
    } else if (cmd) {
      print(`Unknown command: ${cmd}`)
    }
    state.rl!.prompt()
  })

  state.rl.prompt()
}
