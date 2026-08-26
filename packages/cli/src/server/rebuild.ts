import { command } from "flux:subprocess"
import { dir, file } from "flux:fs"
import { state } from "./state"
import { armWatcher } from "./watcher"
import type { BundleOutput } from "../types/bundle"

// The single rebuild-and-push path: the initial bundle at start, every
// MCP reload and every reload-on-save go through here. The bundle itself
// runs in a bun subprocess (`srt bundle --json`, the one thing only bun can
// do); this side latches the result for late-joining clients, broadcasts
// it, and re-arms the watcher from the bundle's inputs (watcher.ts).

// Build the reload message for the client protocol. `manifest` is the
// bundle's version manifest JSON string; when present, clients install the
// push into their version store before applying it (absent for the
// build-failure trigger, which must never be installed). proxyHttp is a
// message flag, not a build input.
function buildReload(code: string, manifest?: string) {
  let config = state.config
  return {
    type: "reload",
    proxyHttp: config.proxyHttp,
    args: config.args,
    ...(manifest ? { manifest } : {}),
    code,
  }
}

// Reload code that fails to start the engine on purpose. The runtime treats a
// startup error like any app that never called render() and falls back to its
// baked-in BSOD screen, so a build that doesn't compile shows the BSOD instead
// of leaving the previous app frozen on screen.
const BSOD_TRIGGER = `throw new Error("SolidRT: build failed")`

function latchAndSend(text: string) {
  state.currentReload = text
  for (let ws of state.clients.keys()) ws.send(text)
}

/** Latch the build-failure trigger and push it, so every client (and any
 * that connects later) shows the BSOD instead of a stale app. */
export function showBuildFailure() {
  state.currentMaps = null
  latchAndSend(JSON.stringify(buildReload(BSOD_TRIGGER)))
}

// Rebuild from config.entry via `srt bundle --json` (a bun subprocess run
// in config.cwd with config.entryArgs, so it resolves the mode this server
// did), then latch (for late-joining clients) and broadcast the reload to
// every connected client. Resolves with an error message on failure (a build
// error), or null on success.
export async function rebuildAndBroadcast(): Promise<string | null> {
  let config = state.config
  let args = [...config.srt.slice(1), "bundle", "--json", "--dev", ...config.entryArgs]
  if (config.minify) args.push("--minify")

  let result = await command(config.srt[0]!, args, { cwd: config.cwd }).output()
  if (!result.success) {
    let stderr = typeof result.stderr === "string" ? result.stderr : ""
    armWatcher(null)
    return `Rebuild failed:\n${stderr.trim()}`
  }

  // The bundle is one JSON object on stdout (types/bundle.d.ts).
  let bundle: BundleOutput
  try {
    bundle = JSON.parse(typeof result.stdout === "string" ? result.stdout : "")
  } catch {
    armWatcher(null)
    return "Rebuild failed: unreadable bundler output"
  }
  armWatcher(bundle.inputs ?? null)
  // Isolate bundles are manifest assets clients fetch from our /isolates/
  // route (served from cacheDir), so they must be on disk before the push.
  let maps: Record<string, string> = {}
  if (bundle.map) maps.main = bundle.map
  for (let isolate of bundle.isolates ?? []) {
    let path = `${config.cacheDir}/isolates/${isolate.id}.js`
    await dir(path.slice(0, path.lastIndexOf("/"))).create()
    await file(path).write(isolate.code)
    if (isolate.map) maps[isolate.id] = isolate.map
  }
  state.currentMaps = Object.keys(maps).length ? maps : null
  latchAndSend(JSON.stringify(buildReload(bundle.code ?? "", bundle.manifest)))
  return null
}
