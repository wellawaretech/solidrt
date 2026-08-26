import { readdirSync } from "node:fs"
import { join, resolve } from "node:path"
import { source, toolArgs } from "../lib/args"

// srt tool: the tools the installed @solidrt packages ship - build-time
// helpers that belong to an extension, not to core (a model converter in
// @solidrt/3d, say). Discovery is by convention, like demos: every
// `<package>/tools/<name>.ts` is a tool named `<package>/<name>`, run with
// bun in the caller's cwd with the arguments after the tool name passed
// through untouched. srt knows nothing about what a tool does; a tool
// prints its own usage.

type Tool = { name: string; script: string }

const SCOPE = join("node_modules", "@solidrt")

/** Every tool installed here, sorted so the listing is stable. The cwd and
 * nothing above it - the same rule demos follow, so this lists what THIS
 * project installed. */
function discover(): Tool[] {
  let tools: Tool[] = []
  for (let pkg of names(SCOPE)) {
    let dir = join(SCOPE, pkg, "tools")
    for (let file of names(dir)) {
      if (!file.endsWith(".ts")) continue
      tools.push({ name: `${pkg}/${file.slice(0, -".ts".length)}`, script: resolve(dir, file) })
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

  let proc = Bun.spawn([process.execPath, picked.script, ...toolArgs], {
    stdio: ["inherit", "inherit", "inherit"],
  })
  process.exit(await proc.exited)
}
