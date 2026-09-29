// srt tool 3d/model: bake a glTF into a .srtm model file - the parse
// (src/gltf.ts) run once here, the result written in the exact layout
// loadModel views without any per-vertex work at runtime. Same parser,
// same subset, same result as loadGltf; this only moves the cost to build
// time. Put the output under assets/ so it ships with the app.
//
// With --ktx2 the images are baked too: each PNG or JPEG becomes a KTX2
// compressed texture with its mip chain, which the device turns into its
// own block format at load (a quarter of the GPU memory of the decoded
// image). One file for every platform, so the tool takes no platform.
// The codec and quality per kind of map (color, normal, data) are the
// app's: the `textures` group of the `solidrt` key in the package.json of
// the project the tool runs in, over the defaults
// (@solidrt/core/textures).
//
// A flux tool (the file name says so): the texture encoder is the
// runtime's, flux:image. The model side comes from the runtime-free
// `@solidrt/3d/model` entry (src/model-data.ts), the same surface an
// app's own bake script uses under bun.
//
//   srt tool 3d/model <in.gltf|in.glb> [-o <out.srtm>] [--ktx2]

import { file } from "flux:fs"
import { decodeImage, encodeTexture } from "flux:image"
import { basename, dirname, extname, join } from "flux:path"
import { argv, exit } from "flux:process"
import { isKtx2, textureSettings } from "@solidrt/core/textures"
import type { TextureKind, TextureSettings } from "@solidrt/core/textures"
import { encodeModel, gltfExternalUris, layoutStride, modelImageUses, parseGltf } from "../src/model-data.ts"
import type { ModelData } from "../src/model-data.ts"

// The project's package.json, where the texture settings live.
const PROJECT_CONFIG = "package.json"

const USAGE = "Usage: srt tool 3d/model <in.gltf|in.glb> [-o <out.srtm>] [--ktx2]"

function usage(error?: string): never {
  if (error) console.error(error)
  console.log(USAGE)
  exit(error ? 1 : 0)
}

function kib(bytes: number): string {
  return (bytes / 1024).toFixed(0) + " KiB"
}

// The texture settings of the project the tool runs in; the defaults where
// it has no package.json.
async function projectTextureSettings(): Promise<TextureSettings> {
  let config = file(PROJECT_CONFIG)
  return textureSettings((await config.exists()) ? await config.json() : undefined)
}

type Baked = Record<TextureKind, { images: number; from: number; to: number }>

// The images as KTX2, in place, each by the settings of its kind and in
// the color space createModel samples it in (modelImageUses, the rule the
// two share), mipmapped for repeat wrap. One at a
// time: an encode takes every core. An image that is a KTX2 already is
// kept.
async function bakeImages(data: ModelData, settings: TextureSettings): Promise<Baked> {
  let baked: Baked = { color: { images: 0, from: 0, to: 0 }, normal: { images: 0, from: 0, to: 0 }, data: { images: 0, from: 0, to: 0 } }
  let uses = modelImageUses(data)
  for (let i = 0; i < data.images.length; i++) {
    let source = data.images[i]!
    if (isKtx2(source)) continue
    let { kind, srgb } = uses[i]!
    let encoded = await encodeTexture(decodeImage(source), {
      ...settings[kind],
      srgb,
      mipmap: true,
      wrap: "repeat",
    })
    data.images[i] = encoded
    let sum = baked[kind]
    sum.images++
    sum.from += source.byteLength
    sum.to += encoded.byteLength
  }
  return baked
}

let input: string | undefined
let output: string | undefined
let ktx2 = false
for (let i = 0; i < argv.length; i++) {
  let arg = argv[i]!
  if (arg === "--help" || arg === "-h") usage()
  else if (arg === "-o" || arg === "--output") {
    output = argv[++i]
    if (output === undefined) usage("Missing value for " + arg)
  } else if (arg === "--ktx2") ktx2 = true
  else if (arg.startsWith("-")) usage("Unknown option " + arg)
  else if (input === undefined) input = arg
  else usage("Unexpected argument " + arg)
}
if (input === undefined) usage("Missing input file")
if (output === undefined) output = basename(input, extname(input)) + ".srtm"

// The parse is synchronous, so the external files it will open are read
// ahead of it (the same prefetch loadGltf does).
let dir = dirname(input)
let bytes = await file(input).bytes()
let files = new Map<string, Uint8Array>()
for (let uri of gltfExternalUris(bytes)) {
  if (!files.has(uri)) files.set(uri, await file(join(dir, decodeURIComponent(uri))).bytes())
}

let started = performance.now()
let data = parseGltf(bytes, (uri) => files.get(uri)!)
let parsed = performance.now() - started

// Read before the encode, so a mistake in the settings costs no bake.
let settings = ktx2 ? await projectTextureSettings() : undefined
let baked: Baked | undefined
let bakeMs = 0
if (settings !== undefined) {
  started = performance.now()
  baked = await bakeImages(data, settings)
  bakeMs = performance.now() - started
}

let encoded = encodeModel(data)
await file(output).write(encoded)

let triangles = data.parts.reduce((n, p) => n + p.geometry.indices.length / 3, 0)
let vertices = data.parts.reduce((n, p) => n + p.geometry.vertices.byteLength / layoutStride(p.geometry.layout), 0)
let targets = data.parts.reduce((n, p) => n + (p.geometry.morphs?.names.length ?? 0), 0)
console.log(
  `${output}: ${data.nodes.length} nodes, ${data.parts.length} parts, ${vertices} vertices, ${triangles} triangles, ` +
    (targets > 0 ? `${targets} morph targets, ` : "") +
    `${data.clips.length} clips, ${data.materials.length} materials, ${data.images.length} images, ${kib(encoded.byteLength)} ` +
    `(parsed in ${parsed.toFixed(0)} ms)`,
)
if (settings !== undefined && baked !== undefined) {
  console.log(`Images as KTX2 (encoded in ${(bakeMs / 1000).toFixed(1)} s):`)
  for (let kind of ["color", "normal", "data"] as const) {
    let sum = baked[kind]
    if (sum.images === 0) continue
    let setting = settings[kind]
    console.log(`  ${kind}: ${sum.images} images, ${setting.codec} at quality ${setting.quality}, ${kib(sum.from)} to ${kib(sum.to)}`)
  }
}
