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
const CHUNK_XML_END_ELEMENT = 0x0103

// Attribute value types (Res_value dataType).
const TYPE_STRING = 0x03
const TYPE_INT_BOOLEAN = 0x12
// A boolean attribute's data word: all ones for true, zero for false.
export const BOOL_TRUE = 0xffffffff

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

// Replace pool strings by index, append new ones after the last (their
// indices are the old count upward, which is why appending never moves a
// reference), and return the whole file with the rebuilt pool spliced in and
// the file-header size (u32 at offset 4 in both formats) fixed up.
export function replacePoolStrings(file: Buffer, poolOff: number, replacements: Map<number, string>, append: string[] = []): Buffer {
  let pool = parsePool(file, poolOff)
  for (let [index, value] of replacements) {
    if (index < 0 || index >= pool.strings.length) throw new Error(`String index ${index} out of range`)
    pool.strings[index] = value
  }
  pool.strings.push(...append)

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

// One element chunk of a compiled manifest. Chunk header (8) + line number
// + comment = 16 bytes, then for a start element: ns, name, attrStart,
// attrSize, attrCount (attrStart is relative to the element, i.e. chunk
// offset + 16); an end element carries only ns and name.
type Element = { off: number; size: number; end: boolean; name: string; attrs: Attribute[] }
// Attribute: ns u32, name u32, rawValue u32, then the typed value (size u16,
// res0 u8, dataType u8, data u32). `off` is the attribute's start.
type Attribute = { off: number; name: string; type: number; data: number }

// Every element chunk of a compiled manifest, in file order, found by
// actually parsing the chunks rather than assuming pool positions.
function* elements(file: Buffer, pool: Pool): Generator<Element> {
  if (file.readUInt16LE(0) !== CHUNK_XML) throw new Error("Not a binary AndroidManifest.xml")
  let off = XML_POOL_OFFSET + pool.chunkSize
  while (off < file.length) {
    let type = file.readUInt16LE(off)
    let size = file.readUInt32LE(off + 4)
    if (type === CHUNK_XML_START_ELEMENT || type === CHUNK_XML_END_ELEMENT) {
      let name = pool.strings[file.readUInt32LE(off + 20)]!
      let attrs: Attribute[] = []
      if (type === CHUNK_XML_START_ELEMENT) {
        let attrBase = off + 16 + file.readUInt16LE(off + 24)
        let attrSize = file.readUInt16LE(off + 26)
        let attrCount = file.readUInt16LE(off + 28)
        for (let i = 0; i < attrCount; i++) {
          let attr = attrBase + i * attrSize
          attrs.push({
            off: attr,
            name: pool.strings[file.readUInt32LE(attr + 4)]!,
            type: file[attr + 15]!,
            data: file.readUInt32LE(attr + 16),
          })
        }
      }
      yield { off, size, end: type === CHUNK_XML_END_ELEMENT, name, attrs }
    }
    off += size
  }
}

// The identity attributes of the root <manifest> element. The package
// (application id) and versionName values are pool indices - both the
// raw-value and typed-value fields of their attributes reference the same
// index, so replacing the pool string covers both. versionCode is a typed
// integer, so what is returned is the absolute byte offset of its
// attribute's data word, for an in-place write (which must happen before
// any pool rewrite recomputes the file).
export function manifestInfo(file: Buffer): {
  packageIndex: number
  packageValue: string
  versionNameIndex: number
  versionCodeOffset: number
} {
  let pool = parsePool(file, XML_POOL_OFFSET)
  for (let el of elements(file, pool)) {
    if (el.name !== "manifest") throw new Error(`Manifest root element is <${el.name}>, expected <manifest>`)
    let packageIndex = -1
    let versionNameIndex = -1
    let versionCodeOffset = -1
    for (let attr of el.attrs) {
      if (attr.name === "package") packageIndex = attr.data
      if (attr.name === "versionName") versionNameIndex = attr.data
      if (attr.name === "versionCode") versionCodeOffset = attr.off + 16
    }
    if (packageIndex < 0) throw new Error("<manifest> has no package attribute")
    if (versionNameIndex < 0 || versionCodeOffset < 0) {
      throw new Error("<manifest> has no versionCode/versionName attributes")
    }
    return { packageIndex, packageValue: pool.strings[packageIndex]!, versionNameIndex, versionCodeOffset }
  }
  throw new Error("No element chunks in the manifest")
}

// The absolute byte offset of a boolean attribute's data word on the
// <application> element (allowBackup), for an in-place write of BOOL_TRUE
// or 0 before any pool rewrite.
export function applicationFlagOffset(file: Buffer, name: string): number {
  let pool = parsePool(file, XML_POOL_OFFSET)
  for (let el of elements(file, pool)) {
    if (el.end || el.name !== "application") continue
    let attr = el.attrs.find((a) => a.name === name)
    if (!attr) throw new Error(`<application> has no ${name} attribute`)
    if (attr.type !== TYPE_INT_BOOLEAN) throw new Error(`<application> ${name} is not a boolean`)
    return attr.off + 16
  }
  throw new Error("No <application> element in the manifest")
}

// Declare permissions: one <uses-permission android:name="..."/> per name
// not already declared, inserted right before <application>. The runner
// declares none itself, so the element chunks are built here: the name
// attribute and the android namespace are taken from any existing
// android:name attribute (every activity has one), the element name and the
// values are appended to the pool. The pool rewrite that appends the strings
// also fixes the file size, which the splice grew.
export function addUsesPermissions(file: Buffer, names: string[]): Buffer {
  let pool = parsePool(file, XML_POOL_OFFSET)
  let declared: string[] = []
  let nameAttr: Attribute | undefined
  let insertAt = -1
  let start: Element | undefined
  for (let el of elements(file, pool)) {
    if (el.name === "application" && !el.end) {
      insertAt = el.off
      break
    }
    if (el.name !== "uses-permission") {
      nameAttr ??= el.attrs.find((a) => a.name === "name" && a.type === TYPE_STRING)
      continue
    }
    if (!el.end) {
      start = el
      continue
    }
    let attr = start?.attrs[0]
    if (!start || start.attrs.length !== 1 || attr!.name !== "name" || attr!.type !== TYPE_STRING) {
      throw new Error("<uses-permission> does not carry exactly one string name attribute")
    }
    declared.push(pool.strings[attr!.data]!)
    nameAttr ??= attr
    start = undefined
  }
  if (insertAt < 0) throw new Error("No <application> element in the manifest")
  if (!nameAttr) throw new Error("Manifest has no android:name attribute to model the permission's on")

  let added = names.filter((name, i) => !declared.includes(name) && names.indexOf(name) === i)
  if (added.length === 0) return file

  // Pool additions: the element name (unless the runner already has one)
  // followed by the values, in that order.
  let append: string[] = []
  let elementIndex = pool.strings.indexOf("uses-permission")
  if (elementIndex < 0) {
    elementIndex = pool.strings.length
    append.push("uses-permission")
  }
  let nsIndex = file.readInt32LE(nameAttr.off)
  let nameIndex = file.readUInt32LE(nameAttr.off + 4)
  let chunks = added.map((name) => {
    let valueIndex = pool.strings.length + append.length
    append.push(name)
    return usesPermissionChunks(elementIndex, nsIndex, nameIndex, valueIndex)
  })
  let spliced = Buffer.concat([file.subarray(0, insertAt), ...chunks, file.subarray(insertAt)])
  return replacePoolStrings(spliced, XML_POOL_OFFSET, new Map(), append)
}

// A start + end element chunk pair for <uses-permission android:name="..."/>,
// laid out as aapt2 writes them: 16-byte chunk header (type, header size,
// chunk size, line number, comment), then the element body. The start
// element carries one attribute whose raw value and typed string value both
// reference the value's pool index.
function usesPermissionChunks(elementIndex: number, nsIndex: number, nameIndex: number, valueIndex: number): Buffer {
  let start = Buffer.alloc(56)
  start.writeUInt16LE(CHUNK_XML_START_ELEMENT, 0)
  start.writeUInt16LE(16, 2)
  start.writeUInt32LE(56, 4)
  start.writeUInt32LE(0, 8) // line number
  start.writeInt32LE(-1, 12) // comment
  start.writeInt32LE(-1, 16) // element namespace
  start.writeUInt32LE(elementIndex, 20)
  start.writeUInt16LE(20, 24) // attrStart
  start.writeUInt16LE(20, 26) // attrSize
  start.writeUInt16LE(1, 28) // attrCount
  // id, class and style attribute indices (30, 32, 34) stay 0: none.
  start.writeInt32LE(nsIndex, 36)
  start.writeUInt32LE(nameIndex, 40)
  start.writeUInt32LE(valueIndex, 44)
  start.writeUInt16LE(8, 48) // typed value size
  start[50] = 0 // res0
  start[51] = TYPE_STRING
  start.writeUInt32LE(valueIndex, 52)

  let end = Buffer.alloc(24)
  end.writeUInt16LE(CHUNK_XML_END_ELEMENT, 0)
  end.writeUInt16LE(16, 2)
  end.writeUInt32LE(24, 4)
  end.writeUInt32LE(0, 8)
  end.writeInt32LE(-1, 12)
  end.writeInt32LE(-1, 16)
  end.writeUInt32LE(elementIndex, 20)
  return Buffer.concat([start, end])
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
