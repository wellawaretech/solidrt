import { colord, extend } from "colord"
import namesPlugin from "colord/plugins/names"
extend([namesPlugin])

// Parses any CSS color string and returns a packed u32: 0xRRGGBBAA
export function parseColorToU32(color: string): number {
  let { r, g, b, a } = colord(color).toRgb()
  return (((r & 0xFF) << 24) | ((g & 0xFF) << 16) | ((b & 0xFF) << 8) | ((a * 255) & 0xFF)) >>> 0
}
