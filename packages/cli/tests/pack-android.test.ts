// The icon background through pack --apk: the config spellings
// (iconBackgroundRgb) and the splash color patched into the resource table
// (splash.ts) over a hand-built table holding the placeholder. The slot PNG
// (icon.ts) needs node:zlib, which flux:test does not bundle.
import { test, expect } from "flux:test"
import { iconBackgroundRgb } from "../src/lib/icon-background.ts"
import { patchSplash } from "../src/pack/android/splash.ts"
import { applicationIdRenames } from "../src/pack/android/strings.ts"

// An RGB8 color Res_value as aapt2 writes it: size 8, zero, type 0x1d, data.
function colorValue(data: number): number[] {
  return [8, 0, 0, 0x1d, data & 0xff, (data >>> 8) & 0xff, (data >>> 16) & 0xff, data >>> 24]
}

const PLACEHOLDER = 0xffc39d6f
const FILLER = [1, 2, 3, 4, 5, 6, 7, 8]

// A stand-in resource table: filler, then each color value and filler.
function table(...colors: number[]): Uint8Array {
  return new Uint8Array([...FILLER, ...colors.flatMap((c) => [...colorValue(c), ...FILLER])])
}

function patched(arsc: Uint8Array, background: [number, number, number]): Uint8Array {
  patchSplash(arsc, background)
  return arsc
}

test("the config takes #rrggbb only", () => {
  expect(iconBackgroundRgb("#1c1926")).toEqual([0x1c, 0x19, 0x26])
  expect(iconBackgroundRgb("#1C1926")).toEqual([0x1c, 0x19, 0x26])
  for (let bad of ["#fff", "1c1926", "#1c192680", "#gggggg", "transparent", ""]) {
    expect(iconBackgroundRgb(bad)).toBe(null)
  }
})

test("the splash takes the icon background", () => {
  expect(patched(table(PLACEHOLDER), [0x1c, 0x19, 0x26])).toEqual(table(0xff1c1926))
  expect(patched(table(1, PLACEHOLDER, 2), [0, 0, 0])).toEqual(table(1, 0xff000000, 2))
})

test("a table without the placeholder, or with two, is refused", () => {
  expect(() => patchSplash(table(0xff1a1a1a), [0, 0, 0])).toThrow("older runner build")
  expect(() => patchSplash(table(PLACEHOLDER, PLACEHOLDER), [0, 0, 0])).toThrow("ambiguous")
})

test("the application id and every name derived from it move to the app's", () => {
  let pool = [
    "com.solidrt.runner",
    "com.solidrt.runner.androidx-startup",
    "com.solidrt.runner.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
    "com.solidrt.app.MainActivity",
    "com.solidrt.runnerx",
    "versionName",
  ]
  expect([...applicationIdRenames(pool, "com.solidrt.runner", "com.example.app")]).toEqual([
    [0, "com.example.app"],
    [1, "com.example.app.androidx-startup"],
    [2, "com.example.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"],
  ])
})
