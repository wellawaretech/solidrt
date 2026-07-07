import { source, values } from "../args"
import { requireBinary, run } from "../util"
import { bundleTo } from "../bundler"
import { resolve } from "path"

export async function runRecordCommand() {
  let jsOutfile = source!.replace(/\.[jt]sx$/, "") + ".srt.js"
  await bundleTo(jsOutfile)
  let runner = requireBinary("solidrt-go")
  let outFile = values.out ?? source!.replace(/\.[jt]sx$/, "") + ".script.json"
  let recordArgs = ["--record", resolve(outFile)]
  if (values.size) recordArgs.push("--size", values.size)
  recordArgs.push(resolve(jsOutfile))
  let exit = await run(runner, recordArgs)
  process.exit(exit)
}
