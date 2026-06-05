#!/usr/bin/env bun

import { values, command, source, isTsx, isTs, isSource, isPrebuilt, printUsage } from "./args"
import { runBundleCommand } from "./commands/bundle"
import { runPackCommand } from "./commands/pack"
import { runRecordCommand } from "./commands/record"
import { runServerCommand } from "./commands/server"
import { runClientCommand } from "./commands/client"
import { spawnClient } from "./client"

// -- Validate args --

if (command === "bundle" && (!source || (!isSource && !isPrebuilt))) {
  console.error("Usage: srt bundle [options] <entry.[tsx|jsx|ts|js|srt.js|srt.bin]>")
  process.exit(1)
}

if (command === "record" && (!source || !isTsx)) {
  console.error("Usage: srt record <entry.[tsx|jsx]>")
  process.exit(1)
}

if (command === "pack") {
  if (values.flux) {
    if (!source || !isTs) {
      console.error("Usage: srt pack --flux [options] <entry.[ts|js]>")
      process.exit(1)
    }
  } else if (!source || !isSource) {
    console.error("Usage: srt pack [options] <entry.[tsx|jsx|ts|js]>")
    process.exit(1)
  }
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
let isProdBuild = (command === "bundle" || command === "record" || command === "pack") && !values.dev
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

// -- Dispatch --

if (command === "bundle") {
  await runBundleCommand()
} else if (command === "pack") {
  await runPackCommand()
} else if (command === "record") {
  await runRecordCommand()
} else if (command === "client") {
  await runClientCommand()
} else if (command === "server") {
  await runServerCommand()
} else if (command === "run") {
  spawnClient()
  await runServerCommand()
} else {
  printUsage()
  process.exit(1)
}