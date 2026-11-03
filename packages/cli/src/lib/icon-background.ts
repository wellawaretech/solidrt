// The `solidrt.iconBackground` value (project.ts checks it, pack --apk
// writes it into the icon and the splash), in a module of its own so
// flux:test can load it.

export type Rgb = [r: number, g: number, b: number]

/**
 * An `iconBackground` value, "#rrggbb", as [r, g, b] bytes, or null when it
 * is not one. Opaque only: Android draws an adaptive icon's layers over
 * black (AdaptiveIconDrawable.draw), so a transparent ground shows black.
 */
export function iconBackgroundRgb(value: string): Rgb | null {
  if (!/^#[0-9a-fA-F]{6}$/.test(value)) return null
  return [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16)) as Rgb
}
