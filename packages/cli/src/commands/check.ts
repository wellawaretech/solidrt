import { existsSync, mkdirSync, rmSync } from "node:fs"
import { Glob } from "bun"
import { dirname, join, resolve } from "node:path"
import { source } from "../args"
import { bundleWith, findIsolateModules } from "../bundler"

// srt check: verify the app without side effects. Bundles in memory (nothing
// written, so no dev-server reload fires and no build outputs land in the
// project) and typechecks with the project's own tsc, reporting only
// diagnostics in app code. @solidrt packages ship raw .ts sources, so a strict
// consumer config surfaces their internal errors too; those are counted and
// hidden, not the caller's problem to wade through.

// Walk up from the entry to the enclosing project (tsconfig.json or, failing
// that, package.json).
export function findProjectRoot(entry: string): string | null {
  let dir = dirname(resolve(entry))
  let byConfig: string | null = null
  let byPackage: string | null = null
  while (true) {
    if (!byConfig && existsSync(join(dir, "tsconfig.json"))) byConfig = dir
    if (!byPackage && existsSync(join(dir, "package.json"))) byPackage = dir
    let parent = dirname(dir)
    if (parent === dir) return byConfig ?? byPackage
    dir = parent
  }
}

// One tsc --pretty false diagnostic: the "path(line,col): error TS...: ..."
// head line plus any indented continuation lines.
type Diagnostic = { head: string; lines: string[]; inDependencies: boolean }

function parseDiagnostics(output: string): Diagnostic[] {
  let diagnostics: Diagnostic[] = []
  let current: Diagnostic | null = null
  for (let line of output.split("\n")) {
    let head = /^(.*?)\(\d+,\d+\): (error|warning) TS\d+: /.exec(line) ?? /^(error|warning) TS\d+: /.exec(line)
    if (head) {
      let file = line.includes("): ") ? head[1]! : ""
      current = { head: line, lines: [line], inDependencies: file.includes("node_modules") }
      diagnostics.push(current)
    } else if (current && line.trim() !== "") {
      current.lines.push(line)
    }
  }
  return diagnostics
}

// The project's tsc, found by walking up from the project root: an example
// app can carry a tsconfig without its own node_modules (monorepo case).
function findTsc(fromDir: string): string | null {
  let dir = fromDir
  while (true) {
    let tsc = join(dir, "node_modules", ".bin", process.platform === "win32" ? "tsc.exe" : "tsc")
    if (existsSync(tsc)) return tsc
    let parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

// Typecheck the entry's program, not the enclosing project: a transient
// config extends the project's tsconfig and roots the program at the entry
// alone (plus the project's ambient declarations), so tsc checks exactly the
// entry's import closure - unrelated files are excluded by construction. The
// config lives in the project-local .srt-data (the dev-artifact dir; absolute
// paths inside, so its location only matters for type-package resolution,
// which walks up to the project's node_modules from there).
export async function typecheck(root: string, entry: string): Promise<{ app: Diagnostic[]; hidden: number } | null> {
  let tsconfig = join(root, "tsconfig.json")
  if (!existsSync(tsconfig)) {
    console.warn("Typecheck skipped: no tsconfig.json above the entry")
    return null
  }
  let tsc = findTsc(root)
  if (!tsc) {
    console.warn("Typecheck skipped: no tsc in the project (add the typescript devDependency)")
    return null
  }
  let dataDir = join(root, ".srt-data")
  mkdirSync(dataDir, { recursive: true })
  let config = join(dataDir, `typecheck-${process.pid}.tsconfig.json`)
  // The include narrows the inherited one (files and include are unioned, so
  // without it a base config's include would drag the whole project back into
  // the program) down to declaration files only. Those are the one thing the
  // entry's import closure cannot reach: an ambient `declare module "*.glsl"`
  // applies precisely because nothing imports it, so entry-only rooting would
  // silently drop it and every asset import would fail with TS2307. The
  // pattern is relative to this config, which sits one level under the root.
  // Isolate modules are program roots of their own: main reaches them by
  // `import type` at most, and one nothing imports would otherwise go
  // unchecked.
  let files = [resolve(entry), ...findIsolateModules(dirname(resolve(entry))).map((m) => m.path)]
  await Bun.write(config, JSON.stringify({ extends: tsconfig, include: ["../**/*.d.ts"], files }))
  try {
    let proc = Bun.spawn([tsc, "-p", config, "--noEmit", "--pretty", "false"], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    })
    let [out, err] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text()])
    await proc.exited
    let diagnostics = parseDiagnostics(out + err)
    let app = diagnostics.filter((d) => !d.inDependencies)
    return { app, hidden: diagnostics.length - app.length }
  } finally {
    rmSync(config, { force: true })
  }
}

// Print a typecheck result (diagnostics, then the one-line verdict) and return
// whether app-code errors were found. Callers pass repl-aware printers when
// the output lands over a live prompt (the dev-server startup check).
export function reportTypes(
  types: { app: Diagnostic[]; hidden: number },
  log: (...args: any[]) => void = console.log,
  error: (...args: any[]) => void = console.error,
): boolean {
  for (let d of types.app) error(d.lines.join("\n"))
  if (types.app.length > 0) {
    let hidden = types.hidden > 0 ? ` (${types.hidden} in dependencies hidden)` : ""
    error(`${types.app.length} type error${types.app.length === 1 ? "" : "s"} in app code${hidden}`)
    return true
  }
  if (types.hidden > 0) log(`Types OK (${types.hidden} dependency-internal errors hidden)`)
  else log("Types OK")
  return false
}

// Check one entry: bundle in memory, then typecheck. Returns whether it passed.
async function checkEntry(entry: string): Promise<boolean> {
  let failed = false
  let result = await bundleWith({ entry, dev: true, minify: false })
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

// The app entries a bare `srt check` covers, relative to the cwd: every
// example app and every package example. The same set CI gates, so one call
// here answers "did I break any example" before pushing.
const CHECK_ALL_GLOBS = ["examples/*/src/index.tsx", "packages/*/examples/*.tsx"]
function discoverEntries(): string[] {
  let entries: string[] = []
  for (let pattern of CHECK_ALL_GLOBS) entries.push(...new Glob(pattern).scanSync({ cwd: process.cwd() }))
  return entries.sort()
}

export async function runCheckCommand() {
  if (source) {
    let entry = source
    if (!existsSync(entry)) {
      // Without this, the missing file surfaces later as an internal ENOENT
      // stack trace (scandir/Bun.build), which reads as a CLI bug - the common
      // cause is just running from the wrong directory.
      console.error(`No such entry: ${entry} (resolved from ${process.cwd()})`)
      process.exit(1)
    }
    if (!(await checkEntry(entry))) process.exit(1)
    console.log("Check passed")
    process.exit(0)
  }

  let entries = discoverEntries()
  if (entries.length === 0) {
    console.error(`No entries found under ${process.cwd()} (looked for ${CHECK_ALL_GLOBS.join(", ")})`)
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
