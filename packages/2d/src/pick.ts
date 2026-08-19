// Picking narrowphase, pure: no GPU or GUI imports, so the checks rig
// (checks/pick-check.ts) exercises this module headless on the flux binary.
// layer.ts walks its draw order calling this per sprite (topmost first).

/** Exact containment test against a rotated rect (center, size, rotation). */
export function pointInSprite(
  px: number,
  py: number,
  cx: number,
  cy: number,
  w: number,
  h: number,
  rotation: number,
): boolean {
  let dx = px - cx
  let dy = py - cy
  let c = Math.cos(-rotation)
  let s = Math.sin(-rotation)
  let lx = dx * c - dy * s
  let ly = dx * s + dy * c
  return Math.abs(lx) <= w / 2 && Math.abs(ly) <= h / 2
}
