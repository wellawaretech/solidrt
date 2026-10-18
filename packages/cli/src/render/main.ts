import { appArgs, clientStorageArgs, values } from "../lib/args"
import { requireBinary, run } from "../lib/util"
import { bundleSolid, writeIsolates } from "../bundle/bundler"
import { fail } from "../lib/fail"
import { collectAssets, loadAppIdentity } from "../lib/project"
import { resolveMode } from "../lib/mode"
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs"
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "path"

// The staged run dir, render's subdir of the project's tool state (the
// .solidrt-data the dev server keeps its own in): wiped by every render,
// owned by nothing else.
const STAGE_DIR = join(".solidrt-data", "render")
// Where frames land by default, one new folder per render: output the user
// keeps, so a render never wipes it or writes into another render's folder.
const RENDERS_DIR = "renders"

export async function main() {
  let mode = resolveMode()
  let entry = mode.entry
  // Both under the cwd, the build root (see the bundle command): a project
  // command runs in its root, and a file on its own renders where it is run.
  let stageDir = resolve(STAGE_DIR)
  // Checked before anything is built or wiped.
  let output = values.output === undefined ? null : checkOutput(resolve(values.output), stageDir)
  let result = await bundleSolid(mode)
  // The staged run dir (okf/done/build-output-dirs.md): bundle + isolates/ +
  // assets/ under one root - the shape of an installed version dir, so the
  // runtime's assets mount resolves both trees. Wiped first so removed
  // isolates and deleted assets cannot go stale.
  rmSync(stageDir, { recursive: true, force: true })
  let jsOutfile = join(stageDir, basename(entry).replace(/\.[jt]sx?$/, "") + ".sol.js")
  await Bun.write(jsOutfile, result.code)
  writeIsolates(join(stageDir, "isolates"), result.isolates)
  // The manifest's font annotations are what the runtime registers the
  // project's fonts from (solidrt.fonts, over its embedded defaults).
  await Bun.write(join(stageDir, "manifest.json"), result.manifest)
  // The project's assets/ tree, copied in (dotfiles filtered, like a pack)
  // so `assets/...` resolves like it does under the dev server and in a
  // packed app (the runtime's cwd is the data sandbox, which holds no
  // assets). File mode collects none.
  for (let asset of collectAssets(mode.projectDir).assets) {
    let dest = join(stageDir, asset.path)
    mkdirSync(dirname(dest), { recursive: true })
    cpSync(join(mode.projectDir!, asset.path), dest)
  }
  let runner = requireBinary("solidrt-go")
  let framesDir = output ?? freeRenderDir(loadAppIdentity(entry, mode.projectDir).appId.split(".").pop()!)
  mkdirSync(framesDir, { recursive: true })
  console.log(`>> Writing frames to ${framesDir}`)
  // The same client tree a dev client uses (--data-root, -c): with the
  // manifest's appId the render anchors into that app's sandbox, so it
  // renders the state dev shows.
  let renderArgs = ["--render", ...clientStorageArgs()]
  if (values.fps) renderArgs.push("--fps", values.fps)
  if (values.duration) renderArgs.push("--duration", values.duration)
  if (values.size) renderArgs.push("--size", values.size)
  if (values.script) renderArgs.push("--script", resolve(values.script))
  // The launch link (env.launchLink): a router renders the screen it names.
  if (values.link) renderArgs.push("--link", values.link)
  // The app is run to rest before the first frame; errors logged fail the
  // render; another Math.random sequence.
  if (values.settle) renderArgs.push("--settle")
  if (values.strict) renderArgs.push("--strict")
  if (values.seed !== undefined) renderArgs.push("--seed", values.seed)
  // Absolute (resolved above): the runtime chdirs into the app's data
  // sandbox before frames are written, so a relative path would land the
  // PNGs there.
  renderArgs.push("--out", framesDir)
  renderArgs.push("--assets", stageDir)
  renderArgs.push(jsOutfile)
  // The runner takes everything after the source path verbatim as the app's
  // argument vector (flux:process argv).
  renderArgs.push(...appArgs)
  let exit = await run(runner, renderArgs)
  process.exit(exit)
}

// An --output the frames may land in: outside the staging dir, which the
// next render wipes, and missing or empty, so a render never mixes its
// frames with another's or writes over the user's files.
function checkOutput(dir: string, stageDir: string): string {
  let rel = relative(stageDir, dir)
  if (rel !== ".." && !rel.startsWith(".." + sep) && !isAbsolute(rel)) {
    fail(`${dir} is inside ${stageDir}, which every render wipes; choose another --output`)
  }
  if (existsSync(dir) && (!statSync(dir).isDirectory() || readdirSync(dir).length > 0)) {
    fail(`${dir} is not an empty directory; choose another --output`)
  }
  return dir
}

// A new folder under renders/ for this render's frames, named by the app and
// the local time the render started; a second render in the same second
// takes the next free -N.
function freeRenderDir(name: string): string {
  let base = resolve(RENDERS_DIR, `${name}-${timestamp(new Date())}`)
  let dir = base
  for (let n = 2; existsSync(dir); n++) dir = `${base}-${n}`
  return dir
}

// YYYY-MM-DD-HHMMSS: sorts in render order, and has no character a file
// name cannot hold on any platform.
function timestamp(d: Date): string {
  let two = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}-${two(d.getHours())}${two(d.getMinutes())}${two(d.getSeconds())}`
}
