import { fileURLToPath } from "node:url"
import { existsSync, unlinkSync } from "node:fs"
import { networkInterfaces, tmpdir } from "node:os"
import { resolve } from "node:path"
import { values, port, appArgs, clientStorageArgs } from "../args"
import { requireBinary } from "../util"
import { resolveMode, sourceDirOf } from "../mode"
import { liveRecords, sameKey } from "../registry"
import { serverDir } from "../dev-dir"
import { buildServerBundle } from "../server-bundle"
import type { ServerConfig } from "../../shared/config"

// `run` and `server`: decide what is served, resolve the binaries, and hand
// everything to the dev server, a flux process (packages/cli/server/) that
// owns the port, the registry record, the local client and the bundle from
// there. This process only launches it and relays the signals that end it.

// The server script the flux binary runs: the prebuilt bundle a published
// CLI ships, or (in a checkout) one built now into a temp file, removed on
// exit.
async function serverScript(): Promise<{ path: string; temp: boolean }> {
  let prebuilt = fileURLToPath(new URL("../../dist/server.js", import.meta.url))
  if (existsSync(prebuilt)) return { path: prebuilt, temp: false }
  let outfile = resolve(tmpdir(), `srt-dev-server-${process.pid}.js`)
  await buildServerBundle(outfile)
  return { path: outfile, temp: true }
}

export async function runServerCommand(withClient: boolean) {
  let mode = resolveMode()

  // One server per key: a second run in the same project (or on the same
  // file) points at the running one instead of racing it.
  let running = liveRecords().find((r) => sameKey(r.key, mode.key))
  if (running) {
    console.error(`A dev server already serves ${mode.key} on port ${running.port} (pid ${running.pid}). Stop it first, or attach a client with srt client.`)
    process.exit(1)
  }

  let flux = requireBinary("flux")
  let runner = withClient ? requireBinary("solidrt-go") : null
  let script = await serverScript()
  let dir = serverDir(mode.key)

  // The LAN address (for --lan): the server has no OS module, so it is
  // computed here and passed down.
  let lanAddress = Object.values(networkInterfaces())
    .flat()
    .find((i) => i?.family === "IPv4" && !i.internal)?.address

  // How the server rebuilds and typechecks: it cannot call Bun.build or the
  // project's tsc itself (it is a flux process), so it spawns srt's own bun
  // on the standalone bundle-cli and typecheck-cli entries (entries/). A prebuilt
  // .srt.js has no checkable program.
  let bundleCli = fileURLToPath(new URL("../entries/bundle-cli.ts", import.meta.url))
  let typecheckCli = fileURLToPath(new URL("../entries/typecheck-cli.ts", import.meta.url))
  let typecheckCmd = mode.entry.endsWith(".srt.js") ? null : [process.execPath, typecheckCli, mode.entry]

  let clientArgs = [...clientStorageArgs()]
  if (values.size) clientArgs.push("--size", values.size)

  let config: ServerConfig = {
    mode: mode.mode,
    key: mode.key,
    serverDir: dir,
    entry: mode.entry,
    sourceDir: sourceDirOf(mode),
    projectDir: mode.projectDir,
    port,
    lan: values.lan,
    address: lanAddress ?? "127.0.0.1",
    proxyHttp: values["proxy-http"],
    args: appArgs,
    minify: values.minify,
    bundlerCmd: [process.execPath, bundleCli],
    typecheckCmd,
    cache: values["proxy-http"],
    // Build outputs and the proxy cache: the project's .srt-data, or the
    // server folder for a file served on its own (nothing else owns it).
    cacheDir: mode.projectDir ? resolve(mode.projectDir, ".srt-data") : resolve(dir, "data"),
    capture: values.capture ? resolve(values.capture) : undefined,
    stats: values.stats,
    tunnel: values.tunnel,
    client: runner ? { cmd: runner, args: clientArgs } : null,
  }

  let proc = Bun.spawn([flux, script.path, JSON.stringify(config)], {
    stdio: ["ignore", "inherit", "inherit"],
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
