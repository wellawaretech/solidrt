import { source, values } from "../args"
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
  playbackArgs.push(resolve(jsOutfile))
  let exit = await run(runner, playbackArgs)
  process.exit(exit)
}
