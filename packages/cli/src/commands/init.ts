import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { source, values } from "../args"
import { select } from "../prompt"

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

// Templates are the directories under scaffold/templates/; each holds the files
// that become the new project's src/. `default` sorts first as the starting
// point, the rest alphabetically.
async function listTemplates(): Promise<string[]> {
  let entries = await readdir(TEMPLATES_DIR, { withFileTypes: true })
  return entries
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort((a, b) =>
      a === DEFAULT_TEMPLATE ? -1 : b === DEFAULT_TEMPLATE ? 1 : a.localeCompare(b),
    )
}

// Resolve which template to scaffold from: an explicit --template if valid, an
// interactive picker on a TTY, else `default` (or the first available).
async function resolveTemplate(): Promise<string> {
  let templates = await listTemplates()
  if (templates.length === 0) {
    console.error(`!! No templates found in ${TEMPLATES_DIR}`)
    process.exit(1)
  }
  let chosen = values.template
  if (chosen) {
    if (!templates.includes(chosen)) {
      console.error(`!! Unknown template "${chosen}"; choose from: ${templates.join(", ")}`)
      process.exit(1)
    }
    return chosen
  }
  if (process.stdin.isTTY) return select("Select a template", templates)
  return templates.includes(DEFAULT_TEMPLATE) ? DEFAULT_TEMPLATE : templates[0]!
}

export async function runInitCommand() {
  // The target folder is required (validateArgs enforces it) and must be empty
  // or absent, so init can never overwrite files in an existing project.
  let dir = source!
  let existing = await readdir(dir).catch(() => null)
  if (existing && existing.length > 0) {
    console.error(`!! ${resolve(dir)} already exists and is not empty; choose a new folder name`)
    process.exit(1)
  }

  let template = await resolveTemplate()

  console.log(`>> Scaffolding SolidRT project in ${resolve(dir)} (${template})`)
  for (let { from, to } of TEMPLATE_FILES) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    await writeFile(dest, await readFile(join(SCAFFOLD_DIR, from)))
    console.log(`   Write ${to}`)
  }

  // The chosen template's files become the project's src/.
  let templateDir = join(TEMPLATES_DIR, template)
  for (let file of await readdir(templateDir)) {
    let dest = join(dir, "src", file)
    await mkdir(dirname(dest), { recursive: true })
    await copyFile(join(templateDir, file), dest)
    console.log(`   Write src/${file}`)
  }

  // The scaffold package.json carries a placeholder name; set it from the
  // target folder.
  let pkgPath = join(dir, "package.json")
  let pkg = JSON.parse(await readFile(pkgPath, "utf8"))
  pkg.name = packageName(dir)
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