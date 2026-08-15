import { command } from "flux:subprocess"
import { dir, file } from "flux:fs"
import { state } from "./state"

// Server-owned "rebuild and push": the single place the running app is rebuilt
// from source on demand (an MCP reload). The srt repl still bundles in-process
// for its own keystroke reloads, but both routes call the same bundle-cli, so
// the bundling logic cannot drift. Making the server the rebuild authority is
// the interim step toward folding the whole CLI into flux (see
// okf/backlog/cli-flux-migration.md).

// Build the reload message the same way srt's buildReload does, so a
// server-triggered reload is indistinguishable from a repl-triggered one to
// clients. proxyHttp is a message flag, not a build input.
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

// Rebuild from state.config.entry via the external bundle-cli subprocess, then
// latch (for late-joining clients) and broadcast the reload to every connected
// client. Resolves with an error message on failure (no entry configured, or a
// build error), or null on success.
export async function rebuildAndBroadcast(): Promise<string | null> {
  let config = state.config
  if (!config.entry) {
    return "No app entry to rebuild. Start srt with a source file (srt run src/index.tsx)."
  }

  let params = JSON.stringify({
    entry: config.entry,
    devBase: state.serverUrl,
    dev: true,
    minify: config.minify,
  })

  let result = await command(config.bundlerCmd[0]!, [...config.bundlerCmd.slice(1), params]).output()
  if (!result.success) {
    let stderr = typeof result.stderr === "string" ? result.stderr : ""
    return `Rebuild failed:\n${stderr.trim()}`
  }

  // bundle-cli writes one JSON object { code, map, manifest, isolates } to stdout.
  let bundle: { code?: string; map?: string | null; manifest?: string; isolates?: { id: string; code: string }[] }
  try {
    bundle = JSON.parse(typeof result.stdout === "string" ? result.stdout : "")
  } catch {
    return "Rebuild failed: unreadable bundler output"
  }
  // Isolate bundles are manifest assets clients fetch from our /isolates/
  // route (served from cacheDir), so they must be on disk before the push.
  for (let isolate of bundle.isolates ?? []) {
    let path = `${config.cacheDir}/isolates/${isolate.id}.js`
    await dir(path.slice(0, path.lastIndexOf("/"))).create()
    await file(path).write(isolate.code)
  }
  state.currentMap = bundle.map ?? null
  let text = JSON.stringify(buildReload(bundle.code ?? "", bundle.manifest))
  state.currentReload = text
  for (let ws of state.clients.keys()) ws.send(text)
  return null
}
