import { existsSync } from "node:fs"
import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { styleText } from "node:util"
import { source, values } from "../lib/args"
import { CLI_VERSION } from "../lib/project"
import { abort, intro, multiselect, outro, spinner, step, text } from "../lib/prompt"
import { runQuiet } from "../lib/util"
import { RUNTIME_DIR, RUNTIME_TYPES, renderRuntime, withRuntime, withRuntimeIgnore, withRuntimeTypes } from "./runtime"

const DEFAULT_NAME = "solidrt-app"

// The @solidrt/cli package this runs from (runtime/ beside src/), and the
// checkout it sits in when it is the checkout's own.
const PACKAGE_DIR = join(import.meta.dir, "..", "..")
const CHECKOUT_DIR = join(PACKAGE_DIR, "..", "..")
const SCAFFOLD_DIR = join(import.meta.dir, "scaffold")
const TEMPLATES_DIR = join(SCAFFOLD_DIR, "templates")

// Shared project files written for every template. Sources live in
// cli/src/init/scaffold/. The .gitignore is stored there as `gitignore` because npm
// strips files literally named `.gitignore` from published packages, so it is
// renamed on the way out. The starter test sits outside a tests/ folder there,
// where this repo's own `sol test` would pick it up and fail on its `@/`
// import. The per-template src/ comes from scaffold/templates/.
const TEMPLATE_FILES: Array<{ from: string; to: string }> = [
  { from: "package.json", to: "package.json" },
  { from: "tsconfig.json", to: "tsconfig.json" },
  { from: "gitignore", to: ".gitignore" },
  { from: "mcp.json", to: ".mcp.json" },
  { from: "AGENTS.md", to: "AGENTS.md" },
  { from: "README.md", to: "README.md" },
  { from: "app.test.tsx", to: "tests/app.test.tsx" },
]

// The app's own runtime (--runtime): a cargo project over lattice with a
// starter module, from scaffold/runtime/. The test goes into the app's
// tests/ (stored outside a tests/ folder here, like app.test.tsx). The
// manifest is stored as Cargo.toml.template: cargo parses every Cargo.toml
// in this repository when it resolves the scaffold's git dependency on
// lattice, and one with placeholders fails that parse.
const RUNTIME_FILES: Array<{ from: string; to: string }> = [
  { from: "Makefile", to: `${RUNTIME_DIR}/Makefile` },
  { from: "Cargo.toml.template", to: `${RUNTIME_DIR}/Cargo.toml` },
  { from: "lib.rs", to: `${RUNTIME_DIR}/src/lib.rs` },
  { from: "hello.rs", to: `${RUNTIME_DIR}/src/hello.rs` },
  { from: "solidrt.rs", to: `${RUNTIME_DIR}/src/bin/solidrt.rs` },
  { from: "solidrt-go.rs", to: `${RUNTIME_DIR}/src/bin/solidrt-go.rs` },
  { from: "hello.d.ts", to: `${RUNTIME_TYPES}/hello.d.ts` },
  { from: "runtime.test.tsx", to: "tests/runtime.test.tsx" },
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
// scaffold/templates/. Extensions that bring a starter combine: the
// folder for several is their templates joined with `+`, in the order
// below (`router+components`).
interface Extension {
  pkg: string
  key: string
  template?: string
  description: string
}

const EXTENSIONS: Extension[] = [
  { pkg: "@solidrt/router", key: "router", template: "router", description: "declarative routing between screens" },
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

// The starter src/ comes from the selected extensions that bring a
// template, combined; with none, the core `default` starter.
function resolveTemplate(extensions: Extension[]): string {
  let templates = extensions.flatMap((e) => e.template ?? [])
  return templates.join("+") || DEFAULT_TEMPLATE
}

// Write the runtime into the project at `dir` (its package.json read for
// the app's name) and wire it in: the runtime key, the types in tsconfig,
// the target dir ignored. What cannot be edited is said, not skipped.
async function writeRuntime(dir: string): Promise<void> {
  let pkgPath = join(dir, "package.json")
  let pkg = JSON.parse(await readFile(pkgPath, "utf8"))
  let templates: Record<string, string> = {}
  for (let { from, to } of RUNTIME_FILES) {
    templates[to] = await readFile(join(SCAFFOLD_DIR, "runtime", from), "utf8")
  }
  for (let [to, body] of Object.entries(renderRuntime(templates, pkg.name ?? basename(resolve(dir)), CLI_VERSION))) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    await writeFile(dest, body)
  }
  // Cargo.lock is seeded from the release's, shipped beside runtime.mk in
  // the package (release.yml); the checkout seeds from its own. The first
  // build settles it to this manifest (lattice as a git dependency, the
  // graph the runtime's features keep) and committed it is what CI builds
  // against (runtime.mk, LOCKED): the same crates as the release, the
  // JavaScript engine among them, which sol's bytecode compiler must match.
  let lock = [join(PACKAGE_DIR, "runtime", "Cargo.lock"), ...(existsSync(join(CHECKOUT_DIR, "lattice", "Cargo.toml")) ? [join(CHECKOUT_DIR, "Cargo.lock")] : [])].find((path) => existsSync(path))
  if (lock) await cp(lock, join(dir, RUNTIME_DIR, "Cargo.lock"))
  else console.log(`No Cargo.lock to seed ${RUNTIME_DIR}/ from; the first build writes one`)
  await writeFile(pkgPath, withRuntime(await readFile(pkgPath, "utf8")))
  let tsconfigPath = join(dir, "tsconfig.json")
  let tsconfig = existsSync(tsconfigPath) ? withRuntimeTypes(await readFile(tsconfigPath, "utf8")) : null
  if (tsconfig) await writeFile(tsconfigPath, tsconfig)
  else console.log(`Add "${RUNTIME_TYPES}" to the "include" of tsconfig.json for the module types`)
  let ignorePath = join(dir, ".gitignore")
  await writeFile(ignorePath, withRuntimeIgnore(existsSync(ignorePath) ? await readFile(ignorePath, "utf8") : null))
  if (!pkg.devDependencies?.["@solidrt/test"] && !pkg.dependencies?.["@solidrt/test"]) {
    console.log("tests/runtime.test.tsx imports @solidrt/test: add it with `bun add -d @solidrt/test`")
  }
  step(`Wrote ${RUNTIME_DIR}/ (runtime:hello, built with make -C ${RUNTIME_DIR}; commit its Cargo.lock after the first build)`)
}

// `sol init --runtime` with no folder: add a runtime to the project here.
async function addRuntime(): Promise<void> {
  intro(`Add a runtime to this SolidRT app ${styleText("dim", CLI_VERSION)}`)
  if (!existsSync("package.json")) abort("No package.json here; run it in an app, or `sol init <dir> --runtime` for a new one")
  if (existsSync(RUNTIME_DIR)) abort(`${resolve(RUNTIME_DIR)} already exists`)
  await writeRuntime(".")
  outro(`Done. Next: make -C ${RUNTIME_DIR}, then bun run dev`)
  process.exit()
}

export async function main() {
  if (values.runtime && !source) return addRuntime()

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
    if (to === "README.md") body = body.toString("utf8").replaceAll("{{name}}", packageName(dir))
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
  // (`sol pack --apk`; SVG cannot be rasterized at pack time).
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

  if (values.runtime) await writeRuntime(dir)

  // Deps are declared in scaffold/package.json (Solid peers resolve via
  // @solidrt/core's peerDependencies), so a plain install is enough. Its
  // output only matters when it fails, so it is held until then.
  let progress = spinner()
  progress.start("Installing dependencies")
  let install = await runQuiet(["bun", "install"], dir)
  if (install.code !== 0) {
    progress.error("Dependency install failed")
    process.stderr.write(install.output)
    abort("Retry with `bun install` in the project")
  }
  progress.stop("Installed dependencies")

  let prefix = dir === "." ? "" : `cd ${dir} && `
  let build = values.runtime ? `make -C ${RUNTIME_DIR} && ` : ""
  outro(`Done. Next: ${prefix}${build}bun run dev`)
  process.exit()
}
