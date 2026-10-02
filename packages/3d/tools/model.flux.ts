// sol tool 3d/model: bake a glTF into a .sol3m model file - the parse
// (src/gltf.ts) run once here, the result written in the exact layout
// loadModel views without any per-vertex work at runtime. Same parser,
// same subset, same result as loadGltf; this only moves the cost to build
// time. Put the output under assets/ so it ships with the app.
//
// The images are baked too: each PNG or JPEG becomes a KTX2 compressed
// texture with its mip chain, which the device turns into its own block
// format at load (a quarter of the GPU memory of the decoded image). One
// file for every platform, so the tool takes no platform. How each image
// is compressed, and whether, is the app's: the `textures` group of the
// `solidrt` key in the package.json of the project the tool runs in, over
// the defaults (@solidrt/core/textures) - per kind of map (color, normal,
// data) and per file, an image named by its path from the project root
// and an embedded one by `<model path>#<its name>`. --compress and
// --no-compress stand in for the app's own `compress` for one run.
//
// A flux tool (the file name says so): the texture encoder is the
// runtime's, flux:image. The model side comes from the runtime-free
// `@solidrt/3d/model` entry (src/model-data.ts), the same surface an
// app's own bake script uses under bun.
//
//   sol tool 3d/model <in.gltf|in.glb> [-o <out.sol3m>] [--compress|--no-compress]

import { file, glob } from "flux:fs"
import { decodeImage, encodeTexture } from "flux:image"
import { basename, dirname, extname, join, matchesGlob, relative } from "flux:path"
import { argv, exit } from "flux:process"
import { isKtx2, textureSettingFor, textureSettings, unmatchedTextureFiles, unmatchedTextureNames } from "@solidrt/core/textures"
import type { TextureKind, TextureSetting, TextureSettings, TextureSource } from "@solidrt/core/textures"
import { encodeModel, gltfExternalUris, layoutStride, modelImageUses, parseGltf } from "../src/model-data.ts"
import type { ModelData, ModelImage } from "../src/model-data.ts"

// The project's package.json, where the texture settings live.
const PROJECT_CONFIG = "package.json"

// How many images are decoded and encoded at once: what the runtime
// encodes at a time. More would only queue, each with its decoded pixels.
const IMAGES_IN_FLIGHT = 4

// The project the tool runs in: its root is the cwd, which the patterns
// of the texture settings are written from.
const PROJECT_ROOT = "."

const USAGE = "Usage: sol tool 3d/model <in.gltf|in.glb> [-o <out.sol3m>] [--compress|--no-compress]"

function usage(error?: string): never {
  if (error) console.error(error)
  console.log(USAGE)
  exit(error ? 1 : 0)
}

function kib(bytes: number): string {
  return (bytes / 1024).toFixed(0) + " KiB"
}

function images(count: number): string {
  return count === 1 ? "1 image" : count + " images"
}

// The texture settings of the project the tool runs in; the defaults where
// it has no package.json. `compress` is the run's own word, when it has
// one.
async function projectTextureSettings(compress: boolean | undefined): Promise<TextureSettings> {
  let config = file(PROJECT_CONFIG)
  return textureSettings((await config.exists()) ? await config.json() : undefined, { compress })
}

// What an image is to the texture settings: its file as the path from the
// project root, or for an embedded image the model's file and its name.
function imageSource(image: ModelImage, model: string): TextureSource {
  if (image.uri === undefined) return { file: relative(PROJECT_ROOT, model), name: image.name }
  return { file: relative(PROJECT_ROOT, join(dirname(model), decodeURIComponent(image.uri))) }
}

// The entries of the settings that name nothing, which is a misspelled
// pattern: a file the project does not have, a name the model does not.
async function unmatchedPatterns(data: ModelData, model: string, settings: TextureSettings): Promise<string[]> {
  let files = await unmatchedTextureFiles(settings, (pattern) => glob(pattern, { cwd: PROJECT_ROOT }))
  let embedded = data.images.filter((image) => image.uri === undefined).map((image) => image.name)
  return [...files, ...unmatchedTextureNames(settings, relative(PROJECT_ROOT, model), embedded, matchesGlob)]
}

// The images of one kind compressed the same way, and the ones kept.
type Group = { kind: TextureKind; setting: TextureSetting; images: number; from: number; to: number }
type Baked = { groups: Group[]; kept: { images: number; bytes: number } }

// The images as KTX2, in place, each as the settings say for it and in
// the color space createModel samples it in (modelImageUses, the rule the
// two share), mipmapped for repeat wrap. IMAGES_IN_FLIGHT at a time, each
// decoded when its turn comes. An image the settings leave alone is kept,
// and so is one that is a KTX2 already.
async function bakeImages(data: ModelData, model: string, settings: TextureSettings): Promise<Baked> {
  let baked: Baked = { groups: [], kept: { images: 0, bytes: 0 } }
  let groupOf = (kind: TextureKind, setting: TextureSetting): Group => {
    let group = baked.groups.find((g) => g.kind === kind && g.setting.codec === setting.codec && g.setting.quality === setting.quality)
    if (group === undefined) {
      group = { kind, setting, images: 0, from: 0, to: 0 }
      baked.groups.push(group)
    }
    return group
  }
  let uses = modelImageUses(data)
  let next = 0
  let lane = async () => {
    while (next < data.images.length) {
      let i = next++
      let image = data.images[i]!
      let source = image.bytes
      let { kind, srgb } = uses[i]!
      let setting = textureSettingFor(settings, kind, imageSource(image, model), matchesGlob)
      if (!setting.compress || isKtx2(source)) {
        baked.kept.images++
        baked.kept.bytes += source.byteLength
        continue
      }
      image.bytes = await encodeTexture(decodeImage(source), {
        codec: setting.codec,
        quality: setting.quality,
        srgb,
        mipmap: true,
        wrap: "repeat",
      })
      let group = groupOf(kind, setting)
      group.images++
      group.from += source.byteLength
      group.to += image.bytes.byteLength
    }
  }
  await Promise.all(Array.from({ length: IMAGES_IN_FLIGHT }, lane))
  let order: TextureKind[] = ["color", "normal", "data"]
  baked.groups.sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind))
  return baked
}

let input: string | undefined
let output: string | undefined
let compress: boolean | undefined
for (let i = 0; i < argv.length; i++) {
  let arg = argv[i]!
  if (arg === "--help" || arg === "-h") usage()
  else if (arg === "-o" || arg === "--output") {
    output = argv[++i]
    if (output === undefined) usage("Missing value for " + arg)
  } else if (arg === "--compress") compress = true
  else if (arg === "--no-compress") compress = false
  else if (arg.startsWith("-")) usage("Unknown option " + arg)
  else if (input === undefined) input = arg
  else usage("Unexpected argument " + arg)
}
if (input === undefined) usage("Missing input file")
if (output === undefined) output = basename(input, extname(input)) + ".sol3m"

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

// Read and checked before the encode, so a mistake in the settings costs
// no bake.
let settings: TextureSettings
let unmatched: string[]
try {
  settings = await projectTextureSettings(compress)
  unmatched = await unmatchedPatterns(data, input, settings)
} catch (e) {
  console.error(`The texture settings in ${PROJECT_CONFIG} cannot be used: ${e instanceof Error ? e.message : String(e)}`)
  exit(1)
}
if (unmatched.length > 0) {
  console.error(`The texture settings in ${PROJECT_CONFIG} (solidrt.textures.files) name nothing with:`)
  for (let pattern of unmatched) console.error(`  ${pattern}`)
  console.error("A pattern is a path from the project root; after a model's path, #name picks an image embedded in it.")
  exit(1)
}
started = performance.now()
let baked = await bakeImages(data, input, settings)
let bakeMs = performance.now() - started

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
if (baked.groups.length > 0) {
  console.log(`Images as KTX2 (encoded in ${(bakeMs / 1000).toFixed(1)} s):`)
  for (let group of baked.groups) {
    console.log(`  ${group.kind}: ${images(group.images)}, ${group.setting.codec} at quality ${group.setting.quality}, ${kib(group.from)} to ${kib(group.to)}`)
  }
}
if (baked.kept.images > 0) console.log(`Kept as they are: ${images(baked.kept.images)}, ${kib(baked.kept.bytes)}`)
