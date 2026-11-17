#!/usr/bin/env bun
//
// The website docs (docs/**/*.md) show the API in fenced `tsx` blocks, and
// nothing else compiles them: a snippet keeps an old signature long after
// the API moved. This stages each page's blocks as one entry under
// .solidrt-data/docs-snippets/ and runs `sol check` on it, so CI fails the
// moment a snippet drifts from the API it shows.
//
// Each block is a module of its own (a page may declare one name in two
// blocks); the page entry imports them all. A relative asset import
// (`./sheet.png`) gets an empty stand-in file, since the check only bundles
// and typechecks. A block that is a fragment by design (free names the
// prose supplies) opts out with `nocheck` on its fence: ```tsx nocheck.
// Exits 1 naming each page that fails.

import { mkdirSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join, relative } from "node:path"
import { fileURLToPath } from "node:url"
import { Glob } from "bun"

let REPO = join(dirname(fileURLToPath(import.meta.url)), "..")
let DOCS = join(REPO, "docs")
let STAGE = join(REPO, ".solidrt-data", "docs-snippets")
let SOL = join(REPO, "packages", "cli", "src", "main.ts")

// The fence opener of a checked block, with its info string's words.
let FENCE_OPEN = /^```tsx(?:\s+(.*))?$/
let FENCE_CLOSE = /^```\s*$/
let OPT_OUT = "nocheck"
// A relative import whose target is not a source module: an asset.
let ASSET_IMPORT = /from\s+["'](\.{1,2}\/[^"']+)["']/g
let SOURCE_EXTENSION = /\.(tsx?|jsx?)$/

type Block = { line: number; code: string }

function extractBlocks(markdown: string): Block[] {
  let blocks: Block[] = []
  let lines = markdown.split("\n")
  let open: { line: number; checked: boolean; body: string[] } | null = null
  for (let [index, line] of lines.entries()) {
    if (!open) {
      let match = FENCE_OPEN.exec(line)
      if (match) {
        let words = (match[1] ?? "").split(/\s+/)
        open = { line: index + 1, checked: !words.includes(OPT_OUT), body: [] }
      }
    } else if (FENCE_CLOSE.test(line)) {
      if (open.checked) blocks.push({ line: open.line, code: open.body.join("\n") })
      open = null
    } else {
      open.body.push(line)
    }
  }
  return blocks
}

// Stage one page: a block-<line>.tsx per block, asset stand-ins beside them,
// and the index.tsx entry importing every block. Returns the entry path.
function stagePage(page: string, blocks: Block[]): string {
  let dir = join(STAGE, page.replace(/\.md$/, ""))
  mkdirSync(dir, { recursive: true })
  let imports: string[] = []
  for (let block of blocks) {
    let name = `block-${block.line}.tsx`
    writeFileSync(join(dir, name), `// ${page}:${block.line}\n${block.code}\n`)
    imports.push(`import "./${name}"`)
    for (let match of block.code.matchAll(ASSET_IMPORT)) {
      let asset = match[1]!
      if (SOURCE_EXTENSION.test(asset)) continue
      let path = join(dir, asset)
      mkdirSync(dirname(path), { recursive: true })
      writeFileSync(path, "")
    }
  }
  let entry = join(dir, "index.tsx")
  writeFileSync(entry, imports.join("\n") + "\n")
  return entry
}

rmSync(STAGE, { recursive: true, force: true })
mkdirSync(STAGE, { recursive: true })
// The snippets are app code, so the stage is typed as a scaffolded app is:
// the repo's compiler options with the scaffold's types (flux's globals and
// the bundler's asset imports).
writeFileSync(
  join(STAGE, "tsconfig.json"),
  JSON.stringify({ extends: join(REPO, "tsconfig.json"), compilerOptions: { types: ["@solidrt/flux-types", "@solidrt/cli"] } }),
)
let pages = [...new Glob("**/*.md").scanSync({ cwd: DOCS })].sort()
let failures: string[] = []
let checked = 0
for (let page of pages) {
  let blocks = extractBlocks(await Bun.file(join(DOCS, page)).text())
  if (blocks.length === 0) continue
  let entry = stagePage(page, blocks)
  console.log(`== docs/${page} (${blocks.length} block${blocks.length === 1 ? "" : "s"})`)
  let proc = Bun.spawn(["bun", SOL, "check", relative(REPO, entry)], { cwd: REPO, stdout: "inherit", stderr: "inherit" })
  if ((await proc.exited) !== 0) failures.push(`docs/${page}`)
  checked++
}
if (failures.length > 0) {
  console.error(`${failures.length} of ${checked} docs pages have snippets that fail sol check:\n  ${failures.join("\n  ")}`)
  process.exit(1)
}
console.log(`Checked the tsx snippets of ${checked} docs pages`)
