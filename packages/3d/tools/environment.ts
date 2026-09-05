#!/usr/bin/env bun

// srt tool 3d/environment: bake a Radiance .hdr panorama into a .srte
// environment file - the six cube faces and their GGX-prefiltered mip
// chain in linear float, the exact chain loadEnvironment uploads as an
// explicit rgba16f cube (no runtime conversion, no generated mipmaps, no
// half-float render support needed). Unity convolves its reflection
// probes at import the same way; Three (PMREMGenerator) and Godot do it
// at runtime every time the sky changes. Put the output under assets/ so
// it ships with the app.
//
//   srt tool 3d/environment <in.hdr> [-o <out.srte>] [--size <edge>]
//
// --size is the cube's face edge: 128 (default) is Unity's probe default
// and enough for every surface short of a mirror; 256 (Godot's radiance
// default, Three's PMREM cap) for a mirror-finish showpiece, at four
// times the bytes (a float32 file is 21 KiB per 1k texels: 2 MiB at 128).

import { readFileSync, writeFileSync } from "node:fs"
import { basename, extname } from "node:path"
import { decodeHdr, encodeEnvironment, panoramaToCube, prefilterCube } from "../src/environment-bake.ts"

// The face edge default and its allowed range (powers of two).
const DEFAULT_SIZE = 128
const MIN_SIZE = 16
const MAX_SIZE = 1024

function usage(error?: string): never {
  if (error) console.error(error)
  console.log("Usage: srt tool 3d/environment <in.hdr> [-o <out.srte>] [--size <edge>]")
  process.exit(error ? 1 : 0)
}

let input: string | undefined
let output: string | undefined
let size = DEFAULT_SIZE
let args = process.argv.slice(2)
for (let i = 0; i < args.length; i++) {
  let arg = args[i]!
  if (arg === "--help" || arg === "-h") usage()
  else if (arg === "-o" || arg === "--output") {
    output = args[++i]
    if (output === undefined) usage("Missing value for " + arg)
  } else if (arg === "--size") {
    let value = args[++i]
    if (value === undefined) usage("Missing value for --size")
    size = Number(value)
    if (!Number.isInteger(size) || size < MIN_SIZE || size > MAX_SIZE || (size & (size - 1)) !== 0) {
      usage("--size must be a power of two from " + MIN_SIZE + " to " + MAX_SIZE + ", got " + value)
    }
  } else if (arg.startsWith("-")) usage("Unknown option " + arg)
  else if (input === undefined) input = arg
  else usage("Unexpected argument " + arg)
}
if (input === undefined) usage("Missing input file")
if (output === undefined) output = basename(input, extname(input)) + ".srte"

let started = performance.now()
let panorama = decodeHdr(new Uint8Array(readFileSync(input)))
let decoded = performance.now()
let levels = prefilterCube(panoramaToCube(panorama, size), size)
let baked = performance.now()
let encoded = encodeEnvironment(levels, size)
writeFileSync(output, encoded)

console.log(
  `${output}: ${size} cube, ${levels.length} levels, ${(encoded.byteLength / 1024).toFixed(0)} KiB ` +
    `(${panorama.width}x${panorama.height} panorama decoded in ${(decoded - started).toFixed(0)} ms, baked in ${(baked - decoded).toFixed(0)} ms)`,
)
