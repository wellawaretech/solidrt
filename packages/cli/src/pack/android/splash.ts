// The splash color patch (patchApk calls it), in a module of its own so
// flux:test can load it: apk.ts and icon.ts need node:zlib, which flux:test
// bundles do not have.

import type { Rgb } from "../../lib/icon-background"

// The API 31+ splash background the runner's theme names (runner
// res/values/colors.xml, splash_background), located by value like the
// label in apk.ts: a placeholder color no other resource uses. aapt2 compiles
// "#rrggbb" to an RGB8 Res_value (u16 size, u8 zero, u8 type, u32 data)
// whose data word is 0xffrrggbb.
const SPLASH_PLACEHOLDER = 0xffc39d6f
const RES_VALUE_SIZE = 8
const TYPE_INT_COLOR_RGB8 = 0x1d
// An RGB8 value's data word carries full alpha above the color.
const OPAQUE_ALPHA = 0xff000000

function colorValue(data: number): Uint8Array {
  let value = new Uint8Array(RES_VALUE_SIZE)
  let view = new DataView(value.buffer)
  view.setUint16(0, RES_VALUE_SIZE, true)
  value[3] = TYPE_INT_COLOR_RGB8
  view.setUint32(4, data, true)
  return value
}

function indexOf(bytes: Uint8Array, pattern: Uint8Array, from: number): number {
  for (let i = from; i + pattern.length <= bytes.length; i++) {
    if (pattern.every((byte, j) => bytes[i + j] === byte)) return i
  }
  return -1
}

/** Replace, in place, the splash placeholder in the resource table `arsc`
 * with the icon background. */
export function patchSplash(arsc: Uint8Array, background: Rgb) {
  let placeholder = colorValue(SPLASH_PLACEHOLDER)
  let at = indexOf(arsc, placeholder, 0)
  if (at < 0) throw new Error("Base APK splash color not found in resources.arsc; is the base an older runner build?")
  if (indexOf(arsc, placeholder, at + 1) >= 0) throw new Error("Base APK splash color is ambiguous in resources.arsc")
  let [r, g, b] = background
  arsc.set(colorValue((OPAQUE_ALPHA | (r << 16) | (g << 8) | b) >>> 0), at)
}
