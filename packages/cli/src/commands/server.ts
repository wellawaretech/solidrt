import pkg from "../../package.json"
import { source, isSource, isPrebuilt, values } from "../args"
import { state, shutdown } from "../util"
import { bundle } from "../bundler"
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
      state.currentMap = initialResult.map
      await sendReload(buildReload({ code: state.currentCode }), { latch: true, map: state.currentMap })
    } else {
      await showBuildFailure()
    }
  } else if (source && isPrebuilt && source.endsWith(".srt.js")) {
    state.currentCode = await Bun.file(resolve(source)).text()
    await sendReload(buildReload({ code: state.currentCode }), { latch: true })
  }

  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)

  let version = pkg.version === "0.0.0" ? "" : " version " + pkg.version
  console.log(`[cli] Welcome to SolidRT${version}!`)
  startRepl()
  startWatcher()
}
