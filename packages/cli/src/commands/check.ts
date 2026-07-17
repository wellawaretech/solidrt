import { existsSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { source } from "../args"
import { bundleWith } from "../bundler"

// srt check: verify the app without side effects. Bundles in memory (nothing
// written, so no dev-server reload fires and no build outputs land in the
// project) and typechecks with the project's own tsc, reporting only
// diagnostics in app code. @solidrt packages ship raw .ts sources, so a strict
// consumer config surfaces their internal errors too; those are counted and
// hidden, not the caller's problem to wade through.

// Walk up from the entry to the enclosing project (tsconfig.json or, failing
// that, package.json).
function findProjectRoot(entry: string): string | null {
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

async function typecheck(root: string): Promise<{ app: Diagnostic[]; hidden: number } | null> {
  let tsc = join(root, "node_modules", ".bin", process.platform === "win32" ? "tsc.exe" : "tsc")
  if (!existsSync(tsc)) {
    console.warn("Typecheck skipped: no tsc in the project (add the typescript devDependency)")
    return null
  }
  let proc = Bun.spawn([tsc, "--noEmit", "--pretty", "false"], { cwd: root, stdout: "pipe", stderr: "pipe" })
  let [out, err] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text()])
  await proc.exited
  let diagnostics = parseDiagnostics(out + err)
  let app = diagnostics.filter((d) => !d.inDependencies)
  return { app, hidden: diagnostics.length - app.length }
}

export async function runCheckCommand() {
  let entry = source!
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
    let types = await typecheck(root)
    if (types) {
      for (let d of types.app) console.error(d.lines.join("\n"))
      if (types.app.length > 0) {
        failed = true
        let hidden = types.hidden > 0 ? ` (${types.hidden} in dependencies hidden)` : ""
        console.error(`${types.app.length} type error${types.app.length === 1 ? "" : "s"} in app code${hidden}`)
      } else if (types.hidden > 0) {
        console.log(`Types OK (${types.hidden} dependency-internal errors hidden)`)
      } else {
        console.log("Types OK")
      }
    }
  }

  if (failed) process.exit(1)
  console.log("Check passed")
  process.exit(0)
}