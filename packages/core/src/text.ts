import type { MeasureTextOptions } from "./types"

export function measureText(text: string, options?: MeasureTextOptions): { width: number, height: number } {
  return ffi.measureText(text, options)
}