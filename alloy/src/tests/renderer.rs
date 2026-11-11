// The renderer seam: every recording op of the contract, driven through the
// aliases the way the rendertree drives them, records and builds without a
// GL context, and the list reads back what was recorded (renderer::oplog,
// test builds only): every op in order with its arguments, a paint as its
// fields, a path as its segments. A renderer that implements the contract
// but refuses an op (returns no list, panics in a forwarder) or logs it
// wrong fails here, before a snapshot suite has to find it. Then `placed`,
// the helper that applies the saves, restores and transforms.

use crate::renderer::oplog::{placed, Op, PaintLog, PathLog, PathSegment};
use crate::renderer::prelude::*;
use crate::renderer::{
  BlendMode, ClipOperation, Color, ColorFilter, ColorMatrix, ColorSource, DisplayListBuilder, DrawStyle, FillType,
  ImageFilter, MaskFilter, Matrix, Paint, PathBuilder, Point, Rect, RoundingRadii, Size, StrokeCap, StrokeJoin,
  TextureSampling, TileMode,
};
use std::sync::Arc;

// How far a mapped coordinate may sit from the expected one: float noise.
const EPS: f32 = 1e-4;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn close_rect(got: Option<Rect>, want: Rect) -> bool {
  got.is_some_and(|got| {
    let (a, b) = (got.to_box2d(), want.to_box2d());
    [a.min.x - b.min.x, a.min.y - b.min.y, a.max.x - b.max.x, a.max.y - b.max.y].iter().all(|d| d.abs() < EPS)
  })
}

// The fields `full_paint` sets, as the log reads them.
fn full_paint_log() -> PaintLog {
  PaintLog {
    color: Color::new_srgba(0.2, 0.4, 0.6, 0.8),
    draw_style: DrawStyle::StrokeAndFill,
    blend_mode: BlendMode::Multiply,
    stroke_width: 3.0,
    stroke_cap: StrokeCap::Round,
    stroke_join: StrokeJoin::Bevel,
    stroke_miter: 2.0,
    color_source: false,
    color_filter: true,
    image_filter: true,
    mask_filter: true,
  }
}

// The paint a one-rect list drew its rect with.
fn drawn_paint(paint: &Paint) -> PaintLog {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 1.0, 1.0), paint);
  let list = b.build().expect("the list builds");
  *list.ops()[0].paint().expect("a draw has a paint")
}

// A paint with every field set, the filters included.
fn full_paint() -> Paint {
  let mut paint = Paint::default();
  paint
    .set_color(Color::new_srgba(0.2, 0.4, 0.6, 0.8))
    .set_draw_style(DrawStyle::StrokeAndFill)
    .set_blend_mode(BlendMode::Multiply)
    .set_stroke_width(3.0)
    .set_stroke_cap(StrokeCap::Round)
    .set_stroke_join(StrokeJoin::Bevel)
    .set_stroke_miter(2.0)
    .set_color_filter(&ColorFilter::matrix(&ColorMatrix { m: [1.0; 20] }))
    .set_image_filter(&ImageFilter::gaussian_blur(1.5, 1.5, TileMode::Decal))
    .set_mask_filter(&MaskFilter::blur(2.0));
  paint
}

#[test]
fn every_recording_op_builds_a_list() {
  let stops = [Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)];
  let offsets = [0.0, 1.0];
  let linear =
    ColorSource::linear_gradient(Point::zero(), Point::new(10.0, 0.0), &stops, &offsets, TileMode::Clamp, None);
  let radial = ColorSource::radial_gradient(
    Point::new(5.0, 5.0),
    5.0,
    &stops,
    &offsets,
    TileMode::Mirror,
    Some(&Matrix::new_2d(2.0, 0.0, 0.0, 2.0, 1.0, 1.0)),
  );
  let mut gradient_paint = Paint::default();
  gradient_paint.set_color_source(&linear);
  let mut radial_paint = Paint::default();
  radial_paint.set_color_source(&radial);

  let mut pen = PathBuilder::default();
  pen.move_to(Point::zero()).line_to(Point::new(10.0, 0.0)).quadratic_to(Point::new(10.0, 10.0), Point::new(0.0, 10.0));
  pen.cubic_to(Point::new(-5.0, 5.0), Point::new(-5.0, 0.0), Point::zero()).close();
  let path = pen.build(FillType::EvenOdd);

  let paint = full_paint();
  let mut inner = DisplayListBuilder::new(None);
  inner.draw_rect(&rect(0.0, 0.0, 4.0, 4.0), &paint);
  let inner = inner.build().expect("the nested list builds");

  let mut b = DisplayListBuilder::new(Some(&rect(0.0, 0.0, 100.0, 100.0)));
  b.save();
  b.translate(1.0, 2.0).scale(2.0, 3.0).transform(&Matrix::identity());
  b.clip_rect(&rect(0.0, 0.0, 50.0, 50.0), ClipOperation::Intersect);
  b.clip_oval(&rect(40.0, 40.0, 5.0, 5.0), ClipOperation::Difference);
  b.clip_rounded_rect(
    &rect(0.0, 0.0, 50.0, 50.0),
    &RoundingRadii::circular([1.0, 2.0, 3.0, 4.0]),
    ClipOperation::Intersect,
  );
  b.draw_line(Point::zero(), Point::new(10.0, 10.0), &paint);
  b.draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &gradient_paint);
  b.draw_oval(&rect(0.0, 0.0, 10.0, 10.0), &radial_paint);
  b.draw_rounded_rect(&rect(0.0, 0.0, 10.0, 10.0), &RoundingRadii::circular([2.0; 4]), &paint);
  b.draw_path(&path, &paint);
  b.save_layer(&rect(0.0, 0.0, 20.0, 20.0), Some(&paint));
  b.draw_display_list(&inner, 0.5);
  b.restore();
  b.save_backdrop_layer(
    &rect(0.0, 0.0, 20.0, 20.0),
    Some(&paint),
    Some(&ImageFilter::gaussian_blur(0.5, 0.5, TileMode::Clamp)),
  );
  b.restore();
  b.save_backdrop_layer(&rect(0.0, 0.0, 20.0, 20.0), None, None);
  b.restore();
  b.restore();
  let list = b.build().expect("the list builds");

  // What was recorded reads back op by op, in order, with its arguments.
  let full = full_paint_log();
  let source = PaintLog { color_source: true, ..PaintLog::default() };
  let box10 = rect(0.0, 0.0, 10.0, 10.0);
  let layer = rect(0.0, 0.0, 20.0, 20.0);
  let path_log = PathLog {
    segments: vec![
      PathSegment::MoveTo(Point::zero()),
      PathSegment::LineTo(Point::new(10.0, 0.0)),
      PathSegment::QuadraticTo { ctrl: Point::new(10.0, 10.0), to: Point::new(0.0, 10.0) },
      PathSegment::CubicTo { ctrl1: Point::new(-5.0, 5.0), ctrl2: Point::new(-5.0, 0.0), to: Point::zero() },
      PathSegment::Close,
    ],
    fill: FillType::EvenOdd,
  };
  let nested: Arc<[Op]> = vec![Op::DrawRect { rect: rect(0.0, 0.0, 4.0, 4.0), paint: full }].into();
  let expected = [
    Op::Save,
    Op::Translate { x: 1.0, y: 2.0 },
    Op::Scale { x: 2.0, y: 3.0 },
    Op::Transform(Matrix::identity()),
    Op::ClipRect { rect: rect(0.0, 0.0, 50.0, 50.0), op: ClipOperation::Intersect },
    Op::ClipOval { bounds: rect(40.0, 40.0, 5.0, 5.0), op: ClipOperation::Difference },
    Op::ClipRoundedRect {
      rect: rect(0.0, 0.0, 50.0, 50.0),
      radii: RoundingRadii::circular([1.0, 2.0, 3.0, 4.0]),
      op: ClipOperation::Intersect,
    },
    Op::DrawLine { from: Point::zero(), to: Point::new(10.0, 10.0), paint: full },
    Op::DrawRect { rect: box10, paint: source },
    Op::DrawOval { bounds: box10, paint: source },
    Op::DrawRoundedRect { rect: box10, radii: RoundingRadii::circular([2.0; 4]), paint: full },
    Op::DrawPath { path: Arc::new(path_log), paint: full },
    Op::SaveLayer { bounds: layer, paint: Some(full) },
    Op::DrawDisplayList { ops: nested, opacity: 0.5 },
    Op::Restore,
    Op::SaveBackdropLayer { bounds: layer, paint: Some(full), filter: true },
    Op::Restore,
    Op::SaveBackdropLayer { bounds: layer, paint: None, filter: false },
    Op::Restore,
    Op::Restore,
  ];
  let ops = list.ops();
  assert_eq!(ops.len(), expected.len(), "{ops:#?}");
  for (i, (got, want)) in ops.iter().zip(expected.iter()).enumerate() {
    assert_eq!(got, want, "op {i}");
  }
  // The nested list is the inner recording, opened from the op.
  let Op::DrawDisplayList { ops: opened, .. } = &ops[13] else { panic!("op 13 nests the inner list") };
  assert_eq!(&opened[..], inner.ops());

  // The list is shareable: a clone is the same recording, usable from
  // another thread.
  let shared = list.clone();
  std::thread::spawn(move || drop(shared)).join().expect("the clone drops on another thread");
  let _ = TextureSampling::Linear;
}

#[test]
fn circular_radii_follow_css_corner_order() {
  let radii = RoundingRadii::circular([1.0, 2.0, 3.0, 4.0]);
  assert_eq!(radii.top_left, Point::new(1.0, 1.0));
  assert_eq!(radii.top_right, Point::new(2.0, 2.0));
  assert_eq!(radii.bottom_right, Point::new(3.0, 3.0));
  assert_eq!(radii.bottom_left, Point::new(4.0, 4.0));
}

// A paint reads as the default the contract states until a field is set,
// and a field set reads back while the rest stay default.
#[test]
fn a_paint_reads_as_the_contract_default_until_set() {
  let default = PaintLog {
    color: Color::new_srgba(0.0, 0.0, 0.0, 1.0),
    draw_style: DrawStyle::Fill,
    blend_mode: BlendMode::SourceOver,
    stroke_width: 1.0,
    stroke_cap: StrokeCap::Butt,
    stroke_join: StrokeJoin::Miter,
    stroke_miter: 4.0,
    color_source: false,
    color_filter: false,
    image_filter: false,
    mask_filter: false,
  };
  assert_eq!(PaintLog::default(), default);
  assert_eq!(drawn_paint(&Paint::default()), default);

  let mut stroked = Paint::default();
  stroked.set_draw_style(DrawStyle::Stroke).set_stroke_width(2.5);
  assert_eq!(drawn_paint(&stroked), PaintLog { draw_style: DrawStyle::Stroke, stroke_width: 2.5, ..default });
  assert_eq!(drawn_paint(&full_paint()), full_paint_log());
}

// A list recorded under a scale of 1.5, as a frame is recorded under the
// display scale: a translate, a scale and a transform each compose onto the
// transform in effect, applied before it; a restore brings back the state
// of its save; a nested list lands under the transform at the nest point.
#[test]
fn placed_ops_carry_the_transform_they_were_recorded_under() {
  const SCALE: f32 = 1.5;
  // The rotation of the tilted draw: any rotation leaves no device box.
  const TILT_DEG: f32 = 10.0;
  let mut stroke = Paint::default();
  stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(1.0);
  let mut inner = DisplayListBuilder::new(None);
  inner.draw_rect(&rect(0.0, 0.0, 2.0, 2.0), &stroke);
  let inner = inner.build().expect("the nested list builds");

  let mut b = DisplayListBuilder::new(None);
  b.scale(SCALE, SCALE);
  b.save();
  b.translate(10.0, 20.0);
  b.clip_rect(&rect(0.0, 0.0, 4.0, 4.0), ClipOperation::Intersect);
  b.scale(2.0, 2.0);
  b.draw_rect(&rect(1.0, 1.0, 2.0, 2.0), &stroke);
  b.restore();
  b.draw_rect(&rect(0.0, 0.0, 4.0, 4.0), &stroke);
  b.save();
  b.translate(10.0, 0.0);
  b.transform(&Matrix::scale(2.0, 1.0, 1.0));
  b.draw_rect(&rect(1.0, 1.0, 1.0, 1.0), &stroke);
  b.restore();
  b.save();
  b.transform(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(TILT_DEG)));
  b.draw_rect(&rect(0.0, 0.0, 4.0, 4.0), &stroke);
  b.restore();
  b.save();
  b.translate(4.0, 4.0);
  b.draw_display_list(&inner, 1.0);
  b.restore();
  b.restore();
  let list = b.build().expect("the list builds");
  let transformed = placed(list.ops());
  assert_eq!(transformed.len(), 7, "the clip, five draws and the nest: {transformed:#?}");

  // Translated, then the clip: (10, 20) at 1.5.
  assert!(matches!(transformed[0].op, Op::ClipRect { .. }));
  assert!(close_rect(transformed[0].device_rect(), rect(15.0, 30.0, 6.0, 6.0)), "{:?}", transformed[0].device_rect());
  // Scaled by two inside the translate: the stroke scales with the box.
  assert!(close_rect(transformed[1].device_rect(), rect(18.0, 33.0, 6.0, 6.0)), "{:?}", transformed[1].device_rect());
  assert!(transformed[1].device_stroke_width().is_some_and(|w| (w - 2.0 * SCALE).abs() < EPS));
  // After the restore: the state before the save, the outer scale alone.
  assert_eq!(transformed[2].to_device, Matrix::scale(SCALE, SCALE, 1.0));
  assert!(close_rect(transformed[2].device_rect(), rect(0.0, 0.0, 6.0, 6.0)), "{:?}", transformed[2].device_rect());
  assert!(transformed[2].device_stroke_width().is_some_and(|w| (w - SCALE).abs() < EPS));
  // A transform applies before the translate in effect: x is scaled, then
  // moved. A scale on one axis has no single stroke width.
  assert!(close_rect(transformed[3].device_rect(), rect(18.0, 1.5, 3.0, 1.5)), "{:?}", transformed[3].device_rect());
  assert_eq!(transformed[3].device_stroke_width(), None);
  // Under a rotation there is no device box and no device width.
  assert_eq!(transformed[4].device_rect(), None);
  assert_eq!(transformed[4].device_stroke_width(), None);
  // The nest is yielded as itself, then its rect under the translate.
  assert!(matches!(transformed[5].op, Op::DrawDisplayList { .. }));
  assert_eq!(transformed[5].device_rect(), None);
  assert!(close_rect(transformed[6].device_rect(), rect(6.0, 6.0, 3.0, 3.0)), "{:?}", transformed[6].device_rect());

  // A layer saves like a save: the draw after its restore has the transform
  // from before the layer, and the enclosing save still restores after it.
  // A restore with nothing saved changes nothing, and a nested list that
  // leaves a save and a translate open leaves the outer state as it was.
  // Only a draw that strokes has a stroke width: a line always, a shape
  // when it is not filled.
  let fill = Paint::default();
  let mut open = DisplayListBuilder::new(None);
  open.save();
  open.translate(5.0, 5.0);
  open.draw_rect(&rect(0.0, 0.0, 2.0, 2.0), &stroke);
  let open = open.build().expect("the nested list builds");

  let mut b = DisplayListBuilder::new(None);
  b.scale(SCALE, SCALE);
  b.restore();
  b.save();
  b.translate(10.0, 0.0);
  b.save_layer(&rect(0.0, 0.0, 8.0, 8.0), Some(&fill));
  b.translate(0.0, 10.0);
  b.draw_oval(&rect(0.0, 0.0, 2.0, 2.0), &stroke);
  b.restore();
  b.draw_rounded_rect(&rect(0.0, 0.0, 2.0, 2.0), &RoundingRadii::circular([1.0; 4]), &fill);
  b.save_backdrop_layer(&rect(0.0, 0.0, 8.0, 8.0), None, None);
  b.translate(0.0, 10.0);
  b.restore();
  b.draw_line(Point::zero(), Point::new(2.0, 0.0), &fill);
  b.restore();
  b.draw_rect(&rect(0.0, 0.0, 2.0, 2.0), &fill);
  b.draw_display_list(&open, 1.0);
  b.draw_rect(&rect(0.0, 0.0, 2.0, 2.0), &stroke);
  let list = b.build().expect("the list builds");
  let layered = placed(list.ops());
  assert_eq!(layered.len(), 9, "two layers, five draws, the nest and its draw: {layered:#?}");
  let outer = Matrix::scale(SCALE, SCALE, 1.0);
  let shifted = Matrix::translation(10.0, 0.0, 0.0).then(&outer);

  // The layer, under the enclosing translate, has a box and no stroke.
  assert_eq!(layered[0].to_device, shifted);
  assert!(close_rect(layered[0].device_rect(), rect(15.0, 0.0, 12.0, 12.0)), "{:?}", layered[0].device_rect());
  assert_eq!(layered[0].device_stroke_width(), None);
  // Inside the layer, a stroked oval.
  assert!(close_rect(layered[1].device_rect(), rect(15.0, 15.0, 3.0, 3.0)), "{:?}", layered[1].device_rect());
  assert!(layered[1].device_stroke_width().is_some_and(|w| (w - SCALE).abs() < EPS));
  // After the layer's restore: the transform from before the layer. Filled,
  // so no stroke width.
  assert_eq!(layered[2].to_device, shifted);
  assert!(close_rect(layered[2].device_rect(), rect(15.0, 0.0, 3.0, 3.0)), "{:?}", layered[2].device_rect());
  assert_eq!(layered[2].device_stroke_width(), None);
  // The backdrop layer saves the same way.
  assert!(close_rect(layered[3].device_rect(), rect(15.0, 0.0, 12.0, 12.0)), "{:?}", layered[3].device_rect());
  assert_eq!(layered[4].to_device, shifted);
  // A line has no box and strokes with a fill paint too.
  assert_eq!(layered[4].device_rect(), None);
  assert!(layered[4].device_stroke_width().is_some_and(|w| (w - SCALE).abs() < EPS));
  // The enclosing restore: the outer scale alone, the stray restore before
  // the save having changed nothing.
  assert_eq!(layered[5].to_device, outer);
  assert_eq!(layered[5].device_stroke_width(), None);
  // The nest's own save and translate apply inside it, and end with it.
  assert!(matches!(layered[6].op, Op::DrawDisplayList { .. }));
  assert!(close_rect(layered[7].device_rect(), rect(7.5, 7.5, 3.0, 3.0)), "{:?}", layered[7].device_rect());
  assert_eq!(layered[8].to_device, outer);
  assert!(close_rect(layered[8].device_rect(), rect(0.0, 0.0, 3.0, 3.0)), "{:?}", layered[8].device_rect());
}
