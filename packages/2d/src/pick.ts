// Picking narrowphase, pure: no GPU or GUI imports, so its test
// (tests/pick.test.ts) exercises this module headless on the flux binary.
// records.ts walks its draw order calling pointInSprite per record
// (topmost first); layer.ts runs it over the core's candidates when a
// screen-size clamp is in play, sizing the rect with core's
// screenSizeScale (the JS twin of the vertex stages' clamp); floorReach
// bounds that candidate search.

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

/**
 * How far, in view pixels, a floored `w x h` sprite can reach from its
 * center once the floor is in force: half its drawn diagonal at the floor,
 * which depends on the aspect only (the smaller axis is minScreenPx, the
 * larger minScreenPx times the aspect). The candidate radius a pick
 * searches around the pointer, divided by the zoom; 0 when the floor is
 * off or the sprite is collapsed. A ceiling only shrinks the rect, so it
 * never widens the reach. The reach is highest at a size transition's
 * ends (the aspect of two linearly moving axes is monotone), so the
 * targets bound the flight.
 */
export function floorReach(w: number, h: number, minScreenPx: number): number {
  let aw = Math.abs(w)
  let ah = Math.abs(h)
  let smallest = Math.min(aw, ah)
  if (!(minScreenPx > 0 && smallest > 0)) return 0
  let aspect = Math.max(aw, ah) / smallest
  return (minScreenPx * Math.hypot(1, aspect)) / 2
}
