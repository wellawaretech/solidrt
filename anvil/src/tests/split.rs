// The rounded fill split (renderer/walk.rs `split_rounded`): a large
// rounded rect fill, or a plain rect fill under a rounded clip, cut into
// plain pieces the fill program draws and a rim the rounded program draws,
// pure and through the walk, with no GL context.

use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{
  split_rounded, walk, Batch, BatchProgram, GradientClass, Instance, Walked, JOIN_SCAN_LIMIT, KIND_RECT,
  KIND_ROUNDED_RECT, ROUNDED_QUAD_PAD, ROUNDED_SPLIT_AREA, ROUNDED_SPLIT_MARGIN, TILE_CLAMP,
};
use crate::{ClipOperation, Color, ColorSource, DisplayListBuilder, GradientKind, ISize, Matrix, Paint, Point, Rect};
use crate::{RoundingRadii, Size, TileMode};
use euclid::default::Box2D;

// How far a walked value may sit from the expected one: float noise.
const EPS: f32 = 1e-3;

// The target every walk here draws into, in device pixels: large enough
// for the split's panels.
const TARGET: f32 = 1000.0;

// The margin of a split at scale 1, in local units.
const MARGIN: f32 = ROUNDED_QUAD_PAD + ROUNDED_SPLIT_MARGIN;

// The panel the tests split: 400 x 200 at radius 16.
const PANEL_W: f32 = 400.0;
const PANEL_H: f32 = 200.0;
const PANEL_RADIUS: f32 = 16.0;

// A large rounded rect fill's pieces: the corners and the four edge bands
// (eight rim pieces, merged along each row), the inner rect's three rows
// (three plain ones).
const PANEL_RIM: usize = 8;
const PANEL_PLAIN: usize = 3;

// The space between two panels side by side.
const PANEL_GAP: f32 = 20.0;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn opaque() -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
  paint
}

fn walked(b: DisplayListBuilder) -> Walked {
  walk(
    &b.build().expect("a builder always builds a list"),
    ISize::new(TARGET as i64, TARGET as i64),
    &mut RampTable::new(),
  )
}

fn panel() -> Rect {
  rect(0.0, 0.0, PANEL_W, PANEL_H)
}

fn padded(r: Rect) -> Rect {
  r.inflate(ROUNDED_QUAD_PAD, ROUNDED_QUAD_PAD)
}

fn area(r: &Rect) -> f32 {
  r.size.width * r.size.height
}

// The area two rects share.
fn overlap(a: &Rect, b: &Rect) -> f32 {
  a.intersection(b).map_or(0.0, |r| area(&r))
}

// The pieces neither overlap nor leave a gap in `covered`, the area they
// should tile.
fn assert_tiles(pieces: &[(Rect, bool)], covered: f32) {
  let total: f32 = pieces.iter().map(|(r, _)| area(r)).sum();
  assert!((total - covered).abs() < EPS * covered, "{total} against {covered}");
  for (i, (a, _)) in pieces.iter().enumerate() {
    for (b, _) in &pieces[i + 1..] {
      assert!(overlap(a, b) < EPS, "{a:?} overlaps {b:?}");
    }
  }
}

fn counts(pieces: &[(Rect, bool)]) -> (usize, usize) {
  let plain = pieces.iter().filter(|(_, plain)| *plain).count();
  (pieces.len() - plain, plain)
}

fn quad(i: &Instance) -> Rect {
  rect(i.rect[0], i.rect[1], i.rect[2], i.rect[3])
}

fn batch(start: usize, count: usize, program: BatchProgram, gradient: Option<GradientClass>) -> Batch {
  Batch { start, count, program, texture: None, policy: None, gradient, paint: None, stencil: 0 }
}

#[test]
fn a_large_rounded_rect_splits_into_a_rim_and_plain_pieces() {
  let q = panel();
  let r = PANEL_RADIUS;
  let fill = padded(q);
  let pieces = split_rounded(&fill, &[(q, [r; 4])], MARGIN);
  assert_eq!(counts(&pieces), (PANEL_RIM, PANEL_PLAIN));
  // Nothing of a rounded rect's own quad is dropped.
  assert_tiles(&pieces, area(&fill));
  let reach = MARGIN + r;
  for (piece, plain) in &pieces {
    if *plain {
      // At least the margin inside every edge, and outside every corner
      // square.
      assert!(q.inflate(-MARGIN, -MARGIN).contains_rect(piece), "{piece:?}");
      let corners = [
        rect(q.min_x(), q.min_y(), reach, reach),
        rect(q.max_x() - reach, q.min_y(), reach, reach),
        rect(q.max_x() - reach, q.max_y() - reach, reach, reach),
        rect(q.min_x(), q.max_y() - reach, reach, reach),
      ];
      for corner in corners {
        assert!(overlap(piece, &corner) < EPS, "{piece:?} in the corner {corner:?}");
      }
    } else {
      // Within the margin and the radius of an edge.
      let near = piece.max_x() <= q.min_x() + reach
        || piece.min_x() >= q.max_x() - reach
        || piece.max_y() <= q.min_y() + reach
        || piece.min_y() >= q.max_y() - reach;
      assert!(near, "{piece:?}");
    }
  }
}

#[test]
fn a_small_rounded_rect_does_not_split() {
  let small = rect(10.0, 10.0, 40.0, 40.0);
  assert!(area(&padded(small)) < ROUNDED_SPLIT_AREA);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&small, &RoundingRadii::circular([8.0; 4]), &opaque());
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  // The whole quad in `tex`, the rounded program's shape.
  assert_eq!(i.tex, i.rect);
}

#[test]
fn a_rect_well_inside_a_rounded_clip_is_one_plain_piece() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(0.0, 0.0, 400.0, 400.0), &RoundingRadii::circular([20.0; 4]), ClipOperation::Intersect)
    .draw_rect(&rect(50.0, 50.0, 300.0, 300.0), &opaque());
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_RECT);
  assert_eq!(i.rect, [50.0, 50.0, 300.0, 300.0]);
  // No rounded clip: its coverage under the clip is 1.
  assert_eq!((i.rclip, i.rclip_radii), ([0.0; 4], [0.0; 4]));
  assert_eq!(out.batches, vec![batch(0, 1, BatchProgram::Fill, None)]);
}

#[test]
fn a_rect_across_a_rounded_clip_edge_has_a_rim_along_it() {
  // The clip's bottom edge at y 400 crosses the rect, away from the corners.
  let clip = rect(0.0, 0.0, 400.0, 400.0);
  let drawn = rect(100.0, 300.0, 200.0, 200.0);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&clip, &RoundingRadii::circular([20.0; 4]), ClipOperation::Intersect)
    .draw_rect(&drawn, &opaque());
  let out = walked(b);
  let bottom = clip.max_y();
  let (rim, plain): (Vec<&Instance>, Vec<&Instance>) = out.instances.iter().partition(|i| i.rclip_radii != [0.0; 4]);
  assert!(!rim.is_empty() && !plain.is_empty());
  // The rim is the band of the margin on either side of the edge; the
  // plain pieces lie above it; below the band nothing draws.
  for i in &rim {
    let r = quad(i);
    assert!((r.min_y() - (bottom - MARGIN)).abs() < EPS && (r.max_y() - (bottom + MARGIN)).abs() < EPS, "{r:?}");
    assert_eq!(i.rclip_radii, [20.0; 4]);
  }
  for i in &plain {
    assert!(quad(i).max_y() <= bottom - MARGIN + EPS, "{:?}", i.rect);
    assert_eq!(i.rclip_radii, [0.0; 4]);
  }
  let total: f32 = out.instances.iter().map(|i| area(&quad(i))).sum();
  let kept = drawn.size.width * (bottom + MARGIN - drawn.min_y());
  assert!((total - kept).abs() < EPS * kept, "{total} against {kept}");
}

#[test]
fn a_rounded_rect_under_a_rounded_clip_takes_both_cut_sets() {
  let q = panel();
  let fill = padded(q);
  // The clip's top-left corner lies inside the panel, at (100, 50).
  let clip = rect(100.0, 50.0, 400.0, 400.0);
  let (r, rc) = (PANEL_RADIUS, 20.0);
  let pieces = split_rounded(&fill, &[(q, [r; 4]), (clip, [rc; 4])], MARGIN);
  let xs: Vec<f32> = pieces.iter().flat_map(|(p, _)| [p.min_x(), p.max_x()]).collect();
  let ys: Vec<f32> = pieces.iter().flat_map(|(p, _)| [p.min_y(), p.max_y()]).collect();
  let has = |edges: &[f32], at: f32| edges.iter().any(|e| (e - at).abs() < EPS);
  // The clip's own cuts: its left and top edges less and plus the margin,
  // its top-left corner's; the shape's right and bottom edges', inside the
  // clip.
  for x in [clip.min_x() - MARGIN, clip.min_x() + MARGIN, clip.min_x() + rc + MARGIN] {
    assert!(has(&xs, x), "{x} in {xs:?}");
  }
  for y in [clip.min_y() - MARGIN, clip.min_y() + MARGIN, clip.min_y() + rc + MARGIN] {
    assert!(has(&ys, y), "{y} in {ys:?}");
  }
  for x in [q.max_x() - r - MARGIN, q.max_x() - MARGIN] {
    assert!(has(&xs, x), "{x} in {xs:?}");
  }
  assert!(has(&ys, q.max_y() - r - MARGIN) && has(&ys, q.max_y() - MARGIN), "{ys:?}");
  // Left of and above the clip, less the margin, nothing draws: the rest
  // of the quad is tiled.
  let (x0, y0) = (clip.min_x() - MARGIN, clip.min_y() - MARGIN);
  assert!(pieces.iter().all(|(p, _)| p.min_x() >= x0 - EPS && p.min_y() >= y0 - EPS));
  let kept = rect(x0, y0, fill.max_x() - x0, fill.max_y() - y0);
  assert_tiles(&pieces, area(&kept));
  let (rim, plain) = counts(&pieces);
  assert!(rim > 0 && plain > 0);
}

#[test]
fn a_rotated_rounded_rect_does_not_split() {
  let mut b = DisplayListBuilder::new(None);
  b.transform(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(30.0))).draw_rounded_rect(
    &panel().translate(euclid::vec2(300.0, 100.0)),
    &RoundingRadii::circular([PANEL_RADIUS; 4]),
    &opaque(),
  );
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  assert_eq!(out.instances[0].tex, out.instances[0].rect);
  assert_eq!(out.batches, vec![batch(0, 1, BatchProgram::Rounded, None)]);
}

#[test]
fn a_mirrored_rounded_clip_splits_as_the_unmirrored_one() {
  // One rounded corner, top-left in local space: under a mirrored x it
  // lands top-right in device space and comes back top-left.
  let clip = rect(0.0, 0.0, 400.0, 300.0);
  let radii = RoundingRadii::circular([40.0, 0.0, 0.0, 0.0]);
  let drawn = rect(-50.0, -50.0, 500.0, 400.0);
  let pieces = |mirrored: bool| {
    let mut b = DisplayListBuilder::new(None);
    if mirrored {
      b.translate(500.0, 0.0).scale(-1.0, 1.0);
    }
    b.clip_rounded_rect(&clip, &radii, ClipOperation::Intersect).draw_rect(&drawn, &opaque());
    let out = walked(b);
    let mut pieces: Vec<([f32; 4], bool)> =
      out.instances.iter().map(|i| (i.rect.map(|v| v.round()), i.rclip_radii == [0.0; 4])).collect();
    pieces.sort_by(|a, b| a.partial_cmp(b).expect("finite pieces"));
    pieces
  };
  let plain = pieces(false);
  assert!(plain.iter().any(|(_, plain)| *plain));
  assert_eq!(pieces(true), plain);
}

#[test]
fn a_split_panel_walks_into_a_rounded_and_a_fill_batch() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&panel(), &RoundingRadii::circular([PANEL_RADIUS; 4]), &opaque());
  let out = walked(b);
  assert_eq!(
    out.batches,
    vec![batch(0, PANEL_RIM, BatchProgram::Rounded, None), batch(PANEL_RIM, PANEL_PLAIN, BatchProgram::Fill, None)]
  );

  // A gradient panel: the rim and the plain pieces carry the op's color,
  // clip and rows; the rim its kind, radii and whole padded quad in `tex`.
  let mut graded = opaque();
  graded.set_color_source(&ColorSource::linear_gradient(
    Point::zero(),
    Point::new(PANEL_W, 0.0),
    &[Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  ));
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&panel(), &RoundingRadii::circular([PANEL_RADIUS; 4]), &graded);
  let out = walked(b);
  let class = Some(GradientClass { kind: GradientKind::Linear as i32 as f32, tile: TILE_CLAMP });
  assert_eq!(
    out.batches,
    vec![batch(0, PANEL_RIM, BatchProgram::Rounded, class), batch(PANEL_RIM, PANEL_PLAIN, BatchProgram::Fill, class)]
  );
  let first = out.instances[0];
  let whole = padded(panel());
  assert_eq!(first.tex, [whole.origin.x, whole.origin.y, whole.size.width, whole.size.height]);
  for (n, i) in out.instances.iter().enumerate() {
    assert_eq!(
      (i.color, i.clip, i.grad_x, i.grad_y, i.persp),
      (first.color, first.clip, first.grad_x, first.grad_y, first.persp)
    );
    if n < PANEL_RIM {
      assert_eq!((i.offset_kind_stroke[2], i.radii, i.tex), (KIND_ROUNDED_RECT, [PANEL_RADIUS; 4], first.tex));
    } else {
      assert_eq!((i.offset_kind_stroke[2], i.radii, i.rclip_radii), (KIND_RECT, [0.0; 4], [0.0; 4]));
    }
  }
}

#[test]
fn unequal_radii_cut_at_each_corner() {
  let q = panel();
  let [tl, tr, br, bl] = [16.0, 0.0, 100.0, 40.0];
  let fill = padded(q);
  let pieces = split_rounded(&fill, &[(q, [tl, tr, br, bl])], MARGIN);
  assert_tiles(&pieces, area(&fill));
  // Each corner's own square, its radius and the margin wide.
  let square = |x: f32, y: f32, r: f32| rect(x, y, r + MARGIN, r + MARGIN);
  let corners = [
    square(q.min_x(), q.min_y(), tl),
    square(q.max_x() - tr - MARGIN, q.min_y(), tr),
    square(q.max_x() - br - MARGIN, q.max_y() - br - MARGIN, br),
    square(q.min_x(), q.max_y() - bl - MARGIN, bl),
  ];
  let (rim, plain) = counts(&pieces);
  assert!(rim > 0 && plain > 0);
  for (piece, _) in pieces.iter().filter(|(_, plain)| *plain) {
    assert!(q.inflate(-MARGIN, -MARGIN).contains_rect(piece), "{piece:?}");
    for corner in &corners {
      assert!(overlap(piece, corner) < EPS, "{piece:?} in the corner {corner:?}");
    }
  }
}

// The pieces a full-target rect draws under `map` and a rounded clip of
// `clip` with `radii`, in local space, rounded to whole units, sorted, each
// with whether it is plain.
fn clipped_pieces(map: Option<Matrix>, clip: Rect, radii: [f32; 4]) -> Vec<([f32; 4], bool)> {
  let mut b = DisplayListBuilder::new(None);
  if let Some(m) = map {
    b.transform(&m);
  }
  b.clip_rounded_rect(&clip, &RoundingRadii::circular(radii), ClipOperation::Intersect)
    .draw_rect(&rect(-50.0, -50.0, 500.0, 400.0), &opaque());
  let out = walked(b);
  let mut pieces: Vec<([f32; 4], bool)> =
    out.instances.iter().map(|i| (i.rect.map(|v| v.round()), i.rclip_radii == [0.0; 4])).collect();
  pieces.sort_by(|a, b| a.partial_cmp(b).expect("finite pieces"));
  pieces
}

#[test]
fn a_y_mirrored_rounded_clip_splits_as_the_unmirrored_one() {
  // One rounded corner, top-left in local space: under a mirrored y it
  // lands bottom-left in device space and comes back top-left.
  let clip = rect(0.0, 0.0, 400.0, 300.0);
  let radii = [40.0, 0.0, 0.0, 0.0];
  let mirror = Matrix::scale(1.0, -1.0, 1.0).then(&Matrix::translation(0.0, 400.0, 0.0));
  let unmirrored = clipped_pieces(None, clip, radii);
  assert!(unmirrored.iter().any(|(_, plain)| *plain));
  assert_eq!(clipped_pieces(Some(mirror), clip, radii), unmirrored);
}

#[test]
fn a_non_uniform_draw_scale_divides_the_clip_radii_by_the_smaller_scale() {
  // The clip is made under scale 1 (device radius 40); the draw under 2 x 1
  // reads it back as local radius 40 / 1 on both axes, the larger rim. Its
  // margin is the pad and one device pixel over the smaller scale.
  let clip = rect(0.0, 0.0, 400.0, 300.0);
  let radius = 40.0;
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&clip, &RoundingRadii::circular([radius; 4]), ClipOperation::Intersect)
    .scale(2.0, 1.0)
    .draw_rect(&rect(0.0, 0.0, 300.0, 400.0), &opaque());
  let out = walked(b);
  // Local x: the clip spans 0 to 200, its corner squares radius + margin.
  let reach = radius + MARGIN;
  let plain: Vec<Rect> = out.instances.iter().filter(|i| i.rclip_radii == [0.0; 4]).map(quad).collect();
  assert!(!plain.is_empty());
  for piece in &plain {
    let inside = piece.min_x() >= MARGIN - EPS && piece.max_x() <= clip.max_x() / 2.0 - MARGIN + EPS;
    assert!(inside, "{piece:?}");
    let top_left = rect(0.0, 0.0, reach, reach);
    assert!(overlap(piece, &top_left) < EPS, "{piece:?}");
  }
  // The rim pieces carry the device clip unchanged.
  assert!(out.instances.iter().any(|i| i.rclip_radii == [radius; 4]));
}

#[test]
fn a_rect_that_is_all_rim_stays_one_instance() {
  // A circle as a rounded rect: every cell is rim, so the split saves
  // nothing.
  let circle = rect(100.0, 100.0, 200.0, 200.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&circle, &RoundingRadii::circular([100.0; 4]), &opaque());
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  assert_eq!(out.instances[0].tex, out.instances[0].rect);
  assert_eq!(out.batches, vec![batch(0, 1, BatchProgram::Rounded, None)]);
}

fn panel_radii() -> RoundingRadii {
  RoundingRadii::circular([PANEL_RADIUS; 4])
}

fn background() -> Rect {
  rect(0.0, 0.0, TARGET, TARGET)
}

#[test]
fn two_split_panels_draw_in_two_batches() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&panel(), &panel_radii(), &opaque())
    .save()
    .translate(PANEL_W + PANEL_GAP, 0.0)
    .draw_rounded_rect(&panel(), &panel_radii(), &opaque())
    .restore();
  let out = walked(b);
  assert_eq!(
    out.batches,
    vec![
      batch(0, 2 * PANEL_RIM, BatchProgram::Rounded, None),
      batch(2 * PANEL_RIM, 2 * PANEL_PLAIN, BatchProgram::Fill, None)
    ]
  );
  let (rims, plains) = out.instances.split_at(2 * PANEL_RIM);
  assert!(rims.iter().all(|i| i.offset_kind_stroke[2] == KIND_ROUNDED_RECT));
  assert!(plains.iter().all(|i| i.offset_kind_stroke[2] == KIND_RECT && i.rclip_radii == [0.0; 4]));
}

#[test]
fn a_plain_piece_joins_the_fill_batch_before_the_rims() {
  // The panel's plain pieces share their edges with its rim pieces, which
  // counts as disjoint.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&background(), &opaque()).draw_rounded_rect(&panel(), &panel_radii(), &opaque());
  let out = walked(b);
  assert_eq!(
    out.batches,
    vec![
      batch(0, 1 + PANEL_PLAIN, BatchProgram::Fill, None),
      batch(1 + PANEL_PLAIN, PANEL_RIM, BatchProgram::Rounded, None)
    ]
  );
  assert_eq!(out.instances[0].rect, [0.0, 0.0, TARGET, TARGET]);
}

#[test]
fn an_overlapping_fill_does_not_reorder() {
  // Over the panel's bottom-right corner, one of its rim pieces.
  let corner = rect(PANEL_W - PANEL_GAP, PANEL_H - PANEL_GAP, 2.0 * PANEL_GAP, 2.0 * PANEL_GAP);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&background(), &opaque())
    .draw_rounded_rect(&panel(), &panel_radii(), &opaque())
    .draw_rect(&corner, &opaque());
  let out = walked(b);
  let fills = 1 + PANEL_PLAIN;
  assert_eq!(
    out.batches,
    vec![
      batch(0, fills, BatchProgram::Fill, None),
      batch(fills, PANEL_RIM, BatchProgram::Rounded, None),
      batch(fills + PANEL_RIM, 1, BatchProgram::Fill, None)
    ]
  );
  let last = out.instances.last().expect("the walk drew instances");
  assert_eq!(quad(last), corner);
}

#[test]
fn a_fill_inside_a_stencil_clip_does_not_join_the_batch_before_the_clip() {
  // An oval clip goes through the stencil (depth 1) and keeps the map 2D
  // and axis-aligned, so the panel inside it still splits. Its plain pieces
  // and the rect beside it do not move over the `Clip` batch into the
  // background's batch: they draw in a fill batch of their own at depth 1.
  let inside = panel().translate(euclid::vec2(PANEL_GAP, PANEL_GAP));
  let beside = rect(PANEL_W + 2.0 * PANEL_GAP, PANEL_GAP, PANEL_GAP, PANEL_GAP);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&background(), &opaque())
    .save()
    .clip_oval(&background(), ClipOperation::Intersect)
    .draw_rounded_rect(&inside, &panel_radii(), &opaque())
    .draw_rect(&beside, &opaque())
    .restore();
  let out = walked(b);
  let programs: Vec<(BatchProgram, usize, u8)> = out.batches.iter().map(|b| (b.program, b.count, b.stencil)).collect();
  assert_eq!(
    programs,
    vec![
      (BatchProgram::Fill, 1, 0),
      (BatchProgram::Clip, 1, 0),
      (BatchProgram::Rounded, PANEL_RIM, 1),
      (BatchProgram::Fill, PANEL_PLAIN + 1, 1),
      (BatchProgram::Clip, 1, 0)
    ]
  );
  // The background stays alone in the first batch.
  assert_eq!(out.instances[0].rect, [0.0, 0.0, TARGET, TARGET]);
  assert_eq!(quad(out.instances.last().expect("the walk drew instances")), beside);
}

// A background fill, `n` small rounded rects (each one instance, below
// ROUNDED_SPLIT_AREA) in a grid, then a plain rect away from all of them.
fn rounded_grid(n: usize) -> Walked {
  // The grid's cell and the rounded rect in it, in device pixels.
  let cell = 20.0;
  let side = 10.0;
  let columns = 40;
  assert!(area(&padded(rect(0.0, 0.0, side, side))) < ROUNDED_SPLIT_AREA);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&background(), &opaque());
  for k in 0..n {
    let (x, y) = ((k % columns) as f32 * cell, (k / columns) as f32 * cell);
    b.draw_rounded_rect(&rect(x, y, side, side), &RoundingRadii::circular([2.0; 4]), &opaque());
  }
  let rows = n.div_ceil(columns) as f32;
  assert!(rows * cell < TARGET - cell);
  b.draw_rect(&rect(TARGET - cell, TARGET - cell, side, side), &opaque());
  walked(b)
}

#[test]
fn the_join_scan_is_bounded() {
  // At the limit the plain rect joins the background's batch.
  let out = rounded_grid(JOIN_SCAN_LIMIT);
  assert_eq!(
    out.batches,
    vec![batch(0, 2, BatchProgram::Fill, None), batch(2, JOIN_SCAN_LIMIT, BatchProgram::Rounded, None)]
  );
  // Past it the plain rect opens a fill batch of its own.
  let past = JOIN_SCAN_LIMIT + 1;
  let out = rounded_grid(past);
  assert_eq!(
    out.batches,
    vec![
      batch(0, 1, BatchProgram::Fill, None),
      batch(1, past, BatchProgram::Rounded, None),
      batch(1 + past, 1, BatchProgram::Fill, None)
    ]
  );
}

#[test]
fn pieces_outside_the_target_are_not_pushed() {
  // The panel's lower half lies below the target.
  let shift = euclid::vec2(0.0, TARGET - PANEL_H / 2.0);
  let target = Box2D::from_size(Size::new(TARGET, TARGET));
  let pieces = split_rounded(&padded(panel()), &[(panel(), [PANEL_RADIUS; 4])], MARGIN);
  let kept: Vec<&(Rect, bool)> =
    pieces.iter().filter(|(r, _)| r.translate(shift).to_box2d().intersects(&target)).collect();
  assert!(kept.len() < pieces.len());
  let (rims, plains) =
    (kept.iter().filter(|(_, plain)| !plain).count(), kept.iter().filter(|(_, plain)| *plain).count());
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&background(), &opaque())
    .save()
    .translate(shift.x, shift.y)
    .draw_rounded_rect(&panel(), &panel_radii(), &opaque())
    .restore()
    .draw_rounded_rect(&panel(), &panel_radii(), &opaque());
  let out = walked(b);
  // The count is the culling check: the batches below would read the same
  // with the outside pieces pushed, as their empty boxes meet nothing.
  assert_eq!(out.instances.len(), 1 + kept.len() + PANEL_RIM + PANEL_PLAIN);
  // The second panel still joins: its rims the first one's, its plain
  // pieces the background's.
  let fills = 1 + plains + PANEL_PLAIN;
  assert_eq!(
    out.batches,
    vec![batch(0, fills, BatchProgram::Fill, None), batch(fills, rims + PANEL_RIM, BatchProgram::Rounded, None)]
  );
}

// The panel grid of a TV bench: a 5 x 6 grid of 160 x 90 panels at radius 6,
// 8 apart and 8 in from the edges, under scale 2 in a 1920 x 1080 target, so
// the sixth row is cut by the target's bottom edge.
const GRID_COLUMNS: usize = 5;
const GRID_PANELS: usize = 30;
const GRID_PANEL_W: f32 = 160.0;
const GRID_PANEL_H: f32 = 90.0;
const GRID_RADIUS: f32 = 6.0;
const GRID_GAP: f32 = 8.0;
const GRID_SCALE: f32 = 2.0;
const GRID_TARGET_W: i64 = 1920;
const GRID_TARGET_H: i64 = 1080;
// The alpha of the full-window rect drawn over the grid.
const GRID_VEIL_ALPHA: f32 = 0.5;

#[test]
fn a_gradient_panel_grid_draws_in_four_batches() {
  let full = rect(0.0, 0.0, GRID_TARGET_W as f32, GRID_TARGET_H as f32);
  let mut graded = opaque();
  graded.set_color_source(&ColorSource::linear_gradient(
    Point::zero(),
    Point::new(GRID_PANEL_W, 0.0),
    &[Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  ));
  let mut veil = Paint::default();
  veil.set_color(Color::new_srgba(0.0, 0.0, 0.0, GRID_VEIL_ALPHA));
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&full, &opaque()).save().scale(GRID_SCALE, GRID_SCALE);
  for k in 0..GRID_PANELS {
    let (column, row) = ((k % GRID_COLUMNS) as f32, (k / GRID_COLUMNS) as f32);
    let at = Point::new(GRID_GAP + column * (GRID_PANEL_W + GRID_GAP), GRID_GAP + row * (GRID_PANEL_H + GRID_GAP));
    let card = Rect::new(at, Size::new(GRID_PANEL_W, GRID_PANEL_H));
    b.draw_rounded_rect(&card, &RoundingRadii::circular([GRID_RADIUS; 4]), &graded);
  }
  b.restore().draw_rect(&full, &veil);
  let out = walk(
    &b.build().expect("a builder always builds a list"),
    ISize::new(GRID_TARGET_W, GRID_TARGET_H),
    &mut RampTable::new(),
  );
  let programs: Vec<(BatchProgram, bool)> = out.batches.iter().map(|b| (b.program, b.gradient.is_some())).collect();
  assert_eq!(
    programs,
    vec![
      (BatchProgram::Fill, false),
      (BatchProgram::Rounded, true),
      (BatchProgram::Fill, true),
      (BatchProgram::Fill, false)
    ]
  );
  let (rims, plains) = (&out.batches[1], &out.batches[2]);
  assert_eq!((out.batches[0].count, out.batches[3].count), (1, 1));
  assert_eq!(rims.count + plains.count, out.instances.len() - 2);
  let run = |b: &Batch| &out.instances[b.start..b.start + b.count];
  assert!(run(rims).iter().all(|i| i.offset_kind_stroke[2] == KIND_ROUNDED_RECT));
  assert!(run(plains).iter().all(|i| i.offset_kind_stroke[2] == KIND_RECT && i.rclip_radii == [0.0; 4]));
}
