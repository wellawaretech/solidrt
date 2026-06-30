import { watch } from "node:fs"
import { resolve, dirname } from "path"
import { state, print, printErr } from "./util"
import { buildReload, showBuildFailure } from "./dev-server"
import { bundle, codeFromOutputs } from "./bundler"

let currentWatcher: ReturnType<typeof watch> | null = null

export function stopWatcher() {
  if (currentWatcher) {
    currentWatcher.close()
    currentWatcher = null
  }
}

export function startWatcher() {
  if (!state.source) return
  let watchDir = dirname(resolve(state.source))

  if (currentWatcher) currentWatcher.close()

  print(`[cli] Watching ${watchDir} for changes...`)
  currentWatcher = watch(watchDir, { recursive: true }, async (_event, filename) => {
    if (!filename) return
    if (!/\.(tsx?|jsx?)$/.test(filename)) return

    print(`[cli] Change detected: ${filename}`)
    let result = await bundle(state.source)
    if (!result) {
      printErr("[cli] Build failed, waiting for changes...")
      showBuildFailure()
      return
    }
    state.currentCode = await codeFromOutputs(result.outputs)
    let msg = buildReload({ code: state.currentCode })
    for (let ws of state.clients.keys()) {
      ws.send(msg)
    }
  })
}