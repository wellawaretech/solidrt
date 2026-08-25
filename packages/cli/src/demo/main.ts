import { readdirSync } from "node:fs"
import { join, resolve } from "node:path"
import { source } from "../lib/args"

// srt demo: the demos the installed @solidrt packages ship. A package's
// demos are ONE project (`<package>/demos/`: package.json, one assets/, and
// src/<name>.tsx per demo), so a demo runs as the project it lives in - this
// file only lists and resolves, and main.ts starts the ordinary dev server
// with its cwd set to that project. Nothing downstream knows about demos.

export type Demo = { name: string; cwd: string; entry: string }

const SCOPE = join("node_modules", "@solidrt")

/** Every demo installed here, sorted so the printed numbers are stable.
 * The cwd and nothing above it - the same rule the server's mode resolution
 * follows (server/mode.ts), so this lists what THIS project installed. */
function discover(): Demo[] {
  let demos: Demo[] = []
  for (let pkg of names(SCOPE)) {
    let dir = join(SCOPE, pkg, "demos")
    for (let file of names(join(dir, "src"))) {
      if (!file.endsWith(".tsx")) continue
      demos.push({ name: `${pkg}/${file.slice(0, -".tsx".length)}`, cwd: resolve(dir), entry: join("src", file) })
    }
  }
  return demos
}

// A missing folder is the normal case (most packages ship no demos), so it
// reads as an empty one rather than an error.
function names(dir: string): string[] {
  try {
    return readdirSync(dir).sort()
  } catch {
    return []
  }
}

function list(demos: Demo[]) {
  for (let [i, demo] of demos.entries()) console.log(`  ${String(i + 1).padStart(2)}  ${demo.name}`)
}

/** Lists (no argument) or resolves one demo for the dev server to serve. */
export async function main(): Promise<{ cwd: string; entry: string } | undefined> {
  let demos = discover()
  if (demos.length === 0) {
    console.error(`No demos installed in ${process.cwd()} (looked in ${SCOPE}/*/demos/src/)`)
    process.exit(1)
  }

  if (source === undefined) {
    list(demos)
    console.log("\nRun one with: srt demo <number>")
    return undefined
  }

  // A number picks from the printed list; the qualified name it printed
  // works too, so a demo can be named in a script without a fixed index.
  let picked = /^\d+$/.test(source) ? demos[Number(source) - 1] : demos.find((d) => d.name === source)
  if (!picked) {
    console.error(`No such demo: ${source}`)
    list(demos)
    process.exit(1)
  }
  return { cwd: picked.cwd, entry: picked.entry }
}
