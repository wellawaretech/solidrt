// Zip read/write for APK patching. An APK is a zip with two extra rules the
// standard tools break: uncompressed entries must be 4-aligned and native libs
// 16 KB-page-aligned (Android 15 rejects unaligned installs), and the whole
// file is covered by the APK Signing Block digest (sign.ts), so the central
// directory is rewritten anyway. Entries are parsed off the central directory
// and their (possibly compressed) data is carried verbatim; writing rebuilds
// local headers with recomputed zero-fill extra-field padding, exactly the
// bytes zipalign -P 16 would produce (and what the shipped solidrt-go.apk
// carries: zero flags, no data descriptors, zero-padded extra fields).

// 16 KB zip alignment for stored native libs, so Android can mmap them from
// the APK on 16 KB-page devices (Android 15's install-time requirement).
const LIB_ALIGN = 16384
// zipalign's default for every other stored entry (resources.arsc, assets).
const STORED_ALIGN = 4
// Local file headers must not carry the data-descriptor flag: sizes and crc
// are written in the header itself (they are known from the central
// directory), and apksigner-style output never uses descriptors.
const FLAG_DATA_DESCRIPTOR = 0x08

export type ZipEntry = {
  name: Buffer // raw bytes, never re-encoded (flag bit 11 says how to read it)
  verMade: number
  verNeed: number
  flags: number
  method: number // 0 = stored, 8 = deflated
  time: number
  date: number
  crc: number
  usize: number
  intAttr: number
  extAttr: number
  data: Buffer // raw entry data, compressed when method = 8
}

const EOCD_SIG = 0x06054b50
const CD_SIG = 0x02014b50
const LOCAL_SIG = 0x04034b50
// EOCD is 22 bytes plus a comment of at most 65535 bytes; the signature scan
// from the end never needs to look further back than that.
const EOCD_SCAN_MAX = 22 + 65535

export function parseZip(file: Buffer): ZipEntry[] {
  let eocd = -1
  for (let i = file.length - 22; i >= Math.max(0, file.length - EOCD_SCAN_MAX); i--) {
    if (file.readUInt32LE(i) === EOCD_SIG) {
      eocd = i
      break
    }
  }
  if (eocd < 0) throw new Error("Not a zip: no end-of-central-directory record")
  let count = file.readUInt16LE(eocd + 10)
  let cdOffset = file.readUInt32LE(eocd + 16)
  if (count === 0xffff || cdOffset === 0xffffffff) throw new Error("Zip64 archives are not supported")

  let entries: ZipEntry[] = []
  let pos = cdOffset
  for (let i = 0; i < count; i++) {
    if (file.readUInt32LE(pos) !== CD_SIG) throw new Error(`Bad central directory record at ${pos}`)
    let csize = file.readUInt32LE(pos + 20)
    let nlen = file.readUInt16LE(pos + 28)
    let elen = file.readUInt16LE(pos + 30)
    let clen = file.readUInt16LE(pos + 32)
    let lhOff = file.readUInt32LE(pos + 42)
    let name = file.subarray(pos + 46, pos + 46 + nlen)
    // The local header's own name/extra lengths locate the data; the central
    // directory's extra field can differ from the local one (padding).
    if (file.readUInt32LE(lhOff) !== LOCAL_SIG) throw new Error(`Bad local header for ${name} at ${lhOff}`)
    let dataOff = lhOff + 30 + file.readUInt16LE(lhOff + 26) + file.readUInt16LE(lhOff + 28)
    entries.push({
      name: Buffer.from(name),
      verMade: file.readUInt16LE(pos + 4),
      verNeed: file.readUInt16LE(pos + 6),
      flags: file.readUInt16LE(pos + 8) & ~FLAG_DATA_DESCRIPTOR,
      method: file.readUInt16LE(pos + 10),
      time: file.readUInt16LE(pos + 12),
      date: file.readUInt16LE(pos + 14),
      crc: file.readUInt32LE(pos + 16),
      usize: file.readUInt32LE(pos + 24),
      intAttr: file.readUInt16LE(pos + 34),
      extAttr: file.readUInt32LE(pos + 38),
      data: Buffer.from(file.subarray(dataOff, dataOff + csize)),
    })
    pos += 46 + nlen + elen + clen
  }
  return entries
}

// Data-offset alignment for an entry; compressed data is extracted anyway, so
// only stored entries align.
function alignFor(entry: ZipEntry): number {
  if (entry.method !== 0) return 1
  return entry.name.toString("latin1").endsWith(".so") ? LIB_ALIGN : STORED_ALIGN
}

// The entries region and the central directory, as separate buffers: the APK
// Signing Block is inserted between them (sign.ts), which shifts the central
// directory but not the local header offsets it records.
export function writeZip(entries: ZipEntry[]): { local: Buffer; cd: Buffer } {
  let localParts: Buffer[] = []
  let cdParts: Buffer[] = []
  let pos = 0
  for (let entry of entries) {
    let align = alignFor(entry)
    let dataOff = pos + 30 + entry.name.length
    let pad = (align - (dataOff % align)) % align
    let header = Buffer.alloc(30 + entry.name.length + pad) // zero fill = the padding
    header.writeUInt32LE(LOCAL_SIG, 0)
    header.writeUInt16LE(entry.verNeed, 4)
    header.writeUInt16LE(entry.flags, 6)
    header.writeUInt16LE(entry.method, 8)
    header.writeUInt16LE(entry.time, 10)
    header.writeUInt16LE(entry.date, 12)
    header.writeUInt32LE(entry.crc, 14)
    header.writeUInt32LE(entry.data.length, 18)
    header.writeUInt32LE(entry.usize, 22)
    header.writeUInt16LE(entry.name.length, 26)
    header.writeUInt16LE(pad, 28)
    entry.name.copy(header, 30)
    localParts.push(header, entry.data)

    let cd = Buffer.alloc(46 + entry.name.length)
    cd.writeUInt32LE(CD_SIG, 0)
    cd.writeUInt16LE(entry.verMade, 4)
    cd.writeUInt16LE(entry.verNeed, 6)
    cd.writeUInt16LE(entry.flags, 8)
    cd.writeUInt16LE(entry.method, 10)
    cd.writeUInt16LE(entry.time, 12)
    cd.writeUInt16LE(entry.date, 14)
    cd.writeUInt32LE(entry.crc, 16)
    cd.writeUInt32LE(entry.data.length, 20)
    cd.writeUInt32LE(entry.usize, 24)
    cd.writeUInt16LE(entry.name.length, 28)
    cd.writeUInt16LE(entry.intAttr, 34)
    cd.writeUInt32LE(entry.extAttr, 38)
    cd.writeUInt32LE(pos, 42)
    entry.name.copy(cd, 46)
    cdParts.push(cd)

    pos += header.length + entry.data.length
  }
  return { local: Buffer.concat(localParts), cd: Buffer.concat(cdParts) }
}

// End-of-central-directory record pointing the central directory at cdOffset.
// The digest computation and the final file disagree on that offset (the
// signing block sits in between), so the caller builds it twice.
export function buildEocd(count: number, cdSize: number, cdOffset: number): Buffer {
  let eocd = Buffer.alloc(22)
  eocd.writeUInt32LE(EOCD_SIG, 0)
  eocd.writeUInt16LE(count, 8)
  eocd.writeUInt16LE(count, 10)
  eocd.writeUInt32LE(cdSize, 12)
  eocd.writeUInt32LE(cdOffset, 16)
  return eocd
}

// Plain zlib-polynomial CRC-32 for new stored entries; small enough to own
// rather than depend on a runtime-specific hash API.
let CRC_TABLE: Uint32Array | null = null
export function crc32(data: Buffer): number {
  if (!CRC_TABLE) {
    CRC_TABLE = new Uint32Array(256)
    for (let n = 0; n < 256; n++) {
      let c = n
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
      CRC_TABLE[n] = c >>> 0
    }
  }
  let crc = 0xffffffff
  for (let byte of data) crc = CRC_TABLE[(crc ^ byte) & 0xff]! ^ (crc >>> 8)
  return (crc ^ 0xffffffff) >>> 0
}
