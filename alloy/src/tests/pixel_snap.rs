//! Where the paint walk leaves the device grid alone
//! (okf/done/pixel-snap-at-draw.md): under a running track that moves a
//! box, never under a held write, and under `pixelSnap={false}`; what a
//! recording made while the rounding was off does when it comes back; and
//! when a running track keeps a rect's stroke off its device-pixel floor.
//! Headless: the walk's counters and the tree's own answers, no pixels (the
//! pixels are alloy/examples/pixel_snap.rs and the core app tests).
use crate::renderer::prelude::*;
use crate::renderer::{Color, DisplayListBuilder, DrawStyle};
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

  // One frame as a producer runs it: the tracks advance, then the paint.
  fn frame(&self, tree: &mut RenderTree, now_ms: f64) -> PaintStats {
    tree.set_transition_now(now_ms);
    tree.advance_transitions();
    apply_content_changes(tree, &self.platform, &self.alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &self.platform, &self.alloy)
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

// The wiring through the real paint walk: a stroked rect whose width track
// runs, inside a recording. The walk sets the flag from the query for the
// rect it builds; what the build then paints is not observable headless
// (alloy/examples/pixel_snap.rs reads pixels), so this holds the two ends:
// the query is true for the node through the run, and the recording made
// in flight still counts the stroke as made for one scale.
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
