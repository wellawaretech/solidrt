#!/usr/bin/env bun

// Build/run script that bundles/runs a Solid-RT app for the QuickJS runtime.
//
// Usage:
//   srt run                             - start dev server + client
//   srt run examples/hello.tsx          - start dev server + client, bundle + push via WS
//   srt server examples/hello.tsx       - start dev server only, no client
//   srt client                          - start dev client only (connects to WS server)
//   srt bundle examples/hello.tsx       - bundle TSX to .srt.js
//   srt bundle -c examples/hello.tsx    - bundle TSX to .srt.js + compile to .srt.bin
//   srt bundle examples/hello.srt.js   - compile .srt.js to .srt.bin
//   srt record examples/hello.tsx       - bundle TSX and run with frame capture

import pkg from "../package.json"
import { values, command, source, isTsx, isPrebuilt, printUsage } from "./args"
import { state, requireBinary, run, shutdown } from "./util"
import { bundle, bundleTo, runBuildCommand } from "./build"
import { startServer } from "./server"
import { spawnClient } from "./client"
import { startRepl } from "./repl"
import { startWatcher } from "./watcher"
import * as cache from "./cache"
import { resolve, dirname } from "path"

// -- Validate args --

let COMMANDS = ["run", "server", "client", "bundle", "record"]

if (!command || !COMMANDS.includes(command)) {
  printUsage()
  process.exit(1)
}

if (command === "bundle" && (!source || (!isTsx && !isPrebuilt))) {
  console.error("Usage: srt bundle [options] <entry.[tsx|jsx|srt.js|srt.bin]>")
  process.exit(1)
}

if (command === "record" && (!source || !isTsx)) {
  console.error("Usage: srt record <entry.[tsx|jsx]>")
  process.exit(1)
}

// Force the production export condition for prod bundles. Bun auto-activates the
// "development" condition whenever NODE_ENV != "production" (read once at startup),
// and an auto-active condition cannot be turned off via Bun.build({ conditions }).
// Our deps (@solidjs/signals, solid-js) only expose a "development" branch + a
// default fallback - no "production" key - so adding conditions does nothing; the
// only way to reach the default (smaller, no extra invariants) build is to stop
// the auto-activation by setting NODE_ENV=production. Since that is read at startup,
// we re-exec rather than mutate process.env. Assumes srt runs via bun (argv is
// [bun, script, ...]); would need rework if ever shipped as a compiled binary.
let isProdBuild = (command === "bundle" || command === "record") && !values.dev
if (isProdBuild && process.env.NODE_ENV !== "production") {
  let proc = Bun.spawnSync({
    cmd: [process.execPath, ...process.argv.slice(1)],
    env: { ...process.env, NODE_ENV: "production" },
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  })
  process.exit(proc.exitCode ?? 0)
}

// -- Bundle command --

if (command === "bundle") {
  await runBuildCommand()
}

// -- Record command --

if (command === "record") {
  let jsOutfile = source!.replace(/\.[jt]sx$/, "") + ".srt.js"
  await bundleTo(jsOutfile)
  let runner = requireBinary("solidrt-go")
  let recordArgs = ["--record", resolve(jsOutfile)]
  if (values.fps) recordArgs.push("--fps", values.fps)
  if (values.duration) recordArgs.push("--duration", values.duration)
  if (values.size) recordArgs.push("--size", values.size)
  let exit = await run(runner, recordArgs)
  process.exit(exit)
}

// -- Client command --

if (command === "client") {
  let runner = requireBinary("solidrt-go")
  let args: string[] = []
  if (values.size) args.push("--size", values.size)
  //TODO add dev server connection
  // if (source) args.push("--dev-server", source)
  let exit = await run(runner, args)
  process.exit(exit)
}

// -- Server / Run command --

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
if (source && isTsx) {
  let initialResult = await bundle()
  if (initialResult) {
    for (let output of initialResult.outputs) {
      state.currentCode = await output.text()
    }
  }
} else if (source && isPrebuilt && source.endsWith(".srt.js")) {
  state.currentCode = await Bun.file(resolve(source)).text()
}

if (command === "run") {
  spawnClient()
}

process.on("SIGINT", shutdown)
process.on("SIGTERM", shutdown)

let version = pkg.version === "0.0.0" ? "" : " version " + pkg.version
console.log(`[cli] Welcome to SolidRT${version}!`)
startRepl()
startWatcher()
