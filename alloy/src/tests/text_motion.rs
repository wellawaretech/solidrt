// Raster hysteresis for text under a scale animation
// (okf/done/text-layer-motion.md): the rule a text layer and a recording
// boundary share, and a recording under a scale write that replays while
// the scale moves and re-records once it holds. Headless, so the text's
// own glyph pass fails and it holds no layer; the rule on a layer that
// exists is pinned on the function, the boundary's side on the walk.
use crate::impellers::DisplayListBuilder;
use crate::rendertree::composite::{apply_content_changes, paint_phase};
use crate::rendertree::text::{raster_density, raster_scale, scale_held, scale_observed, RasterScale, ScaleSeen, Text};
use crate::rendertree::*;
use std::sync::Arc;
use taffy::prelude::*;

fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None)
}

fn size(tree: &mut RenderTree, id: u64, w: f32, h: f32) {
  let style = tree.node_mut(id).style_mut().expect("laid-out node");
  style.size = taffy::Size { width: length(w), height: length(h) };
}

const ROOT: u64 = 1;
const ZOOMER: u64 = 2;
const LABEL: u64 = 3;

// root(1, 400x300) > zoomer(2, 200x100, a Recording boundary) > label(3)
fn tree_with_label() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  let mut zoomer = View::default().with_layout();
  zoomer.repaint_boundary = BoundaryMode::Recording;
  tree.create_node(ZOOMER, zoomer);
  let mut label = Text::default();
  label.set_plain_text("Zoom".to_string());
  tree.create_node(LABEL, label.with_layout());
  tree.insert_node(ROOT, ZOOMER, None).expect("insert");
  tree.insert_node(ZOOMER, LABEL, None).expect("insert");
  tree.root = Some(ROOT);
  size(&mut tree, ROOT, 400.0, 300.0);
  size(&mut tree, ZOOMER, 200.0, 100.0);
  tree
}

fn label_scale(tree: &RenderTree) -> Option<f32> {
  match &tree.node(LABEL).kind {
    ElementKind::Text(text) => text.chosen_scale(),
    _ => None,
  }
}

fn zoom(tree: &mut RenderTree, scale: f32) {
  tree.edit(ZOOMER, |e| {
    let ElementKind::View(view) = &mut e.kind else { panic!("the zoomer is a view") };
    view.set_scale_x(Some(scale));
    view.set_scale_y(Some(scale))
  });
}

#[test]
fn the_raster_scale_follows_rest() {
  // The raster's own scale, up to composition noise: nothing happens.
  assert_eq!(raster_scale(1.0, 1.0 + 1e-7, false), RasterScale::Keep);
  assert_eq!(raster_scale(1.0, 1.0 + 1e-7, true), RasterScale::Keep);
  // A zoom in flight: the raster holds, stretched, and waits for rest.
  for step in [1.01, 1.03, 1.05, 1.08, 1.1, 0.9] {
    assert_eq!(raster_scale(1.0, step, false), RasterScale::Wait, "at {step}");
  }
  // The scale held: re-rasterize, once, for exactly that scale (no
  // tolerance at rest: a text resting at 1.015 is not left stretched).
  assert_eq!(raster_scale(1.0, 1.1, true), RasterScale::Rerasterize);
  assert_eq!(raster_scale(1.0, 1.015, true), RasterScale::Rerasterize);
  assert_eq!(raster_scale(1.1, 1.0, true), RasterScale::Rerasterize);
  // Past the motion bound the raster is too soft to keep, moving or not.
  assert_eq!(raster_scale(1.0, 1.6, false), RasterScale::Rerasterize);
  assert_eq!(raster_scale(1.6, 1.0, false), RasterScale::Rerasterize);
  // A scale that is no scale keeps what there is.
  assert_eq!(raster_scale(1.0, 0.0, true), RasterScale::Keep);
  assert_eq!(raster_scale(1.0, f32::NAN, true), RasterScale::Keep);
  // Held means equal up to composition noise, not up to the tolerance.
  assert!(scale_held(1.1, 1.1 + 1e-7));
  assert!(!scale_held(1.1, 1.1005));
}

#[test]
fn a_known_zoom_rasters_for_its_end_at_once() {
  // No transition running (factor 1): the first raster is at the composite
  // scale, a moving scale keeps it and waits, rest re-rasters exactly.
  assert_eq!(raster_density(None, 1.0, 1.0, false), (1.0, false));
  assert_eq!(raster_density(Some(1.0), 1.03, 1.0, false), (1.0, true));
  assert_eq!(raster_density(Some(1.0), 0.97, 1.0, false), (1.0, true));
  assert_eq!(raster_density(Some(1.0), 1.015, 1.0, true), (1.015, false));
  // A zoom to 1.1 under way (the chain's factor says where it ends): the
  // raster is made for the end on the first look, then kept and minified
  // while the scale climbs, and nothing is left to do at rest.
  let end = 1.1_f32;
  let (made, wait) = raster_density(Some(1.0), 1.0, end, false);
  assert!((made - end).abs() < 1e-6 && !wait, "{made} {wait}");
  let (made, wait) = raster_density(Some(1.0), 1.01, end / 1.01, false);
  assert!((made - end).abs() < 1e-6 && !wait, "{made} {wait}");
  let (kept, wait) = raster_density(Some(end), 1.05, end / 1.05, false);
  assert!((kept - end).abs() < 1e-6 && wait, "{kept} {wait}");
  assert_eq!(raster_density(Some(end), end, 1.0, true), (end, false));
  // A text mounting mid-zoom rasters for the end straight away.
  let (made, wait) = raster_density(None, 1.04, end / 1.04, false);
  assert!((made - end).abs() < 1e-6 && !wait);
  // A zoom out (the end below the raster's scale): kept and minified in
  // flight, re-made at the resting scale.
  assert_eq!(raster_density(Some(end), 1.05, 1.0 / 1.05, false), (end, true));
  assert_eq!(raster_density(Some(end), 1.0, 1.0, true), (1.0, false));
  // Past the motion bound the raster is re-made in flight, for the larger
  // of now and the end.
  let (made, wait) = raster_density(Some(1.0), 1.6, 2.0 / 1.6, false);
  assert!((made - 2.0).abs() < 1e-6 && !wait);
}

#[test]
fn a_scale_is_at_rest_after_three_consecutive_looks() {
  // The first look holds nothing; every look a frame later at the same
  // scale adds one, and the third makes rest.
  let first = scale_observed(None, 1.1, 10);
  assert_eq!(first, ScaleSeen { scale: 1.1, frame: 10, held: 0 });
  let second = scale_observed(Some(first), 1.1, 11);
  let third = scale_observed(Some(second), 1.1, 12);
  let fourth = scale_observed(Some(third), 1.1, 13);
  assert_eq!((second.held, third.held, fourth.held), (1, 2, 3));
  assert!(!third.at_rest() && fourth.at_rest());
  // A moved scale starts over, and so does a look that skipped a frame:
  // a writer at half the refresh rate never rests between its writes.
  assert_eq!(scale_observed(Some(fourth), 1.2, 14).held, 0);
  assert_eq!(scale_observed(Some(fourth), 1.1, 15).held, 0);
  let mut seen = scale_observed(None, 1.0, 20);
  for frame in 21..30 {
    let scale = if frame % 2 == 0 { 1.0 } else { 1.0 + 0.05 * (frame as f32) };
    seen = scale_observed(Some(seen), scale, frame);
    assert!(!seen.at_rest(), "frame {frame}");
  }
}

#[test]
fn a_recording_under_a_zoom_replays_in_motion_and_re_records_at_rest() {
  let mut tree = tree_with_label();
  let platform = crate::tests::text_platform();
  let alloy = headless();
  platform.set_window_size(400.0, 300.0);
  let paint = |tree: &mut RenderTree| {
    apply_content_changes(tree, &platform, &alloy);
    paint_phase(&mut DisplayListBuilder::new(None), tree, &platform, &alloy)
  };
  // The first frame records the zoomer, its label rasterized at the
  // display scale.
  let first = paint(&mut tree);
  assert_eq!((first.boundaries_recorded, first.boundaries_reused), (1, 0));
  assert_eq!(label_scale(&tree), Some(1.0));
  // The zoom, a step per frame: the recording replays stretched, nothing
  // inside is rebuilt, and the boundary waits for the scale to hold.
  for step in [1.03, 1.06, 1.1] {
    zoom(&mut tree, step);
    let moving = paint(&mut tree);
    assert_eq!((moving.boundaries_recorded, moving.boundaries_reused), (0, 1), "at {step}");
    assert_eq!(label_scale(&tree), Some(1.0), "at {step}");
  }
  // The frames after the last write: the wait brings a look each, the
  // scale holds, and on the third look the recording re-records and the
  // label rasters at the resting scale.
  for look in 1..3 {
    let holding = paint(&mut tree);
    assert_eq!((holding.boundaries_recorded, holding.boundaries_reused), (0, 1), "look {look}");
    assert_eq!(label_scale(&tree), Some(1.0), "look {look}");
  }
  let settled = paint(&mut tree);
  assert_eq!((settled.boundaries_recorded, settled.boundaries_reused), (1, 0));
  assert_eq!(label_scale(&tree), Some(1.1));
  // At rest nothing waits: the recording replays from here.
  let idle = paint(&mut tree);
  assert_eq!((idle.boundaries_recorded, idle.boundaries_reused), (0, 1));
  // A press that returns to the recorded scale never re-records: the
  // drift at rest is back within the tolerance.
  zoom(&mut tree, 1.07);
  paint(&mut tree);
  zoom(&mut tree, 1.1);
  for _ in 0..4 {
    let back = paint(&mut tree);
    assert_eq!((back.boundaries_recorded, back.boundaries_reused), (0, 1));
  }
  assert_eq!(label_scale(&tree), Some(1.1));
}
