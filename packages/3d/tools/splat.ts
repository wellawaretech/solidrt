#!/usr/bin/env bun
/// <reference path="./node-env.d.ts" />

// sol tool 3d/splat: bake a captured splat cloud into a .sol3s file - the
// parse, the chunk quantization and SH packing (src/splat-data.ts) run
// once here under bun, the result written in the exact texel layout
// createSplatMesh uploads and the engine sorts, without any per-splat
// work at runtime. Put the output under assets/ so it ships with the app.
// Everything here comes from the runtime-free `@solidrt/3d/splat` entry,
// the same surface an app's own bake script uses.
//
//   sol tool 3d/splat <in.ply|in.splat|in.spz> [-o <out.sol3s>] [--sh <0..3>] [--keep-orientation] [--antialiased]
//
// Inputs: a trainer's .ply, the de-facto .splat interchange, or Niantic's
// .spz (gunzipped here; version 2 or 3). A y-down capture (.ply, .splat)
// is stood up to y-up unless --keep-orientation; .spz is y-up already.
// --sh bakes that many spherical-harmonic bands beyond the base color
// (capped at the capture's; default 0 - degree 1/2/3 adds 16/32/48 bytes
// per splat of texture). --antialiased marks a capture trained with the
// antialiasing compensation (Mip-Splatting, nerfstudio's antialiased
// mode), which the renderer then applies. The bake is lossy (positions
// and scales quantized against 256-splat chunks, SH to bytes) and prints
// what it lost: the chunk position step against each splat's largest
// axis (median, 99th percentile, the share above a tenth), and the SH
// coefficients clamped.

import { readFileSync, writeFileSync } from "node:fs"
import { gunzipSync } from "node:zlib"
import { basename, extname } from "node:path"
import { encodeSplat, parseSplat, SPLAT_STEP_VISIBLE } from "../src/splat-data.ts"

function usage(error?: string): never {
  if (error) console.error(error)
  console.log("Usage: sol tool 3d/splat <in.ply|in.splat|in.spz> [-o <out.sol3s>] [--sh <0..3>] [--keep-orientation] [--antialiased]")
  process.exit(error ? 1 : 0)
}

let input: string | undefined
let output: string | undefined
let keepOrientation = false
let antialiased = false
let shDegree = 0
let args = process.argv.slice(2)
for (let i = 0; i < args.length; i++) {
  let arg = args[i]!
  if (arg === "--help" || arg === "-h") usage()
  else if (arg === "-o" || arg === "--output") {
    output = args[++i]
    if (output === undefined) usage("Missing value for " + arg)
  } else if (arg === "--sh") {
    let value = args[++i]
    if (value === undefined) usage("Missing value for " + arg)
    shDegree = Number(value)
    if (!(Number.isInteger(shDegree) && shDegree >= 0 && shDegree <= 3)) usage("--sh takes a degree 0..3, got " + value)
  } else if (arg === "--keep-orientation") keepOrientation = true
  else if (arg === "--antialiased") antialiased = true
  else if (arg.startsWith("-")) usage("Unknown option " + arg)
  else if (input === undefined) input = arg
  else usage("Unexpected argument " + arg)
}
if (input === undefined) usage("Missing input file")
if (output === undefined) output = basename(input, extname(input)) + ".sol3s"

let bytes = new Uint8Array(readFileSync(input))
// .spz is gzip-compressed; the parser takes the inflated bytes.
if (bytes.length >= 2 && bytes[0] === 0x1f && bytes[1] === 0x8b) bytes = new Uint8Array(gunzipSync(bytes))
let started = performance.now()
let data = parseSplat(bytes, { keepOrientation, shDegree, antialiased })
let parsed = performance.now() - started
let encoded = encodeSplat(data)
writeFileSync(output, encoded)

let b = data.bounds
let extent = (i: number) => (b[i + 3]! - b[i]!).toFixed(1)
let report = data.report!
console.log(
  `${output}: ${data.count} splats, bounds ${extent(0)} x ${extent(1)} x ${extent(2)}, SH degree ${data.shDegree}` +
    `${data.antialiased ? ", antialiased" : ""}, ${(encoded.byteLength / (1024 * 1024)).toFixed(1)} MiB (baked in ${parsed.toFixed(0)} ms)`,
)
let pct = (v: number) => (v * 100).toFixed(2) + "%"
console.log(
  `  ${report.chunks} chunks; position step over the splat's largest axis: median ${pct(report.positionStep.median)}, ` +
    `99th percentile ${pct(report.positionStep.p99)}, ${pct(report.positionStep.visibleShare)} of splats above ${pct(SPLAT_STEP_VISIBLE)}; ` +
    `${report.shClamped} SH coefficients clamped`,
)
