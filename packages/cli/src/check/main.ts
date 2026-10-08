import { existsSync, statSync } from "node:fs"
import { Glob } from "bun"
import { join, relative, resolve } from "node:path"
import { source } from "../lib/args"
import { bundleWith } from "../bundle/bundler"
import { findProject } from "../lib/project"
import type { Project } from "../lib/project"
import { findProjectRoot, reportTypes, typecheck } from "./typecheck"

// sol check: verify the app without side effects. Bundles in memory (nothing
// written, so no dev-server reload fires and no build outputs land in the
// project) and typechecks with the project's own tsc (typecheck.ts).

// The CLI's own commands are bun programs (bin/sol runs them under bun), so
// they bundle for bun, where their node: imports are builtins; its server
// folder is a flux script and bundles like app code. Everything else the
// check covers is app code.
function bundleTarget(entry: string, project: Project | null): "browser" | "bun" {
  if (project?.name !== "@solidrt/cli") return "browser"
  let inside = relative(project.dir, resolve(entry))
  return inside.startsWith(join("src", "server")) ? "browser" : "bun"
}

// Check one entry: bundle in memory, then typecheck. Returns whether it passed.
async function checkEntry(entry: string): Promise<boolean> {
  let failed = false
  // check verifies trees of entries from one cwd, so it is the one command
  // that walks up from each entry to its project.
  let project = findProject(entry)
  let result = await bundleWith({ entry, dev: true, minify: false, project: project?.dir ?? null, target: bundleTarget(entry, project) })
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

// The entries `sol check <folder>` covers, relative to the folder (a bare
// `sol check` is `sol check .`): the app itself, its own examples and test
// files, and in a monorepo every example app, package example, package demo
// and package test file, the flux module tests under flux/tests/, and the
// CLI's own command modules (its router loads them on demand, so the
// router's import closure reaches none of them). The same set CI gates, so
// one call at the repo root answers "did I break any example" before
// pushing. Entries, not files: a source no entry imports is not checked.
const CHECK_ALL_GLOBS = [
  "src/index.tsx",
  "examples/*.tsx",
  "examples/*/src/index.tsx",
  "packages/*/examples/*.tsx",
  "packages/*/demos/src/*.tsx",
  "tests/*.test.{ts,tsx}",
  "packages/*/tests/*.test.{ts,tsx}",
  "packages/*/tests/fixtures/*/tests/*.test.{ts,tsx}",
  "flux/tests/*.test.ts",
  "packages/cli/src/main.ts",
  "packages/cli/src/*/main.ts",
]
function discoverEntries(root: string): string[] {
  let entries: string[] = []
  for (let pattern of CHECK_ALL_GLOBS) {
    entries.push(...[...new Glob(pattern).scanSync({ cwd: root })].map((e) => join(root, e)))
  }
  return entries.sort()
}

export async function main() {
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
