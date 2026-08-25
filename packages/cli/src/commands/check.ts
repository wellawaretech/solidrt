import { existsSync, statSync } from "node:fs"
import { Glob } from "bun"
import { join, resolve } from "node:path"
import { source } from "../args"
import { bundleWith } from "../bundler"
import { findProject } from "../project"
import { findProjectRoot, reportTypes, typecheck } from "../typecheck"

// srt check: verify the app without side effects. Bundles in memory (nothing
// written, so no dev-server reload fires and no build outputs land in the
// project) and typechecks with the project's own tsc (typecheck.ts).

// Check one entry: bundle in memory, then typecheck. Returns whether it passed.
async function checkEntry(entry: string): Promise<boolean> {
  let failed = false
  // check verifies trees of entries from one cwd, so it is the one command
  // that walks up from each entry to its project.
  let result = await bundleWith({ entry, dev: true, minify: false, project: findProject(entry)?.dir ?? null })
  if (!result) {
    // bundleWith already printed the compile errors.
    failed = true
  }
  let root = findProjectRoot(entry)
  if (!root) {
    console.warn("Typecheck skipped: no tsconfig.json or package.json above the entry")
  } else {
    let types = await typecheck(root, entry)
    if (types && reportTypes(types)) failed = true
  }
  return !failed
}

// The entries `srt check <folder>` covers, relative to the folder (a bare
// `srt check` is `srt check .`): the app itself, its own examples, and in
// a monorepo every example app and package example. The same set CI
// gates, so one call at the repo root answers "did I break any example"
// before pushing. Entries, not files: a source no entry imports is not
// checked.
const CHECK_ALL_GLOBS = ["src/index.tsx", "examples/*.tsx", "examples/*/src/index.tsx", "packages/*/examples/*.tsx"]
function discoverEntries(root: string): string[] {
  let entries: string[] = []
  for (let pattern of CHECK_ALL_GLOBS) {
    entries.push(...[...new Glob(pattern).scanSync({ cwd: root })].map((e) => join(root, e)))
  }
  return entries.sort()
}

export async function runCheckCommand() {
  let target = source ?? "."
  if (!existsSync(target)) {
    // Without this, the missing file surfaces later as an internal ENOENT
    // stack trace (scandir/Bun.build), which reads as a CLI bug - the common
    // cause is just running from the wrong directory.
    console.error(`No such entry: ${target} (resolved from ${process.cwd()})`)
    process.exit(1)
  }
  if (!statSync(target).isDirectory()) {
    if (!(await checkEntry(target))) process.exit(1)
    console.log("Check passed")
    process.exit(0)
  }

  let entries = discoverEntries(target)
  if (entries.length === 0) {
    console.error(`No entries found under ${resolve(target)} (looked for ${CHECK_ALL_GLOBS.join(", ")})`)
    process.exit(1)
  }
  let failures: string[] = []
  for (let entry of entries) {
    console.log(`== ${entry}`)
    if (!(await checkEntry(entry))) failures.push(entry)
  }
  if (failures.length > 0) {
    console.error(`${failures.length} of ${entries.length} entries failed:\n  ${failures.join("\n  ")}`)
    process.exit(1)
  }
  console.log(`Check passed (${entries.length} entries)`)
  process.exit(0)
}
