// ResStringPool surgery, the one primitive both binary-Android formats need:
// the compiled manifest (AndroidManifest.xml, a RES_XML file whose pool starts
// at offset 8) and the resource table (resources.arsc, a RES_TABLE file whose
// global pool starts at offset 12) both store their text in one string-pool
// chunk. Replacing a string rebuilds that chunk (re-encoded strings, rebuilt
// offset table, fixed chunk and file sizes) and leaves every other chunk
// byte-identical - references are by pool index, and indices do not move.

const CHUNK_STRING_POOL = 0x0001
const CHUNK_TABLE = 0x0002
const CHUNK_XML = 0x0003
const CHUNK_XML_START_ELEMENT = 0x0102

// Pool flag: strings are UTF-8 (else UTF-16LE).
const FLAG_UTF8 = 1 << 8
// Pool flag: strings are sorted for binary-search lookup. A content edit can
// break the order, so the rewrite clears it; it is only a lookup hint.
const FLAG_SORTED = 1 << 0

// Where each format's pool chunk starts: right after the 8-byte RES_XML file
// header, or the 12-byte RES_TABLE one.
export const XML_POOL_OFFSET = 8
export const TABLE_POOL_OFFSET = 12

type Pool = {
  strings: string[]
  utf8: boolean
  flags: number
  headerSize: number
  chunkSize: number
}

function parsePool(file: Buffer, off: number): Pool {
  if (file.readUInt16LE(off) !== CHUNK_STRING_POOL) throw new Error(`No string pool chunk at offset ${off}`)
  let headerSize = file.readUInt16LE(off + 2)
  let chunkSize = file.readUInt32LE(off + 4)
  let count = file.readUInt32LE(off + 8)
  let styleCount = file.readUInt32LE(off + 12)
  if (styleCount !== 0) throw new Error("String pools with style spans are not supported")
  let flags = file.readUInt32LE(off + 16)
  let stringsStart = file.readUInt32LE(off + 20)
  let utf8 = (flags & FLAG_UTF8) !== 0

  let strings: string[] = []
  for (let i = 0; i < count; i++) {
    let p = off + stringsStart + file.readUInt32LE(off + headerSize + 4 * i)
    if (utf8) {
      // Two varlen prefixes (UTF-16 char count, then byte count), each one
      // byte, or two with the high bit marking the long form.
      if (file[p]! & 0x80) p += 2
      else p += 1
      let len = file[p]!
      if (len & 0x80) {
        len = ((len & 0x7f) << 8) | file[p + 1]!
        p += 2
      } else {
        p += 1
      }
      strings.push(file.subarray(p, p + len).toString("utf8"))
    } else {
      let len = file.readUInt16LE(p)
      if (len & 0x8000) {
        len = ((len & 0x7fff) << 16) | file.readUInt16LE(p + 2)
        p += 4
      } else {
        p += 2
      }
      strings.push(file.subarray(p, p + 2 * len).toString("utf16le"))
    }
  }
  return { strings, utf8, flags, headerSize, chunkSize }
}

// The varlen length prefix of UTF-8 pool strings: one byte, or two with the
// high bit set on the first. UTF-16 uses the same scheme on u16s; pools that
// long never occur in a manifest or a small arsc, so the long u16 form is
// unimplemented and oversized strings throw.
function encodeString(value: string, utf8: boolean): Buffer {
  if (utf8) {
    let bytes = Buffer.from(value, "utf8")
    let prefix = (len: number) => (len > 0x7f ? Buffer.from([0x80 | (len >> 8), len & 0xff]) : Buffer.from([len]))
    if (value.length > 0x7fff || bytes.length > 0x7fff) throw new Error("String too long for a pool entry")
    return Buffer.concat([prefix(value.length), prefix(bytes.length), bytes, Buffer.from([0])])
  }
  if (value.length >= 0x8000) throw new Error("String too long for a pool entry")
  let out = Buffer.alloc(2 + 2 * value.length + 2)
  out.writeUInt16LE(value.length, 0)
  out.write(value, 2, "utf16le")
  return out
}

// Replace pool strings by index and return the whole file with the rebuilt
// pool spliced in and the file-header size (u32 at offset 4 in both formats)
// fixed up.
export function replacePoolStrings(file: Buffer, poolOff: number, replacements: Map<number, string>): Buffer {
  let pool = parsePool(file, poolOff)
  for (let [index, value] of replacements) {
    if (index < 0 || index >= pool.strings.length) throw new Error(`String index ${index} out of range`)
    pool.strings[index] = value
  }

  let offsets = Buffer.alloc(4 * pool.strings.length)
  let encoded: Buffer[] = []
  let dataLen = 0
  for (let i = 0; i < pool.strings.length; i++) {
    offsets.writeUInt32LE(dataLen, 4 * i)
    let bytes = encodeString(pool.strings[i]!, pool.utf8)
    encoded.push(bytes)
    dataLen += bytes.length
  }
  let pad = (4 - (dataLen % 4)) % 4
  if (pad) encoded.push(Buffer.alloc(pad))
  dataLen += pad

  let header = Buffer.alloc(pool.headerSize)
  let stringsStart = pool.headerSize + offsets.length
  header.writeUInt16LE(CHUNK_STRING_POOL, 0)
  header.writeUInt16LE(pool.headerSize, 2)
  header.writeUInt32LE(stringsStart + dataLen, 4)
  header.writeUInt32LE(pool.strings.length, 8)
  header.writeUInt32LE(0, 12) // style count
  header.writeUInt32LE(pool.flags & ~FLAG_SORTED, 16)
  header.writeUInt32LE(stringsStart, 20)
  header.writeUInt32LE(0, 24) // styles start

  let out = Buffer.concat([
    file.subarray(0, poolOff),
    header,
    offsets,
    ...encoded,
    file.subarray(poolOff + pool.chunkSize),
  ])
  out.writeUInt32LE(out.length, 4)
  return out
}

// The pool index holding the manifest's package attribute value (the
// application id), found by actually parsing the element chunks rather than
// assuming a pool position: the root <manifest> element's "package"
// attribute. Both its raw-value and typed-value fields reference this one
// index, so replacing the string covers both.
export function manifestPackageIndex(file: Buffer): { index: number; value: string } {
  if (file.readUInt16LE(0) !== CHUNK_XML) throw new Error("Not a binary AndroidManifest.xml")
  let pool = parsePool(file, XML_POOL_OFFSET)
  let off = XML_POOL_OFFSET + pool.chunkSize
  while (off < file.length) {
    let type = file.readUInt16LE(off)
    let size = file.readUInt32LE(off + 4)
    if (type === CHUNK_XML_START_ELEMENT) {
      // Chunk header (8) + line number + comment = 16 bytes, then the
      // element: ns, name, attrStart, attrSize, attrCount (attrStart is
      // relative to the element, i.e. chunk offset + 16).
      let name = pool.strings[file.readUInt32LE(off + 20)]
      if (name !== "manifest") throw new Error(`Manifest root element is <${name}>, expected <manifest>`)
      let attrBase = off + 16 + file.readUInt16LE(off + 24)
      let attrSize = file.readUInt16LE(off + 26)
      let attrCount = file.readUInt16LE(off + 28)
      for (let i = 0; i < attrCount; i++) {
        let attr = attrBase + i * attrSize
        if (pool.strings[file.readUInt32LE(attr + 4)] === "package") {
          let index = file.readUInt32LE(attr + 8)
          return { index, value: pool.strings[index]! }
        }
      }
      throw new Error("<manifest> has no package attribute")
    }
    off += size
  }
  throw new Error("No element chunks in the manifest")
}

// The strings of a file's pool, for values located by content (the label in
// resources.arsc, whose resource-table position would otherwise take a full
// table parse to resolve).
export function poolStrings(file: Buffer, poolOff: number): string[] {
  if (poolOff === TABLE_POOL_OFFSET && file.readUInt16LE(0) !== CHUNK_TABLE) {
    throw new Error("Not a resources.arsc file")
  }
  return parsePool(file, poolOff).strings
}
