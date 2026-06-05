import { requireBinary } from "./util"
import { compileToBytecode } from "./bundler"

// Trailer magic identifying the runner an embedded payload belongs to. Must match
// the runner-side checks: fluxrt -> flux/src/bin/fluxrt.rs, solidrt-runner ->
// lattice/src/main.rs (load_embedded_bytecode).
const MAGIC = {
  fluxrt: Buffer.from([0x46, 0x4c, 0x55, 0x58, 0x52, 0x54, 0x88, 0x44]), // "FLUXRT\x88\x44"
  "solidrt-runner": Buffer.from([0x53, 0x4f, 0x4c, 0x49, 0x44, 0x52, 0x54, 0x88, 0x44]), // "SOLIDRT\x88\x44"
}

export type Runner = keyof typeof MAGIC

// Compile JS to bytecode and append it to the runner binary, followed by a
// trailer of [u64 offset LE][8-byte magic]. The runner reads its own image at
// startup, validates the magic, and slices the bytecode back out.
export async function packRunner(runner: Runner, jsCode: string): Promise<Buffer> {
  let bytecode = await compileToBytecode(jsCode)

  let runnerPath = requireBinary(runner)
  let runnerBytes = Buffer.from(await Bun.file(runnerPath).arrayBuffer())

  let offsetBuf = Buffer.allocUnsafe(8)
  offsetBuf.writeBigUInt64LE(BigInt(runnerBytes.length))

  return Buffer.concat([runnerBytes, bytecode, offsetBuf, MAGIC[runner]])
}