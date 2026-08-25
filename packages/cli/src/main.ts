#!/usr/bin/env bun

// bin/srt lands here. This file routes the command word and nothing else:
// the bun commands live one folder each (src/<command>/main.ts) and load on
// demand, and `run`/`server` launch the flux dev server (src/server/), a
// process complete on its own that this launcher only resolves the binaries
// for (okf/done/srt-command-folders.md).

import { fileURLToPath } from "node:url"
import { existsSync, unlinkSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, resolve } from "node:path"
import { values, command, source, port, appArgs, validateArgs } from "./lib/args"
import { printUsage, printVersion, hint } from "./lib/usage"
import { requireBinary } from "./lib/util"
import { buildServerBundle } from "./lib/server-bundle"

// -- Help and version --

// Answered before anything else: neither takes a command, so they must not
// fall through to the usage error.
if (values.help) {
  printUsage()
  process.exit(0)
}
if (values.version) {
  printVersion()
  process.exit(0)
}

// -- Validate args --

validateArgs()

// Force the production export condition for prod bundles. Bun auto-activates the
// "development" condition whenever NODE_ENV != "production" (read once at startup),
// and an auto-active condition cannot be turned off via Bun.build({ conditions }).
// Our deps (@solidjs/signals, solid-js) only expose a "development" branch + a
// default fallback - no "production" key - so adding conditions does nothing; the
// only way to reach the default (smaller, no extra invariants) build is to stop
// the auto-activation by setting NODE_ENV=production. Since that is read at startup,
// we re-exec rather than mutate process.env. Assumes srt runs via bun (argv is
// [bun, script, ...]); would need rework if ever shipped as a compiled binary.
let isProdBuild = (command === "bundle" || command === "render" || command === "pack") && !values.dev
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

// -- run / server: launch the dev server --

// The server script the flux binary runs: the prebuilt bundle a published
// CLI ships, or (in a checkout) one built now into a temp file, removed on
// exit.
async function serverScript(): Promise<{ path: string; temp: boolean }> {
  let prebuilt = fileURLToPath(new URL("../dist/server.js", import.meta.url))
  if (existsSync(prebuilt)) return { path: prebuilt, temp: false }
  let outfile = resolve(tmpdir(), `srt-dev-server-${process.pid}.js`)
  await buildServerBundle(outfile)
  return { path: outfile, temp: true }
}

async function launchServer(withClient: boolean) {
  let flux = requireBinary("flux")
  let script = await serverScript()

  let args: string[] = []
  if (source !== undefined) args.push(source)
  if (values.project) args.push("--project")
  if (values.file) args.push("--file")
  if (port !== undefined) args.push("--port", String(port))
  for (let flag of ["lan", "proxy-http", "tunnel", "stats", "minify"] as const) {
    if (values[flag]) args.push(`--${flag}`)
  }
  if (values.capture) args.push("--capture", values.capture)
  if (withClient) {
    args.push("--client", values.client ?? "0")
    if (values["data-root"]) args.push("--data-root", values["data-root"])
    if (values.size) args.push("--size", values.size)
  }
  if (appArgs.length) args.push("--", ...appArgs)

  let proc = Bun.spawn([flux, script.path, ...args], {
    stdio: ["ignore", "inherit", "inherit"],
    env: {
      ...process.env,
      SRT_PLATFORM_DIR: dirname(flux),
      SRT_BUN: process.execPath,
      SRT_CLI: fileURLToPath(new URL("..", import.meta.url)),
    },
  })
  // The server ends itself on these (drops its record, stops the client);
  // this process just relays them and waits.
  let relay = () => proc.kill("SIGTERM")
  process.on("SIGINT", relay)
  process.on("SIGTERM", relay)
  let code = await proc.exited
  if (script.temp) {
    try {
      unlinkSync(script.path)
    } catch {}
  }
  process.exit(code)
}

// -- Dispatch --

const COMMANDS = ["init", "bundle", "check", "pack", "render", "client", "android", "mcp"] as const
type Command = (typeof COMMANDS)[number]

if (command === "server") {
  await launchServer(false)
} else if (command === "run") {
  await launchServer(true)
} else if (command !== undefined && (COMMANDS as readonly string[]).includes(command)) {
  let { main } = await import(`./${command as Command}/main`)
  await main()
} else {
  hint(command === undefined ? undefined : `Unknown command "${command}"`)
}
