import { appArgs, source, values } from "../args"
import { requireBinary, run } from "../util"
import { bundleTo } from "../bundler"
import { resolve } from "path"

export async function runRenderCommand() {
  let jsOutfile = source!.replace(/\.[jt]sx$/, "") + ".srt.js"
  await bundleTo(jsOutfile)
  let runner = requireBinary("solidrt-go")
  let playbackArgs = ["--playback"]
  if (values.fps) playbackArgs.push("--fps", values.fps)
  if (values.duration) playbackArgs.push("--duration", values.duration)
  if (values.size) playbackArgs.push("--size", values.size)
  if (values.script) playbackArgs.push("--script", resolve(values.script))
  // Always absolute: the runtime chdirs into the app's data sandbox before
  // frames are written, so a bare prefix would land the PNGs there.
  playbackArgs.push("--out", resolve(values.output ?? "."))
  playbackArgs.push(resolve(jsOutfile))
  // The runner takes everything after the source path verbatim as the app's
  // argument vector (flux:process argv).
  playbackArgs.push(...appArgs)
  let exit = await run(runner, playbackArgs)
  process.exit(exit)
}
