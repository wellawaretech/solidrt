import pkg from "../../package.json"
import { source, isSource, isPrebuilt, values } from "../args"
import { state, shutdown } from "../util"
import { bundle } from "../bundler"
import { startServer } from "../server"
import { spawnClient } from "../client"
import { startRepl } from "../repl"
import { startWatcher } from "../watcher"
import * as cache from "../cache"
import { resolve, dirname } from "path"

// Brings up the dev server (HTTP/WS + initial bundle + repl + watcher). When
// `withClient` is set (the `run` command) it also spawns a local client wired
// into the dev-server lifecycle via spawnClient(); this is distinct from the
// standalone `client` command.
export async function runServerCommand({ withClient = false } = {}) {
  // Initialize state from args
  state.source = source
  state.sourceDir = source ? dirname(resolve(source)) : process.cwd()

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
      for (let output of initialResult.outputs) {
        state.currentCode = await output.text()
      }
    }
  } else if (source && isPrebuilt && source.endsWith(".srt.js")) {
    state.currentCode = await Bun.file(resolve(source)).text()
  }

  if (withClient) {
    spawnClient()
  }

  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)

  let version = pkg.version === "0.0.0" ? "" : " version " + pkg.version
  console.log(`[cli] Welcome to SolidRT${version}!`)
  startRepl()
  startWatcher()
}