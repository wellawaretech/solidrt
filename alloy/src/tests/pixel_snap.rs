//! Where the paint walk leaves the device grid alone
//! (okf/done/pixel-snap-at-draw.md): under a running track that moves a
//! box, never under a held write, and under `pixelSnap={false}`; what a
//! recording made while the rounding was off does when it comes back; and
//! when a running track keeps a rect's stroke off its device-pixel floor.
//! Headless: the walk's counters, the tree's own answers, and what the walk
//! drew, read back from the frame's display list (`DisplayList::ops` and
//! `renderer::oplog::placed`, test builds only): a rect's stroke width and
//! its device edges. No pixels (the pixels are alloy/examples/pixel_snap.rs
//! and the core app tests).
use crate::renderer::oplog::{placed, Op, Placed};
use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayList, DisplayListBuilder, DrawStyle, Rect};
use crate::rendertree::composite::{apply_content_changes, paint_phase, PaintStats};
use crate::rendertree::{transitions, *};
use std::sync::Arc;
use taffy::prelude::*;

const ROOT: u64 = 1;
const MOVER: u64 = 2;
const BOUNDARY: u64 = 3;
const FILL: u64 = 4;

const LINEAR_100: TransitionSpec = TransitionSpec::Tween { duration_ms: 100.0, curve: Curve::Linear };
// How long the delayed entry holds a write before its track starts.
const HOLD_MS: f32 = 50.0;

fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

fn size(tree: &mut RenderTree, id: u64, w: f32, h: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.size = taffy::Size { width: length(w), height: length(h) };
}

// root(1) > mover(2, a view whose writes animate) > boundary(3, a Recording
// boundary) > fill(4, a rect)
fn tree_with_mover(entry: TransitionEntry) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.create_node(MOVER, View::default().with_layout());
  let mut boundary = View::default().with_layout();
  boundary.repaint_boundary = BoundaryMode::Recording;
  tree.create_node(BOUNDARY, boundary);
  tree.create_node(FILL, Rectangle::default().with_layout());
  tree.insert_node(ROOT, MOVER, None).expect("insert");
  tree.insert_node(MOVER, BOUNDARY, None).expect("insert");
  tree.insert_node(BOUNDARY, FILL, None).expect("insert");
  tree.root = Some(ROOT);
  size(&mut tree, ROOT, 400.0, 300.0);
  size(&mut tree, MOVER, 200.0, 100.0);
  size(&mut tree, BOUNDARY, 100.0, 50.0);
  size(&mut tree, FILL, 10.0, 10.0);
  tree.edit(MOVER, |el| {
    el.transitions = Some(Box::new(TransitionConfig { all: Some(entry), ..Default::default() }));
    Damage::None
  });
  tree
}

fn scalar(v: f32) -> transitions::AnimValue {
  transitions::AnimValue::Scalar(v)
}

struct Stage {
  platform: PlatformContext,
  alloy: crate::Context,
}

impl Stage {
  fn new() -> Self {
    let platform = crate::tests::text_platform();
    platform.set_window_size(400.0, 300.0);
    Self { platform, alloy: headless() }
  }

  fn at_scale(display_scale: f32) -> Self {
    let stage = Self::new();
    stage.platform.set_display_scale(display_scale);
    stage
  }

  // One frame as a producer runs it: the tracks advance, then the paint.
  fn frame(&self, tree: &mut RenderTree, now_ms: f64) -> PaintStats {
    tree.set_transition_now(now_ms);
    tree.advance_transitions();
    apply_content_changes(tree, &self.platform, &self.alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &self.platform, &self.alloy)
  }

  // The same frame recorded the way `composite::render` records one, under
  // the display scale, so the list's root space is device pixels; the list
  // it built.
  fn drawn(&self, tree: &mut RenderTree, now_ms: f64) -> DisplayList {
    tree.set_transition_now(now_ms);
    tree.advance_transitions();
    apply_content_changes(tree, &self.platform, &self.alloy);
    let mut builder = DisplayListBuilder::new(None);
    let scale = self.platform.display_scale();
    builder.scale(scale, scale);
    paint_phase(&mut builder, tree, &self.platform, &self.alloy);
    builder.build().expect("the frame builds")
  }
}

#[test]
fn a_running_track_is_unrounded_and_a_held_write_is_not() {
  let mut tree = tree_with_mover(TransitionEntry { spec: LINEAR_100, delay_ms: HOLD_MS, from: None, exit: None });
  let stage = Stage::new();
  stage.frame(&mut tree, 0.0);
  assert!(!tree.unrounded(MOVER), "at rest");
  // The write is held for its delay: the box sits still, so it stays
  // rounded, and its text stays crisp for the length of the hold.
  assert!(tree.transition_write(MOVER, AnimProp::X, Some(scalar(80.0))));
  stage.frame(&mut tree, 10.0);
  assert!(!tree.unrounded(MOVER), "a held write is not motion");
  // The hold expires and the track runs: the rounding is off for the
  // subtree (the children take it from the walk, not from this answer).
  stage.frame(&mut tree, f64::from(HOLD_MS) + 10.0);
  assert!(tree.unrounded(MOVER), "a running track moves the box");
  assert!(!tree.unrounded(BOUNDARY), "the flag is the mover's own; the walk carries it down");
  // The settling tick drops the track ahead of the frame's paint, so the
  // frame that shows the end is already rounded.
  tree.set_transition_now(f64::from(HOLD_MS) + 200.0);
  tree.advance_transitions();
  assert!(!tree.unrounded(MOVER), "settled before the paint");
}

#[test]
fn a_track_on_a_property_that_moves_no_box_keeps_the_rounding() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  let stage = Stage::new();
  stage.frame(&mut tree, 0.0);
  assert!(tree.transition_write(MOVER, AnimProp::Opacity, Some(scalar(0.5))));
  stage.frame(&mut tree, 10.0);
  assert!(!tree.unrounded(MOVER), "a fade moves nothing");
}

#[test]
fn pixel_snap_off_is_unrounded_at_rest() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  assert!(!tree.unrounded(BOUNDARY));
  tree.edit(BOUNDARY, |el| {
    el.pixel_snap_off = true;
    Damage::Paint
  });
  assert!(tree.unrounded(BOUNDARY), "no track needed");
  assert!(!tree.unrounded(FILL), "the prop is the node's own; the walk carries it down");
}

#[test]
fn a_recording_made_under_a_track_re_records_once_when_it_ends() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  let stage = Stage::new();
  let first = stage.frame(&mut tree, 0.0);
  assert_eq!((first.boundaries_recorded, first.boundaries_reused), (1, 0));
  // A track starts above a recording made at rest: it replays as it is,
  // nothing re-records for the start.
  assert!(tree.transition_write(MOVER, AnimProp::X, Some(scalar(80.0))));
  let started = stage.frame(&mut tree, 10.0);
  assert_eq!((started.boundaries_recorded, started.boundaries_reused), (0, 1), "the start keeps the recording");
  // Its content changes in flight: this recording is made with the
  // rounding off.
  tree.apply_damage(FILL, Damage::Paint);
  let moving = stage.frame(&mut tree, 50.0);
  assert_eq!((moving.boundaries_recorded, moving.boundaries_reused), (1, 0));
  let replayed = stage.frame(&mut tree, 60.0);
  assert_eq!((replayed.boundaries_recorded, replayed.boundaries_reused), (0, 1), "it replays while the track runs");
  // The track ends: the interior goes back on the resting grid, once.
  let settled = stage.frame(&mut tree, 200.0);
  assert_eq!((settled.boundaries_recorded, settled.boundaries_reused), (1, 0), "re-recorded at rest");
  tree.apply_damage(MOVER, Damage::Compose);
  let idle = stage.frame(&mut tree, 210.0);
  assert_eq!((idle.boundaries_recorded, idle.boundaries_reused), (0, 1), "and never again");
}

#[test]
fn a_recording_under_pixel_snap_off_does_not_re_record_when_a_track_ends() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  let stage = Stage::new();
  stage.frame(&mut tree, 0.0);
  // The prop arrives: the recording below holds the other rounding.
  tree.edit(MOVER, |el| {
    el.pixel_snap_off = true;
    Damage::Paint
  });
  let switched = stage.frame(&mut tree, 10.0);
  assert_eq!((switched.boundaries_recorded, switched.boundaries_reused), (1, 0), "re-made for the new rounding");
  // A track runs and ends: the rounding was off before, during and after,
  // so the recording replays throughout.
  assert!(tree.transition_write(MOVER, AnimProp::X, Some(scalar(80.0))));
  for now in [20.0, 60.0, 200.0, 210.0] {
    tree.apply_damage(MOVER, Damage::Compose);
    let frame = stage.frame(&mut tree, now);
    assert_eq!((frame.boundaries_recorded, frame.boundaries_reused), (0, 1), "at {now}");
  }
}

// A rect's stroke is not floored to device pixels while a track moves its
// width (okf/done/border-width-device-pixels.md): the query the walk hands
// the build counts running tracks only, so a held write is floored, and
// the settling tick drops the track ahead of the frame that shows the end.
#[test]
fn a_running_stroke_width_track_moves_the_width_and_a_held_write_does_not() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  tree.edit(FILL, |el| {
    let entry = TransitionEntry { spec: LINEAR_100, delay_ms: HOLD_MS, from: None, exit: None };
    el.transitions = Some(Box::new(TransitionConfig { all: Some(entry), ..Default::default() }));
    Damage::None
  });
  let stage = Stage::new();
  stage.frame(&mut tree, 0.0);
  assert!(!tree.stroke_width_moving(FILL), "at rest");
  assert!(tree.transition_write(FILL, AnimProp::StrokeWidth, Some(scalar(3.0))));
  stage.frame(&mut tree, 10.0);
  assert!(!tree.stroke_width_moving(FILL), "a held write is not motion");
  stage.frame(&mut tree, f64::from(HOLD_MS) + 10.0);
  assert!(tree.stroke_width_moving(FILL), "a running track moves the width");
  assert!(!tree.stroke_width_moving(BOUNDARY), "the track is the rect's own");
  tree.set_transition_now(f64::from(HOLD_MS) + 200.0);
  tree.advance_transitions();
  assert!(!tree.stroke_width_moving(FILL), "settled before the paint");
}

// The rect build's width and count, headless
// (okf/done/border-width-device-pixels.md): `painted_stroke_width` against
// a context whose grid, rest factor and width flag are set by hand. The
// width is in frame units; the count is what an enclosing recording reads
// to know it holds content made for one scale.
#[test]
fn the_rect_build_floors_its_stroke_and_counts_it() {
  let platform = crate::tests::text_platform();
  let alloy = headless();
  let stroked = || {
    let mut rect = Rectangle::default();
    rect.paint.draw_style = DrawStyle::Stroke;
    rect.paint.stroke_width = 1.0;
    rect
  };
  // The width a rect paints at under (grid scale, rest factor, a moving
  // width), and how much it added to the count.
  let built = |rect: &Rectangle, grid: f32, rest: f32, moving: bool| {
    let mut ctx = BuildContext::new(&platform, &alloy);
    ctx.grid = crate::rendertree::grid::at_scale(grid);
    ctx.scale_target = rest;
    ctx.stroke_width_moving = moving;
    let width = rect.painted_stroke_width(&mut ctx);
    (width, ctx.scale_bound)
  };
  let close = |a: f32, b: f32| (a - b).abs() < 1e-4;

  // 1 px at 1.5x: one device pixel, counted.
  let (width, count) = built(&stroked(), 1.5, 1.0, false);
  assert!(close(width, 1.0 / 1.5) && count == 1, "{width} {count}");
  // A running track on the width: the declared width, still counted.
  let (width, count) = built(&stroked(), 1.5, 1.0, true);
  assert!(close(width, 1.0) && count == 1, "{width} {count}");
  // Headed to twice the scale: floored at 3x, three device pixels.
  let (width, count) = built(&stroked(), 1.5, 2.0, false);
  assert!(close(width, 1.0) && count == 1, "{width} {count}");
  // Geometry of its own: as declared, not counted.
  let own = Rectangle { w: Some(30.0), ..stroked() };
  let (width, count) = built(&own, 1.5, 1.0, false);
  assert!(close(width, 1.0) && count == 0, "{width} {count}");
  // A plain fill strokes nothing and is not counted.
  let (_, count) = built(&Rectangle::default(), 1.5, 1.0, false);
  assert_eq!(count, 0);
  // A stroke that paints nothing (a border with a width and no color) is
  // floored like any other but not counted.
  let mut clear = stroked();
  clear.paint.color = Color::new_srgba(0.0, 0.0, 0.0, 0.0);
  let (width, count) = built(&clear, 1.5, 1.0, false);
  assert!(close(width, 1.0 / 1.5) && count == 0, "{width} {count}");
}

// The wiring through the real paint walk, by the recording's count: a
// stroked rect whose width track runs, inside a recording. The query is
// true for the node through the run, and the recording made in flight
// still counts the stroke as made for one scale. What the build then
// paints is read back from the frame's list below
// (a_running_width_track_draws_the_declared_width_and_the_end_its_floor).
#[test]
fn a_recording_of_a_rect_with_a_running_width_track_is_scale_bound() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  tree.edit(FILL, |el| {
    el.transitions = Some(Box::new(TransitionConfig { all: Some(LINEAR_100.into()), ..Default::default() }));
    let paint = el.kind.paint_mut().expect("a rect paints");
    paint.set_draw_style(Some(DrawStyle::Stroke));
    paint.set_stroke_width(Some(1.0))
  });
  let stage = Stage::new();
  stage.frame(&mut tree, 0.0);
  assert!(tree.transition_write(FILL, AnimProp::StrokeWidth, Some(scalar(3.0))));
  let moving = stage.frame(&mut tree, 50.0);
  assert!(tree.stroke_width_moving(FILL), "the track runs through the frame");
  assert_eq!(moving.boundaries_recorded, 1, "the width moved: the recording is made in flight");
  let scale_bound = match &*tree.node(BOUNDARY).paint_cache.borrow() {
    Some(PaintCache::Recording(rec)) => rec.scale_bound,
    _ => panic!("the boundary holds a recording"),
  };
  assert!(scale_bound, "the moving stroke is still counted");
}

// The display scale of the stroke readback tests: one where a 1 px stroke
// and a fractional box offset both fall between device pixels.
const SNAP_SCALE: f32 = 1.5;
// The fill rect's margin, in logical px: 15.45 device px at SNAP_SCALE, so
// the snap moves the box's near edges.
const FRACTIONAL_MARGIN: f32 = 10.3;
// The fill rect's side, in logical px: 15.75 device px at SNAP_SCALE, so
// its far edges (at 31.2 device px) are snapped too.
const FRACTIONAL_BOX: f32 = 10.5;
// A declared width of 2.85 device px at SNAP_SCALE: one that floors to 2
// and would round to 3, above the one-pixel minimum.
const WIDE_STROKE: f32 = 1.9;
// The width the running track heads to: 4.5 device px at SNAP_SCALE,
// floored to 4 once the track settles.
const TRACK_END: f32 = 3.0;
// A frame inside the track's run, where the width it has reached is no
// whole number of device px.
const MID_TRACK_MS: f64 = 30.0;
// A frame past the track's end.
const SETTLED_MS: f64 = 200.0;
// How far a device coordinate or width may sit from the expected one:
// float noise.
const EPS: f32 = 1e-4;

fn whole(v: f32) -> bool {
  (v - v.round()).abs() < EPS
}

fn margin(tree: &mut RenderTree, id: u64, m: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.margin = taffy::Rect { left: length(m), top: length(m), right: zero(), bottom: zero() };
}

// The fill rect stroked at `width`, through its paint props.
fn stroke_fill(tree: &mut RenderTree, width: f32) {
  tree.edit(FILL, |el| {
    let paint = el.kind.paint_mut().expect("a rect paints");
    paint.set_draw_style(Some(DrawStyle::Stroke));
    paint.set_stroke_width(Some(width))
  });
}

// The one stroked rect a frame of these trees draws, with the transform it
// was drawn under, and its rect in the frame it was drawn in.
fn stroked_rect(ops: &[Op]) -> (Placed<'_>, Rect) {
  let mut found = placed(ops)
    .into_iter()
    .filter(|p| matches!(p.op, Op::DrawRect { paint, .. } if paint.draw_style == DrawStyle::Stroke));
  let drawn = found.next().expect("a stroked rect is drawn");
  assert!(found.next().is_none(), "one stroked rect");
  let Op::DrawRect { rect, .. } = drawn.op else { unreachable!("filtered on rect draws") };
  (drawn, *rect)
}

// The width a placed draw's paint strokes at, in its own frame.
fn stroke_width(drawn: &Placed) -> f32 {
  drawn.op.paint().expect("a draw paints").stroke_width
}

// A stroked rect that takes its geometry from the frame, at a fractional
// offset and of a fractional size, drawn by the real walk at 1.5x: its 1 px
// stroke is drawn at one device pixel (1 / 1.5 in frame units), its drawn
// rect is the snapped box inset by half that width, and the box's device
// edges (the drawn rect outset by half the width) are whole.
#[test]
fn a_rect_stroke_is_drawn_at_its_device_pixel_floor_inside_whole_edges() {
  let (near, far) = (FRACTIONAL_MARGIN * SNAP_SCALE, (FRACTIONAL_MARGIN + FRACTIONAL_BOX) * SNAP_SCALE);
  assert!(!whole(near) && !whole(far), "both edges are fractional on the device grid");
  let mut tree = tree_with_mover(LINEAR_100.into());
  size(&mut tree, FILL, FRACTIONAL_BOX, FRACTIONAL_BOX);
  margin(&mut tree, FILL, FRACTIONAL_MARGIN);
  stroke_fill(&mut tree, 1.0);
  let stage = Stage::at_scale(SNAP_SCALE);
  let list = stage.drawn(&mut tree, 0.0);
  let (drawn, rect) = stroked_rect(list.ops());

  let width = stroke_width(&drawn);
  assert!((width - 1.0 / SNAP_SCALE).abs() < EPS, "{width}");
  let device_width = drawn.device_stroke_width().expect("an upright frame has one device width");
  assert!((device_width - 1.0).abs() < EPS, "{device_width}");
  // The box spans 15.45 to 31.2 device px; the snap rounds both edges, to
  // 15 and 31 (16 device px), and the drawn rect is that box inset by half
  // the width.
  let (near, far) = (near.round(), far.round());
  let snapped = (far - near) / SNAP_SCALE;
  let inset = width / 2.0;
  let local = [rect.min_x(), rect.min_y(), rect.width(), rect.height()];
  let want = [inset, inset, snapped - width, snapped - width];
  assert!(local.iter().zip(want).all(|(got, want)| (got - want).abs() < EPS), "{rect:?}");
  let device = drawn.device_rect().expect("an upright frame has a device box");
  let half = device_width / 2.0;
  let edges = [device.min_x() - half, device.min_y() - half, device.max_x() + half, device.max_y() + half];
  assert!(edges.iter().all(|e| whole(*e)), "{edges:?}");
  let want = [near, near, far, far];
  assert!(edges.iter().zip(want).all(|(got, want)| (got - want).abs() < EPS), "{edges:?}");
}

// The stroke is drawn at the whole number of device pixels its width
// floors to, not at any whole number and not the nearest one: 2.85 device
// px paints 2.
#[test]
fn a_rect_stroke_is_drawn_at_the_whole_device_pixels_its_width_floors_to() {
  assert!((WIDE_STROKE * SNAP_SCALE).fract() > 0.5, "the floor and the rounding differ");
  let mut tree = tree_with_mover(LINEAR_100.into());
  margin(&mut tree, FILL, FRACTIONAL_MARGIN);
  stroke_fill(&mut tree, WIDE_STROKE);
  let stage = Stage::at_scale(SNAP_SCALE);
  let list = stage.drawn(&mut tree, 0.0);
  let (drawn, _) = stroked_rect(list.ops());
  let device_width = drawn.device_stroke_width().expect("an upright frame has one device width");
  assert!((device_width - 2.0).abs() < EPS, "{device_width}");
  let device = drawn.device_rect().expect("an upright frame has a device box");
  let half = device_width / 2.0;
  let edges = [device.min_x() - half, device.min_y() - half, device.max_x() + half, device.max_y() + half];
  assert!(edges.iter().all(|e| whole(*e)), "{edges:?}");
}

// The walk hands a running width track to the rect build
// (composite::record_node): mid-track the stroke is drawn at the width the
// track has reached, the rect's declared width, where its floor would be
// another; once the track settles, at the floor of the end width.
#[test]
fn a_running_width_track_draws_the_declared_width_and_the_end_its_floor() {
  let mut tree = tree_with_mover(LINEAR_100.into());
  stroke_fill(&mut tree, 1.0);
  tree.edit(FILL, |el| {
    el.transitions = Some(Box::new(TransitionConfig { all: Some(LINEAR_100.into()), ..Default::default() }));
    Damage::None
  });
  let stage = Stage::at_scale(SNAP_SCALE);
  stage.drawn(&mut tree, 0.0);
  assert!(tree.transition_write(FILL, AnimProp::StrokeWidth, Some(scalar(TRACK_END))));

  let list = stage.drawn(&mut tree, MID_TRACK_MS);
  assert!(tree.stroke_width_moving(FILL), "the track runs through the frame");
  let declared = tree.node(FILL).kind.paint().expect("a rect paints").stroke_width;
  assert!(declared > 1.0 && declared < TRACK_END, "mid-track: {declared}");
  assert!(!whole(declared * SNAP_SCALE), "its floor is another width: {declared}");
  let (drawn, _) = stroked_rect(list.ops());
  let width = stroke_width(&drawn);
  assert!((width - declared).abs() < EPS, "drawn {width}, declared {declared}");

  let list = stage.drawn(&mut tree, SETTLED_MS);
  assert!(!tree.stroke_width_moving(FILL), "settled");
  let floored = (TRACK_END * SNAP_SCALE).floor() / SNAP_SCALE;
  assert!((floored - TRACK_END).abs() > EPS, "the end width has a floor of its own");
  let (drawn, _) = stroked_rect(list.ops());
  let width = stroke_width(&drawn);
  assert!((width - floored).abs() < EPS, "drawn {width}, floor {floored}");
}
