import { requireBinary } from "./util"
import { compileToBytecode } from "./bundler"
import type { PackFont } from "./fonts"

// Trailer magic identifying the runner an embedded payload belongs to. Must match
// the runner-side checks: fluxrt -> flux/src/bin/fluxrt.rs, solidrt ->
// lattice/src/main.rs (load_embedded_payload).
const MAGIC = {
  fluxrt: Buffer.from([0x46, 0x4c, 0x55, 0x58, 0x52, 0x54, 0x88, 0x44]), // "FLUXRT\x88\x44"
  solidrt: Buffer.from([0x53, 0x4f, 0x4c, 0x49, 0x44, 0x52, 0x54, 0x88, 0x44]), // "SOLIDRT\x88\x44"
}

export type Runner = keyof typeof MAGIC

// Section kinds in the solidrt trailer. Must match lattice/src/main.rs.
const SECTION_BYTECODE = 1
const SECTION_FONT = 2

type Section = { kind: number; bytes: Buffer; alias?: string }

// Append sections to the runner image: each section's bytes, then a table of
// section entries, then [table offset u64 LE][entry count u32 LE][magic].
// Table entry: [kind u32 LE][offset u64 LE][len u64 LE][alias len u8][alias].
// Offsets are absolute file offsets. The runner reads its own image at
// startup, validates the magic, and slices the sections back out.
function packSections(runnerBytes: Buffer, sections: Section[], magic: Buffer): Buffer {
  let parts: Buffer[] = [runnerBytes]
  let entries: Buffer[] = []
  let offset = runnerBytes.length
  for (let section of sections) {
    parts.push(section.bytes)
    let alias = Buffer.from(section.alias ?? "", "utf8")
    let entry = Buffer.allocUnsafe(21 + alias.length)
    entry.writeUInt32LE(section.kind, 0)
    entry.writeBigUInt64LE(BigInt(offset), 4)
    entry.writeBigUInt64LE(BigInt(section.bytes.length), 12)
    entry.writeUInt8(alias.length, 20)
    alias.copy(entry, 21)
    entries.push(entry)
    offset += section.bytes.length
  }
  let tail = Buffer.allocUnsafe(12)
  tail.writeBigUInt64LE(BigInt(offset), 0) // the table starts where the sections end
  tail.writeUInt32LE(sections.length, 8)
  return Buffer.concat([...parts, ...entries, tail, magic])
}

// Compile JS to bytecode and append it to the runner binary. solidrt gets the
// sectioned trailer (bytecode + fonts); fluxrt keeps the single-payload
// trailer of [bytecode][u64 offset LE][8-byte magic].
export async function packRunner(runner: Runner, jsCode: string, fonts: PackFont[] = []): Promise<Buffer> {
  let bytecode = await compileToBytecode(jsCode)

  let runnerPath = requireBinary(runner)
  let runnerBytes = Buffer.from(await Bun.file(runnerPath).arrayBuffer())

  if (runner === "solidrt") {
    let sections: Section[] = [
      { kind: SECTION_BYTECODE, bytes: bytecode },
      ...fonts.map((f) => ({ kind: SECTION_FONT, bytes: f.bytes, alias: f.alias })),
    ]
    return packSections(runnerBytes, sections, MAGIC.solidrt)
  }

  let offsetBuf = Buffer.allocUnsafe(8)
  offsetBuf.writeBigUInt64LE(BigInt(runnerBytes.length))
  return Buffer.concat([runnerBytes, bytecode, offsetBuf, MAGIC[runner]])
}
