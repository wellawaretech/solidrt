// Layers with group opacity (renderer/walk.rs `open_layer`, `close_layer`,
// `fold_layer`): a `save_layer` paint's alpha and a nested list below
// opacity 1 walked into a layer between two marker batches, or folded into
// the content's colors where that is exact, with no GL context; and the
// composite's texel rows rescaled to a layer's storage (renderer/layers.rs).

use crate::oplog::Op as LogOp;
use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{
  walk, Batch, BatchProgram, ClipDraw, Instance, Walked, KIND_GLYPH, KIND_RECT, KIND_TEXTURED,
};
use crate::renderer::{report, rescale_composite, test_texture};
use crate::{ClipOperation, Color, CoveragePolicy, DisplayList, DisplayListBuilder, FillType, GlyphQuad, ISize};
use crate::{Matrix, Paint, PathBuilder, Point, Rect, RoundingRadii, Size, TextureSampling};
use euclid::default::{Box2D, Point2D};
use std::sync::Arc;

// How far a walked value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// The target every walk here draws into, in device pixels.
const TARGET: f32 = 100.0;

// The group alpha the layers here fade by.
const HALF: f32 = 0.5;

// A second group alpha, for a layer inside a layer.
const QUARTER: f32 = 0.25;

// The storage a 30-pixel square layer takes: one bucket (renderer/layers.rs
// LAYER_BUCKET).
const STORAGE: i32 = 64;

// The texture the texture quad case samples: its GL name and side.
const TEXTURE_NAME: u32 = 9;
const TEXTURE_SIDE: i64 = 16;

// The rotation a stencil clip is under, in degrees: anything that keeps a
// rect off the axes.
const DEGREES: f32 = 30.0;

// The corner radius of the rounded clip case.
const CLIP_RADIUS: f32 = 8.0;

// The atlas the glyph case samples: its GL name and side.
const ATLAS_NAME: u32 = 7;
const ATLAS_SIDE: i64 = 64;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn paint(alpha: f32) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, alpha));
  paint
}

fn opaque() -> Paint {
  paint(1.0)
}

fn build(b: DisplayListBuilder) -> DisplayList {
  b.build().expect("a builder always builds a list")
}

fn walked(b: DisplayListBuilder) -> Walked {
  walk(&build(b), ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
}

fn approx(got: [f32; 4], want: [f32; 4]) -> bool {
  got.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < EPS)
}

fn ibox(x0: i32, y0: i32, x1: i32, y1: i32) -> Box2D<i32> {
  Box2D::new(Point2D::new(x0, y0), Point2D::new(x1, y1))
}

fn marker(program: BatchProgram, layer: usize, stencil: u8) -> Batch {
  Batch { start: layer, count: 0, program, texture: None, policy: None, gradient: None, paint: None, stencil }
}

fn fills(start: usize, count: usize, stencil: u8) -> Batch {
  Batch { start, count, program: BatchProgram::Fill, texture: None, policy: None, gradient: None, paint: None, stencil }
}

// The programs and stencil depths of a walk's batches, in order.
fn programs(out: &Walked) -> Vec<(BatchProgram, u8)> {
  out.batches.iter().map(|b| (b.program, b.stencil)).collect()
}

// The rotation by DEGREES about the target's center.
fn rotated() -> Matrix {
  let c = TARGET / 2.0;
  Matrix::translation(-c, -c, 0.0)
    .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(DEGREES)))
    .then(&Matrix::translation(c, c, 0.0))
}

// Two overlapping opaque rects: their union is (10, 10) to (40, 40).
fn overlapping(b: &mut DisplayListBuilder) {
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque()).draw_rect(&rect(20.0, 20.0, 20.0, 20.0), &opaque());
}

// u and v of the composite's texel rows at the device point (x, y).
fn texel(i: &Instance, x: f32, y: f32) -> (f32, f32) {
  let u = i.grad_x[0] * x + i.grad_x[1] * y + i.grad_x[2];
  let v = i.grad_y[0] * x + i.grad_y[1] * y + i.grad_y[2];
  (u, v)
}

#[test]
fn a_layer_around_overlapping_rects_draws_offscreen() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  let out = walked(b);
  assert_eq!(
    out.batches,
    [marker(BatchProgram::LayerBegin, 0, 0), fills(0, 2, 0), marker(BatchProgram::LayerEnd, 0, 0)]
  );
  assert_eq!(out.layers.len(), 1);
  let layer = &out.layers[0];
  assert!(!layer.folded);
  assert_eq!(layer.rect, ibox(10, 10, 40, 40));
  assert_eq!(layer.stencil, 0);
  let c = &layer.composite;
  let (ox, oy, w, h) = (10.0, 10.0, 30.0, 30.0);
  assert!(approx(c.rect, [ox, oy, w, h]), "{:?}", c.rect);
  assert!(approx(c.linear, [1.0, 0.0, 0.0, 1.0]), "{:?}", c.linear);
  assert!(approx(c.offset_kind_stroke, [0.0, 0.0, KIND_RECT + KIND_TEXTURED, 0.0]), "{:?}", c.offset_kind_stroke);
  assert!(approx(c.color, [HALF; 4]), "{:?}", c.color);
  assert!(approx(c.grad_x, [1.0 / w, 0.0, -ox / w, 0.0]), "{:?}", c.grad_x);
  assert!(approx(c.grad_y, [0.0, -1.0 / h, 1.0 + oy / h, 0.0]), "{:?}", c.grad_y);
  assert!(approx(c.clip, [0.0, 0.0, TARGET, TARGET]), "{:?}", c.clip);
  assert_eq!(c.rclip_radii, [0.0; 4]);
  // The top-left corner samples the last texel row (v = 1), the bottom-right
  // the first (v = 0): GL counts rows from the bottom.
  assert!(approx(c.tex, [0.0, 1.0, 1.0, 0.0]), "{:?}", c.tex);
  let (u, v) = texel(c, ox, oy);
  assert!(u.abs() < EPS && (v - 1.0).abs() < EPS, "{u} {v}");
  let (u, v) = texel(c, ox + w, oy + h);
  assert!((u - 1.0).abs() < EPS && v.abs() < EPS, "{u} {v}");
  // The content keeps its colors: the composite fades it.
  for i in &out.instances {
    assert!(approx(i.color, [1.0, 0.0, 0.0, 1.0]), "{:?}", i.color);
  }
  let r = report(&out);
  assert_eq!((r.layers, r.draw_calls), (1, 2), "the fill batch and the composite");
}

#[test]
fn a_layer_over_disjoint_rects_folds_its_alpha() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(out.batches, [fills(0, 1, 0)]);
  assert!(out.layers[0].folded);
  assert!(approx(out.instances[0].color, [HALF, 0.0, 0.0, HALF]), "{:?}", out.instances[0].color);
  let r = report(&out);
  assert_eq!((r.layers, r.draw_calls), (0, 1));

  // Two disjoint rects, and two that only share an edge, fold too.
  for second in [rect(50.0, 50.0, 10.0, 10.0), rect(30.0, 10.0, 10.0, 20.0)] {
    let mut b = DisplayListBuilder::new(None);
    b.save_layer(&Rect::zero(), Some(&paint(HALF)));
    b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque()).draw_rect(&second, &opaque());
    b.restore();
    let out = walked(b);
    assert_eq!(out.batches, [fills(0, 2, 0)], "{second:?}");
    assert!(out.layers[0].folded);
    for i in &out.instances {
      assert!(approx(i.color, [HALF, 0.0, 0.0, HALF]), "{:?}", i.color);
    }
  }
}

#[test]
fn a_glyph_run_folds_into_its_straight_alpha() {
  let atlas = test_texture(ATLAS_NAME, ISize::new(ATLAS_SIDE, ATLAS_SIDE));
  let quads: Arc<[GlyphQuad]> = vec![
    GlyphQuad { dst: [10.0, 10.0, 8.0, 10.0], src: [0.0, 0.0, 8.0, 10.0] },
    GlyphQuad { dst: [20.0, 10.0, 8.0, 10.0], src: [8.0, 0.0, 8.0, 10.0] },
  ]
  .into();
  let white = [1.0, 1.0, 1.0, 1.0];
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_glyphs(&atlas, &quads, white, CoveragePolicy::default(), None);
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [(BatchProgram::Glyph, 0)]);
  assert!(out.layers[0].folded);
  assert_eq!(out.instances.len(), 2);
  for i in &out.instances {
    assert_eq!(i.offset_kind_stroke[2], KIND_GLYPH);
    assert!(approx(i.color, [1.0, 1.0, 1.0, HALF]), "{:?}", i.color);
  }
}

#[test]
fn a_layer_under_a_stencil_clip_composites_at_its_depth() {
  // Overlapping content: a layer, its composite at the clip's depth, its
  // content at 0, and no stencil reset for the layer's own stencil.
  let mut b = DisplayListBuilder::new(None);
  b.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 40.0, 40.0), ClipOperation::Intersect);
  b.transform(&rotated().inverse().expect("a rotation inverts"));
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(40.0, 40.0, 10.0, 10.0), &opaque()).draw_rect(&rect(45.0, 45.0, 10.0, 10.0), &opaque());
  b.restore();
  b.restore();
  let out = walked(b);
  assert_eq!(
    programs(&out),
    [
      (BatchProgram::Clip, 0),
      (BatchProgram::LayerBegin, 0),
      (BatchProgram::Fill, 0),
      (BatchProgram::LayerEnd, 1),
      // The outer restore resets the outer clip's raise.
      (BatchProgram::Clip, 0),
    ]
  );
  assert_eq!(out.layers[0].stencil, 1);
  assert_eq!(out.clip_draws.len(), 2);

  // A stencil clip inside the layer raises the layer's stencil from 0, and
  // the layer's restore leaves it: no reset clip draw for it.
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 40.0, 40.0), ClipOperation::Intersect);
  b.transform(&rotated().inverse().expect("a rotation inverts"));
  b.draw_rect(&rect(40.0, 40.0, 10.0, 10.0), &opaque());
  b.restore();
  b.draw_rect(&rect(10.0, 10.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  // The list ends with the layer open: it closes at the end of the walk.
  assert_eq!(
    programs(&out),
    [
      (BatchProgram::LayerBegin, 0),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 1),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 0),
      (BatchProgram::LayerEnd, 0),
    ]
  );

  // One rect folds, its batch at the clip's depth.
  let mut b = DisplayListBuilder::new(None);
  b.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 40.0, 40.0), ClipOperation::Intersect);
  b.transform(&rotated().inverse().expect("a rotation inverts"));
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(40.0, 40.0, 10.0, 10.0), &opaque());
  b.restore();
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [(BatchProgram::Clip, 0), (BatchProgram::Fill, 1), (BatchProgram::Clip, 0)]);
  assert!(out.layers[0].folded);
  assert!(approx(out.instances[0].color, [HALF, 0.0, 0.0, HALF]), "{:?}", out.instances[0].color);
}

#[test]
fn a_layer_under_a_rounded_clip_does_not_fold() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(
    &rect(10.0, 10.0, 50.0, 50.0),
    &RoundingRadii::circular([CLIP_RADIUS; 4]),
    ClipOperation::Intersect,
  );
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(20.0, 20.0, 10.0, 10.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [(BatchProgram::LayerBegin, 0), (BatchProgram::Fill, 0), (BatchProgram::LayerEnd, 0)]);
  let layer = &out.layers[0];
  assert!(!layer.folded);
  assert_eq!(layer.rect, ibox(20, 20, 30, 30));
  // The rounded clip applies at the composite, not inside.
  assert_eq!(layer.composite.rclip_radii, [CLIP_RADIUS; 4]);
  assert!(approx(layer.composite.rclip, [10.0, 10.0, 60.0, 60.0]), "{:?}", layer.composite.rclip);
  assert!(approx(layer.composite.clip, [10.0, 10.0, 60.0, 60.0]), "{:?}", layer.composite.clip);
  assert_eq!(out.instances[0].rclip_radii, [0.0; 4]);
}

#[test]
fn a_layer_whose_content_is_clipped_away_is_empty() {
  // A stencil clip inside keeps the layer from folding; the rect lies
  // outside it.
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Intersect);
  b.transform(&rotated().inverse().expect("a rotation inverts"));
  b.draw_rect(&rect(80.0, 80.0, 10.0, 10.0), &opaque());
  b.restore();
  b.restore();
  let out = walked(b);
  assert_eq!(
    programs(&out),
    [(BatchProgram::LayerBegin, 0), (BatchProgram::Clip, 0), (BatchProgram::Clip, 0), (BatchProgram::LayerEnd, 0)]
  );
  assert!(out.instances.is_empty());
  let layer = &out.layers[0];
  assert!(!layer.folded);
  assert!(layer.rect.is_empty(), "{:?}", layer.rect);
  let r = report(&out);
  assert_eq!((r.layers, r.draw_calls), (0, 0), "the executor skips an empty layer whole");
  assert_eq!(out.scissor, None);
}

#[test]
fn a_nested_list_below_opacity_1_is_a_layer() {
  let mut inner = DisplayListBuilder::new(None);
  overlapping(&mut inner);
  let inner = build(inner);
  let mut b = DisplayListBuilder::new(None);
  b.draw_display_list(&inner, HALF);
  let nested = walked(b);

  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  let layered = walked(b);
  assert_eq!(nested.batches, layered.batches);
  assert_eq!(nested.layers, layered.layers);
  assert_eq!(nested.instances, layered.instances);

  // Faded to nothing, it draws nothing.
  let mut b = DisplayListBuilder::new(None);
  b.draw_display_list(&inner, 0.0);
  let none = walked(b);
  assert!(none.instances.is_empty() && none.batches.is_empty() && none.layers.is_empty());
}

#[test]
fn a_layer_with_no_effect_is_a_plain_save() {
  for paint in [Some(opaque()), None] {
    let mut b = DisplayListBuilder::new(None);
    b.save_layer(&Rect::zero(), paint.as_ref());
    overlapping(&mut b);
    b.restore();
    let list = build(b);
    assert!(matches!(list.ops()[0], LogOp::SaveLayer { .. }), "the log keeps the layer");
    let out = walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new());
    assert!(out.layers.is_empty());
    assert_eq!(out.batches, [fills(0, 2, 0)]);
  }
}

#[test]
fn a_layer_inside_a_layer_bounds_the_outer_one() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  b.draw_rect(&rect(60.0, 60.0, 10.0, 10.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(
    out.batches,
    [
      marker(BatchProgram::LayerBegin, 0, 0),
      marker(BatchProgram::LayerBegin, 1, 0),
      fills(0, 2, 0),
      marker(BatchProgram::LayerEnd, 1, 0),
      fills(2, 1, 0),
      marker(BatchProgram::LayerEnd, 0, 0),
    ]
  );
  assert_eq!(out.layers[1].rect, ibox(10, 10, 40, 40));
  // The inner composite is the outer layer's content: its rect counts.
  assert_eq!(out.layers[0].rect, ibox(10, 10, 70, 70));
  assert!(!out.layers[0].folded && !out.layers[1].folded);
  let r = report(&out);
  assert_eq!((r.layers, r.draw_calls), (2, 4), "two fill batches and two composites");
}

#[test]
fn a_path_inside_a_layer_blocks_the_fold_and_bounds_the_layer() {
  let mut pb = PathBuilder::default();
  pb.move_to(Point::new(0.0, 0.0)).line_to(Point::new(20.0, 0.0)).line_to(Point::new(0.0, 30.0)).close();
  let path = pb.build(FillType::NonZero);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 80.0, 30.0), ClipOperation::Intersect);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.translate(70.0, 10.0).draw_path(&path, &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [(BatchProgram::LayerBegin, 0), (BatchProgram::Mesh, 0), (BatchProgram::LayerEnd, 0)]);
  // The mesh's bounds (70, 10) to (90, 40), under the translate, cut to the
  // clip (80, 30).
  assert_eq!(out.layers[0].rect, ibox(70, 10, 80, 30));
  assert!(!out.layers[0].folded);
}

#[test]
fn no_instance_joins_across_a_layer_marker() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  b.draw_rect(&rect(80.0, 80.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert_eq!(
    out.batches,
    [
      fills(0, 1, 0),
      marker(BatchProgram::LayerBegin, 0, 0),
      fills(1, 2, 0),
      marker(BatchProgram::LayerEnd, 0, 0),
      fills(3, 1, 0),
    ]
  );

  // A fill after a glyph run after the layer: the batch before the run is
  // the end marker, which takes no fill, though the fill meets none of the
  // run's glyphs.
  let atlas = test_texture(ATLAS_NAME, ISize::new(ATLAS_SIDE, ATLAS_SIDE));
  let quads: Arc<[GlyphQuad]> = vec![GlyphQuad { dst: [50.0, 50.0, 8.0, 10.0], src: [0.0, 0.0, 8.0, 10.0] }].into();
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  b.draw_glyphs(&atlas, &quads, [1.0; 4], CoveragePolicy::default(), None);
  b.draw_rect(&rect(80.0, 80.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert_eq!(
    programs(&out),
    [
      (BatchProgram::LayerBegin, 0),
      (BatchProgram::Fill, 0),
      (BatchProgram::LayerEnd, 0),
      (BatchProgram::Glyph, 0),
      (BatchProgram::Fill, 0),
    ]
  );
  assert_eq!(out.batches[1], fills(0, 2, 0));
}

#[test]
fn the_composite_rows_rescale_to_the_storage() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  let out = walked(b);
  let layer = &out.layers[0];
  let content = (layer.rect.width(), layer.rect.height());
  let scaled = rescale_composite(&layer.composite, content, (content.0 * 2, content.1 * 2));
  let (ox, oy) = (layer.rect.min.x as f32, layer.rect.min.y as f32);
  let (x1, y1) = (layer.rect.max.x as f32, layer.rect.max.y as f32);
  let (u, v) = texel(&scaled, ox, oy);
  assert!(u.abs() < EPS && (v - HALF).abs() < EPS, "{u} {v}");
  let (u, v) = texel(&scaled, x1, y1);
  assert!((u - HALF).abs() < EPS && v.abs() < EPS, "{u} {v}");
  assert!(approx(scaled.tex, [0.0, HALF, HALF, 0.0]), "{:?}", scaled.tex);
  // Nothing else of the instance changes.
  assert_eq!(scaled.rect, layer.composite.rect);
  assert_eq!(scaled.color, layer.composite.color);
  assert_eq!(scaled.clip, layer.composite.clip);
}

#[test]
fn a_folded_layer_inside_a_layer_scales_by_its_own_alpha_once() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.save_layer(&Rect::zero(), Some(&paint(QUARTER)));
  b.draw_rect(&rect(60.0, 60.0, 10.0, 10.0), &opaque());
  b.restore();
  overlapping(&mut b);
  b.restore();
  let out = walked(b);
  assert_eq!(
    out.batches,
    [marker(BatchProgram::LayerBegin, 0, 0), fills(0, 3, 0), marker(BatchProgram::LayerEnd, 0, 0)]
  );
  assert!(out.layers[1].folded && !out.layers[0].folded);
  // The inner alpha once; the outer one is the composite's alone.
  assert!(approx(out.instances[0].color, [QUARTER, 0.0, 0.0, QUARTER]), "{:?}", out.instances[0].color);
  for i in &out.instances[1..] {
    assert!(approx(i.color, [1.0, 0.0, 0.0, 1.0]), "{:?}", i.color);
  }
  assert!(approx(out.layers[0].composite.color, [HALF; 4]), "{:?}", out.layers[0].composite.color);
  assert_eq!(out.layers[0].rect, ibox(10, 10, 70, 70));
}

#[test]
fn a_nested_list_left_open_closes_its_layer() {
  // The nested list leaves a save and a stencil clip open.
  let mut inner = DisplayListBuilder::new(None);
  inner.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 40.0, 40.0), ClipOperation::Intersect);
  inner.transform(&rotated().inverse().expect("a rotation inverts"));
  inner.draw_rect(&rect(45.0, 45.0, 5.0, 5.0), &opaque());
  let inner = build(inner);
  let mut b = DisplayListBuilder::new(None);
  b.save().transform(&rotated()).clip_rect(&rect(30.0, 30.0, 40.0, 40.0), ClipOperation::Intersect);
  b.transform(&rotated().inverse().expect("a rotation inverts"));
  b.draw_display_list(&inner, HALF);
  b.restore();
  let out = walked(b);
  assert_eq!(
    programs(&out),
    [
      (BatchProgram::Clip, 0),
      (BatchProgram::LayerBegin, 0),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 1),
      (BatchProgram::LayerEnd, 1),
      (BatchProgram::Clip, 0),
    ]
  );
  // Two raises and the outer restore's reset: none for the layer's own.
  let resets = out.clip_draws.iter().filter(|d| matches!(d, ClipDraw::Restore { .. })).count();
  assert_eq!((out.clip_draws.len(), resets), (3, 1));
}

#[test]
fn a_layer_under_a_clip_rect_scissors_to_the_clip() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(10.5, 10.5, 30.0, 30.0), ClipOperation::Intersect);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  let out = walked(b);
  assert!(!out.layers[0].folded);
  assert_eq!(out.scissor, Some(ibox(10, 10, 41, 41)));
}

#[test]
fn the_composite_rows_sample_texel_centres_in_a_bucket() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  overlapping(&mut b);
  b.restore();
  let out = walked(b);
  let layer = &out.layers[0];
  let content = (layer.rect.width(), layer.rect.height());
  assert_eq!(content, (30, 30));
  let scaled = rescale_composite(&layer.composite, content, (STORAGE, STORAGE));
  let (ox, oy) = (layer.rect.min.x as f32, layer.rect.min.y as f32);
  let side = STORAGE as f32;
  let (u, v) = texel(&scaled, ox + 0.5, oy + 0.5);
  assert!((u - 0.5 / side).abs() < EPS && (v - 29.5 / side).abs() < EPS, "{u} {v}");
  let (u, v) = texel(&scaled, ox + 29.5, oy + 29.5);
  assert!((u - 29.5 / side).abs() < EPS && (v - 0.5 / side).abs() < EPS, "{u} {v}");
}

#[test]
fn a_texture_quad_folds_in_all_four_channels() {
  let texture = test_texture(TEXTURE_NAME, ISize::new(TEXTURE_SIDE, TEXTURE_SIDE));
  let side = TEXTURE_SIDE as f32;
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_texture_rect(
    &texture,
    &rect(0.0, 0.0, side, side),
    &rect(10.0, 10.0, 20.0, 20.0),
    TextureSampling::Linear,
    None,
  );
  b.restore();
  let out = walked(b);
  assert!(out.layers[0].folded);
  assert_eq!(programs(&out), [(BatchProgram::Shape, 0)]);
  assert!(approx(out.instances[0].color, [HALF; 4]), "{:?}", out.instances[0].color);
}

#[test]
fn a_layer_at_alpha_0_draws_nothing() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(0.0)));
  overlapping(&mut b);
  b.restore();
  b.draw_rect(&rect(60.0, 60.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert!(out.layers.is_empty());
  assert_eq!(out.batches, [fills(0, 1, 0)]);
  assert_eq!(out.instances.len(), 1);
  // The restore popped the culling clip.
  assert!(approx(out.instances[0].clip, [0.0, 0.0, TARGET, TARGET]), "{:?}", out.instances[0].clip);
}

#[test]
fn a_folded_layer_joins_the_batch_before_it() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(50.0, 50.0, 5.0, 5.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(out.batches, [fills(0, 2, 0)]);
  assert!(approx(out.instances[0].color, [1.0, 0.0, 0.0, 1.0]), "{:?}", out.instances[0].color);
  assert!(approx(out.instances[1].color, [HALF, 0.0, 0.0, HALF]), "{:?}", out.instances[1].color);
}
