// The renderer seam: every recording op of the contract, driven through the
// aliases the way the rendertree drives them, records and builds without a
// GL context. A renderer that implements the contract but refuses an op
// (returns no list, panics in a forwarder) fails here, before a snapshot
// suite has to find it.

use crate::renderer::prelude::*;
use crate::renderer::{
  BlendMode, ClipOperation, Color, ColorFilter, ColorMatrix, ColorSource, DisplayListBuilder, DrawStyle, FillType,
  ImageFilter, MaskFilter, Matrix, Paint, PathBuilder, Point, Rect, RoundingRadii, Size, StrokeCap, StrokeJoin,
  TextureSampling, TileMode,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
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
  b.translate(1.0, 2.0).scale(2.0, 2.0).transform(&Matrix::identity());
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
  b.save_layer(&rect(0.0, 0.0, 20.0, 20.0), Some(&paint), Some(&ImageFilter::gaussian_blur(0.5, 0.5, TileMode::Clamp)));
  b.draw_display_list(&inner, 0.5);
  b.restore();
  b.restore();
  let list = b.build().expect("the list builds");

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
