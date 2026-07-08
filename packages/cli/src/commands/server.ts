import pkg from "../../package.json"
import { source, isSource, isPrebuilt, values } from "../args"
import { state, shutdown } from "../util"
import { bundle, codeFromOutputs } from "../bundler"
import { startServer, showBuildFailure } from "../dev-server"
import { startRepl } from "../repl"
import { startWatcher } from "../watcher"
import * as cache from "../cache"
import { resolve, dirname } from "path"
import { writeFileSync } from "node:fs"

// Brings up the dev server (HTTP/WS + initial bundle + repl + watcher). The
// `run` command spawns a local client on top of this from main.ts.
export async function runServerCommand() {
  // Initialize state from args
  state.source = source
  state.sourceDir = source ? dirname(resolve(source)) : process.cwd()
  state.stats = values.stats
  state.capture = values.capture ? resolve(values.capture) : undefined
  state.captureStartMs = Date.now()
  // Start each capture from an empty file: appendFileSync (dev-server.ts)
  // would otherwise tack onto whatever a previous run left behind.
  if (state.capture) writeFileSync(state.capture, "")

  if (values["proxy-http"]) {
    cache.initCache({ dir: process.cwd() })
    console.log("[cli] HTTP cache enabled")
  }

  startServer()

  // Bundle initial code if source file given (after server start so the
  // dev base URL is available to the bundler).
  if (source && isSource) {
    let initialResult = await bundle()
    if (initialResult) {
      state.currentCode = await codeFromOutputs(initialResult.outputs)
    } else {
      showBuildFailure()
    }
  } else if (source && isPrebuilt && source.endsWith(".srt.js")) {
    state.currentCode = await Bun.file(resolve(source)).text()
  }

  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)

  let version = pkg.version === "0.0.0" ? "" : " version " + pkg.version
  console.log(`[cli] Welcome to SolidRT${version}!`)
  startRepl()
  startWatcher()
}