import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { styleText } from "node:util"
import { source, values } from "../lib/args"
import { CLI_VERSION } from "../lib/project"
import { abort, intro, multiselect, outro, spinner, step, text } from "../lib/prompt"

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
// otherwise), to a marker key fencing its lines in scaffold/AGENTS.md (also
// its label in the picker), and optionally to a starter under
// scaffold/templates/.
interface Extension {
  pkg: string
  key: string
  template?: string
  description: string
}

const EXTENSIONS: Extension[] = [
  { pkg: "@solidrt/router", key: "router", description: "declarative routing between screens" },
  { pkg: "@solidrt/components", key: "components", template: "components", description: "widgets and theming" },
  { pkg: "@solidrt/2d", key: "2d", description: "general purpose 2D library" },
  { pkg: "@solidrt/3d", key: "3d", description: "general purpose 3D library" },
]

// Resolve which extensions the app takes: an interactive picker on a TTY,
// else none (core only). Extensions are ordinary dependencies, so a script
// adds them afterwards with `bun add`.
async function resolveExtensions(): Promise<Extension[]> {
  if (!process.stdin.isTTY) return []
  // Core is the runtime every app has, so it is not offered: the picker only
  // adds to it.
  let picked = await multiselect(
    "What would you like to add to your app?",
    EXTENSIONS.map((e) => ({ label: e.key, hint: e.description, value: e.pkg })),
  )
  return EXTENSIONS.filter((e) => picked.includes(e.pkg))
}

// The starter src/ comes from the first selected extension that brings a
// template; with none, the core `default` starter.
function resolveTemplate(extensions: Extension[]): string {
  return extensions.find((e) => e.template)?.template ?? DEFAULT_TEMPLATE
}

export async function main() {
  intro(`Create a SolidRT app ${styleText("dim", CLI_VERSION)}`)

  // The target folder comes from the positional arg, or an interactive prompt
  // (defaulting to a suggested name) when omitted.
  let dir = source
  if (!dir) {
    dir = await text("Project name (target directory)", DEFAULT_NAME)
    if (!dir) abort("A project name is required")
  }

  // The folder must not exist yet, so init can never touch an existing project.
  let existing = await readdir(dir).catch(() => null)
  if (existing) abort(`${resolve(dir)} already exists; choose a new folder name`)

  let extensions = await resolveExtensions()
  let template = resolveTemplate(extensions)
  let summary = ["@solidrt/core", ...extensions.map((e) => e.pkg)].join(", ")

  for (let { from, to } of TEMPLATE_FILES) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    let body: string | Buffer = await readFile(join(SCAFFOLD_DIR, from))
    if (to === "AGENTS.md") body = resolveMarkers(body.toString("utf8"), extensions)
    await writeFile(dest, body)
  }

  // The template's files become the project's src/. Entries may be nested
  // directories (e.g. an asset folder), so copy recursively.
  let templateDir = join(TEMPLATES_DIR, template)
  await mkdir(join(dir, "src"), { recursive: true })
  for (let file of await readdir(templateDir)) {
    await cp(join(templateDir, file), join(dir, "src", file), { recursive: true })
  }

  // The assets/ convention folder, created up front: everything in it ships
  // with the app, and the dev watcher only picks up an assets/ folder that
  // exists when it starts. It starts with the SolidRT logo as the app icon,
  // for the author to replace: the .svg feeds the player tiles and desktop
  // window icon, the pre-rendered .png sibling the Android launcher icon
  // (`srt pack --apk`; SVG cannot be rasterized at pack time).
  await mkdir(join(dir, "assets"), { recursive: true })
  for (let icon of ["icon.svg", "icon.png"]) {
    await writeFile(join(dir, "assets", icon), await readFile(join(SCAFFOLD_DIR, icon)))
  }

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
  step(`Scaffolded ${resolve(dir)} (${summary})`)

  // Deps are declared in scaffold/package.json (Solid peers resolve via
  // @solidrt/core's peerDependencies), so a plain install is enough. Its
  // output only matters when it fails, so it is held until then.
  let progress = spinner()
  progress.start("Installing dependencies")
  let install = Bun.spawn(["bun", "install"], { cwd: dir, stdout: "pipe", stderr: "pipe" })
  let [out, err, code] = await Promise.all([
    new Response(install.stdout).text(),
    new Response(install.stderr).text(),
    install.exited,
  ])
  if (code !== 0) {
    progress.error("Dependency install failed")
    process.stderr.write(out + err)
    abort("Retry with `bun install` in the project")
  }
  progress.stop("Installed dependencies")

  let prefix = dir === "." ? "" : `cd ${dir} && `
  outro(`Done. Next: ${prefix}bun run dev`)
  process.exit()
}
