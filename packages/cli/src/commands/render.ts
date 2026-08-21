import { appArgs, source, values } from "../args"
import { requireBinary, run } from "../util"
import { bundleSolid, writeIsolates } from "../bundler"
import { collectAssets, projectDirFor } from "../project"
import { cpSync, mkdirSync, rmSync } from "node:fs"
import { basename, dirname, join, resolve } from "path"

export async function runRenderCommand() {
  let entry = resolve(source!)
  let projectDir = projectDirFor(entry)
  let result = await bundleSolid()
  // The staged run dir (okf/backlog/build-output-dirs.md): bundle +
  // isolates/ + assets/ under one root - the shape of an installed version
  // dir, so the runtime's assets mount resolves both trees. Wiped first so
  // removed isolates and deleted assets cannot go stale; render owns this
  // subdir and nothing else under dist/.
  let outDir = join(projectDir, "dist", "render")
  rmSync(outDir, { recursive: true, force: true })
  let jsOutfile = join(outDir, basename(entry).replace(/\.[jt]sx?$/, "") + ".srt.js")
  await Bun.write(jsOutfile, result.code)
  writeIsolates(join(outDir, "isolates"), result.isolates)
  // The project's assets/ tree, copied in (dotfiles filtered, like a pack)
  // so `assets/...` resolves like it does under the dev server and in a
  // packed app (the runtime's cwd is the data sandbox, which holds no
  // assets).
  for (let asset of collectAssets(entry).assets) {
    let dest = join(outDir, asset.path)
    mkdirSync(dirname(dest), { recursive: true })
    cpSync(join(projectDir, asset.path), dest)
  }
  let runner = requireBinary("solidrt-go")
  let playbackArgs = ["--playback"]
  if (values.fps) playbackArgs.push("--fps", values.fps)
  if (values.duration) playbackArgs.push("--duration", values.duration)
  if (values.size) playbackArgs.push("--size", values.size)
  if (values.script) playbackArgs.push("--script", resolve(values.script))
  // Always absolute: the runtime chdirs into the app's data sandbox before
  // frames are written, so a bare prefix would land the PNGs there.
  playbackArgs.push("--out", resolve(values.output ?? "."))
  playbackArgs.push("--assets", outDir)
  playbackArgs.push(jsOutfile)
  // The runner takes everything after the source path verbatim as the app's
  // argument vector (flux:process argv).
  playbackArgs.push(...appArgs)
  let exit = await run(runner, playbackArgs)
  process.exit(exit)
}
