#!/usr/bin/env bun

// srt tool 3d/model: bake a glTF into a .srtm model file - the parse
// (src/gltf.ts) run once here under bun, the result written in the exact
// layout loadModel views without any per-vertex work at runtime. Same
// parser, same subset, same result as loadGltf; this only moves the cost
// to build time. Put the output under assets/ so it ships with the app.
//
//   srt tool 3d/model <in.gltf|in.glb> [-o <out.srtm>]

import { readFileSync, writeFileSync } from "node:fs"
import { basename, dirname, extname, join } from "node:path"
import { parseGltf } from "../src/gltf.ts"
import { encodeModel } from "../src/model-file.ts"

function usage(error?: string): never {
  if (error) console.error(error)
  console.log("Usage: srt tool 3d/model <in.gltf|in.glb> [-o <out.srtm>]")
  process.exit(error ? 1 : 0)
}

let input: string | undefined
let output: string | undefined
let args = process.argv.slice(2)
for (let i = 0; i < args.length; i++) {
  let arg = args[i]!
  if (arg === "--help" || arg === "-h") usage()
  else if (arg === "-o" || arg === "--output") {
    output = args[++i]
    if (output === undefined) usage("Missing value for " + arg)
  } else if (arg.startsWith("-")) usage("Unknown option " + arg)
  else if (input === undefined) input = arg
  else usage("Unexpected argument " + arg)
}
if (input === undefined) usage("Missing input file")
if (output === undefined) output = basename(input, extname(input)) + ".srtm"

let dir = dirname(input)
let bytes = new Uint8Array(readFileSync(input))
let started = performance.now()
let data = parseGltf(bytes, (uri) => new Uint8Array(readFileSync(join(dir, decodeURIComponent(uri)))))
let parsed = performance.now() - started
let encoded = encodeModel(data)
writeFileSync(output, encoded)

let triangles = data.parts.reduce((n, p) => n + p.geometry.indices.length / 3, 0)
let vertices = data.parts.reduce((n, p) => n + p.geometry.vertices.length / 8, 0)
console.log(
  `${output}: ${data.parts.length} parts, ${vertices} vertices, ${triangles} triangles, ` +
    `${data.materials.length} materials, ${data.images.length} images, ${(encoded.byteLength / 1024).toFixed(0)} KiB ` +
    `(parsed in ${parsed.toFixed(0)} ms)`,
)
