import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { source, values } from "../args"
import { select, text } from "../prompt"

const DEFAULT_NAME = "solidrt-app"

const SCAFFOLD_DIR = join(import.meta.dir, "../../scaffold")
const TEMPLATES_DIR = join(SCAFFOLD_DIR, "templates")

// Shared project files written for every template. Sources live in
// cli/scaffold/. The .gitignore is stored there as `gitignore` because npm
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
const TEMPLATE_MANIFEST = "template.json"

// Each template's template.json declares which level the scaffolded app is
// written at: "core" (only @solidrt/core, no component framework) or
// "components" (built with @solidrt/components). The level decides the
// generated dependencies; the description labels the template in the picker.
interface TemplateInfo {
  name: string
  level: "core" | "components"
  description: string
}

// Templates are the directories under scaffold/templates/; each holds the files
// that become the new project's src/, plus a template.json manifest. `default`
// sorts first as the starting point, the rest alphabetically.
async function listTemplates(): Promise<TemplateInfo[]> {
  let entries = await readdir(TEMPLATES_DIR, { withFileTypes: true })
  let names = entries
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort((a, b) =>
      a === DEFAULT_TEMPLATE ? -1 : b === DEFAULT_TEMPLATE ? 1 : a.localeCompare(b),
    )
  let templates: TemplateInfo[] = []
  for (let name of names) {
    // A missing manifest falls back to the components level: it keeps every
    // dependency, so the scaffolded app works at either level.
    let manifest = await readFile(join(TEMPLATES_DIR, name, TEMPLATE_MANIFEST), "utf8")
      .then((raw) => JSON.parse(raw))
      .catch(() => ({}))
    templates.push({
      name,
      level: manifest.level === "core" ? "core" : "components",
      description: typeof manifest.description === "string" ? manifest.description : "",
    })
  }
  return templates
}

// Resolve which template to scaffold from: an explicit --template if valid, an
// interactive picker on a TTY, else `default` (or the first available).
async function resolveTemplate(): Promise<TemplateInfo> {
  let templates = await listTemplates()
  if (templates.length === 0) {
    console.error(`!! No templates found in ${TEMPLATES_DIR}`)
    process.exit(1)
  }
  let chosen = values.template
  if (chosen) {
    let found = templates.find((t) => t.name === chosen)
    if (!found) {
      let names = templates.map((t) => t.name).join(", ")
      console.error(`!! Unknown template "${chosen}"; choose from: ${names}`)
      process.exit(1)
    }
    return found
  }
  if (process.stdin.isTTY) {
    let picked = await select(
      "Select a template",
      templates.map((t) => ({
        label: t.description ? `${t.name} - ${t.description}` : t.name,
        value: t.name,
      })),
    )
    return templates.find((t) => t.name === picked)!
  }
  return templates.find((t) => t.name === DEFAULT_TEMPLATE) ?? templates[0]!
}

export async function runInitCommand() {
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

  let template = await resolveTemplate()

  console.log(`>> Scaffolding SolidRT project in ${resolve(dir)} (${template.name})`)
  for (let { from, to } of TEMPLATE_FILES) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    await writeFile(dest, await readFile(join(SCAFFOLD_DIR, from)))
    console.log(`   Write ${to}`)
  }

  // The chosen template's files become the project's src/. Entries may be
  // nested directories (e.g. an asset folder), so copy recursively. The
  // manifest describes the template rather than belonging to the app.
  let templateDir = join(TEMPLATES_DIR, template.name)
  await mkdir(join(dir, "src"), { recursive: true })
  for (let file of await readdir(templateDir)) {
    if (file === TEMPLATE_MANIFEST) continue
    await cp(join(templateDir, file), join(dir, "src", file), { recursive: true })
    console.log(`   Write src/${file}`)
  }

  // The assets/ convention folder, created up front: everything in it ships
  // with the app, and the dev watcher only picks up an assets/ folder that
  // exists when it starts.
  await mkdir(join(dir, "assets"), { recursive: true })
  console.log("   Write assets/")

  // The scaffold package.json carries a placeholder name; set it from the
  // target folder. A core-level app gets no component framework dependency.
  let pkgPath = join(dir, "package.json")
  let pkg = JSON.parse(await readFile(pkgPath, "utf8"))
  pkg.name = packageName(dir)
  if (template.level === "core") delete pkg.dependencies["@solidrt/components"]
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