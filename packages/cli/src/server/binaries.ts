// Where the server finds what it spawns (okf/done/srt-command-folders.md):
//
// - the platform binaries (solidrt-go) in SRT_PLATFORM_DIR, which srt sets to
//   the platform package it resolved, else a checkout's SRT_HOME/dist/<triple>;
// - srt itself, run as `bun <SRT_CLI>/bin/srt`, so the bundle and the startup
//   typecheck run by command name (`srt bundle --json`, `srt check`): SRT_CLI
//   is the @solidrt/cli package root (srt sets it; a checkout's is
//   SRT_HOME/packages/cli) and SRT_BUN the bun to use (srt's own; else PATH).
//
// So `flux server.js` started by hand needs SRT_HOME (a built checkout) or
// the three variables, and started by srt needs nothing.

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
  if (env.SRT_PLATFORM_DIR) return env.SRT_PLATFORM_DIR
  let triple = TRIPLES[`${platform}-${arch}`]
  return env.SRT_HOME && triple ? join(env.SRT_HOME, "dist", triple) : null
}

/** The absolute path of a platform binary, or a failed launch. */
export async function requireBinary(name: string): Promise<string> {
  let dir = platformDir()
  let path = dir ? join(dir, name + (platform === "win32" ? ".exe" : "")) : null
  if (path && (await file(path).exists())) return path
  fail(
    `Could not find the ${name} binary${dir ? ` in ${dir}` : ""}. Run through srt, or set SRT_HOME to a SolidRT checkout built with make client.`,
  )
}

/** The srt command prefix, `[bun, <cli>/bin/srt]`, for the bun-side commands. */
export function srtCommand(): string[] {
  let cli = env.SRT_CLI ?? (env.SRT_HOME ? join(env.SRT_HOME, "packages", "cli") : undefined)
  if (!cli) fail("Could not find srt: set SRT_CLI to the @solidrt/cli package root, or SRT_HOME to a SolidRT checkout.")
  return [env.SRT_BUN ?? "bun", join(cli, "bin", "srt")]
}
