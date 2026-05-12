import { watch } from "node:fs"
import { resolve, dirname } from "path"
import { state, print, printErr } from "./util"
import { bundle } from "./build"

let currentWatcher: ReturnType<typeof watch> | null = null

export function startWatcher() {
  if (!state.source) return
  let watchDir = dirname(resolve(state.source))

  if (currentWatcher) currentWatcher.close()

  print(`[dev] Watching ${watchDir} for changes...`)
  currentWatcher = watch(watchDir, { recursive: true }, async (_event, filename) => {
    if (!filename) return
    if (!/\.(tsx?|jsx?)$/.test(filename)) return

    print(`[watch] Change detected: ${filename}`)
    let result = await bundle(state.source)
    if (!result) {
      printErr("[dev] Build failed, waiting for changes...")
      return
    }
    for (let output of result.outputs) {
      state.currentCode = await output.text()
    }
    for (let ws of state.clients.keys()) {
      ws.send(JSON.stringify({ type: "reload", code: state.currentCode }))
    }
  })
}