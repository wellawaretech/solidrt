// Gradients on every quad (gradient.rs, renderer/record.rs, renderer/walk.rs):
// the affine toolkit and the ramp sampler, a paint's gradient source
// resolved at record time, and the walk's per-instance rows, ramp rows and
// graded batches, with no GL context.

use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::ramps::RAMP_ROWS;
use crate::renderer::test_texture;
use crate::renderer::walk::{
  walk, BatchProgram, Instance, Walked, KIND_LINE, KIND_OVAL, KIND_RECT, KIND_ROUNDED_RECT, TILE_CLAMP, TILE_REPEAT,
};
use crate::{affine, Affine, Color, ColorSource, CoveragePolicy, GlyphQuad, GradientKind, RampGradient, RAMP_WIDTH};
use crate::{DisplayListBuilder, DrawStyle, ISize, Paint, Point, Rect, RoundingRadii, Size, TileMode};
use std::sync::Arc;

// How far a mapped value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// The target every test walks into, in device pixels.
const TARGET: f32 = 200.0;

// The linear gradient's axis length: from (0, 0) to (AXIS, 0).
const AXIS: f32 = 100.0;

// The paint alpha the gradient draws fade by.
const ALPHA: f32 = 0.5;

const RED: Color = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
const BLUE: Color = Color::new_srgba(0.0, 0.0, 1.0, 1.0);

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn walked(b: DisplayListBuilder) -> Walked {
  walk(
    &b.build().expect("a builder always builds a list"),
    ISize::new(TARGET as i64, TARGET as i64),
    &mut RampTable::new(),
  )
}

// A paint at ALPHA whose source is `source`.
fn sourced(source: ColorSource) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.2, 0.4, 0.6, ALPHA)).set_color_source(&source);
  paint
}

// Red to blue along x from 0 to AXIS, clamped.
fn linear() -> ColorSource {
  ColorSource::linear_gradient(Point::zero(), Point::new(AXIS, 0.0), &[RED, BLUE], &[0.0, 1.0], TileMode::Clamp, None)
}

// The parameter-space point `rows` maps (x, y) to.
fn eval(rows: &Affine, x: f32, y: f32) -> (f32, f32) {
  let dot = |r: [f32; 3]| r[0] * x + r[1] * y + r[2];
  (dot(rows[0]), dot(rows[1]))
}

fn near(got: f32, want: f32) -> bool {
  (got - want).abs() < EPS
}

// The rows an instance carries, over its quad's local space.
fn rows(instance: &Instance) -> Affine {
  let ([x0, x1, x2, _], [y0, y1, y2, _]) = (instance.grad_x, instance.grad_y);
  [[x0, x1, x2], [y0, y1, y2]]
}

// The kind and the tile an instance carries, apart: `grad_y`'s last float
// and `persp`'s.
fn kind_tile(instance: &Instance) -> (f32, f32) {
  (instance.grad_y[3], instance.persp[3])
}

// The kind and the tile of a clamped linear gradient, as an instance
// carries them.
fn linear_clamp() -> (f32, f32) {
  (GradientKind::Linear as i32 as f32, TILE_CLAMP)
}

// Each batch's start, count, program and gradedness.
fn programs(out: &Walked) -> Vec<(usize, usize, BatchProgram, bool)> {
  out.batches.iter().map(|b| (b.start, b.count, b.program, b.graded)).collect()
}

// The kind an instance carries.
fn kind(instance: &Instance) -> f32 {
  instance.offset_kind_stroke[2]
}

#[test]
fn invert_round_trips_a_translate_scale_map() {
  let map = affine::compose(affine::translate(10.0, -4.0), affine::scale_by(3.0));
  let inverse = affine::invert(map).expect("a scale of 3 is invertible");
  let there = eval(&map, 7.0, 2.0);
  assert!(near(there.0, 31.0) && near(there.1, 2.0), "{there:?}");
  let back = eval(&inverse, there.0, there.1);
  assert!(near(back.0, 7.0) && near(back.1, 2.0), "{back:?}");
  assert!(affine::invert(affine::scale_by(0.0)).is_none(), "a collapsed map has no inverse");
}

#[test]
fn compose_applies_b_first() {
  // Scale by 2, then translate by 5: (1, 1) -> (2, 2) -> (7, 7).
  let a_after_b = affine::compose(affine::translate(5.0, 5.0), affine::scale_by(2.0));
  let p = eval(&a_after_b, 1.0, 1.0);
  assert!(near(p.0, 7.0) && near(p.1, 7.0), "{p:?}");
  // The other order: (1, 1) -> (6, 6) -> (12, 12).
  let b_after_a = affine::compose(affine::scale_by(2.0), affine::translate(5.0, 5.0));
  let p = eval(&b_after_a, 1.0, 1.0);
  assert!(near(p.0, 12.0) && near(p.1, 12.0), "{p:?}");
}

#[test]
fn the_ramp_samples_its_stops() {
  let ramp = RampGradient::ramp(&[RED, BLUE], &[0.0, 1.0]).expect("two stops");
  assert_eq!(ramp.len(), RAMP_WIDTH);
  assert_eq!(ramp[0], [255, 0, 0, 255]);
  assert_eq!(ramp[RAMP_WIDTH - 1], [0, 0, 255, 255]);
  // The middle texel sits at 128 / 255 along: within one level of the mix.
  let middle = ramp[RAMP_WIDTH / 2];
  let t = (RAMP_WIDTH / 2) as f32 / (RAMP_WIDTH - 1) as f32;
  let want = [(1.0 - t) * 255.0, 0.0, t * 255.0, 255.0];
  for (got, want) in middle.iter().zip(want.iter()) {
    assert!((*got as f32 - want).abs() <= 1.0, "middle {middle:?}, want {want:?}");
  }

  // One stop: every texel that color.
  let one = RampGradient::ramp(&[BLUE], &[0.5]).expect("one stop");
  assert!(one.iter().all(|t| *t == [0, 0, 255, 255]));

  // No stops, or colors without offsets: none.
  assert!(RampGradient::ramp(&[], &[]).is_none());
  assert!(RampGradient::ramp(&[RED], &[]).is_none());
}

#[test]
fn a_linear_gradient_projects_onto_its_axis() {
  let g = RampGradient::linear(
    Point::zero(),
    Point::new(AXIS, 0.0),
    &[RED, BLUE],
    &[0.0, 1.0],
    TileMode::Clamp,
    affine::IDENTITY,
  )
  .expect("a linear gradient");
  assert_eq!(g.kind, GradientKind::Linear);
  let rows = [g.x_row, g.y_row];
  assert!(near(eval(&rows, 50.0, 0.0).0, 0.5));
  assert!(near(eval(&rows, 0.0, 20.0).0, 0.0));
}

#[test]
fn a_radial_gradient_normalizes_by_its_radius() {
  let g =
    RampGradient::radial(Point::new(10.0, 10.0), 20.0, &[RED, BLUE], &[0.0, 1.0], TileMode::Clamp, affine::IDENTITY)
      .expect("a radial gradient");
  assert_eq!(g.kind, GradientKind::Radial);
  let q = eval(&[g.x_row, g.y_row], 30.0, 10.0);
  assert!(near(q.0.hypot(q.1), 1.0), "{q:?}");
  let center = eval(&[g.x_row, g.y_row], 10.0, 10.0);
  assert!(near(center.0.hypot(center.1), 0.0), "{center:?}");
}

#[test]
fn gradient_and_solid_rects_take_separate_fill_batches() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let solid = Paint::default();
  let graded = sourced(linear());
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &solid).draw_rect(&quad, &graded).draw_rect(&quad, &solid);
  let out = walked(b);
  assert_eq!(out.instances.len(), 3);
  assert_eq!(
    programs(&out),
    vec![(0, 1, BatchProgram::Fill, false), (1, 1, BatchProgram::Fill, true), (2, 1, BatchProgram::Fill, false)]
  );
  let i = &out.instances[1];
  assert_eq!(kind(i), KIND_RECT);
  assert_eq!(i.color, [ALPHA; 4]);
  assert_eq!(i.tex, [0.0; 4]);
  assert_eq!(kind_tile(i), linear_clamp());
  assert!(out.batches[1].texture.is_none());
  // The solid ones carry no gradient, and no tile in the projective row.
  assert_eq!((out.instances[0].grad_x, out.instances[0].grad_y), ([0.0; 4], [0.0; 4]));
  assert_eq!(out.instances[0].persp, [0.0, 0.0, 1.0, 0.0]);
}

#[test]
fn consecutive_gradient_rects_share_one_batch() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let mut other = sourced(linear());
  other.set_color_source(&ColorSource::linear_gradient(
    Point::zero(),
    Point::new(AXIS, AXIS),
    &[BLUE, RED],
    &[0.0, 1.0],
    TileMode::Repeat,
    None,
  ));
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &sourced(linear())).draw_rect(&quad, &other).draw_rect(&quad, &sourced(linear()));
  let out = walked(b);
  assert_eq!(programs(&out), vec![(0, 3, BatchProgram::Fill, true)]);
  // Three gradients, three ramp rows: none rewritten under another batch.
  let ramp_rows: Vec<f32> = out.instances.iter().map(|i| i.grad_x[3]).collect();
  assert_eq!(ramp_rows, vec![0.0, 1.0, 2.0]);
  // The kind and the tile ride apart: the repeat tile in the projective
  // row's last float, the kind alone in `grad_y`'s.
  let linear = GradientKind::Linear as i32 as f32;
  assert_eq!(kind_tile(&out.instances[1]), (linear, TILE_REPEAT));
  assert_eq!(kind_tile(&out.instances[0]), (linear, TILE_CLAMP));
}

#[test]
fn one_gradient_drawn_twice_takes_one_ramp_row() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let shared = sourced(linear());
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &shared).draw_rect(&quad, &shared);
  let out = walked(b);
  assert_eq!(out.instances[0].grad_x[3], out.instances[1].grad_x[3]);
}

#[test]
fn the_rows_are_the_ops_own_under_any_map() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 0.0).scale(2.0, 2.0).draw_rect(&rect(0.0, 0.0, 60.0, 10.0), &sourced(linear()));
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  let rows = rows(&out.instances[0]);
  // The quad's local space is the op's: local (50, 0) reads t 0.5, local
  // (0, 0) t 0, whatever the map puts them on.
  assert!(near(eval(&rows, 50.0, 0.0).0, 0.5), "{rows:?}");
  assert!(near(eval(&rows, 0.0, 0.0).0, 0.0), "{rows:?}");
}

#[test]
fn gradient_shapes_share_a_graded_shape_batch() {
  let quad = rect(0.0, 0.0, 40.0, 40.0);
  let graded = sourced(linear());
  // Stroked: a gradient rounded rect fill is a rounded batch of its own.
  let mut graded_stroke = sourced(linear());
  graded_stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&quad, &RoundingRadii::circular([4.0; 4]), &graded_stroke)
    .draw_oval(&quad, &graded)
    .draw_oval(&quad, &Paint::default());
  let out = walked(b);
  let kinds: Vec<_> = out.instances.iter().map(kind).collect();
  assert_eq!(kinds, vec![KIND_ROUNDED_RECT, KIND_OVAL, KIND_OVAL]);
  // The gradient shapes draw in one graded batch; the solid oval after
  // them opens a solid one.
  assert_eq!(programs(&out), vec![(0, 2, BatchProgram::Shape, true), (2, 1, BatchProgram::Shape, false)]);
  assert_eq!(out.instances[0].color, [ALPHA; 4]);
  assert_eq!(out.instances[1].color, [ALPHA; 4]);
}

#[test]
fn a_small_solid_fill_does_not_fold_into_a_graded_shape_batch() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&rect(0.0, 0.0, 40.0, 40.0), &sourced(linear())).draw_rect(&rect(0.0, 0.0, 4.0, 4.0), &Paint::default());
  let out = walked(b);
  assert_eq!(programs(&out), vec![(0, 1, BatchProgram::Shape, true), (1, 1, BatchProgram::Fill, false)]);
  // Nor a gradient fill into a solid shape batch.
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&rect(0.0, 0.0, 40.0, 40.0), &Paint::default()).draw_rect(&rect(0.0, 0.0, 4.0, 4.0), &sourced(linear()));
  let out = walked(b);
  assert_eq!(programs(&out), vec![(0, 1, BatchProgram::Shape, false), (1, 1, BatchProgram::Fill, true)]);
}

#[test]
fn a_line_gradient_is_in_the_op_space_not_the_segment_frame() {
  // A gradient along the op's x from 20 to 80, on a line from (20, 0) to
  // (80, 0): the quad's frame starts at op x 20, so quad-local (30, 0) is
  // op x 50, t 0.5.
  let source = ColorSource::linear_gradient(
    Point::new(20.0, 0.0),
    Point::new(80.0, 0.0),
    &[RED, BLUE],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  let mut paint = sourced(source);
  paint.set_stroke_width(8.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_line(Point::new(20.0, 0.0), Point::new(80.0, 0.0), &paint);
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  // The vertex stage evaluates the instance's rows at the quad's corners
  // and the fragment reads the interpolation: the rows themselves must
  // map quad-local (30, 0) to t 0.5.
  let local = rows(&out.instances[0]);
  assert!(near(eval(&local, 30.0, 0.0).0, 0.5), "{local:?}");
  assert_eq!(kind_tile(&out.instances[0]), linear_clamp());

  let mut paint = sourced(linear());
  paint.set_stroke_width(8.0);
  // A line along the gradient's axis, and one starting past the origin:
  // either way op-space x 50 is t 0.5, quad-local x 50 - from.
  for from in [0.0, 20.0] {
    let mut b = DisplayListBuilder::new(None);
    b.draw_line(Point::new(from, 50.0), Point::new(100.0, 50.0), &paint);
    let out = walked(b);
    assert_eq!(out.instances.len(), 1);
    assert_eq!(kind(&out.instances[0]), KIND_LINE);
    assert_eq!(out.instances[0].color, [ALPHA; 4]);
    assert_eq!(programs(&out), vec![(0, 1, BatchProgram::Shape, true)]);
    let rows = rows(&out.instances[0]);
    assert!(near(eval(&rows, 50.0 - from, 0.0).0, 0.5), "from x {from}: {rows:?}");
  }
}

#[test]
fn a_degenerate_gradient_draws_the_paint_color() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let premultiplied = [0.2 * ALPHA, 0.4 * ALPHA, 0.6 * ALPHA, ALPHA];
  // A zero-length axis has no parameter.
  let flat = ColorSource::linear_gradient(
    Point::new(5.0, 5.0),
    Point::new(5.0, 5.0),
    &[RED, BLUE],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &sourced(flat));
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  assert!(out.instances[0].color.iter().zip(premultiplied.iter()).all(|(a, b)| near(*a, *b)));
  assert!(!out.batches[0].graded);
}

#[test]
fn gradients_past_the_ramp_rows_draw_the_paint_color() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let premultiplied = [0.2 * ALPHA, 0.4 * ALPHA, 0.6 * ALPHA, ALPHA];
  // One distinct gradient more than the atlas has rows, in one frame.
  let mut b = DisplayListBuilder::new(None);
  for _ in 0..=RAMP_ROWS {
    b.draw_rect(&quad, &sourced(linear()));
  }
  let out = walked(b);
  let last = out.instances.last().expect("every rect draws");
  assert_eq!(out.instances.len(), RAMP_ROWS as usize + 1);
  assert!(last.color.iter().zip(premultiplied.iter()).all(|(a, b)| near(*a, *b)));
  assert_eq!(last.grad_y, [0.0; 4]);
  assert_eq!(last.persp[3], 0.0);
  assert_eq!(
    programs(&out),
    vec![(0, RAMP_ROWS as usize, BatchProgram::Fill, true), (RAMP_ROWS as usize, 1, BatchProgram::Fill, false)]
  );
}

#[test]
fn a_glyph_gradient_carries_the_ops_rows() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let gradient = Arc::new(
    RampGradient::linear(
      Point::zero(),
      Point::new(AXIS, 0.0),
      &[RED, BLUE],
      &[0.0, 1.0],
      TileMode::Clamp,
      affine::IDENTITY,
    )
    .expect("a linear gradient"),
  );
  let quads: Arc<[GlyphQuad]> = Arc::from([GlyphQuad { dst: [30.0, 10.0, 4.0, 6.0], src: [0.0; 4] }]);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).scale(2.0, 2.0).draw_glyphs(
    &atlas,
    &quads,
    [1.0; 4],
    CoveragePolicy::default(),
    Some(&gradient),
  );
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  assert_eq!(out.batches[0].program, BatchProgram::Glyph);
  let i = &out.instances[0];
  // The quad is in the op's space: its rows are the gradient's own, and
  // its corner (30, 10) reads t 0.3.
  assert_eq!(rows(i), [gradient.x_row, gradient.y_row]);
  assert_eq!(kind_tile(i), linear_clamp());
  assert!(near(eval(&rows(i), 30.0, 10.0).0, 0.3));
}
