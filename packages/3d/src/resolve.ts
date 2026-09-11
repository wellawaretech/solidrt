// A target's RESOLVE: the rgba8 draw target the app displays, holding one
// covering-triangle entry that samples the buffer as `uScene` and writes
// display pixels (exposure, tone mapping, the encode - the RESOLVE set in
// glsl.ts). An ordinary auto target: it re-renders exactly when the buffer
// does, so a static scene still costs zero passes. The pipeline and
// program are the target's own (a custom source recompiles them). The
// stock bloom hangs on it: a chain of shader textures over the buffer
// (threshold, then blur across and down per round) at a fraction of the
// target's size, bound on the resolve target as `uBloom`.
import { addDraw, createDrawTarget, createShaderTexture, createTexture, destroyProgram, destroyRenderPipeline, destroyTexture, removeDraw, setDrawTextures, setTargetParams, setTargetSize, setTargetTextures } from "@solidrt/core/gpu"
import type { DrawId, FilterMode, ProgramId, RenderPipelineId, TextureBindings, TextureId, WrapMode } from "@solidrt/core/gpu"
import { BLOOM_BLUR, BLOOM_RESOLVE, BLOOM_THRESHOLD, DEFAULT_RESOLVE } from "./glsl.ts"
import { resolvePipeline } from "./material.ts"

/** A custom resolve (SceneOptions.resolve, scene.setResolve): the fragment
 * source and the extra textures it declares (a chain's result), bound
 * beside `uScene`. */
export type ResolveOptions = { source: string; textures?: TextureBindings }
/** What the `resolve` option takes: a source, ResolveOptions, or a
 * function of the buffer id (`hdrTexture`) returning either - the form
 * that builds a chain over the buffer before the resolve is made. */
export type ResolveInput = string | ResolveOptions | ((hdr: TextureId) => string | ResolveOptions)

/** The stock bloom (SceneOptions.bloom, scene.setBloom): radiance above
 * `threshold` (default 1: what a white surface lit to 1 reaches, so only
 * brighter blooms) is blurred over `radius` rounds (default 2) of a
 * separable blur at a quarter of the target's size and added back with
 * `intensity` (default 0.5). Three's UnrealBloomPass strength / radius /
 * threshold, Unity's Bloom threshold / intensity / scatter, Godot's glow. */
export type BloomOptions = { threshold?: number; intensity?: number; radius?: number }

// The bloom defaults: only radiance above a fully lit white surface
// blooms, at half weight, blurred over two rounds.
const BLOOM_THRESHOLD_DEFAULT = 1
const BLOOM_INTENSITY_DEFAULT = 0.5
const BLOOM_RADIUS_DEFAULT = 2
// The chain renders at this fraction of the target on each axis: a blur
// is low-frequency, and a coarse chain is what makes a 9-tap kernel wide
// on screen.
const BLOOM_SCALE = 4

type Bloom = {
  threshold: TextureId
  blurs: TextureId[]
  radius: number
}

export type ResolveRecord = {
  target: TextureId
  entry: DrawId
  pipeline: RenderPipelineId
  program: ProgramId
  source: string
  /** Whether the app gave the source (then bloom never swaps it). */
  custom: boolean
  textures: TextureBindings | undefined
  bloom: Bloom | null
  /** What `uBloom` binds while bloom is off (a target binding cannot be
   * removed, and the chain it named is destroyed): a 1x1 black texture,
   * made at the first switch. */
  placeholder: TextureId | null
  label: string
}

function resolveSource(r: ResolveInput | undefined, buffer: TextureId): ResolveOptions {
  if (r === undefined) return { source: DEFAULT_RESOLVE }
  if (typeof r === "function") r = r(buffer)
  return typeof r === "string" ? { source: r } : r
}

export function makeResolve(
  buffer: TextureId,
  width: number,
  height: number,
  r: ResolveInput | undefined,
  sampler: { filter?: FilterMode; wrap?: WrapMode },
  label: string,
): ResolveRecord {
  let o = resolveSource(r, buffer)
  let built = resolvePipeline(o.source, label + "-resolve")
  let target = createDrawTarget(width, height, null, { filter: sampler.filter, wrap: sampler.wrap, label: label + "-resolve", autoFree: false })
  let entry = addDraw(target, built.pipeline, null, { vertexCount: 3, textures: { ...o.textures, uScene: buffer } })
  return { target, entry, pipeline: built.pipeline, program: built.program, source: o.source, custom: r !== undefined, textures: o.textures, bloom: null, placeholder: null, label }
}

// Swap the entry's program for `source`, keeping the bindings.
function recompile(rec: ResolveRecord, buffer: TextureId, source: string): void {
  let built = resolvePipeline(source, rec.label + "-resolve")
  removeDraw(rec.target, rec.entry)
  destroyRenderPipeline(rec.pipeline)
  destroyProgram(rec.program)
  rec.entry = addDraw(rec.target, built.pipeline, null, { vertexCount: 3, textures: { ...rec.textures, uScene: buffer } })
  rec.pipeline = built.pipeline
  rec.program = built.program
  rec.source = source
}

export function replaceResolve(rec: ResolveRecord, buffer: TextureId, r: ResolveInput): void {
  let o = resolveSource(r, buffer)
  rec.custom = true
  rec.textures = o.textures
  if (o.source === rec.source) {
    setDrawTextures(rec.target, rec.entry, { ...o.textures, uScene: buffer })
    return
  }
  recompile(rec, buffer, o.source)
}

function chainSize(size: number): number {
  return Math.max(1, Math.round(size / BLOOM_SCALE))
}

function makeChain(rec: ResolveRecord, buffer: TextureId, width: number, height: number, threshold: number, radius: number): Bloom {
  let w = chainSize(width)
  let h = chainSize(height)
  let label = rec.label + "-bloom"
  let first = createShaderTexture(BLOOM_THRESHOLD, w, h, { uThreshold: threshold }, { textures: { uSource: buffer }, label: label + "-threshold", autoFree: false })
  let blurs: TextureId[] = []
  let source = first
  for (let i = 0; i < radius; i++) {
    source = createShaderTexture(BLOOM_BLUR, w, h, { uDir: [1, 0] }, { textures: { uSource: source }, label: label + "-x", autoFree: false })
    blurs.push(source)
    source = createShaderTexture(BLOOM_BLUR, w, h, { uDir: [0, 1] }, { textures: { uSource: source }, label: label + "-y", autoFree: false })
    blurs.push(source)
  }
  return { threshold: first, blurs, radius }
}

function destroyChain(bloom: Bloom): void {
  for (let t of bloom.blurs) destroyTexture(t)
  destroyTexture(bloom.threshold)
}

/** Switch the stock bloom on (options) or off (null) on a resolve. */
export function setResolveBloom(rec: ResolveRecord, buffer: TextureId, width: number, height: number, opts: BloomOptions | null): void {
  if (opts === null) {
    if (rec.bloom === null) return
    if (rec.placeholder === null) rec.placeholder = createTexture(new Uint8Array([0, 0, 0, 255]), 1, 1, { label: rec.label + "-bloom-none", autoFree: false })
    setTargetTextures(rec.target, { uBloom: rec.placeholder })
    setTargetParams(rec.target, { uBloomIntensity: 0 })
    destroyChain(rec.bloom)
    rec.bloom = null
    if (!rec.custom) recompile(rec, buffer, DEFAULT_RESOLVE)
    return
  }
  let threshold = opts.threshold ?? BLOOM_THRESHOLD_DEFAULT
  let intensity = opts.intensity ?? BLOOM_INTENSITY_DEFAULT
  let radius = opts.radius ?? BLOOM_RADIUS_DEFAULT
  if (!Number.isFinite(threshold) || threshold < 0) throw new Error("bloom: threshold must be a finite number >= 0, got " + threshold)
  if (!Number.isFinite(intensity) || intensity < 0) throw new Error("bloom: intensity must be a finite number >= 0, got " + intensity)
  if (!Number.isInteger(radius) || radius < 1) throw new Error("bloom: radius must be a positive integer (blur rounds), got " + radius)
  if (rec.bloom !== null && rec.bloom.radius !== radius) {
    destroyChain(rec.bloom)
    rec.bloom = null
  }
  if (rec.bloom === null) {
    rec.bloom = makeChain(rec, buffer, width, height, threshold, radius)
    setTargetTextures(rec.target, { uBloom: rec.bloom.blurs[rec.bloom.blurs.length - 1]! })
  } else {
    setTargetParams(rec.bloom.threshold, { uThreshold: threshold })
  }
  setTargetParams(rec.target, { uBloomIntensity: intensity })
  if (!rec.custom && rec.source !== BLOOM_RESOLVE) recompile(rec, buffer, BLOOM_RESOLVE)
}

export function resizeResolve(rec: ResolveRecord, width: number, height: number): void {
  setTargetSize(rec.target, width, height)
  if (rec.bloom === null) return
  let w = chainSize(width)
  let h = chainSize(height)
  setTargetSize(rec.bloom.threshold, w, h)
  for (let t of rec.bloom.blurs) setTargetSize(t, w, h)
}

export function disposeResolve(rec: ResolveRecord): void {
  if (rec.bloom !== null) destroyChain(rec.bloom)
  destroyTexture(rec.target)
  if (rec.placeholder !== null) destroyTexture(rec.placeholder)
  destroyRenderPipeline(rec.pipeline)
  destroyProgram(rec.program)
}
