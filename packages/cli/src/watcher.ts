import { existsSync, watch } from "node:fs"
import { resolve, dirname, sep, basename } from "path"
import { state, print, printErr } from "./util"
import { buildReload, sendReload, showBuildFailure, watchAllowed } from "./dev-server"
import { bundle } from "./bundler"

let watchers: ReturnType<typeof watch>[] = []

export function stopWatcher() {
  for (let w of watchers) w.close()
  watchers = []
}

async function rebuild(filename: string) {
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
  state.currentManifest = result.manifest
  await sendReload(buildReload({ code: state.currentCode, manifest: state.currentManifest }), {
    latch: true,
    map: state.currentMap,
  })
}

export function startWatcher() {
  if (!state.source) return
  let watchDir = dirname(resolve(state.source))

  stopWatcher()

  // An asset edit re-pushes too: the assets/ tree is part of the manifest, so
  // clients install the new version. The tree roots at the project dir; the
  // project dir is an ancestor of (or equal to) the entry's dir, so assets/
  // is inside watchDir exactly when they are the same dir. An assets/ folder
  // created after startup needs a restart or `load` to be picked up.
  let assetsDir = resolve(state.projectDir, "assets")
  let covered = resolve(state.projectDir) === watchDir
  let isAsset = (filename: string) =>
    (filename === "assets" || filename.startsWith("assets" + sep)) && !basename(filename).startsWith(".")

  print(`[cli] Watching ${watchDir} for changes...`)
  watchers.push(
    watch(watchDir, { recursive: true }, (_event, filename) => {
      if (!filename) return
      if (!/\.(tsx?|jsx?)$/.test(filename) && !(covered && isAsset(filename))) return
      rebuild(filename)
    }),
  )

  if (!covered && existsSync(assetsDir)) {
    watchers.push(
      watch(assetsDir, { recursive: true }, (_event, filename) => {
        if (!filename || basename(filename).startsWith(".")) return
        rebuild("assets" + sep + filename)
      }),
    )
  }
}