import { source, values } from "../args"
import { requireBinary, run } from "../util"
import { bundleTo } from "../bundler"
import { resolve } from "path"

export async function runRecordCommand() {
  let jsOutfile = source!.replace(/\.[jt]sx$/, "") + ".srt.js"
  await bundleTo(jsOutfile)
  let runner = requireBinary("solidrt-go")
  let recordArgs = ["--record", resolve(jsOutfile)]
  if (values.fps) recordArgs.push("--fps", values.fps)
  if (values.duration) recordArgs.push("--duration", values.duration)
  if (values.size) recordArgs.push("--size", values.size)
  let exit = await run(runner, recordArgs)
  process.exit(exit)
}