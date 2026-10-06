// The LOAD-segment alignment of a native lib, what Android 15 holds a
// 64-bit lib to (16 KB pages): a lib under it installs but fails to load.
// The stock build checks its staged libs with readelf (lattice's Makefile,
// check-elf-align); a project's runtime libs are checked here before they
// go into an APK. Pure over the bytes, so it runs anywhere.

/** The least LOAD-segment alignment Android accepts in a 64-bit lib. */
export const MIN_LOAD_ALIGN = 16384

// ELF identification and the ELF64 little-endian header and program-header
// fields read here (the System V ABI; Android libs are little-endian).
const ELF_MAGIC = [0x7f, 0x45, 0x4c, 0x46]
const EI_CLASS = 4
const EI_DATA = 5
const CLASS_64 = 2
const DATA_LITTLE_ENDIAN = 1
const E_PHOFF = 0x20
const E_PHENTSIZE = 0x36
const E_PHNUM = 0x38
const P_TYPE = 0
const P_ALIGN = 48
const PT_LOAD = 1

/**
 * The least alignment of the LOAD segments of a 64-bit little-endian ELF
 * file, or null for anything else (a 32-bit lib, which Android does not hold
 * to the page size, or a file that is not an ELF object).
 */
export function elfLoadAlignment(bytes: Uint8Array): number | null {
  if (bytes.length < E_PHNUM + 2 || !ELF_MAGIC.every((b, i) => bytes[i] === b)) return null
  if (bytes[EI_CLASS] !== CLASS_64 || bytes[EI_DATA] !== DATA_LITTLE_ENDIAN) return null
  let view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  let phoff = Number(view.getBigUint64(E_PHOFF, true))
  let phentsize = view.getUint16(E_PHENTSIZE, true)
  let phnum = view.getUint16(E_PHNUM, true)
  let least: number | null = null
  for (let i = 0; i < phnum; i++) {
    let at = phoff + i * phentsize
    if (at + P_ALIGN + 8 > bytes.length) return null
    if (view.getUint32(at + P_TYPE, true) !== PT_LOAD) continue
    let align = Number(view.getBigUint64(at + P_ALIGN, true))
    least = least === null ? align : Math.min(least, align)
  }
  return least
}
