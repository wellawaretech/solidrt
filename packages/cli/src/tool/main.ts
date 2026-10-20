import { existsSync, readdirSync, unlinkSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, relative, resolve } from "node:path"
import { source, toolArgs } from "../lib/args"
import { buildFluxScript, fluxScriptEnv } from "../lib/flux-script"
import { requireBinary } from "../lib/util"
import { hostOf, isScriptPath, isToolFile, nameOf } from "./pick"
import type { Host } from "./pick"

// sol tool: the tools the installed @solidrt packages ship - build-time
// helpers that belong to an extension, not to core (a model converter in
// @solidrt/3d, say) - and a project's own scripts. Discovery of the
// installed ones is by convention, like demos: every
// `<package>/tools/<name>.ts` is a tool named `<package>/<name>`, run in
// the caller's cwd with the arguments after the tool name passed through
// untouched. A project's script is named by its path (`sol tool
// ./scripts/bake.flux.ts`): a source ending in `.ts` is a file, never an
// installed tool's name (a name is its file without the suffix), so the
// two forms cannot collide. sol knows nothing about what a tool does; a
// tool prints its own usage.
//
// A tool runs under bun, unless its file is `<name>.flux.ts`: that one
// runs under the flux binary, for a tool that needs what only the runtime
// has (flux:image's texture encoder, say). The name is the whole opt-in,
// imports are not scanned, and the tool is listed as `<package>/<name>`
// either way. A project's script follows the same suffix rule, so the
// bake a package would ship as a tool and the one a project keeps for
// itself are written the same way.

type Tool = { name: string; script: string; host: Host }

const SCOPE = join("node_modules", "@solidrt")

/** Every tool installed here, sorted so the listing is stable. The cwd and
 * nothing above it - the same rule demos follow, so this lists what THIS
 * project installed. */
function discover(): Tool[] {
  let tools: Tool[] = []
  for (let pkg of names(SCOPE)) {
    let dir = join(SCOPE, pkg, "tools")
    for (let file of names(dir)) {
      if (!isToolFile(file)) continue
      tools.push({ name: `${pkg}/${nameOf(file)}`, script: resolve(dir, file), host: hostOf(file) })
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

/** A project's own script as a tool: named by its path as given, run
 * under the host its suffix picks. */
function scriptTool(source: string): Tool {
  return { name: source, script: resolve(source), host: hostOf(source) }
}

function list(tools: Tool[]) {
  for (let tool of tools) console.log(`  ${tool.name}`)
}

export async function main(): Promise<void> {
  if (source !== undefined && isScriptPath(source)) {
    let tool = scriptTool(source)
    if (!existsSync(tool.script)) {
      console.error(`No such script: ${relative(process.cwd(), tool.script) || tool.script}`)
      process.exit(1)
    }
    process.exit(await run(tool))
  }

  let tools = discover()
  if (tools.length === 0) {
    console.error(`No tools installed in ${process.cwd()} (looked in ${SCOPE}/*/tools/)`)
    console.error("A script of the project runs by its path: sol tool ./scripts/<name>.ts [arguments]")
    process.exit(1)
  }

  if (source === undefined) {
    list(tools)
    console.log("\nRun one with: sol tool <pkg>/<name> [arguments]")
    console.log("A script of the project runs by its path: sol tool ./scripts/<name>.ts [arguments]")
    return
  }

  let picked = tools.find((t) => t.name === source)
  if (!picked) {
    console.error(`No such tool: ${source}`)
    list(tools)
    process.exit(1)
  }

  process.exit(await run(picked))
}

function run(tool: Tool): Promise<number> {
  return tool.host === "flux" ? runUnderFlux(tool) : runUnderBun(tool)
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
  let script = resolve(tmpdir(), `sol-tool-${process.pid}.js`)
  await buildFluxScript(tool.script, script, `the tool ${tool.name}`)
  try {
    let proc = Bun.spawn([flux, script, ...toolArgs], {
      stdio: ["inherit", "inherit", "inherit"],
      env: fluxScriptEnv(flux),
    })
    return await proc.exited
  } finally {
    unlinkSync(script)
  }
}
