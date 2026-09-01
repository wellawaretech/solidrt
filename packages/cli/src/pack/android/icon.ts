// The Android launcher icon's two patchable slots (see the runner's
// ic_launcher_prod.xml): the foreground is the app's own square PNG, dropped
// into the safe-zone inset as-is; the background is a 1x1 PNG of the
// configured color that Android stretches full-bleed. Nothing is ever
// resampled here - Android does all scaling.

import { deflateRawSync } from "node:zlib"
import { crc32 } from "./zip"

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

export function isPng(bytes: Buffer): boolean {
  return bytes.length > PNG_SIGNATURE.length && bytes.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)
}

// One PNG chunk: length, type, data, CRC over type + data.
function pngChunk(type: string, data: Buffer): Buffer {
  let out = Buffer.alloc(12 + data.length)
  out.writeUInt32BE(data.length, 0)
  out.write(type, 4, "latin1")
  data.copy(out, 8)
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length)
  return out
}

/// A 1x1 opaque RGB PNG of `color` ("#rrggbb"): the adaptive-icon background
/// slot. Hand-assembled because it is fixed-shape and tiny; zlib provides the
/// (one-pixel) IDAT stream.
export function backgroundPixel(color: string): Buffer {
  let rgb = [1, 3, 5].map((i) => parseInt(color.slice(i, i + 2), 16))
  let ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(1, 0) // width
  ihdr.writeUInt32BE(1, 4) // height
  ihdr[8] = 8 // bit depth
  ihdr[9] = 2 // color type: truecolor
  // compression, filter, interlace stay 0
  // Scanline: filter byte 0, then the pixel. zlib format = 2-byte header +
  // raw deflate + adler32.
  let scanline = Buffer.from([0, ...rgb])
  let adler = adler32(scanline)
  let idat = Buffer.concat([Buffer.from([0x78, 0x01]), deflateRawSync(scanline), adler])
  return Buffer.concat([PNG_SIGNATURE, pngChunk("IHDR", ihdr), pngChunk("IDAT", idat), pngChunk("IEND", Buffer.alloc(0))])
}

function adler32(data: Buffer): Buffer {
  // Standard zlib checksum; modulo far below overflow for our 4 bytes.
  const MOD_ADLER = 65521
  let a = 1
  let b = 0
  for (let byte of data) {
    a = (a + byte) % MOD_ADLER
    b = (b + a) % MOD_ADLER
  }
  let out = Buffer.alloc(4)
  out.writeUInt32BE(((b << 16) | a) >>> 0, 0)
  return out
}
