// Gradients on every quad (gradient.rs, renderer/record.rs, renderer/walk.rs):
// the affine toolkit and the ramp sampler, a paint's gradient source
// resolved at record time, and the walk's device-space rows and sealed
// gradient batches, with no GL context.

use crate::prelude::*;
use crate::renderer::test_texture;
use crate::renderer::walk::{
  walk, Batch, BatchProgram, GradientDraw, Instance, Walked, KIND_LINE, KIND_OVAL, KIND_RECT, KIND_ROUNDED_RECT,
};
use crate::{affine, Affine, Color, ColorSource, CoveragePolicy, GlyphQuad, GradientKind, RampGradient, RAMP_WIDTH};
use crate::{DisplayListBuilder, ISize, Paint, Point, Rect, RoundingRadii, Size, TileMode};
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
  walk(&b.build().expect("a builder always builds a list"), ISize::new(TARGET as i64, TARGET as i64))
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

// The batch's device rows; panics when it has none.
fn rows(batch: &Batch) -> Affine {
  batch.gradient.as_ref().expect("a gradient batch carries its draw").rows
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
fn a_gradient_rect_is_a_sealed_fill_batch() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let solid = Paint::default();
  let graded = sourced(linear());
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &solid).draw_rect(&quad, &graded).draw_rect(&quad, &solid);
  let out = walked(b);
  assert_eq!(out.instances.len(), 3);
  let programs: Vec<_> = out.batches.iter().map(|b| (b.start, b.count, b.program, b.gradient.is_some())).collect();
  assert_eq!(
    programs,
    vec![(0, 1, BatchProgram::Fill, false), (1, 1, BatchProgram::Fill, true), (2, 1, BatchProgram::Fill, false)]
  );
  let i = &out.instances[1];
  assert_eq!(kind(i), KIND_RECT);
  assert_eq!(i.color, [ALPHA; 4]);
  assert_eq!(i.tex, [0.0; 4]);
  assert!(out.batches[1].texture.is_none());
}

#[test]
fn the_rows_compose_with_the_device_map() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 0.0).scale(2.0, 2.0).draw_rect(&rect(0.0, 0.0, 60.0, 10.0), &sourced(linear()));
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  let rows = rows(&out.batches[0]);
  // Local (50, 0) lands on device (110, 0); local (0, 0) on (10, 0).
  assert!(near(eval(&rows, 110.0, 0.0).0, 0.5), "{rows:?}");
  assert!(near(eval(&rows, 10.0, 0.0).0, 0.0), "{rows:?}");
}

#[test]
fn rounded_rects_ovals_and_lines_take_sealed_shape_batches() {
  let quad = rect(0.0, 0.0, 40.0, 40.0);
  let graded = sourced(linear());
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&quad, &RoundingRadii::circular([4.0; 4]), &graded)
    .draw_oval(&quad, &graded)
    .draw_oval(&quad, &Paint::default());
  let out = walked(b);
  let kinds: Vec<_> = out.instances.iter().map(kind).collect();
  assert_eq!(kinds, vec![KIND_ROUNDED_RECT, KIND_OVAL, KIND_OVAL]);
  // Each gradient shape is a batch of its own; the solid oval after them
  // opens a third.
  let programs: Vec<_> = out.batches.iter().map(|b| (b.start, b.count, b.program, b.gradient.is_some())).collect();
  assert_eq!(
    programs,
    vec![(0, 1, BatchProgram::Shape, true), (1, 1, BatchProgram::Shape, true), (2, 1, BatchProgram::Shape, false)]
  );
  assert_eq!(out.instances[0].color, [ALPHA; 4]);
  assert_eq!(out.instances[1].color, [ALPHA; 4]);
}

#[test]
fn a_line_gradient_is_in_the_op_space_not_the_segment_frame() {
  let mut paint = sourced(linear());
  paint.set_stroke_width(8.0);
  // A line along the gradient's axis, and one starting past the origin:
  // either way device (50, 50) is op-space x 50, t 0.5.
  for from in [0.0, 20.0] {
    let mut b = DisplayListBuilder::new(None);
    b.draw_line(Point::new(from, 50.0), Point::new(100.0, 50.0), &paint);
    let out = walked(b);
    assert_eq!(out.instances.len(), 1);
    assert_eq!(kind(&out.instances[0]), KIND_LINE);
    assert_eq!(out.instances[0].color, [ALPHA; 4]);
    assert_eq!(out.batches.len(), 1);
    assert_eq!(out.batches[0].program, BatchProgram::Shape);
    let rows = rows(&out.batches[0]);
    assert!(near(eval(&rows, 50.0, 50.0).0, 0.5), "from x {from}: {rows:?}");
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
  assert!(out.batches[0].gradient.is_none());

  // A device map that collapses x: the rows have no inverse to compose.
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 10.0).scale(0.0, 1.0).draw_rect(&quad, &sourced(linear()));
  let out = walked(b);
  assert_eq!(out.instances.len(), 1);
  assert!(out.instances[0].color.iter().zip(premultiplied.iter()).all(|(a, b)| near(*a, *b)));
  assert!(out.batches[0].gradient.is_none());
}

#[test]
fn a_glyph_gradient_composes_through_the_state_map() {
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
  let quads: Arc<[GlyphQuad]> = Arc::from([GlyphQuad { dst: [30.0, 10.0, 4.0, 6.0], src: [0.0; 4], color: [1.0; 4] }]);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).scale(2.0, 2.0).draw_glyphs(&atlas, &quads, CoveragePolicy::default(), Some(&gradient));
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  let draw: &GradientDraw = out.batches[0].gradient.as_ref().expect("the run's gradient");
  assert!(Arc::ptr_eq(&draw.gradient, &gradient));
  // The quad's corner (30, 10) in the op's space is device (70, 40): the
  // device rows read there what the op's rows read at the corner.
  let op = eval(&[gradient.x_row, gradient.y_row], 30.0, 10.0);
  let device = eval(&draw.rows, 70.0, 40.0);
  assert!(near(op.0, device.0) && near(op.1, device.1), "{op:?} {device:?}");
  assert!(near(device.0, 0.3));
}
