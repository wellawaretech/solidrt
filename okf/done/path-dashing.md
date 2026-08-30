---
title: Path dashing through the shared dash walker
description: <path>/<d-path> take onLength/offLength/dashOffset with line's semantics - the stroke walks the lyon-flattened subpaths through the walker shared with line (kinds/dash.rs), the fill keeps the true curve, the pattern restarts at each subpath, and a d-path's bounds count dashes as caps; pathLength (SVG) on both kinds makes the pattern fractional for partial draws.
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
- `pathLength` (SVG's attribute, on both kinds): what the geometry's length
  counts as in the pattern's units; the pattern is scaled by the walked
  length over it before the walk. `pathLength={1}` makes the props
  fractions, so `onLength={0.77} offLength={1}` draws the first 77% and an
  `onLength` written or transitioned from 0 to 1 draws the geometry on:
  the standard line-drawing trick without an app-side length. The length
  is the WALKED one (the flattened segments summed, cached with the path's
  geometry; the segment sum for a polyline), not lyon's analytic
  `approximate_length`, so "draw to 1" ends exactly where the walker ends.
  Non-positive declarations are rejected by the property layer and ignored
  by the kinds. Not animatable: it is a unit declaration.

The walker's pieces (`kinds/dash.rs` `Piece`): a segment, or a curve with
its arc length tabulated from lyon's flattening (`for_each_flattened_with_t`,
`DASH_TOLERANCE` 0.25 local units). The pattern is measured along that
table and each run is emitted as the curve split at the matching `t`
(`split_range`), so Impeller strokes real quadratics and cubics and
tessellates them in screen space like the solid stroke: no facets at any
display scale or transform. The tolerance only places the boundaries along
the curve, within a quarter unit of where a polyline of it would.

Not done on purpose: caching the pieces across `dashOffset` writes.
Marching ants rebuild the tables every frame, and measured that is noise
(paint 0.9 ms steady with three dashed nodes plus a 200-point trace, see
Findings; the three dashed boxes added 2026-08-30 did not move it). A
cache would only pay for itself on a path with thousands of curve pieces
re-phased every frame, which nothing asks for.

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
- `pathLength` followed the same day, prompted by "draw only 77% of a path
  or polyline": `Dash::scaled`, `walked_length` in kinds/dash.rs,
  `Path::length` cached in a Cell, `Line::dash(points, closed)` takes the
  geometry it scales against (the endpoint form's length depends on the
  box). Tests: line `path_length_makes_the_pattern_fractional`, path
  `path_length_makes_the_pattern_fractional` and
  `path_length_measures_the_walked_curve` (drawn to 1 ends within 0.01 of
  the curve's end, half of it at the apex), flux
  `path_length_applies_on_both_kinds_and_must_be_positive`. The
  line-points example got a partial-draw row (77% of the curve, a triangle
  drawing on from onFrame).
- Verified on the rebuilt client: the 77% curve's drawn end lands at local
  x 173.5 against 173.8 computed (the 77% arc-length point plus the round
  cap's 2), the curve's true end is unpainted, and two captures of the
  triangle a moment apart show different amounts drawn. 60 fps, JS 1.0 ms,
  paint 0.6 ms, p95 1.9 ms with the extra row.
- Curve pieces followed the same day after "the dashed curve looks a bit
  jagged": the dashes were a polyline flattened at 0.25 local units, which
  facets under a designSize fit, a scale transform or HiDPI while the solid
  path (Impeller's own screen-space tessellation) stays smooth. The walker
  now walks pieces and emits curve splits (see above); the flattening only
  measures. The jaggies actually seen turned out to be something else, see
  the MSAA finding below. Tests:
  `dashes_on_a_curve_are_pieces_of_it` (every run a Q piece, boundaries on
  the curve within 0.01), `a_whole_curve_dashes_as_itself`,
  `a_run_continues_from_a_segment_into_a_curve`. On the rebuilt client the
  scale-4 captures look smooth, the scale-2 capture of the 77% curve diffs
  from the polyline form by 40 of 2800 pixels with the drawn end unmoved,
  and boxes/stats are unchanged (paint 0.9 ms). A pixel metric (stroke
  edge midpoint vs the analytic cubic at 4x) stays within +-0.6 px on
  gentle slopes, which is that estimator's floor for a 16 px AA stroke, so
  it neither shows facets nor rules out sub-pixel ones.
- The jaggies reported were on a 1x display and showed on the SOLID
  yellow triangles as well: a scale-1 capture holds exactly five colours
  (background, stroke, 25/50/75% coverage), i.e. the 4x MSAA every rig
  rasterization runs at (`MSAA_SAMPLES` in gl/rig.rs; Impeller GL has no
  analytic AA). Edge quality is the sample count, not the dash geometry:
  [desktop-msaa-8x](../backlog/desktop-msaa-8x.md). Lesson kept in memory: ask the
  human about the display and what exactly looks wrong before building
  measurements.
- Example layout fix found in the same run: the tile rows were wider than
  the window (626 wide, sized by the window manager while the client asks
  for 1280x720), so the tiles flex-shrank and their box-filling d-rects
  shrank with them while the line and path geometry did not, putting the
  dashed strokes outside their rects. The rows now `flexWrap="wrap"` so a
  tile keeps its design size. A tile holding only detached geometry must
  never be allowed to shrink: wrap the row or give it `flexShrink={0}`.

## Box primitives (2026-08-30)

`rect` and `oval` take the same `DashProps`, prompted by external feedback
("a dashed selection rect is a stock UI element", and rebuilding it as a
96-point `d-line` ring loses the inside-the-box stroke rule).

- The walker's pieces are the inset stroke path itself: `box_outline` in
  `kinds/dash.rs` (edges plus kappa-cubic quarter arcs for the radii, each
  clamped to the half box) and `oval_outline` (four quarter arcs). Both
  start where SVG's `rect`/`ellipse` do (top edge after the top-left
  corner; 3 o'clock) and run clockwise, so `dashOffset` phases match.
- The inside-stroke rule does not conflict with dashing: the outline is
  inset by half the stroke width and a dash's cap reaches along the
  outline by that same half width, so the box still contains every pixel.
  `local_bounds` stays the box; no outset, unlike path's "dashes count as
  caps".
- Stroke-and-fill fills the inset shape through Impeller's own
  `draw_rect`/`draw_rounded_rect`/`draw_oval` and strokes the walked
  dashes via `draw_path`, the split `path.rs` makes. Solid strokes are
  untouched.
- Hit testing stays the solid ring, as on line and path.
- The oval's fill (Impeller's oval) and its dashed stroke (four kappa
  cubics) are different geometry; the cubic strays at most 0.027% of the
  radius, under a pixel at any drawn size.
- `DASH_TOLERANCE` and `dashed_path` moved to `kinds/dash.rs`, shared by
  all four kinds. Tests: alloy `tests/box_dash.rs`, flux
  `box_dash_props_apply_and_transition`.
