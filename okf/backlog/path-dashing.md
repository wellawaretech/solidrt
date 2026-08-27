---
title: Path dashing through the shared dash walker
description: <path>/<d-path> take onLength/offLength/dashOffset with line's semantics - the stroke walks the lyon-flattened subpaths through the walker shared with line (kinds/dash.rs), the fill keeps the true curve, the pattern restarts at each subpath, and a d-path's bounds count dashes as caps.
created: 2026-08-27
---

# Path dashing through the shared dash walker

Line's dash walker ([line-points](line-points.md) stage 2) only needs
`(Point, Point)` segments, and lyon already flattens a path for the stroke
hit test, so a path dashes through the same code: `Dash`, `Pen` and
`walk_dashes` moved from `kinds/line.rs` to `kinds/dash.rs` (with
`Dash::new` holding the both-lengths-and-a-positive-gap rule), and
`Path::walk_dashed` feeds `lyon_path.iter().flattened(DASH_TOLERANCE)`
through it, one subpath at a time.

Contract, in `DashProps` (shared by `LineProps` and `PathProps`):
- Same three props, same semantics as on a line: continuous phase along the
  geometry, `dashOffset` as SVG's stroke-dashoffset, zero `on` as dots.
- The pattern restarts at each subpath, as SVG's does, so a multi-subpath
  `d` never gets a dash bridging two subpaths.
- Dashing is a stroke property: stroke-and-fill fills the true curve and
  strokes the flattened dashes.
- Bounds: every dash has open ends, so a dashed path counts as capped for
  the square-cap outset even when its subpaths are closed.
- The dash props are `Damage::Paint`: the cached geometry stays, only the
  dashed stroke is walked again at build time.

Deferred until measured: caching the flattened segments across
`dashOffset` writes (marching ants re-flatten every frame), and a
scale-aware tolerance (`DASH_TOLERANCE` is 0.25 local units, so a d-path
under a large scale transform shows facets on its dashes).

## Findings (2026-08-27, implemented and verified)

- Verified on the rebuilt client with the line-points example's new tile
  (a two-cubic `d-path`, 10/6 pattern, round caps, `dashOffset` from
  onFrame): captures show the dashes riding the curve with no visible
  facets at 0.25 tolerance, two captures a moment apart show the phase
  moved, and the ring and dotted segment still dash as before. Steady
  state with three dashed nodes plus the 200-point trace: 61 fps, JS
  0.9 ms, paint 0.9 ms, p95 1.7 ms (one 18 ms first frame).
- The tile's `/tree` box is the tight curve extent (20, 22.13) 180x55.19
  (control points would have put y at 0) plus 6: the example leaves the
  default miter join, so the limit-based outset applies (4 x half width),
  not the round cap's 1.5. Same conservative rule as line's, see
  [line-points](line-points.md).
- Walker fix while porting: a run that ended exactly on a vertex was closed
  by the next segment re-emitting that vertex as a zero-length `L`
  (invisible on screen, wrong in the traced runs). It now closes on the
  vertex itself. The end-of-path rule stays as it was (a dash starting
  exactly at the end is not emitted, as Skia's dasher does).
- `cargo test -p flux` builds the gpu examples without the `gui` feature
  and fails; the property tests want `cargo test -p flux --lib --features
  gui` (52 tests).
- Tests: alloy `tests/path.rs` (restart per subpath, closed subpath walked
  through its close, run endpoints on a quadratic within the tolerance,
  dashes count as caps), flux `path_dash_props_apply_and_transition`.
