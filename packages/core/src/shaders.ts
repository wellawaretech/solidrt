// The GPU-free shader helpers: the `glsl` tag every shader source in the
// repo is written with, and the GLSL shared between renderers with its JS
// twins. Pure - no GPU or GUI imports - so the tests run headless on flux
// and the extensions' pure modules can import from here; gpu.ts re-exports
// all of it for apps.
//
// The screen-size clamp, shared by every renderer that draws a world-sized
// quad and lets it hold a size on screen: @solidrt/2d's sprites (the
// camera zoom is the pixels per world unit) and @solidrt/3d's sprite
// material (the projection at the quad's depth is). A quad `w x h` world
// units draws at its world size times the pixels per unit, clamped to
// [minPx, maxPx] on its smaller axis as ONE uniform scale, so its aspect
// holds. No bounds is world size; a floor alone is the marker that stays
// findable at the overview; a ceiling alone is the label that must not
// balloon; equal bounds is a constant screen size (Three's
// `sizeAttenuation: false`), where w and h carry the aspect alone. A zero
// bound is off. The GLSL and the JS below are the same function twice,
// and every picker applies the JS one to the rect it tests, so a hit is
// where the pixel is (tests/shaders.test.ts pins the pair).

/**
 * Tags an inline GLSL source, returning it unchanged. Shaders small enough to
 * belong beside the code that uses them stay in the file; the tag is what makes
 * them legible there, because editors highlight GLSL inside a template literal
 * only when a known tag marks it (the name matters - `glsl` is the one the
 * grammars look for).
 *
 * Interpolated values are stringified verbatim, with no GLSL-aware formatting:
 * `${2}` splices in the int literal `2`, which will not assign to a float. Pass
 * anything that varies as a uniform instead of building it into the source.
 *
 * Raw semantics, so backslashes reach the compiler as written: the GLSL
 * preprocessor continues a line with a trailing `\`, which a cooked template
 * would reject as an invalid escape and silently pass through as `undefined`.
 */
export let glsl = String.raw

/**
 * The GLSL half: `screenSizeScale(size, pxPerUnit, screenPx)` returns the
 * uniform scale a `size` (world units) quad takes so its smaller axis
 * covers at least `screenPx.x` and at most `screenPx.y` pixels when one
 * world unit is `pxPerUnit` pixels; a zero bound is off, a collapsed quad
 * (zero axis) stays collapsed. Paste it before `main`, multiply the quad's
 * size by it.
 */
export const SCREEN_SIZE_GLSL = glsl`
  float screenSizeScale(vec2 size, float pxPerUnit, vec2 screenPx) {
    float smallest = min(abs(size.x), abs(size.y)) * pxPerUnit;
    if (smallest <= 0.0) return 1.0;
    float target = smallest;
    if (screenPx.x > 0.0) target = max(target, screenPx.x);
    if (screenPx.y > 0.0) target = min(target, screenPx.y);
    return target / smallest;
  }
`

/**
 * The JS half of SCREEN_SIZE_GLSL, for the picker: the uniform scale a
 * `w x h` quad draws at when one world unit is `pxPerUnit` pixels and
 * its smaller axis is clamped to [minPx, maxPx] on screen. 1 with both
 * bounds off, when the quad already lies within them, or when it is
 * collapsed.
 */
export function screenSizeScale(w: number, h: number, pxPerUnit: number, minPx: number, maxPx: number): number {
  let smallest = Math.min(Math.abs(w), Math.abs(h)) * pxPerUnit
  if (!(smallest > 0)) return 1
  let target = smallest
  if (minPx > 0) target = Math.max(target, minPx)
  if (maxPx > 0) target = Math.min(target, maxPx)
  return target / smallest
}

/**
 * Validate a screen-size pair (throws - the dev validation policy): each
 * bound finite and >= 0 (0 = off), and a ceiling never below a floor that
 * is on. `site` names the caller in the message.
 */
export function checkScreenSize(site: string, minPx: number, maxPx: number): void {
  if (!(Number.isFinite(minPx) && minPx >= 0)) throw new Error(`${site}: minScreenPx must be a finite number >= 0, got ${minPx}`)
  if (!(Number.isFinite(maxPx) && maxPx >= 0)) throw new Error(`${site}: maxScreenPx must be a finite number >= 0, got ${maxPx}`)
  if (minPx > 0 && maxPx > 0 && maxPx < minPx) throw new Error(`${site}: maxScreenPx ${maxPx} is below minScreenPx ${minPx}`)
}
