// The render origin's snap policy, shared by every consumer that keeps the
// world in the spatial arena's f64 and uploads f32 relative to a point
// near its camera (flux:spatial setView `origin`, setInstanceOrigin): the
// 3d camera (one per target) and the 2d sprite layer (one per pose
// buffer). The origin sits on a power-of-two grid and moves only when the
// eye has left a band of one cell around its own cell, so a camera move
// is a shared uniform write and nothing else, and an origin move - which
// costs a write per entry or a record republish - happens once per cell
// crossed. The cell scales with what the camera resolves (its near plane,
// a 2d view's pixel), so the error at the closest content is the same
// fraction of a pixel in metres, kilometres or texels.

/**
 * The cell the origin snaps on: the largest power of two at or below
 * `scale * factor`, where `scale` is the finest distance the camera
 * resolves (the near plane, a 2d pixel in world units) and `factor` how
 * many of those the eye may stray from the origin. A power of two keeps
 * every origin, and every difference of two origins, exact in f32 as
 * well as f64. A scale that is not a positive finite number yields 0: no
 * snapping, the origin stays.
 */
export function originCell(scale: number, factor: number): number {
  let size = scale * factor
  if (!(size > 0 && Number.isFinite(size))) return 0
  return 2 ** Math.floor(Math.log2(size))
}

/**
 * Re-snap `origin` (in place) to the cell `eye` lies in when the eye has
 * left the band of one cell around the origin's cell on any axis;
 * returns whether it moved. Right after a snap the eye lies within
 * [0, cell) of the origin per axis, and it stays within [-cell, 2 cell)
 * between snaps, so the origin is never further than two cells from the
 * eye. A cell of 0 never moves the origin.
 */
export function snapOrigin(origin: number[], eye: ArrayLike<number>, cell: number): boolean {
  if (!(cell > 0)) return false
  let moved = false
  for (let a = 0; a < origin.length; a++) {
    let d = eye[a]! - origin[a]!
    if (d < -cell || d >= 2 * cell) {
      origin[a] = Math.floor(eye[a]! / cell) * cell
      moved = true
    }
  }
  return moved
}
