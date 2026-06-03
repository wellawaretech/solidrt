import { values, source } from "./args"
import { requireBinary } from "./util"

const MAGIC = Buffer.from([0x46, 0x4c, 0x55, 0x58, 0x52, 0x54, 0x00, 0x01]) // "FLUXRT\x00\x01"

async function bundleFlux(entry: string): Promise<string> {
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    minify: values.minify,
    external: ["qjs:*", "flux:*"],
  })
  if (!result.success) {
    for (let msg of result.logs) console.error(msg)
    console.error("Build failed")
    process.exit(1)
  }
  let jsCode = ""
  for (let output of result.outputs) jsCode += await output.text()
  return jsCode
}

async function compileToBytecode(jsCode: string): Promise<Buffer> {
  let compiler = requireBinary("fluxc")
  let proc = Bun.spawn([compiler], {
    stdin: new Blob([jsCode]),
    stdout: "pipe",
    stderr: "inherit",
  })
  let [bytecode, code] = await Promise.all([new Response(proc.stdout).arrayBuffer(), proc.exited])
  if (code !== 0) process.exit(code)
  return Buffer.from(bytecode)
}

export async function runPackCommand() {
  let outfile = values.output ?? source!.replace(/\.[jt]s$/, "")

  let jsCode = await bundleFlux(source!)

  let bytecode = await compileToBytecode(jsCode)

  let runner = requireBinary("fluxrt")
  let runnerBytes = Buffer.from(await Bun.file(runner).arrayBuffer())

  let offsetBuf = Buffer.allocUnsafe(8)
  offsetBuf.writeBigUInt64LE(BigInt(runnerBytes.length))

  let packed = Buffer.concat([runnerBytes, bytecode, offsetBuf, MAGIC])
  await Bun.write(outfile, packed)

  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", outfile])
  }

  console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
  process.exit()
}