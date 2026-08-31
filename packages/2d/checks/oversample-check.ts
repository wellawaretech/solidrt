// Check for the oversample auto-pick math (oversample-math.ts): fit
// rounding and bounds, the shrink hysteresis and maxOversample cap in
// pickOversample, and the rotation divide-out in tileWorldScale - asserted
// with a full-turn camera sweep against an AABB oracle, so the class of bug
// where a rotating camera re-bakes every chunk cannot come back silently.
// Pure-module input only (oversample-math.ts imports nothing), so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/2d/checks/oversample-check.ts | target/release/flux - [seed]
//
// A seeded PRNG keeps failures reproducible - rerun with the printed seed.
// A failure prints FAIL lines and throws at the end; the flux binary exits 0
// regardless, so read the output, not the exit code.

import { argv } from "flux:process"
import { fitOversampleWithin, pickOversample, tileWorldScale } from "../src/oversample-math.ts"

let seed = Number(argv[0] ?? Math.floor(Math.random() * 0xffffffff))
console.log("seed", seed)

let s = seed >>> 0
function rand(): number {
  s = (Math.imul(s, 1664525) + 1013904223) >>> 0
  return s / 4294967296
}
function range(lo: number, hi: number): number {
  return lo + rand() * (hi - lo)
}

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}

// Generous bounds that never bind, for the cases probing something else.
const BIG_BUDGET = 1e12
const BIG_MAX_SIZE = 1e6

// --- fitOversampleWithin: rounding and the three bounds ---

let fit = (scale: number, w = 100, h = 100, budget = BIG_BUDGET, maxSize = BIG_MAX_SIZE) =>
  fitOversampleWithin(scale, w, h, budget, maxSize)

if (fit(1) !== 1) fail(`fit(1) = ${fit(1)}, want 1`)
if (fit(2.5) !== 3) fail(`fit(2.5) = ${fit(2.5)}, want 3 (ceiling)`)
if (fit(0.3) !== 1) fail(`fit(0.3) = ${fit(0.3)}, want 1 (floor at 1)`)
// FIT_EPSILON: a scale that is 3 up to float noise picks 3, not 4.
if (fit(3 + 1e-7) !== 3) fail(`fit(3 + 1e-7) = ${fit(3 + 1e-7)}, want 3 (epsilon)`)
if (fit(3.1) !== 4) fail(`fit(3.1) = ${fit(3.1)}, want 4`)
// Budget bounds the SCALE, not the rounded factor: 320 x 200 against a
// 2560 x 1440 window fits at sqrt(budget / texels) = 7.589..., which a
// larger scale clamps to and the ceiling rounds to 8 (the doc's example).
{
  let n = fit(20, 320, 200, 2560 * 1440)
  if (n !== 8) fail(`budget-bound fit = ${n}, want 8`)
}
// Device bound: a 1000 x 1000 target on a 4096 device caps at 4.
{
  let n = fit(10, 1000, 1000, BIG_BUDGET, 4096)
  if (n !== 4) fail(`device-bound fit = ${n}, want 4`)
}

// --- pickOversample: hysteresis, cap, validation ---

let pick = (current: number, scale: number, cap?: number) =>
  pickOversample(current, scale, 100, 100, BIG_BUDGET, BIG_MAX_SIZE, cap)

// Growth is immediate.
if (pick(1, 2.2) !== 3) fail(`grow pick(1, 2.2) = ${pick(1, 2.2)}, want 3`)
// Same factor: keep (null), not a redundant set.
if (pick(3, 2.9) !== null) fail(`pick(3, 2.9) = ${pick(3, 2.9)}, want null (same factor)`)
// Shrink hysteresis: at current 3 the lower boundary is 2, the margin holds
// until scale drops below 2 - 0.25 = 1.75.
if (pick(3, 1.9) !== null) fail(`pick(3, 1.9) = ${pick(3, 1.9)}, want null (hysteresis holds)`)
if (pick(3, 1.7) !== 2) fail(`pick(3, 1.7) = ${pick(3, 1.7)}, want 2 (past the margin)`)
// The cap overrides the hysteresis: lowering maxOversample is an explicit ask.
if (pick(3, 2.9, 1) !== 1) fail(`pick(3, 2.9, cap 1) = ${pick(3, 2.9, 1)}, want 1 (cap overrides)`)
// A cap above the pick changes nothing.
if (pick(1, 2.2, 8) !== 3) fail(`pick(1, 2.2, cap 8) = ${pick(1, 2.2, 8)}, want 3`)
// Cap validation throws (dev policy).
for (let bad of [0, 1.5, -1]) {
  let threw = false
  try {
    pick(1, 1, bad)
  } catch {
    threw = true
  }
  if (!threw) fail(`pick with cap ${bad} did not throw`)
}

// --- tileWorldScale: the rotation divide-out ---

// Oracle: the AABB of a worldW x worldH rect scaled by zoom and rotated -
// what getBoundingBoxViewport measures for the world view.
function rotatedBox(worldW: number, worldH: number, zoom: number, r: number): [number, number] {
  let cos = Math.abs(Math.cos(r))
  let sin = Math.abs(Math.sin(r))
  return [zoom * (worldW * cos + worldH * sin), zoom * (worldW * sin + worldH * cos)]
}

// The divide-out must recover the zoom exactly (up to float noise) at any
// rotation, and the pick must therefore never change over a full turn - the
// rotating-camera re-bake bug, asserted dead.
const SWEEPS = 500
const TURN_STEPS = 360
const SCALE_TOLERANCE = 1e-9 // relative: pure float noise, no geometry slack
for (let i = 0; i < SWEEPS; i++) {
  let worldW = range(256, 8192)
  let worldH = range(256, 8192)
  let zoom = range(0.2, 5)
  let changes = 0
  let current = 0
  for (let step = 0; step <= TURN_STEPS; step++) {
    let r = (step / TURN_STEPS) * 2 * Math.PI
    let [boxW, boxH] = rotatedBox(worldW, worldH, zoom, r)
    let scale = tileWorldScale(boxW, boxH, worldW, worldH, r)
    if (Math.abs(scale - zoom) > zoom * SCALE_TOLERANCE) {
      fail(`divide-out at rot ${r}: scale ${scale}, want zoom ${zoom} (world ${worldW} x ${worldH})`)
      break
    }
    let n = pickOversample(current, scale, 512, 512, BIG_BUDGET, BIG_MAX_SIZE, undefined)
    if (n !== null) {
      current = n
      changes++
    }
  }
  if (changes > 1) fail(`full-turn sweep re-picked ${changes} times (world ${worldW} x ${worldH}, zoom ${zoom})`)
}

if (failures === 0) console.log(`PASS: fit bounds, hysteresis/cap, and ${SWEEPS} full-turn sweeps hold`)
else throw new Error(`${failures} FAILURES (seed ${seed})`)
