// The ELF LOAD-alignment read (src/pack/android/elf.ts) over hand-built
// headers: a 64-bit object with two LOAD segments and one other, a 32-bit
// object, and bytes that are no ELF file at all.
import { test, expect } from "flux:test"
import { elfLoadAlignment } from "../src/pack/android/elf.ts"

// An ELF64 header (64 bytes) followed by `segments` program headers (56
// bytes each): [type, align] pairs.
function elf(elfClass: number, segments: [number, number][]): Uint8Array {
  let bytes = new Uint8Array(64 + segments.length * 56)
  let view = new DataView(bytes.buffer)
  bytes.set([0x7f, 0x45, 0x4c, 0x46, elfClass, 1], 0)
  view.setBigUint64(0x20, 64n, true)
  view.setUint16(0x36, 56, true)
  view.setUint16(0x38, segments.length, true)
  segments.forEach(([type, align], i) => {
    view.setUint32(64 + i * 56, type, true)
    view.setBigUint64(64 + i * 56 + 48, BigInt(align), true)
  })
  return bytes
}

test("elfLoadAlignment: the least LOAD alignment, other segments ignored", () => {
  expect(elfLoadAlignment(elf(2, [[1, 16384], [6, 8], [1, 4096]]))).toBe(4096)
  expect(elfLoadAlignment(elf(2, [[1, 16384], [1, 65536]]))).toBe(16384)
})

test("elfLoadAlignment: no LOAD segment reads null", () => {
  expect(elfLoadAlignment(elf(2, [[6, 8]]))).toBe(null)
})

test("elfLoadAlignment: a 32-bit object is not held to the page size", () => {
  expect(elfLoadAlignment(elf(1, [[1, 4096]]))).toBe(null)
})

test("elfLoadAlignment: bytes that are no ELF file read null", () => {
  expect(elfLoadAlignment(new Uint8Array([0x50, 0x4b, 0x03, 0x04]))).toBe(null)
  expect(elfLoadAlignment(new Uint8Array(0))).toBe(null)
})
