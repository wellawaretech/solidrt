import { mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { basename, dirname, join, resolve } from "node:path"
import { source } from "../args"

const SCAFFOLD_DIR = join(import.meta.dir, "../../scaffold")

// What gets written into a new project. Sources live in cli/scaffold/. The
// .gitignore is stored there as `gitignore` because npm strips files literally
// named `.gitignore` from published packages, so it is renamed on the way out.
const TEMPLATE_FILES: Array<{ from: string; to: string }> = [
  { from: "package.json", to: "package.json" },
  { from: "tsconfig.json", to: "tsconfig.json" },
  { from: "gitignore", to: ".gitignore" },
  { from: "AGENTS.md", to: "AGENTS.md" },
  { from: "src/index.tsx", to: "src/index.tsx" },
]

// A valid npm package name derived from the target directory.
function packageName(dir: string): string {
  let name = basename(resolve(dir))
    .toLowerCase()
    .replace(/[^a-z0-9-_.]/g, "-")
  return name || "solidrt-app"
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

  console.log(`>> Scaffolding SolidRT project in ${resolve(dir)}`)
  for (let { from, to } of TEMPLATE_FILES) {
    let dest = join(dir, to)
    await mkdir(dirname(dest), { recursive: true })
    await writeFile(dest, await readFile(join(SCAFFOLD_DIR, from)))
    console.log(`   Write ${to}`)
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