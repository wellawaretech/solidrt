// Where the server finds what it spawns (okf/done/sol-command-folders.md):
//
// - the platform binaries (solidrt-go) in SOLIDRT_PLATFORM_DIR, which sol sets to
//   the platform package it resolved, else a checkout's SOLIDRT_HOME/dist/<triple>;
// - sol itself, run as `bun <SOLIDRT_CLI>/bin/sol`, so the bundle and the startup
//   typecheck run by command name (`sol bundle --json`, `sol check`): SOLIDRT_CLI
//   is the @solidrt/cli package root (sol sets it; a checkout's is
//   SOLIDRT_HOME/packages/cli) and SOLIDRT_BUN the bun to use (sol's own; else PATH).
//
// So `flux server.js` started by hand needs SOLIDRT_HOME (a built checkout) or
// the three variables, and started by sol needs nothing.

import { file } from "flux:fs"
import { join } from "flux:path"
import { arch, env, platform } from "flux:process"
import { fail } from "./args"

// Host platform -> dist triple, as src/artifacts.ts maps it for bun.
const TRIPLES: Record<string, string> = {
  "linux-x64": "linux-x64-gnu",
  "linux-arm64": "linux-arm64-gnu",
  "darwin-arm64": "darwin-arm64",
  "win32-x64": "win32-x64-msvc",
}

function platformDir(): string | null {
  if (env.SOLIDRT_PLATFORM_DIR) return env.SOLIDRT_PLATFORM_DIR
  let triple = TRIPLES[`${platform}-${arch}`]
  return env.SOLIDRT_HOME && triple ? join(env.SOLIDRT_HOME, "dist", triple) : null
}

/**
 * The absolute path of a platform binary, or a failed launch. `runtime` is
 * the project's own runtime directory (mode.ts), tried first for the app
 * runtime binaries; a host it was not built for falls back to the stock
 * binary with a notice.
 */
export async function requireBinary(name: string, runtime: string | null = null): Promise<string> {
  let fileName = name + (platform === "win32" ? ".exe" : "")
  let triple = TRIPLES[`${platform}-${arch}`]
  if (runtime && triple) {
    let own = join(runtime, triple, fileName)
    if (await file(own).exists()) return own
    console.log(`[cli] No ${name} for ${triple} in ${runtime}; using the stock one`)
  }
  let dir = platformDir()
  let path = dir ? join(dir, fileName) : null
  if (path && (await file(path).exists())) return path
  fail(
    `Could not find the ${name} binary${dir ? ` in ${dir}` : ""}. Run through sol, or set SOLIDRT_HOME to a SolidRT checkout built with make client.`,
  )
}

/** The sol command prefix, `[bun, <cli>/bin/sol]`, for the bun-side commands. */
export function solCommand(): string[] {
  let cli = env.SOLIDRT_CLI ?? (env.SOLIDRT_HOME ? join(env.SOLIDRT_HOME, "packages", "cli") : undefined)
  if (!cli) fail("Could not find sol: set SOLIDRT_CLI to the @solidrt/cli package root, or SOLIDRT_HOME to a SolidRT checkout.")
  return [env.SOLIDRT_BUN ?? "bun", join(cli, "bin", "sol")]
}
