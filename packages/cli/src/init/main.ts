import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { source, values } from "../lib/args"
import { multiselect, note, text } from "./prompt"

const DEFAULT_NAME = "solidrt-app"

const SCAFFOLD_DIR = join(import.meta.dir, "scaffold")
const TEMPLATES_DIR = join(SCAFFOLD_DIR, "templates")

// Shared project files written for every template. Sources live in
// cli/src/init/scaffold/. The .gitignore is stored there as `gitignore` because npm
// strips files literally named `.gitignore` from published packages, so it is
// renamed on the way out. The per-template src/ comes from scaffold/templates/.
const TEMPLATE_FILES: Array<{ from: string; to: string }> = [
  { from: "package.json", to: "package.json" },
  { from: "tsconfig.json", to: "tsconfig.json" },
  { from: "gitignore", to: ".gitignore" },
  { from: "mcp.json", to: ".mcp.json" },
  { from: "AGENTS.md", to: "AGENTS.md" },
]

// A valid npm package name derived from the target directory.
function packageName(dir: string): string {
  let name = basename(resolve(dir))
    .toLowerCase()
    .replace(/[^a-z0-9-_.]/g, "-")
  return name || "solidrt-app"
}

const DEFAULT_TEMPLATE = "default"

// One AGENTS.md serves every template, so the lines that point an agent at
// an extension's docs are fenced between `<!-- <key>:begin/end -->` markers:
// with the extension selected only the markers go, without it the block goes
// too, so an app never ships references to files that are not installed.
function resolveMarkers(text: string, extensions: Extension[]): string {
  for (let ext of EXTENSIONS) {
    let selected = extensions.includes(ext)
    let block = new RegExp(`^<!-- ${ext.key}:begin -->\\n[\\s\\S]*?^<!-- ${ext.key}:end -->\\n`, "gm")
    let marker = new RegExp(`^<!-- ${ext.key}:(?:begin|end) -->\\n`, "gm")
    text = selected ? text.replace(marker, "") : text.replace(block, "")
  }
  return text
}

// Optional packages an app can opt into on top of core. Each maps to a
// dependency in the scaffold package.json (kept when selected, removed
// otherwise), to a marker key fencing its lines in scaffold/AGENTS.md, and
// optionally to a starter under scaffold/templates/.
interface Extension {
  pkg: string
  key: string
  template?: string
  description: string
}

const EXTENSIONS: Extension[] = [
  {
    pkg: "@solidrt/components",
    key: "components",
    template: "components",
    description: "component framework: widgets, theming, navigation",
  },
  { pkg: "@solidrt/2d", key: "2d", description: "general purpose 2D library" },
  { pkg: "@solidrt/3d", key: "3d", description: "general purpose 3D library" },
]

// Resolve which extensions the app takes: an interactive picker on a TTY,
// else none (core only). Extensions are ordinary dependencies, so a script
// adds them afterwards with `bun add`.
async function resolveExtensions(): Promise<Extension[]> {
  if (!process.stdin.isTTY) return []
  // Core is the runtime every app has, so it is not a choice.
  note("@solidrt/core is always included", "Packages")
  let picked = await multiselect(
    "Select extensions",
    EXTENSIONS.map((e) => ({ label: `${e.pkg} - ${e.description}`, value: e.pkg })),
  )
  return EXTENSIONS.filter((e) => picked.includes(e.pkg))
}

// The starter src/ comes from the first selected extension that brings a
// template; with none, the core `default` starter.
function resolveTemplate(extensions: Extension[]): string {
  return extensions.find((e) => e.template)?.template ?? DEFAULT_TEMPLATE
}

export async function main() {
  // The target folder comes from the positional arg, or an interactive prompt
  // (defaulting to a suggested name) when omitted.
  let dir = source
  if (!dir) {
    dir = await text("Project name", DEFAULT_NAME)
    if (!dir) {
      console.error("!! A project name is required")
      process.exit(1)
    }
  }

  // The folder must not exist yet, so init can never touch an existing project.
  let existing = await readdir(dir).catch(() => null)
  if (existing) {
    console.error(`!! ${resolve(dir)} already exists; choose a new folder name`)
    process.exit(1)
  }

  let extensions = await resolveExtensions()
  let template = resolveTemplate(extensions)
  let summary = ["@solidrt/core", ...extensions.map((e) => e.pkg)].join(", ")

  console.log(`>> Scaffolding SolidRT project in ${resolve(dir)} (${summary})`)
  for (let { from, to } of TEMPLATE_FILES) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    let body: string | Buffer = await readFile(join(SCAFFOLD_DIR, from))
    if (to === "AGENTS.md") body = resolveMarkers(body.toString("utf8"), extensions)
    await writeFile(dest, body)
    console.log(`   Write ${to}`)
  }

  // The template's files become the project's src/. Entries may be nested
  // directories (e.g. an asset folder), so copy recursively.
  let templateDir = join(TEMPLATES_DIR, template)
  await mkdir(join(dir, "src"), { recursive: true })
  for (let file of await readdir(templateDir)) {
    await cp(join(templateDir, file), join(dir, "src", file), { recursive: true })
    console.log(`   Write src/${file}`)
  }

  // The assets/ convention folder, created up front: everything in it ships
  // with the app, and the dev watcher only picks up an assets/ folder that
  // exists when it starts. It starts with a placeholder app icon (picked up
  // through the assets/icon.svg convention) for the author to replace.
  await mkdir(join(dir, "assets"), { recursive: true })
  await writeFile(join(dir, "assets", "icon.svg"), await readFile(join(SCAFFOLD_DIR, "icon.svg")))
  console.log("   Write assets/icon.svg")

  // The scaffold package.json carries a placeholder name and every extension
  // dependency; set the name from the target folder and keep only the
  // selected extensions.
  let pkgPath = join(dir, "package.json")
  let pkg = JSON.parse(await readFile(pkgPath, "utf8"))
  pkg.name = packageName(dir)
  for (let ext of EXTENSIONS) {
    if (!extensions.includes(ext)) delete pkg.dependencies[ext.pkg]
  }
  await writeFile(pkgPath, JSON.stringify(pkg, null, 2) + "\n")

  // Deps are declared in scaffold/package.json (Solid peers resolve via
  // @solidrt/core's peerDependencies), so a plain install is enough.
  console.log("\n>> Installing dependencies")
  let install = Bun.spawnSync(["bun", "install"], {
    cwd: dir,
    stdout: "inherit",
    stderr: "inherit",
  })
  if (install.exitCode !== 0) {
    console.error("\n!! Dependency install failed; retry with `bun install` in the project")
    process.exit(1)
  }

  let prefix = dir === "." ? "" : `cd ${dir} && `
  console.log(`\n>> Done. Next:\n   ${prefix}bun run dev\n`)
  process.exit()
}
