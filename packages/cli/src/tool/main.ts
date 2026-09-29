import { readdirSync, unlinkSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { source, toolArgs } from "../lib/args"
import { buildFluxScript } from "../lib/flux-script"
import { requireBinary } from "../lib/util"

// srt tool: the tools the installed @solidrt packages ship - build-time
// helpers that belong to an extension, not to core (a model converter in
// @solidrt/3d, say). Discovery is by convention, like demos: every
// `<package>/tools/<name>.ts` is a tool named `<package>/<name>`, run in
// the caller's cwd with the arguments after the tool name passed through
// untouched. srt knows nothing about what a tool does; a tool prints its
// own usage.
//
// A tool runs under bun, unless its file is `<name>.flux.ts`: that one
// runs under the flux binary, for a tool that needs what only the runtime
// has (flux:image's texture encoder, say). The name is the whole opt-in,
// imports are not scanned, and the tool is listed as `<package>/<name>`
// either way.

type Tool = { name: string; script: string; host: "bun" | "flux" }

const SCRIPT_SUFFIX = ".ts"
const FLUX_SUFFIX = ".flux.ts"

const SCOPE = join("node_modules", "@solidrt")

/** Every tool installed here, sorted so the listing is stable. The cwd and
 * nothing above it - the same rule demos follow, so this lists what THIS
 * project installed. */
function discover(): Tool[] {
  let tools: Tool[] = []
  for (let pkg of names(SCOPE)) {
    let dir = join(SCOPE, pkg, "tools")
    for (let file of names(dir)) {
      if (file.endsWith(".d.ts") || !file.endsWith(SCRIPT_SUFFIX)) continue
      let flux = file.endsWith(FLUX_SUFFIX)
      let name = file.slice(0, -(flux ? FLUX_SUFFIX : SCRIPT_SUFFIX).length)
      tools.push({ name: `${pkg}/${name}`, script: resolve(dir, file), host: flux ? "flux" : "bun" })
    }
  }
  return tools
}

// A missing folder is the normal case (most packages ship no tools), so it
// reads as an empty one rather than an error.
function names(dir: string): string[] {
  try {
    return readdirSync(dir).sort()
  } catch {
    return []
  }
}

function list(tools: Tool[]) {
  for (let tool of tools) console.log(`  ${tool.name}`)
}

export async function main(): Promise<void> {
  let tools = discover()
  if (tools.length === 0) {
    console.error(`No tools installed in ${process.cwd()} (looked in ${SCOPE}/*/tools/)`)
    process.exit(1)
  }

  if (source === undefined) {
    list(tools)
    console.log("\nRun one with: srt tool <pkg>/<name> [arguments]")
    return
  }

  let picked = tools.find((t) => t.name === source)
  if (!picked) {
    console.error(`No such tool: ${source}`)
    list(tools)
    process.exit(1)
  }

  process.exit(picked.host === "flux" ? await runUnderFlux(picked) : await runUnderBun(picked))
}

async function runUnderBun(tool: Tool): Promise<number> {
  let proc = Bun.spawn([process.execPath, tool.script, ...toolArgs], {
    stdio: ["inherit", "inherit", "inherit"],
  })
  return proc.exited
}

// The tool bundled into a temp file (lib/flux-script.ts), run by the flux
// binary with the environment the dev server gets, the file removed after.
async function runUnderFlux(tool: Tool): Promise<number> {
  let flux = requireBinary("flux")
  let script = resolve(tmpdir(), `srt-tool-${process.pid}.js`)
  await buildFluxScript(tool.script, script, `the tool ${tool.name}`)
  try {
    let proc = Bun.spawn([flux, script, ...toolArgs], {
      stdio: ["inherit", "inherit", "inherit"],
      env: {
        ...process.env,
        SRT_PLATFORM_DIR: dirname(flux),
        SRT_BUN: process.execPath,
        SRT_CLI: fileURLToPath(new URL("../..", import.meta.url)),
      },
    })
    return await proc.exited
  } finally {
    unlinkSync(script)
  }
}
