import pkg from "../../package.json"
import { source, isSource, isPrebuilt, values } from "../args"
import { state, shutdown, print, printErr } from "../util"
import { findProjectRoot, typecheck, reportTypes } from "./check"
import { bundle, bundleMaps, prebuiltManifest } from "../bundler"
import { projectDirFor } from "../project"
import { startServer, buildReload, sendReload, showBuildFailure } from "../dev-server"
import { startRepl } from "../repl"
import { startWatcher } from "../watcher"
import { resolve, dirname } from "path"

// Brings up the dev server (a spawned flux script serving HTTP/WS) plus the
// initial bundle, repl, and watcher in this process. The `run` command spawns
// a local client on top of this from main.ts.
export async function runServerCommand() {
  // Initialize state from args
  state.source = source
  state.sourceDir = source ? dirname(resolve(source)) : process.cwd()
  // With no entry the project is wherever srt was started: walk up to the
  // nearest package.json exactly like an entry would, so the projectDir the
  // MCP bridge derives for its registry match agrees with ours.
  state.projectDir = projectDirFor(source ? resolve(source) : resolve("package.json"))
  state.stats = values.stats
  state.capture = values.capture ? resolve(values.capture) : undefined

  // Spawns the server process and waits until it answers; it owns the QR and
  // address announcements, the capture file, and the proxy cache.
  await startServer()

  // Bundle initial code if source file given (after server start so the
  // dev base URL is available to the bundler), and latch it on the server
  // for the clients about to connect.
  if (source && isSource) {
    let initialResult = await bundle()
    if (initialResult) {
      state.currentCode = initialResult.code
      state.currentMaps = bundleMaps(initialResult)
      state.currentManifest = initialResult.manifest
      await sendReload(buildReload({ code: state.currentCode, manifest: state.currentManifest }), {
        latch: true,
        maps: state.currentMaps,
      })
    } else {
      await showBuildFailure()
    }
  } else if (source && isPrebuilt && source.endsWith(".srt.js")) {
    let path = resolve(source)
    state.currentCode = await Bun.file(path).text()
    state.currentManifest = prebuiltManifest(state.currentCode, path, state.projectDir)
    await sendReload(buildReload({ code: state.currentCode, manifest: state.currentManifest }), { latch: true })
  }

  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)

  let version = pkg.version === "0.0.0" ? "" : " version " + pkg.version
  console.log(`[cli] Welcome to SolidRT${version}!`)
  startRepl()
  startWatcher()

  // Startup typecheck, deliberately not awaited: diagnostics print over the
  // repl when tsc finishes, and a type error never gates the boot (srt check
  // is the hard gate). Once per server lifetime; hot reloads never typecheck.
  // Source builds only: a prebuilt .srt.js has no checkable project here.
  if (source && isSource) startupTypecheck(source)
}

async function startupTypecheck(entry: string) {
  let root = findProjectRoot(entry)
  if (!root) return
  let types = await typecheck(root, entry)
  if (types) reportTypes(types, print, printErr)
}
