// The rounded fill split (renderer/walk.rs `split_rounded`): a large
// rounded rect fill, or a plain rect fill under a rounded clip, cut into
// plain pieces the fill program draws and a rim the rounded program draws,
// pure and through the walk, with no GL context.

use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{
  split_rounded, walk, Batch, BatchProgram, GradientClass, Instance, Walked, KIND_RECT, KIND_ROUNDED_RECT,
  ROUNDED_QUAD_PAD, ROUNDED_SPLIT_AREA, ROUNDED_SPLIT_MARGIN, TILE_CLAMP,
};
use crate::{ClipOperation, Color, ColorSource, DisplayListBuilder, GradientKind, ISize, Matrix, Paint, Point, Rect};
use crate::{RoundingRadii, Size, TileMode};

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
