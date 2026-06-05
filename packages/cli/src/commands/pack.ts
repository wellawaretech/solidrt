import { values, source } from "../args"
import { requireBinary } from "../util"
import { bundle } from "../bundler"

// Trailer magic identifying the runner an embedded payload belongs to. Must match
// the runner-side checks: FLUX_MAGIC -> flux/src/bin/fluxrt.rs, SOLID_MAGIC ->
// lattice/src/main.rs (load_embedded_bytecode).
const FLUX_MAGIC = Buffer.from([0x46, 0x4c, 0x55, 0x58, 0x52, 0x54, 0x88, 0x44]) // "FLUXRT\x88\x44"
const SOLID_MAGIC = Buffer.from([0x53, 0x4f, 0x4c, 0x49, 0x44, 0x52, 0x54, 0x88, 0x44]) // "SOLIDRT\x88\x44"

// Bundle for the bare Flux runtime: no Solid plugin, flux: modules stay external.
async function bundleFlux(entry: string): Promise<string> {
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    minify: values.minify,
    external: ["flux:*"],
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

// Bundle for the SolidRT runtime: reuse the standard Solid-aware bundler.
async function bundleSolid(): Promise<string> {
  let result = await bundle()
  if (!result) {
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

// Compile JS to bytecode and append it to the runner binary, followed by a
// trailer of [u64 offset LE][8-byte magic]. The runner reads its own image at
// startup, validates the magic, and slices the bytecode back out.
async function packRunner(runnerName: string, magic: Buffer, jsCode: string) {
  let outfile = values.output ?? source!.replace(/\.[jt]sx?$/, "")

  let bytecode = await compileToBytecode(jsCode)

  let runner = requireBinary(runnerName)
  let runnerBytes = Buffer.from(await Bun.file(runner).arrayBuffer())

  let offsetBuf = Buffer.allocUnsafe(8)
  offsetBuf.writeBigUInt64LE(BigInt(runnerBytes.length))

  let packed = Buffer.concat([runnerBytes, bytecode, offsetBuf, magic])
  await Bun.write(outfile, packed)

  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", outfile])
  }

  console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
}

export async function runPackCommand() {
  if (values.flux) {
    let jsCode = await bundleFlux(source!)
    await packRunner("fluxrt", FLUX_MAGIC, jsCode)
  } else {
    let jsCode = await bundleSolid()
    await packRunner("solidrt-runner", SOLID_MAGIC, jsCode)
  }
  process.exit()
}