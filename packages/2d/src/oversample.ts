// The layer oversample: how many target texels one layer pixel is drawn
// as. A layer renders at `oversample` times its size and is composited down
// to its box, so a fractional or HiDPI display scale resamples properly: at
// the display's own scale the composite is 1:1, and anywhere else it
// samples linear. The auto pick rounds the scale up to a multiple of
// `oversampleStep` (an eighth by default); pixel art over a nearest atlas
// sets the step to 1, so each atlas texel covers a whole number of target
// texels and blocks stay square, and the linear composite softens each
// block edge over one device pixel instead of snapping to uneven widths.
// okf/done/2d-layer-display-scale.md. The pure math lives in
// oversample-math.ts (checkable headless); this module binds it to the
// device limits and adds the thrash sentinel.
import { frameTime } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import { DEFAULT_OVERSAMPLE_STEP, fitOversampleWithin, scaledSize } from "./oversample-math.ts"

/** Validate a view's oversample against the target it scales; throws (dev
 * policy). */
export function checkOversample(verb: string, n: number, width: number, height: number): void {
  if (!(Number.isFinite(n) && n > 0)) {
    throw new Error(`${verb}: oversample must be a positive number, got ${n}`)
  }
  checkScaledSize(verb, "oversample", n, width, height)
}

/** Validate the tile layer's bake density: a whole number, so every tile
 * and chunk edge lands on a whole texel (at a fractional density an edge
 * texel is half covered by two tiles, the classic tilemap seam) and a
 * nearest tileset bakes square blocks; throws (dev policy). */
export function checkDensity(verb: string, n: number, width: number, height: number): void {
  if (!(Number.isInteger(n) && n >= 1)) {
    throw new Error(`${verb}: density must be a positive integer, got ${n}`)
  }
  checkScaledSize(verb, "density", n, width, height)
}

function checkScaledSize(verb: string, name: string, n: number, width: number, height: number): void {
  let max = limits.maxTextureSize
  if (scaledSize(width, n) > max || scaledSize(height, n) > max) {
    throw new Error(`${verb}: ${width} x ${height} at ${name} ${n} exceeds maxTextureSize ${max}`)
  }
}

/**
 * The oversample for a layer shown at `scale` device pixels per layer pixel:
 * the scale rounded up to a multiple of `step` (default an eighth, which
 * every common display scale is; 1 for a nearest atlas), so no device
 * pixel goes without a target texel. Bounded by `budget` texels for the
 * `targetW x targetH` target (the window's own device pixel count: a
 * target beyond it buys nothing on screen, and a layer stretched far past
 * the window inside a scroller must not ask for one) and by what the
 * target can grow to on this device.
 */
export function fitOversample(scale: number, targetW: number, targetH: number, budget: number, step = DEFAULT_OVERSAMPLE_STEP): number {
  return fitOversampleWithin(scale, targetW, targetH, budget, limits.maxTextureSize, step)
}

/**
 * The uneven-blocks sentinel: a sprite or record layer over a nearest
 * atlas calls the returned hook with every oversample one of its views
 * takes (creation included), and a fractional one warns once per layer.
 * Each atlas texel then covers a fractional number of target texels, so
 * blocks come out of uneven widths: the shimmer whole-number oversample
 * exists to prevent. A diagnostic only: the oversample is the app's, and
 * stays as set.
 */
export function unevenSentinel(what: string): (n: number) => void {
  let warned = false
  return n => {
    if (warned || Number.isInteger(n)) return
    warned = true
    console.warn(
      `Uneven pixel art: ${what} draws a "nearest" atlas at oversample ${n}, a fractional ` +
        `factor, so its texels come out of uneven widths. For square blocks set ` +
        `\`oversampleStep={1}\` on the component, or give the view a whole oversample ` +
        `(\`fitOversample(..., 1)\`). Warned once per layer.`,
    )
  }
}

/** Oversample changes inside the window before the thrash warning fires. A
 * healthy layer settles in a change or two (mount, then the post-resize
 * scale); reaching this count means something re-picks every frame. */
const THRASH_CHANGES = 4
/** The window those changes must land in, ms of the app's frame time: a
 * warning about what happens per frame counts on the clock frames run on,
 * so it means the same under `sol render` and a paused dev clock. */
const THRASH_WINDOW_MS = 1000

/**
 * The thrash sentinel: each layer calls the returned hook on every ACTUAL
 * oversample change (`knob` names it: the tile layer's bake density is
 * `density`, capped by `maxDensity`, with no step to coarsen), and
 * bunched changes warn once per layer. Every change resizes and redraws
 * the layer's targets (a tile layer re-bakes every resident chunk), so a
 * scale sweeping the pick's steps every frame is this package's most
 * expensive silent mistake - worth a console line the moment it happens
 * rather than a profiling session later.
 */
export function thrashSentinel(what: string, knob = "oversample", cap = "maxOversample", step: string | null = "oversampleStep"): () => void {
  let stamps: number[] = []
  let warned = false
  return () => {
    if (warned) return
    let now = frameTime()
    stamps.push(now)
    while (stamps.length > 0 && now - stamps[0]! > THRASH_WINDOW_MS) stamps.shift()
    if (stamps.length < THRASH_CHANGES) return
    warned = true
    console.warn(
      `Oversample thrash: ${what} changed ${knob} ${THRASH_CHANGES} times within a second. ` +
        `Every change resizes and redraws the layer's targets (a tile layer re-bakes every ` +
        `resident chunk). The usual cause is an animated transform or camera sweeping the ` +
        `measured scale across the pick's steps; pin \`${knob}\`, ` +
        `${step === null ? "" : `coarsen \`${step}\`, `}or bound the sweep with \`${cap}\`. Warned once per layer.`,
    )
  }
}
