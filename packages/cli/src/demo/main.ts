import { readdirSync } from "node:fs"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { source } from "../lib/args"

// srt demo: the package demos the CLI ships pre-bundled (dist/demos/, built
// by `make -C packages/cli demos`: the release workflow before publishing, a
// checkout after editing a demo or its package). dist/demos/<pkg>/ is the
// package's demos project as the dev server serves it - package.json,
// assets/, and <slug>/<slug>.srt.js per demo - so a demo runs as the project
// it lives in: this file only lists and resolves, and main.ts starts the
// ordinary dev server with its cwd set to that project. Nothing downstream
// knows about demos, and a demo shows up in the console like any app.

export type Demo = { name: string; cwd: string; entry: string }

const DEMOS = fileURLToPath(new URL("../../dist/demos", import.meta.url))

/** Every demo the CLI ships, sorted so the printed numbers are stable. */
function discover(): Demo[] {
  let demos: Demo[] = []
  for (let pkg of names(DEMOS)) {
    let cwd = join(DEMOS, pkg)
    for (let slug of names(cwd)) {
      let entry = join(slug, `${slug}.srt.js`)
      if (names(join(cwd, slug)).includes(`${slug}.srt.js`)) demos.push({ name: `${pkg}/${slug}`, cwd, entry })
    }
  }
  return demos
}

// A missing folder reads as an empty one: a checkout without a demos build
// has none, and assets/ is a sibling of the demo dirs, not one of them.
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
    console.error("Demos not built: run make -C packages/cli demos")
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
