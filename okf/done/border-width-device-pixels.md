---
title: Border widths land off the device grid at fractional scales
description: A 1 logical px stroke at 1.5x is 1.5 device pixels, so its inner edge sits on a half pixel and reads as a soft second row whatever the box does; a rect's stroke is floored to whole device pixels (at least one), as browsers snap a border width, for a laid-out rect and for the detached rect that draws every stock border, and a recording that holds such a stroke re-records when its scale changes.
created: 2026-10-08
completed: 2026-11-11
---

# Border widths land off the device grid at fractional scales

## Symptom

The paint walk snaps every axis-aligned box to the device grid
(okf/done/pixel-snapped-paint-boxes.md), so a stroked box's outer edge is
crisp at any scale. The stroke's inner edge is one stroke width in, and
at 1.5x (the tablet) a 1 logical px stroke is 1.5 device pixels: the
inner edge sits on a half pixel and the border reads as one solid row and
one half-covered row. `alloy/examples/pixel_snap.rs` works around it with
a 2 px stroke at 1.5x.

Every stock border is a detached rect: `View`, `Pressable`, `ScrollView`,
`Image` and `Surface` draw theirs as a `<rect detached drawStyle="stroke">`
that fills the frame of the view it sits in. So the case that matters is the
detached one.

## Done looks like

A Rectangle's stroke (an Oval's is curved and stays as it is) is painted at
a whole number of device pixels when the frame is axis-aligned with a
uniform scale: the declared width floored to device pixels, never below
one.

- It applies when the rect takes its whole geometry from the frame: none
  of `x`, `y`, `w` and `h` is set. That is every laid-out rect, and a
  detached rect that fills the frame it inherits. A detached rect with
  geometry of its own is the animation lane and stays as written.
- Movement does not change the width. A running track on a translate, a
  scroll or a slide turns the rounding of positions off
  (okf/done/pixel-snap-at-draw.md); the width keeps its floor.
- `pixelSnap={false}` keeps the floor too. It turns off the rounding of
  positions, and this is a width.
- Under a scale transition the floor is taken for the scale the transition
  ends at, the scale a text below it is rastered for. Where the stroke is
  under one device pixel at that scale there is no whole width to take,
  and the floor is taken at the frame's own scale: a hairline stays one
  device pixel before, through and after a zoom out, and through the hold
  of a delayed one.
- While a track runs on the width itself the stroke is not floored, and
  the floor returns when the track settles.
- A recording boundary that holds a floored stroke re-records when its
  scale changes, by the rule a recording that holds text follows. A
  snapshot boundary needs nothing: it re-rasters on a density change
  already, and inside it the grid is its texture's.
- Hit testing keeps the declared width.
- `alloy/examples/pixel_snap.rs` asserts a 1 px stroke at 1.5x with no
  partial pixel, for a laid-out rect and for a `<rect detached>` border in
  a view.

Floor, not round: a 1 px border at 1.5x is 1 device pixel, which is what a
1 px hairline usually means. It is the rule CSS Values 4 gives as "snap a
length as a border width": above zero and below one device pixel is one,
anything larger rounds down. Chrome, Firefox and Safari all floor; Chrome
rounded once and changed to match the other two.

## Accepted

- The frame is not axis-aligned under a rotation and has no single device
  width under a scale on one axis, so the stroke is as declared there. A
  running track on `rotate`, `scaleX` or `scaleY` alone therefore paints
  the declared width for the run, and the floor returns when the frame is
  uniform and upright again. The box is resampled every frame of the run
  anyway.
- A scale transition that crosses a whole device pixel of width changes
  the logical width once, on the frame the transition is written (1 px at
  1.5x headed to 3x goes from 0.67 to 1 logical px there). The motion
  hides it, as it hides the text re-raster on the same frame.
- A held scale write (a delay, a stagger) counts from the frame it is
  written, as it does for a text raster: for its delay the stroke is
  floored for the end scale while the box waits at the start scale. It
  shows only where the end crosses a whole device pixel of width (1 px at
  1.5x waiting to go to 3x is 1.5 device pixels for the delay). The
  common held write is an enter that waits out a stagger at its `from`
  scale, and that one is right from its first frame. Counting running
  tracks only was tried on paper and dropped: a recording decides its
  density with the held write counted, so it would not re-record when the
  track starts and would keep the floor of the start scale to the end.
- A delayed zoom out of a recording to a scale where the stroke is under
  one device pixel. The recording is made for the end density on the
  frame the write is scheduled, with the stroke floored at the scale of
  that frame, and nothing makes it again when the zoom ends: the stroke
  rests under one device pixel, as it did before the floor. Read from the
  recording arm, not run. A zoom out with no delay is recorded under its
  running track and is made again at rest.
- A scale written from JS frame by frame has no transition. The stroke is
  floored for each frame's scale, so its width steps by a device pixel
  where the scale crosses one. A recording waits for rest, as its text
  does. okf/backlog/pixel-snap-scroll-and-js-motion.md asks about motion
  written from JS as a whole.
- A recording that holds a border and no text re-records for a scale as
  one that holds text does: once where a static scale write comes to
  rest, and at the start and at the end of a scale transition. Before the
  floor it never did.
- A stroke of several device pixels under a zoom out that ends below one
  is floored at each frame's scale, so it steps down by a device pixel on
  the way. A recording holds it at the width of the frame it was made on
  and is made again at the end.
- The floor is paint only. A stock border takes no layout space and draws
  over the children, so nothing shows. An app that pads its content by the
  border width gets padding of the declared width (1.5 device pixels)
  beside a border of 1: a gap of up to one device pixel. Chrome had this
  gap after its change to floor and fixed it by snapping the width ahead
  of layout; layout here does not know the grid.

## What it involves

A new `grid.rs` helper gives the whole-device-pixel width, for a frame that
is axis-aligned with `m11 == m22`. `grid::scale` alone does not qualify: it
answers under a rotation and takes the longer axis. The helper reads the
map itself and not `axis_aligned`, which answers None while the rounding is
off: the floor holds there. `kinds/rect.rs` uses the result in
`stroke_path` and `dashed_outline`, `stroke_inset` takes the width as an
argument instead of reading the declared one (`kinds/oval.rs` passes the
declared width), and the paint is built with it. A zero width still paints
nothing. The floor takes a named epsilon, so matrix noise (3.9999 for 4)
loses no pixel. A scale that is not finite and positive answers None.

The floor is taken at the scale the stroke rests at, the grid scale times
`ctx.scale_target`, so a scale transition carries one logical width from
start to end and nothing pops when it settles. A translate or a scroll
leaves `scale_target` at 1.

The minimum of one device pixel is not taken at the rest scale. Taken
there, a zoom out would widen the stroke at the scale it is drawn at: 1 px
at 1x headed to a scale of 0.5 would paint 2 device pixels on the frame
the transition is written, and for the whole hold of a delayed one. So
where the floor at the rest scale is under one device pixel, the helper
answers with the floor at the frame's own scale, and its minimum of one.

`scale_target` is the factor text and the boundary densities read, a held
write counted (`scale_transition_factor` reads `Transitions::target`). The
floor reads the same one and nothing in the transition engine changes: one
factor, so a stroke in a recording is floored for the density the
recording is made for.

The floor reads one transition only: a running track on the node's
`StrokeWidth` skips it, as the width is what moves. The settling tick
writes the end value, so that frame is floored. A held write does not skip
it: the query is `Transitions::any_track`, which counts tracks alone. A
kind's build sees only the `BuildContext`, so the walk reads the track for
the node and hands the answer over in the context, beside `size`.

`Rectangle::build` serves laid-out and detached rects alike. It needs
nothing from the walk to tell them apart: its own `x`, `y`, `w` and `h`
say whether the geometry is the frame.

### Recordings

A recording holds the stroke at the width it was recorded with, and a
recording with no text never re-records for a scale
(`RecordingCache::holds_text` gates the density compare). Replayed under
another scale the floor is wrong: recorded at 3x and replayed at 1.5x, a
1 px stroke is 1.5 device pixels again; recorded at 1.5x and replayed at
3x it is 2 device pixels where 3 are declared. That is a static scale
write above the boundary, a scale transition, or the boundary's own
hoisted scale.

A floored stroke is content made for one scale, as a text layer is. The
rect build counts in the `BuildContext` counter the text build counts its
layers in (`text_layers`, which only the recording reads), a replayed
recording adds its own to the outer count as it does for text, and the
counter and `holds_text` take one name for both (content made for one
scale). The compare,
the wait for rest (`note_scale_wait`) and the density stored do not change:
the recording follows `text::raster_density`, so it re-records once where
a zoom starts with a known end, or once where the scale comes to rest, and
never for a breathing scale inside the hysteresis. A stroke replayed
within that hysteresis is a few percent off a whole pixel, as the text
beside it is.

The count is of every stroked rect that takes its geometry from the
frame and paints visibly, whatever the floor answered for it: a 1 px
stroke recorded at 2x is 2 device pixels as declared, and still wrong
when replayed at 1.5x.

One more case has no density to compare: a recording made where the frame
had no uniform scale (a rotation above it, a scale on one axis) holds the
declared width, and the density compare does not see the frame come
upright at the same longer-axis scale. The cache stores whether the grid
it was recorded on could floor a stroke, and a replay of content made for
one scale on a grid that can, from a recording made on one that could
not, makes it again. The other way round is left: a stroke floored
upright and replayed under a rotation is a fraction thinner than
declared, on edges that are resampled anyway.

### Tests and documents

- `alloy/src/tests/grid.rs`: the helper. The floor, the minimum of one, a
  zero width, the epsilon, None under a rotation and under a non-uniform
  scale, and the same answer with the rounding off.
- The rect build, headless (`alloy/src/tests/pixel_snap.rs`): the width
  it paints at for a grid, a rest factor, a running width track and
  geometry of its own, and what it counts. The walk-level link, that the
  walk hands the running width track to the build, is tested through the
  display list readback: the drawn width is read from the frame's list
  mid-track and after it settles
  (okf/done/headless-display-list-readback.md).
- `alloy/src/tests/composite.rs`: a recording with a stroked rect and no
  text re-records on a scale change, and one made under a rotation is
  made again when the frame comes upright.
- `alloy/src/tests/box_dash.rs` and `alloy/src/tests/paint.rs` call
  `dashed_outline` and `stroke_inset` directly and follow their new
  arguments.
- `alloy/examples/pixel_snap.rs`: the 1.5x entry of `SCALES` goes from a
  2 px stroke to 1 px. Its scenes already hold the laid-out rects and the
  detached border in a view (`build_nested`), which reads the stroke as
  `stroke * scale` device pixels and takes the floor instead. It measures
  the stroke: the inked run at an edge of a stroked box is the floored
  width, so a stroke of the wrong whole width fails too. It also writes a
  static scale on its recording and asserts one re-record with no partial
  pixel after it.
- `packages/components/docs/types.md` (the source `AGENTS.md` and
  `README.md` of the components are generated from) and
  `docs/guides/coming-from-html.md` say, where they describe
  `borderWidth`, that a border is painted at whole device pixels, at
  least one. `packages/core/AGENTS.md` and `packages/core/src/types.d.ts`
  say the same of a rect's `strokeWidth`, and the `pixelSnap` comment
  says the width keeps its floor.

## Not part of it

A thin filled box. A divider is a filled rect in a view one logical pixel
high, so its thickness comes from the box snap, each edge rounded by
itself: at 1.5x it is 1 or 2 device pixels by the row it lands on. That is
a size and not a stroke width, it takes layout space, and the floor here
does not reach it: okf/backlog/divider-caret-device-pixels.md, which
builds on this note.

## Related

- okf/done/pixel-snapped-paint-boxes.md: the box snap this completes.
- okf/done/pixel-snap-at-draw.md: independent. The frame a detached rect
  inherits is on the grid today and stays there after it. Under its running
  tracks the box is not rounded, and the width keeps its floor. It added
  the tracks-only query (`Transitions::any_track`) and `pixelSnap`.
