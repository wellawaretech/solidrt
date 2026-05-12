import { createInterface } from "node:readline"
import { resolve, dirname } from "path"
import { readdirSync } from "node:fs"
import { state, print, printErr, broadcastStop, shutdown } from "./util"
import { bundle } from "./build"
import { startWatcher } from "./watcher"

function cmdStop(args: string) {
  if (!args) {
    broadcastStop()
    print("[dev] Sent stop to all clients")
    return
  }
  let clientList = [...state.clients.keys()]
  for (let token of args.split(/\s+/)) {
    let idx = parseInt(token, 10)
    if (isNaN(idx) || idx < 0 || idx >= clientList.length) {
      print(`Invalid client index: ${token}`)
      continue
    }
    clientList[idx].send(JSON.stringify({ type: "stop" }))
    print(`[dev] Sent stop to client ${idx}`)
  }
}

async function cmdReload(args: string) {
  if (state.source && state.source.endsWith(".tsx")) {
    let result = await bundle(state.source)
    if (!result) {
      printErr("[dev] Build failed, reload aborted")
      return
    }
    for (let output of result.outputs) {
      state.currentCode = await output.text()
    }
  }
  let msg = JSON.stringify({ type: "reload", code: state.currentCode })
  if (!args) {
    for (let ws of state.clients.keys()) ws.send(msg)
    print("[dev] Sent reload to all clients")
    return
  }
  let clientList = [...state.clients.keys()]
  for (let token of args.split(/\s+/)) {
    let idx = parseInt(token, 10)
    if (isNaN(idx) || idx < 0 || idx >= clientList.length) {
      print(`Invalid client index: ${token}`)
      continue
    }
    clientList[idx].send(msg)
    print(`[dev] Sent reload to client ${idx}`)
  }
}

function cmdList() {
  if (state.clients.size === 0) {
    print("No connected clients")
    return
  }
  print(`${state.clients.size} connected client(s):`)
  let i = 0
  for (let [ws, info] of state.clients) {
    print(`  ${i++}: ${ws.remoteAddress} [${info.platform}, ${info.version}]`)
  }
}

async function cmdLoad(file: string) {
  if (!file) {
    print("Usage: load <file.tsx|.srt.js|.srt.bin>")
    return
  }
  let path = resolve(file)
  if (file.endsWith(".tsx")) {
    let result = await bundle(path)
    if (!result) {
      printErr("[dev] Build failed")
      return
    }
    for (let output of result.outputs) {
      state.currentCode = await output.text()
    }
  } else if (file.endsWith(".srt.js")) {
    state.currentCode = await Bun.file(path).text()
  } else if (file.endsWith(".srt.bin")) {
    let bytes = await Bun.file(path).arrayBuffer()
    let msg = { type: "reload", bytecode: Buffer.from(bytes).toString("base64") }
    for (let ws of state.clients.keys()) ws.send(JSON.stringify(msg))
    print(`[dev] Loaded ${file} (bytecode, ${bytes.byteLength} bytes)`)
    return
  } else {
    print("Unsupported file type. Use .tsx, .srt.js, or .srt.bin")
    return
  }
  state.source = path
  state.sourceDir = dirname(path)
  startWatcher()
  for (let ws of state.clients.keys()) {
    ws.send(JSON.stringify({ type: "reload", code: state.currentCode }))
  }
  print(`[dev] Loaded ${file}`)
}

let COMMANDS = ["load ", "stop", "reload", "list", "quit", "exit", "help"]
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

export function startRepl() {
  state.rl = createInterface({ input: process.stdin, output: process.stdout, completer })
  state.rl.setPrompt("srt> ")

  state.rl.on("close", shutdown)

  state.rl.on("line", (line) => {
    let cmd = line.trim()
    if (cmd === "stop" || cmd.startsWith("stop ")) {
      cmdStop(cmd.slice(5).trim())
    } else if (cmd === "reload" || cmd.startsWith("reload ")) {
      cmdReload(cmd.slice(7).trim())
    } else if (cmd.startsWith("load ")) {
      cmdLoad(cmd.slice(5).trim())
    } else if (cmd === "list") {
      cmdList()
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
      print("Commands: load, stop, reload, list, !<cmd>, quit, help")
    } else if (cmd) {
      print(`Unknown command: ${cmd}`)
    }
    state.rl!.prompt()
  })

  state.rl.prompt()
}