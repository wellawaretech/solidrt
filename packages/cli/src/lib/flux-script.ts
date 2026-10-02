import { dirname } from "node:path"
import { fileURLToPath } from "node:url"

// A flux script is one plain-JS file: the flux binary runs no TypeScript
// and loads no module from disk. So a script written as TypeScript modules
// (the dev server, a package's flux tool) is bundled into one file first.
// Bun is already the bundler; the browser target keeps node builtins out,
// and the flux: capability modules stay external (the runtime provides
// them).
export async function buildFluxScript(entry: string, outfile: string, what: string): Promise<void> {
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    external: ["flux:*"],
  })
  if (!result.success) {
    console.error(`[cli] Failed to bundle ${what}:`)
    for (let log of result.logs) console.error(String(log))
    process.exit(1)
  }
  await Bun.write(outfile, result.outputs[0]!)
}

/**
 * The environment a flux script of sol runs in: the caller's, plus where
 * the script finds the platform binaries next to `flux` (the path of the
 * flux binary that runs it), the bun to run sol with, and sol itself
 * (src/server/binaries.ts reads them).
 */
export function fluxScriptEnv(flux: string): Record<string, string | undefined> {
  return {
    ...process.env,
    SOLIDRT_PLATFORM_DIR: dirname(flux),
    SOLIDRT_BUN: process.execPath,
    SOLIDRT_CLI: fileURLToPath(new URL("../..", import.meta.url)),
  }
}
