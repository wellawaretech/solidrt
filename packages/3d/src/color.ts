// The sRGB transfer functions (IEC 61966-2-1) in JS, the JS half of the
// package's color contract: every [r, g, b] color option - material
// color and emissive, light color, hemisphere sky and ground, fog color -
// is sRGB-encoded, what a color picker shows, and the library decodes it to
// linear light when it writes the uniform (Three's ColorManagement,
// Godot's inspector colors, Unity's linear color space all do the same).
// Shading then runs in linear light and the fragment's output stage
// (OUTPUT in ./glsl) encodes the result back. Values written straight to
// a uniform (scene.setParams, setMeshParams) and vertex colors are linear,
// as in every engine; encode or decode them here when they come from a
// picker.

/** Decode one sRGB-encoded component (0..1) to linear light. Above 1 the
 * curve continues, so an intensity folded into a color survives. */
export function srgbToLinear(c: number): number {
  if (c <= 0.04045) return c / 12.92
  return Math.pow((c + 0.055) / 1.055, 2.4)
}

/** Encode one linear-light component to sRGB: the inverse of srgbToLinear. */
export function linearToSrgb(c: number): number {
  if (c <= 0.0031308) return c * 12.92
  return 1.055 * Math.pow(c, 1 / 2.4) - 0.055
}

/** A copy of an [r, g, b] or [r, g, b, a] color with its rgb decoded to
 * linear light; alpha is coverage, never encoded, and passes through. */
export function linearColor(color: readonly number[]): number[] {
  let out = color.slice()
  for (let i = 0; i < 3 && i < out.length; i++) out[i] = srgbToLinear(out[i]!)
  return out
}
