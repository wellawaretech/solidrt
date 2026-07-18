import { watch } from "node:fs"
import { resolve, dirname } from "path"
import { state, print, printErr } from "./util"
import { buildReload, sendReload, showBuildFailure, watchAllowed } from "./dev-server"
import { bundle } from "./bundler"

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

    if (!(await watchAllowed())) {
      print(`[cli] Change detected: ${filename} (auto-reload paused by agent; "watch on" resumes)`)
      return
    }

    print(`[cli] Change detected: ${filename}`)
    let result = await bundle(state.source)
    if (!result) {
      printErr("[cli] Build failed, waiting for changes...")
      await showBuildFailure()
      return
    }
    state.currentCode = result.code
    state.currentMap = result.map
    await sendReload(buildReload({ code: state.currentCode }), { latch: true, map: state.currentMap })
  })
}