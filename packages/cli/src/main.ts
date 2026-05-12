#!/usr/bin/env bun

// Build/run script that bundles/runs a Solid-RT app for the QuickJS runtime.
//
// Usage:
//   srt run                             - start dev server + client
//   srt run examples/hello.tsx          - start dev server + client, bundle + push via WS
//   srt run --client                    - start dev client only (connects to WS server)
//   srt run --server examples/hello.tsx - start dev server only, no client
//   srt build examples/hello.tsx        - bundle TSX to .srt.js
//   srt build -c examples/hello.tsx     - bundle TSX to .srt.js + compile to .srt.bin
//   srt build examples/hello.srt.js     - compile .srt.js to .srt.bin

import { values, command, source, isTsx, isPrebuilt, printUsage } from "./args"
import { state, requireBinary, run, shutdown } from "./util"
import { bundle, runBuildCommand } from "./build"
import { startServer } from "./server"
import { spawnClient } from "./client"
import { startRepl } from "./repl"
import { startWatcher } from "./watcher"
import { resolve, dirname } from "path"

// -- Validate args --

if (!command || (command !== "build" && command !== "run")) {
  printUsage()
  process.exit(1)
}

if (command === "build" && (!source || (!isTsx && !isPrebuilt))) {
  console.error("Usage: srt build [options] <entry.tsx|.srt.js|.srt.bin>")
  process.exit(1)
}

// -- Build command --

if (command === "build") {
  await runBuildCommand()
}

// -- Run command --

if (values.client) {
  let runner = requireBinary("solid-rt-runner")
  let args = ["--dev"]
  if (source) args.push("--dev-server", source)
  process.exit(await run(runner, args))
}

// Initialize state from args
state.source = source
state.sourceDir = source ? dirname(resolve(source)) : process.cwd()

startServer()

// Bundle initial code if source file given (after server start so the
// dev base URL is available to the bundler).
if (source && isTsx) {
  let initialResult = await bundle()
  if (initialResult) {
    for (let output of initialResult.outputs) {
      state.currentCode = await output.text()
    }
  }
}

if (!values.server) {
  spawnClient()
}

process.on("SIGINT", shutdown)
process.on("SIGTERM", shutdown)

startRepl()
startWatcher()
