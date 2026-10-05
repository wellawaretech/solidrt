// Rect packing, pure: the shelf packer (next-fit decreasing height) that
// places sizes on a sheet of a given width - what extrudeRects re-places a
// hand-packed sheet with, and what a runtime atlas packer allocates from.
// Simple on purpose: a sprite sheet is hundreds of rects, not millions,
// and shelves keep every placement's x and y a sum of the sizes placed
// before it, so sizes that are multiples of g land on multiples of g (the
// alignment extrusion relies on). No imports, so tests/pack.test.ts runs
// headless on the flux binary.

export type Size = { width: number; height: number }
export type Placement = { x: number; y: number }
/** A packing: the sheet size and one placement per input size, in INPUT
 * order (the packer sorts internally and maps back). */
export type Packing = { width: number; height: number; rects: Placement[] }

/**
 * Pack `sizes` onto a sheet `width` wide: tallest first, filled left to
 * right in shelves as tall as their first (tallest) rect, a new shelf when
 * the current one is full. Returns the placements in input order and the
 * height the shelves reach. Throws on a non-positive-integer size or one
 * wider than the sheet.
 */
export function packRects(sizes: readonly Size[], width: number): Packing {
  if (!(Number.isInteger(width) && width > 0)) throw new Error(`packRects: width must be a positive integer, got ${width}`)
  sizes.forEach((s, i) => {
    if (!(Number.isInteger(s.width) && Number.isInteger(s.height) && s.width > 0 && s.height > 0)) {
      throw new Error(`packRects: sizes[${i}] must be positive integers, got ${s.width} x ${s.height}`)
    }
    if (s.width > width) throw new Error(`packRects: sizes[${i}] is ${s.width} wide, past the ${width} sheet`)
  })
  // Tallest first, widest among equals, then input order: a total order,
  // so the packing is the same whatever the input's order of ties.
  let order = sizes.map((_, i) => i).sort((a, b) => sizes[b]!.height - sizes[a]!.height || sizes[b]!.width - sizes[a]!.width || a - b)
  let rects: Placement[] = new Array(sizes.length)
  let x = 0
  let y = 0
  let shelf = 0
  for (let i of order) {
    let s = sizes[i]!
    if (x + s.width > width) {
      y += shelf
      x = 0
      shelf = 0
    }
    rects[i] = { x, y }
    x += s.width
    if (s.height > shelf) shelf = s.height
  }
  return { width, height: y + shelf, rects }
}

/**
 * A sheet width for `sizes`: the smallest power of two that fits the
 * widest size and at least the square root of the total area, so the
 * packing comes out roughly square - the width TexturePacker-class tools
 * default to. A power of two is a multiple of every smaller power of two,
 * which keeps an extruded sheet's edges on mip texel edges.
 */
export function packWidth(sizes: readonly Size[]): number {
  let widest = 0
  let area = 0
  for (let s of sizes) {
    widest = Math.max(widest, s.width)
    area += s.width * s.height
  }
  let side = Math.sqrt(area)
  let width = 1
  while (width < widest || width < side) width *= 2
  return width
}
