// The `solidrt.windowSize` key in package.json: the size a desktop window
// opens at, in logical points (the unit `windowSize()` reports), the same at
// every display scale:
//
//   "solidrt": {
//     "windowSize": { "width": 900, "height": 600 }
//   }
//
// A pack carries it in the manifest, which lattice reads before the window
// exists; `sol run` (server/mode.ts) and `sol client` pass it to the local
// client as `--size`, and a typed `--size` wins. Android and the TV ignore
// it: the app is the whole screen. No maximum: a size larger than the
// display goes to the OS as asked (pack/docs.md). Pure: the shape rules,
// with no file system, so the bun commands and the flux dev server share
// them and they are testable on flux (tests/project-window-size.test.ts).

/** A window size in logical points, each side a whole number of at least 1. */
export type WindowSize = { width: number; height: number }

/** The sides a window size holds, and the only keys its object accepts. */
export const WINDOW_SIZE_SIDES = ["width", "height"]

/**
 * The first thing wrong with `raw`, the key as package.json has it, or null
 * when it is well-formed: an object with exactly "width" and "height", each
 * a whole number of at least 1. A null key is unset and never reaches here.
 */
export function checkWindowSize(raw: unknown): string | null {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return '"solidrt": "windowSize" must be an object with "width" and "height"'
  }
  let size = raw as Record<string, unknown>
  for (let key of Object.keys(size)) {
    if (!WINDOW_SIZE_SIDES.includes(key)) return `"solidrt.windowSize": unknown key "${key}"`
  }
  for (let side of WINDOW_SIZE_SIDES) {
    if (!(side in size)) return `"solidrt.windowSize": "${side}" is missing`
    let value = size[side]
    if (typeof value !== "number" || !Number.isInteger(value) || value < 1) {
      return `"solidrt.windowSize": "${side}" must be a whole number of at least 1 (got ${JSON.stringify(value)})`
    }
  }
  return null
}

/** A window size as the client's `--size` value ("900x600"); undefined without one. */
export function windowSizeFlag(size: WindowSize | undefined): string | undefined {
  return size ? `${size.width}x${size.height}` : undefined
}
