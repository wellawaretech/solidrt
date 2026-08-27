---
title: Line takes a points array (polyline)
description: `<line>`/`<d-line>` grow a `points` prop - a flat [x0, y0, x1, y1, ...] number array - and a `closed` flag, making line the numeric polyline primitive between d-line's two endpoints and d-path's string DSL; then line implements Bounded so culling, capture and getBoundingBox see what it paints instead of the inherited box. Dashing becomes our own walker (continuous through vertices, animatable with `dashOffset`); endpoints, caps and joins stay as they are.
created: 2026-08-27
---

# Line takes a points array (polyline)

## Problem

The only way to draw a polyline today is a `path` with a `d` string. For
geometry that changes every frame (a chart trace, a drawn stroke, a projected
wireframe) that means formatting thousands of numbers into an SVG string in JS
and having the runtime parse them back, per frame. The external report
[velvet-acre](../feedback/velvet-acre.md) item 4 measured exactly this
(~4,000 numbers per frame) and asked for a numeric points primitive between
`d-line` and `d-path`. The line-vs-path decision
([line-layout-endpoints](../done/line-layout-endpoints.md)) already recorded
"a polyline points array - the numbers-not-string middle ground" as line's
natural growth direction, to be opened when a design asks. This is that item.

## Shape

`line` is the primitive whose geometry is numbers. A polyline is the same
primitive with more than two of them, so it goes on `line`/`d-line` rather
than on a new `polyline` element (which would duplicate the paint, dash, cap
and join plumbing for no gain).

```tsx
<d-line points={[10, 10, 60, 40, 110, 10, 160, 40]} color="#e3b341" strokeWidth={3} />
<d-line points={pts()} drawStyle="fill" color="#4a90d9" />
<line points={[0, 0, 100, 30, 200, 0]} strokeWidth={2} />   // laid out: box measures from the points
```

- `points?: number[] | Float32Array | Float64Array` - flat
  `[x0, y0, x1, y1, ...]`, even length, in the element's local space (the
  same space `x1`..`y2` and a path's `d` use). Flat rather than `[x, y][]`
  because it is the shape a typed array has and marshals as one list instead
  of N+1; it is also SVG's `points` shape. Fewer than two points draws nothing; `null` (unset) is the existing
  two-endpoint form.
- `closed?: boolean` - stroke the closing segment and join at the first
  vertex instead of capping both ends. A fill always covers the polygon
  (implicitly closed, as in SVG), so `closed` is a stroke distinction.
  Default false.
- The paint defaults to stroke: `Line::DEFAULT_DRAW_STYLE`, set by
  `Line::default()` and restored by the line adapter on a null `drawStyle`
  write (PaintState's own null reset is fill, which would silently turn a
  line into a polygon). On a polyline `drawStyle="fill"` /
  `"stroke-and-fill"` fill the polygon (nonzero) and hit-test its interior
  (winding number over the ring, no lyon); dashing applies to the stroke
  part. `draw_line` strokes regardless, so fill has no effect on the
  two-point form. This is Konva's model: one Line node, `points` +
  `closed`, painted by fill/stroke. No `fillRule`; additive if a
  self-intersecting shape ever needs even-odd.
- Precedence: a set `points` wins over `x1`/`y1`/`x2`/`y2`; the endpoints
  are ignored while it is set. Documented, not validated: exclusivity checks
  across two props would depend on the order Solid applies attributes in.
- `points` is content, not box geometry (like a path's `d`), so unlike the
  endpoints it is NOT detached-only: a laid-out `<line points>` measures its
  box from the points' extent exactly the way a laid-out `<path>` measures
  from `d`, and draws them unscaled in the box's local space.
- Transitions do not cover `points` (native transitions are scalars and
  colors). Animate a polyline by writing a new array; that is one property
  write of numbers, which is the whole point. Endpoint transitions
  (`x1`..`y2`) keep working on the two-point form.
- Everything else on line applies unchanged: `strokeCap`, `strokeJoin`,
  `strokeMiter`, `drawStyle`, gradients, pointer props, `onLength`/`offLength`
  (see dashing below, which also adds `dashOffset`).

Validation, per the throw-in-dev policy: a non-list, a non-number entry, or
an odd length is an `Err` naming the property and the offending value.

## Stage 1 - the bare minimum

Rust, alloy `rendertree/kinds/line.rs`:

- `Line` gains `points: Option<Vec<f32>>` and `closed: bool`; `set_points`
  returns `Damage::Layout` (it drives measure, like `Path::set_d`),
  `set_closed` returns `Damage::Paint`. The struct keeps deriving
  Clone/Debug/Default: no geometry cache is needed. `Path` caches because
  parsing `d` is expensive and measure/hit need it too; a points array is
  already the geometry, and build only runs on paint damage.
- `build`: with points, walk them into a `PathBuilder` (`move_to`, `line_to`,
  `close` when closed) and `draw_path`; without, the existing two-point
  `draw_line`/`draw_dashed_line`. Dashing on a polyline in this stage: per
  segment through `draw_dashed_line`, so the pattern restarts at every
  vertex. Honest and documented; stage 2 fixes the phase.
- `measure`: known size wins; otherwise the points' bounding-box size, the
  same convention `Path::measure` uses. No points: unchanged.
- Hit test: point-to-segment distance over consecutive point pairs (plus the
  closing pair when closed) against `max(strokeWidth / 2, 2)`, which is the
  existing single-segment test in a loop; the fill styles add a nonzero
  winding test over the vertex ring.
- `cull.rs` (`Extent::Unbounded`) and `local_bounds` (fallback) stay as they
  are, same as path.

Rust, flux `alloy_plugins/properties/line.rs`:

- `"points"` decodes a `PropValue::List` of numbers into `Option<Vec<f32>>`
  (null resets) with the errors above; `"closed"` decodes a bool.
- `read.rs` reports `points` as `Nums` and `closed` as `Bool` when set, like
  `radius`, so `/tree` shows the truth.
- `to_prop_value` (`alloy_plugins/tree.rs`) gains a typed-array branch ahead
  of the plain-object case: `as_object().as_typed_array::<f32>()` (and
  `::<f64>`) yields a `PropValue::List` of numbers. Today a `Float32Array`
  is an object, not an array, so it falls into the Map branch and marshals
  as index-keyed entries - a silent mis-marshal, so this is a fix as much as
  a feature. Read the elements through `as_raw()`/`as_bytes()` (None when
  the buffer is detached, which marshals as an empty list), not
  `AsRef<[T]>`, whose `as_ref` panics on a detached buffer. If the
  per-element enum cost ever shows at 4k+ points, a `PropValue::Floats(Vec<f32>)`
  variant decoded by the same `points` decoder is the follow-up.

TypeScript, `packages/core/src/types.d.ts`:

- `LineProps` gains `points?: number[] | Float32Array | Float64Array` and
  `closed?: boolean` with doc
  comments carrying the rules above; the LineProps preamble drops "for
  polylines ... a path" and states the precedence rule.
- No runtime JS change: arrays already marshal as lists, typed arrays do
  after the branch above.

Docs: `packages/core/docs/reference/drawing.md` (line section),
`docs/reference/detached.md` ("its geometry is its two endpoints"), and the
intrinsics bullet in `packages/core/AGENTS.md`. `types.d.ts` is the source
the reference pages pull from, so most of it is the doc comments.

Tests: `flux/src/tests/properties.rs` (list decodes, odd length / non-number
/ non-list reject with the property named, null resets, closed bool);
`flux/src/tests/value.rs` (a `Float32Array` marshals to a list of numbers,
not a map);
alloy `tests/hit.rs` or a small `tests/line.rs` (hit near a middle segment,
miss between segments, closed adds the closing segment; measure from points).

Done looks like: a `d-line` with 4,000 points animating at 60 fps writes one
array per frame and parses nothing; a closed triangle strokes three sides
with joins; the tree shows `points`.

## Stage 2 - continuous-phase dashing, and `dashOffset`

Replace the per-segment fallback with a walker that carries the dash phase
across vertices: iterate the segments, keep the remaining on/off length,
and emit each "on" run as a `move_to`/`line_to` subpath into one
`PathBuilder`, then `draw_path` with the stroke paint. Impeller's C API has
`DrawDashedLine` for a segment and no dash effect for paths, so this is ours
to do; it is a ~30-line loop with no allocation beyond the path. A dense
polyline (segments shorter than `onLength`) is where stage 1's per-segment
restart shows: every segment is fully "on" and the dash pattern disappears.
That is the trigger for stage 2, and the reason it is not a maybe.

- A run that crosses a vertex keeps its subpath (`line_to` through the
  vertex), so the join style applies inside a dash the way it does on a
  solid stroke; a run that ends mid-segment is capped. `closed` walks the
  closing segment too, so the pattern continues around the ring.
- The two-point form goes through the same walker (a two-vertex polyline)
  instead of `DrawDashedLine`, so everything below applies to both forms and
  `DrawDashedLine` drops out of the build.
- Degenerate patterns: a non-positive `offLength` has no gap and draws solid;
  a zero `onLength` emits zero-length runs (dots under round/square caps, as
  in SVG); negative lengths clamp to zero. No validation beyond the
  existing number check: these are the SVG/Canvas semantics, not errors.
- `dashOffset?: number` (SVG `stroke-dashoffset`, Canvas `lineDashOffset`):
  the distance into the pattern at which the stroke starts, in local units,
  wrapping around the period and accepting negatives, default 0. Impeller's
  `DrawDashedLine` has no phase parameter, so the offset only exists once
  the walker is ours; it is the same state the walker needs to carry the
  phase across a vertex, applied once at the start. Raising it marches the
  dashes toward the line's start (marching ants). It is paint state
  (`Damage::Paint`, the path is rebuilt on the Rust side; a few hundred
  subpaths for a 200-point line with 10 px dashes) and a scalar, so it joins
  `AnimProp` next to `onLength`/`offLength` for one-shot tweens; a
  continuous march is an `onFrame` write. Dashes never affect hit testing:
  the whole stroke hits, as today.
- Tests: the walker against a recording pen (runs continue across a vertex,
  the offset shifts and wraps, the closing segment is walked, off <= 0 is
  solid); the property decodes, reads back and maps to its `AnimProp`.

## Stage 3 - line implements Bounded

`Bounded::local_bounds(fallback) -> Rect` is a kind's painted box relative
to its own origin (alloy/src/rendertree/mod.rs). Line has no impl, so
`ElementKind::local_bounds` answers with the inherited box, and three
consumers see a d-line as its parent's box: paint-viewport culling
(`cull.rs` treats line as `Extent::Unbounded`, and an unbounded child makes
the whole subtree uncullable up to the nearest clipping ancestor),
`captureSnapshot` of a detached node (`composite.rs` sizes the capture from
`local_bounds`), and `getBoundingBox` / the tree's box (`tree.rs`
`compute_corners`). The last one is the d-line case of
[mcp-detached-node-bounds](mcp-detached-node-bounds.md) (a 190 px line
reported as 1692x1128).

- `impl Bounded for Line`: the AABB of the geometry - the points, or the two
  endpoints with their box defaults resolved against `fallback` - inflated by
  the stroke outset. Fewer than two points: a zero rect at the origin (the
  capture path already returns on a zero texture).
- Stroke outset: strokes are centered on the geometry, so the painted box is
  the geometry plus half the stroke width, and more where caps and joins
  poke past it: `strokeWidth / 2` for butt and round caps and bevel/round
  joins, `* sqrt(2)` for square caps, `* strokeMiter` at miter joins (only
  when there are joins: three or more points, or closed). A fill-only
  `drawStyle` has no stroke outset. One small `stroke_outset` helper on
  `PaintState` or in line.rs, exact enough that `getBoundingBox` is tight
  rather than a padded guess.
- Wire the consumers: add `Line` to the `ElementKind::local_bounds` dispatch;
  in `cull.rs` `own_extent`, `ElementKind::Line(l) =>
  inflate(l.local_bounds(frame), AA_OUTSET)` (the stroke is already inside
  the bounds; AA is the only extra), leaving path `Unbounded`.
- Behaviour change to state in the docs: `getBoundingBox` on a `d-line` now
  reports its painted box, and on a laid-out `<line>` the box plus the stroke
  bleed (a line's stroke straddles its box, unlike a rect's, which paints
  inside). Update the detached-bounds wording in `types.d.ts` /
  `docs/reference/detached.md` where it says a d-line reports the inherited
  box, and note in mcp-detached-node-bounds that the d-line case is covered
  and d-path remains.
- Tests: alloy `local_bounds` for the endpoint form (defaults resolved
  against the fallback), the points form, the closed/miter outset; a cull
  test that a d-line entirely outside the cull rect is skipped and one
  crossing it is not (`tests/cull.rs` has the harness).

Path can follow in the same shape later (it already caches its control-point
AABB; add the x/y offset and the same outset), which would close the backlog
item; not part of this one.

## Not in this item

- Arrowhead markers and per-vertex creasing: the other growth direction from
  [line-layout-endpoints](../done/line-layout-endpoints.md), unchanged.
- Curves stay a path's job; a numeric curve primitive is a different item.
- `Bounded` for path (see stage 3).

## Findings

Stage 1 landed 2026-08-27 (uncommitted), verified on the rebuilt client
through the control API: a 200-point `Float32Array` trace rewritten every
frame reads back as 200 pairs in `/tree`, one prop write per frame
(`setPropsPerFrame` 1) at 50 fps with 0.7 ms JS and 3.3 ms paint; the
laid-out `<line points>` measured 120x24 from its zigzag; snapshots show the
stroked trace, closed round/miter joins, open round caps and per-segment
dashing.

- `draw_path` honours the paint's draw style and `PaintState` defaults to
  fill, so the first build filled every non-dashed polyline (`draw_line`
  strokes regardless). Resolved by giving line its own paint default
  (stroke) and honouring the style on the polyline, which is what makes
  polygons fall out of the same primitive.
- A `Float32Array` is an object to rquickjs, so `to_prop_value` needs an
  explicit typed-array branch ahead of the Map case. `as_bytes()` is the
  safe accessor; `AsRef<[T]>` panics on a detached buffer.

Stage 2 landed 2026-08-27 (uncommitted): `walk_dashes` in `kinds/line.rs`
emits the on runs of `segments(points, closed)` into one `PathBuilder`
through a `Pen` trait (the tests record into a Vec); both forms stroke that
path, so `draw_dashed_line` is gone from the build. `dashOffset` is line
state next to `onLength`/`offLength`, reads back, and is `AnimProp::DashOffset`.
Verified on the rebuilt client through the control API: the example's
marching-ants row (a 48-point ring, 6 px segments under 12/8 dashes) reads
back `dashOffset` climbing per frame and two snapshots a moment apart show
the pattern shifted; the two-point line with `onLength` 0 and round caps
renders as dots (Impeller strokes a zero-length subpath as a cap). Steady
state with the trace and both animated offsets: 61 fps, 3 prop writes per
frame, 1.0 ms JS, 0.6 ms paint, p95 1.9 ms, no slow frames.

- The pattern must open with the on run even when it has zero length, or a
  dotted line loses its first dot: the start state is `phase == 0 || phase <
  on`, not `phase < on`. Runs toggle strictly inside a segment, so a boundary
  landing exactly on the end never opens one more (empty) run.
- A non-positive `offLength` short-circuits to solid before the walker: a
  zero period would loop forever, and a zero gap split into subpaths would
  show caps at every break.

Stage 3 landed 2026-08-27 (uncommitted): `impl Bounded for Line` in
`kinds/line.rs` (`geometry(fallback)` + `stroke_outset()`), wired into
`ElementKind::local_bounds` and `cull.rs` (`inflate(local_bounds, AA_OUTSET)`;
path stays unbounded). Verified on the rebuilt client through `/tree`, whose
boxes come from `bounding_box_viewport` -> `local_bounds`: every d-line in
the example reports its painted box (the 48-point ring 90x90 + 6 for the
miter limit = 102x102 inside its 140x120 tile; the two-point dotted line
260x0 + 3 for round caps = 266x6; the laid-out zigzag 120x24 + 1.5), and a
`captureSnapshot` of a d-line now crops to that box instead of the tile.
Tests: local_bounds for both forms, caps/miter/closed-pair/fill-only outsets,
a cull test (a far d-line's extent misses the cull rect, a crossing one
hits, d-path stays unbounded).

- The miter outset is the limit (`strokeMiter * strokeWidth / 2`), not the
  actual join angles, so an acute open polyline with the default limit 4
  reports up to 4x the half width around it (the example's open triangle:
  16 px for an 8 px stroke where the apex really reaches 7.5 px). Exact per
  vertex would need the join angles; cheap to add if a design asks for a
  tighter box.
- Not this item: the example's trace tile lost its height under a small
  window (a tiling WM sized my client 626x334): a view whose children are
  all detached has no min-content size, so the flex column shrank it to 0
  while the tile rows held their 120 px. `flexShrink={0}` on the tile is the
  example's fix; worth remembering for any d-*-only container.
