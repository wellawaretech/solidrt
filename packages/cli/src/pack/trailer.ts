import { readFileSync } from "node:fs"
import { runnerGlLibs } from "../lib/artifacts"
import { requireBinary } from "../lib/util"
import type { PackFolder } from "./layout"

// Trailer magic identifying the runner an embedded payload belongs to. Must
// match the runner-side checks: both runners parse the trailer through
// forge/src/trailer.rs (readers: lattice/src/main.rs, flux/src/bin/fluxrt.rs).
const MAGIC = {
  fluxrt: Buffer.from([0x46, 0x4c, 0x55, 0x58, 0x52, 0x54, 0x88, 0x44]), // "FLUXRT\x88\x44"
  solidrt: Buffer.from([0x53, 0x4f, 0x4c, 0x49, 0x44, 0x52, 0x54, 0x88, 0x44]), // "SOLIDRT\x88\x44"
}

// Section kinds in the trailer. Must match forge/src/trailer.rs (fluxrt only
// consumes kind-2 file sections; solidrt consumes all three).
const SECTION_MANIFEST = 1
const SECTION_FILE = 2
const SECTION_GL_LIB = 3

type Section = { kind: number; bytes: Buffer; name?: string }

// Append sections to the runner image: each section's bytes, then a table of
// section entries, then [table offset u64 LE][entry count u32 LE][magic].
// Table entry: [kind u32 LE][offset u64 LE][len u64 LE][name len u16 LE][name].
// Offsets are absolute file offsets. The runner reads its own image at
// startup, validates the magic, and slices the sections back out.
function packSections(runnerBytes: Buffer, sections: Section[], magic: Buffer): Buffer {
  let parts: Buffer[] = [runnerBytes]
  let entries: Buffer[] = []
  let offset = runnerBytes.length
  for (let section of sections) {
    parts.push(section.bytes)
    let name = Buffer.from(section.name ?? "", "utf8")
    let entry = Buffer.allocUnsafe(22 + name.length)
    entry.writeUInt32LE(section.kind, 0)
    entry.writeBigUInt64LE(BigInt(offset), 4)
    entry.writeBigUInt64LE(BigInt(section.bytes.length), 12)
    entry.writeUInt16LE(name.length, 20)
    name.copy(entry, 22)
    entries.push(entry)
    offset += section.bytes.length
  }
  let tail = Buffer.allocUnsafe(12)
  tail.writeBigUInt64LE(BigInt(offset), 0) // the table starts where the sections end
  tail.writeUInt32LE(sections.length, 8)
  return Buffer.concat([...parts, ...entries, tail, magic])
}

// The pack folder in section form - the canonical manifest verbatim, then
// every manifest-listed file named by its manifest path. Bundle, fonts, and
// identity all come from the manifest; assets are read in place via ranged
// reads at their section offsets, so nothing is unpacked at runtime.
function appSections(folder: PackFolder, bytecode: Buffer): Section[] {
  return [
    { kind: SECTION_MANIFEST, bytes: Buffer.from(folder.manifest, "utf8") },
    { kind: SECTION_FILE, bytes: bytecode, name: "bundle.bin" },
    ...folder.copies.map((c) => ({ kind: SECTION_FILE, bytes: readFileSync(c.from), name: c.to })),
    ...folder.files.map((f) => ({ kind: SECTION_FILE, bytes: f.bytes, name: f.to })),
  ]
}

// The single-file solidrt executable: the runner image plus the app sections.
// GL libraries ride along as kind-3 sections (runtime freight, deliberately
// outside the manifest); the runner extracts those to its cache and preloads
// them before window setup.
export function packSolid(folder: PackFolder, bytecode: Buffer): Buffer {
  let runnerPath = requireBinary("solidrt")
  let sections: Section[] = [
    ...appSections(folder, bytecode),
    ...runnerGlLibs(runnerPath).map((lib) => ({ kind: SECTION_GL_LIB, bytes: readFileSync(lib.path), name: lib.name })),
  ]
  return packSections(readFileSync(runnerPath), sections, MAGIC.solidrt)
}

// A standalone .srtapp: the app sections alone, no runner in front and no GL
// libraries (the runner that loads it brings its own). The runner parses it
// exactly as it parses its own image (lattice/src/main.rs, load_payload), so
// `solidrt <file>.srtapp` runs it with the runner used in place - nothing is
// copied or appended to, and a signed runner stays signed. The extension is
// a convention; the magic is the contract.
export function packApp(folder: PackFolder, bytecode: Buffer): Buffer {
  return packSections(Buffer.alloc(0), appSections(folder, bytecode), MAGIC.solidrt)
}

// The single-file flux executable: the fluxrt runner plus the program in the
// same section trailer packSolid uses, kind-2 file sections only -
// "bundle.bin" is the program, each isolate module "isolates/<id>.bin".
// Like packSolid, this assembles precompiled bytecode; the pack command
// compiles.
export function packFlux(bytecode: Buffer, isolates: { id: string; bytecode: Buffer }[] = []): Buffer {
  let runnerBytes = readFileSync(requireBinary("fluxrt"))
  let sections: Section[] = [
    { kind: SECTION_FILE, bytes: bytecode, name: "bundle.bin" },
    ...isolates.map((i) => ({ kind: SECTION_FILE, bytes: i.bytecode, name: `isolates/${i.id}.bin` })),
  ]
  return packSections(runnerBytes, sections, MAGIC.fluxrt)
}
