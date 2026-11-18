// Anvil's pure walk (renderer/anvil/walk.rs): a list built through the
// aliases, the way the rendertree builds one, walked into the instances the
// shape program draws, with no GL context. The transform stack, the
// device-space clip, the rejection of draws outside it, nested lists and
// the premultiplied color, the texture quads and their batches are all
// decided here, before any GL work, as are the rounded rects' radii and
// quads, the rounded clip in device space and the image-filled shapes'
// uv. The deferred deletion queue a texture handle drops onto is tested
// here too, with no renderer.

use crate::renderer::anvil::walk::{
  walk, Batch, Instance, Walked, CAP_BUTT, CAP_ROUND, CAP_SQUARE, KIND_LINE, KIND_OVAL, KIND_RECT, KIND_ROUNDED_RECT,
  KIND_TEXTURED, ROUNDED_QUAD_PAD,
};
use crate::renderer::anvil::{test_texture, DeletionQueue};
use crate::renderer::prelude::*;
use crate::renderer::{
  ClipOperation, Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, ISize, Matrix, Paint, Point, Rect,
  RoundingRadii, Size, StrokeCap, Texture, TextureSampling, TileMode,
};
use std::sync::Arc;

// How far a walked value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// The same, for a point mapped through a rotation at a magnitude of tens
// of pixels, where the trigonometry's rounding scales with the coordinate.
const MAPPED_EPS: f32 = 1e-3;

// The target every test walks into, in device pixels.
const TARGET: f32 = 100.0;

// The instances a stroke op draws: none until step 4 drew strokes, one
// from step 4 on.
const STROKE_INSTANCES: usize = 1;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn fill(color: Color) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(color);
  paint
}

fn opaque() -> Paint {
  fill(Color::new_srgba(1.0, 0.0, 0.0, 1.0))
}

fn stroked(width: f32) -> Paint {
  let mut paint = opaque();
  paint.set_draw_style(DrawStyle::Stroke).set_stroke_width(width);
  paint
}

fn approx(got: [f32; 4], want: [f32; 4]) -> bool {
  got.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < EPS)
}

fn build(b: DisplayListBuilder) -> DisplayList {
  b.build().expect("a builder always builds a list")
}

fn walked_all(b: DisplayListBuilder) -> Walked {
  walk(&build(b), ISize::new(TARGET as i64, TARGET as i64))
}

fn walked(b: DisplayListBuilder) -> Vec<Instance> {
  walked_all(b).instances
}

fn name(t: &Texture) -> glow::NativeTexture {
  t.gl_name().expect("an anvil texture always has a GL name")
}

fn offset(instance: &Instance) -> [f32; 2] {
  [instance.offset_kind_stroke[0], instance.offset_kind_stroke[1]]
}

// A local point through the instance's 2D map, by hand: euclid's row-vector
// form, device = local * m.
fn mapped(instance: &Instance, x: f32, y: f32) -> [f32; 2] {
  let [m11, m12, m21, m22] = instance.linear;
  let [tx, ty] = offset(instance);
  [x * m11 + y * m21 + tx, x * m12 + y * m22 + ty]
}

fn rounded(b: &mut DisplayListBuilder, r: Rect, radii: [f32; 4], paint: &Paint) {
  b.draw_rounded_rect(&r, &RoundingRadii::circular(radii), paint);
}

// An opaque paint filled with `t`, sampled `sampling`, clamped, under the
// image transform `transform` (texel to local).
fn image_paint(t: &Texture, sampling: TextureSampling, transform: Option<&Matrix>) -> Paint {
  let mut paint = opaque();
  paint.set_color_source(&ColorSource::image(t, TileMode::Clamp, TileMode::Clamp, sampling, transform));
  paint
}

fn line(from: (f32, f32), to: (f32, f32), paint: &Paint) -> Vec<Instance> {
  let mut b = DisplayListBuilder::new(None);
  b.draw_line(Point::new(from.0, from.1), Point::new(to.0, to.1), paint);
  walked(b)
}

#[test]
fn translate_and_scale_place_the_rect() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).scale(2.0, 3.0).draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rect, [0.0, 0.0, 5.0, 5.0]), "{:?}", i.rect);
  assert!(approx(i.linear, [2.0, 0.0, 0.0, 3.0]), "{:?}", i.linear);
  assert!(approx(i.offset_kind_stroke, [10.0, 20.0, KIND_RECT, 0.0]), "{:?}", i.offset_kind_stroke);
  assert!(approx(i.clip, [0.0, 0.0, TARGET, TARGET]), "{:?}", i.clip);
}

#[test]
fn save_and_restore_scope_the_transform() {
  let mut b = DisplayListBuilder::new(None);
  b.save().translate(50.0, 0.0).restore().draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(offset(&out[0]), [0.0, 0.0]);

  // A restore with nothing saved changes nothing.
  let mut b = DisplayListBuilder::new(None);
  b.translate(7.0, 0.0).restore().draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(offset(&out[0]), [7.0, 0.0]);
}

#[test]
fn clip_intersects_in_device_space() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 10.0)
    .clip_rect(&rect(0.0, 0.0, 20.0, 20.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 100.0, 100.0), &opaque())
    .clip_rect(&rect(15.0, 15.0, 100.0, 100.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 100.0, 100.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 2);
  assert!(approx(out[0].clip, [10.0, 10.0, 30.0, 30.0]), "{:?}", out[0].clip);
  assert!(approx(out[1].clip, [25.0, 25.0, 30.0, 30.0]), "{:?}", out[1].clip);
}

#[test]
fn draws_outside_the_clip_are_rejected() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_rect(&rect(20.0, 20.0, 5.0, 5.0), &opaque());
  assert!(walked(b).is_empty());

  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_rect(&rect(5.0, 5.0, 10.0, 10.0), &opaque());
  assert_eq!(walked(b).len(), 1);

  // The initial clip is the target: a draw entirely off it is rejected.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(-50.0, -50.0, 10.0, 10.0), &opaque());
  assert!(walked(b).is_empty());
}

#[test]
fn the_rendertree_infinite_extent_clips_to_the_target() {
  // composite.rs's single-axis clip: "infinite" on x, bounded on y.
  let inf = 1e7;
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(-inf, 0.0, 2.0 * inf, 50.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].clip, [0.0, 0.0, TARGET, 50.0]), "{:?}", out[0].clip);
}

#[test]
fn a_nested_list_draws_in_place_and_leaves_no_state() {
  // Unbalanced on purpose: a save with no restore.
  let mut inner = DisplayListBuilder::new(None);
  inner.save().translate(5.0, 5.0).draw_rect(&rect(0.0, 0.0, 1.0, 1.0), &opaque());
  let inner = build(inner);
  let mut b = DisplayListBuilder::new(None);
  b.translate(1.0, 1.0).draw_display_list(&inner, 1.0).draw_rect(&rect(0.0, 0.0, 1.0, 1.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 2);
  assert_eq!(offset(&out[0]), [6.0, 6.0]);
  assert_eq!(offset(&out[1]), [1.0, 1.0]);
}

#[test]
fn colors_are_premultiplied() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &fill(Color::new_srgba(1.0, 0.5, 0.0, 0.5)));
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].color, [0.5, 0.25, 0.0, 0.5]), "{:?}", out[0].color);

  // Alpha 0 paints nothing under source-over: no instance.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &fill(Color::TRANSPARENT));
  assert!(walked(b).is_empty());
}

#[test]
fn stroke_and_fill_covers_the_union_once() {
  // The fill outset by half the stroke width, as one instance: the
  // stroke's inner half must not blend over the fill a second time.
  let mut both = opaque();
  both.set_draw_style(DrawStyle::StrokeAndFill).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &both);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].rect, [9.0, 9.0, 22.0, 22.0]), "{:?}", out[0].rect);
  assert_eq!(out[0].offset_kind_stroke[3], 0.0);

  let mut stroke = opaque();
  stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &stroke);
  assert_eq!(walked(b).len(), STROKE_INSTANCES);
}

#[test]
fn a_texture_quad_maps_src_onto_dst() {
  let t = test_texture(7, ISize::new(200, 100));
  let mut b = DisplayListBuilder::new(None);
  b.draw_texture_rect(&t, &rect(50.0, 25.0, 100.0, 50.0), &rect(10.0, 10.0, 40.0, 20.0), TextureSampling::Linear, None);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert!(approx(i.rect, [10.0, 10.0, 40.0, 20.0]), "{:?}", i.rect);
  assert!(approx(i.tex, [0.25, 0.25, 0.75, 0.75]), "{:?}", i.tex);
  assert!(approx(i.color, [1.0; 4]), "{:?}", i.color);
  assert_eq!(i.offset_kind_stroke[2], KIND_RECT + KIND_TEXTURED);
  assert_eq!(i.offset_kind_stroke[3], 0.0);
  assert_eq!(out.batches, vec![Batch { start: 0, count: 1, texture: Some((name(&t), TextureSampling::Linear)) }]);
}

#[test]
fn a_texture_quad_fades_by_the_paint_alpha() {
  let t = test_texture(7, ISize::new(200, 100));
  let mut b = DisplayListBuilder::new(None);
  let half = fill(Color::new_srgba(1.0, 0.0, 0.0, 0.5));
  b.draw_texture_rect(
    &t,
    &rect(0.0, 0.0, 10.0, 10.0),
    &rect(0.0, 0.0, 10.0, 10.0),
    TextureSampling::Linear,
    Some(&half),
  );
  let out = walked(b);
  assert_eq!(out.len(), 1);
  // The paint's RGB is ignored: only its alpha fades the texel.
  assert!(approx(out[0].color, [0.5; 4]), "{:?}", out[0].color);

  let mut b = DisplayListBuilder::new(None);
  let none = fill(Color::new_srgba(1.0, 0.0, 0.0, 0.0));
  b.draw_texture_rect(
    &t,
    &rect(0.0, 0.0, 10.0, 10.0),
    &rect(0.0, 0.0, 10.0, 10.0),
    TextureSampling::Linear,
    Some(&none),
  );
  assert!(walked(b).is_empty());
}

#[test]
fn the_top_of_src_is_texel_row_zero() {
  let t = test_texture(7, ISize::new(100, 100));
  let mut b = DisplayListBuilder::new(None);
  b.draw_texture_rect(&t, &rect(0.0, 30.0, 10.0, 10.0), &rect(0.0, 0.0, 10.0, 10.0), TextureSampling::Linear, None);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  // v grows downward with src.y: no flip.
  assert!((out[0].tex[1] - 0.3).abs() < EPS, "{:?}", out[0].tex);
  assert!((out[0].tex[3] - 0.4).abs() < EPS, "{:?}", out[0].tex);
}

#[test]
fn batches_split_at_texture_changes() {
  let a = test_texture(1, ISize::new(10, 10));
  let c = test_texture(2, ISize::new(10, 10));
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let linear = TextureSampling::Linear;
  // A rect filled with `a` as its image source joins `a`'s run.
  let a_filled = image_paint(&a, linear, None);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque())
    .draw_texture_rect(&a, &quad, &quad, linear, None)
    .draw_texture_rect(&a, &quad, &quad, linear, None)
    .draw_rect(&quad, &a_filled)
    .draw_texture_rect(&c, &quad, &quad, linear, None)
    .draw_rect(&quad, &opaque());
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 6);
  assert_eq!(
    out.batches,
    vec![
      Batch { start: 0, count: 4, texture: Some((name(&a), linear)) },
      Batch { start: 4, count: 2, texture: Some((name(&c), linear)) },
    ]
  );

  // The sampling splits a run too.
  let mut b = DisplayListBuilder::new(None);
  b.draw_texture_rect(&a, &quad, &quad, linear, None).draw_texture_rect(
    &a,
    &quad,
    &quad,
    TextureSampling::Nearest,
    None,
  );
  let out = walked_all(b);
  assert_eq!(
    out.batches,
    vec![
      Batch { start: 0, count: 1, texture: Some((name(&a), linear)) },
      Batch { start: 1, count: 1, texture: Some((name(&a), TextureSampling::Nearest)) },
    ]
  );
}

#[test]
fn a_dropped_handle_queues_its_name_once() {
  let queue = Arc::new(DeletionQueue::new());
  let a = Texture::over(queue.clone(), 5, ISize::new(1, 1));
  let five = name(&a);
  let b = a.clone();
  drop(a);
  assert!(queue.drain().is_empty());
  drop(b);
  assert_eq!(queue.drain(), vec![five]);
  assert!(queue.drain().is_empty());
}

#[test]
fn texture_quads_are_rejected_by_the_clip() {
  let t = test_texture(7, ISize::new(10, 10));
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_texture_rect(
    &t,
    &rect(0.0, 0.0, 5.0, 5.0),
    &rect(20.0, 20.0, 5.0, 5.0),
    TextureSampling::Linear,
    None,
  );
  let out = walked_all(b);
  assert!(out.instances.is_empty());
  assert!(out.batches.is_empty());
}

#[test]
fn a_stroked_rect_expands_its_quad_by_half_the_width() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &stroked(4.0));
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rect, [8.0, 8.0, 24.0, 24.0]), "{:?}", i.rect);
  assert!(approx(i.offset_kind_stroke, [0.0, 0.0, KIND_RECT, 4.0]), "{:?}", i.offset_kind_stroke);
  assert_eq!(i.tex, [0.0; 4]);
}

#[test]
fn an_oval_is_its_bounds() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&rect(10.0, 20.0, 30.0, 40.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].rect, [10.0, 20.0, 30.0, 40.0]), "{:?}", out[0].rect);
  assert!(approx(out[0].offset_kind_stroke, [0.0, 0.0, KIND_OVAL, 0.0]), "{:?}", out[0].offset_kind_stroke);

  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&rect(10.0, 20.0, 30.0, 40.0), &stroked(2.0));
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].rect, [9.0, 19.0, 32.0, 42.0]), "{:?}", out[0].rect);
  assert!(approx(out[0].offset_kind_stroke, [0.0, 0.0, KIND_OVAL, 2.0]), "{:?}", out[0].offset_kind_stroke);
}

#[test]
fn a_horizontal_line_is_a_quad_along_it() {
  let out = line((10.0, 20.0), (50.0, 20.0), &stroked(4.0));
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rect, [0.0, -2.0, 40.0, 4.0]), "{:?}", i.rect);
  assert!(approx(i.linear, [1.0, 0.0, 0.0, 1.0]), "{:?}", i.linear);
  assert!(approx(i.offset_kind_stroke, [10.0, 20.0, KIND_LINE, 4.0]), "{:?}", i.offset_kind_stroke);
  assert_eq!(i.tex[0], CAP_BUTT);
  assert!((i.tex[1] - 40.0).abs() < EPS, "{:?}", i.tex);
}

#[test]
fn a_vertical_line_rotates_its_quad() {
  let mut paint = stroked(2.0);
  paint.set_stroke_cap(StrokeCap::Square);
  let out = line((10.0, 20.0), (10.0, 50.0), &paint);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rect, [-1.0, -1.0, 32.0, 2.0]), "{:?}", i.rect);
  // A rotation by +90 degrees in euclid's row-vector form: (len, 0) maps
  // to (0, len).
  assert!(approx(i.linear, [0.0, 1.0, -1.0, 0.0]), "{:?}", i.linear);
  assert!(approx([offset(i)[0], offset(i)[1], 0.0, 0.0], [10.0, 20.0, 0.0, 0.0]), "{:?}", offset(i));
  assert_eq!(i.tex[0], CAP_SQUARE);
  assert!((i.tex[1] - 30.0).abs() < EPS, "{:?}", i.tex);
  // The segment's end in local space lands on `to`.
  let [x, y] = mapped(i, 30.0, 0.0);
  assert!((x - 10.0).abs() < MAPPED_EPS && (y - 50.0).abs() < MAPPED_EPS, "{x} {y}");
}

#[test]
fn a_diagonal_line_maps_its_end_onto_to() {
  let mut paint = stroked(10.0);
  paint.set_stroke_cap(StrokeCap::Round);
  let out = line((0.0, 0.0), (30.0, 40.0), &paint);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert_eq!(i.tex[0], CAP_ROUND);
  assert!((i.tex[1] - 50.0).abs() < EPS, "{:?}", i.tex);
  assert!(approx(i.rect, [-5.0, -5.0, 60.0, 10.0]), "{:?}", i.rect);
  let [x, y] = mapped(i, 50.0, 0.0);
  assert!((x - 30.0).abs() < MAPPED_EPS && (y - 40.0).abs() < MAPPED_EPS, "{x} {y}");
  // The stroke's edge at the start: half the width from `from`, across the
  // segment.
  let [x, y] = mapped(i, 0.0, 5.0);
  assert!(((x * x + y * y).sqrt() - 5.0).abs() < MAPPED_EPS, "{x} {y}");
  assert!((x * 30.0 + y * 40.0).abs() < MAPPED_EPS, "{x} {y}");
}

#[test]
fn a_line_outside_the_clip_is_rejected() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_line(
    Point::new(50.0, 50.0),
    Point::new(60.0, 60.0),
    &stroked(2.0),
  );
  assert!(walked(b).is_empty());

  // A line of zero width draws nothing.
  assert!(line((0.0, 0.0), (10.0, 0.0), &stroked(0.0)).is_empty());
}

#[test]
fn a_line_under_the_state_transform_composes() {
  let mut b = DisplayListBuilder::new(None);
  // Within the target: a translation to its right edge would put the line
  // outside the clip.
  b.translate(50.0, 0.0).draw_line(Point::new(0.0, 0.0), Point::new(10.0, 0.0), &stroked(2.0));
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(offset(&out[0]), [50.0, 0.0]);
}

#[test]
fn a_rounded_rect_carries_its_radii() {
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 20.0, 20.0), [1.0, 2.0, 3.0, 4.0], &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  assert!(approx(i.radii, [1.0, 2.0, 3.0, 4.0]), "{:?}", i.radii);
  let pad = ROUNDED_QUAD_PAD;
  assert!(approx(i.rect, [10.0 - pad, 10.0 - pad, 20.0 + 2.0 * pad, 20.0 + 2.0 * pad]), "{:?}", i.rect);
  // No rounded clip is in effect.
  assert_eq!(i.rclip_radii, [0.0; 4]);
}

#[test]
fn zero_radii_are_a_plain_rect() {
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 20.0, 20.0), [0.0; 4], &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(out[0].offset_kind_stroke[2], KIND_RECT);
  assert!(approx(out[0].rect, [10.0, 10.0, 20.0, 20.0]), "{:?}", out[0].rect);
  assert_eq!(out[0].radii, [0.0; 4]);
}

#[test]
fn radii_are_clamped_as_css_clamps_them() {
  // 20 x 10 with 8 at every corner: the short sides ask for 10 / 16, so
  // every radius scales to 5.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(0.0, 0.0, 20.0, 10.0), [8.0; 4], &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].radii, [5.0; 4]), "{:?}", out[0].radii);

  // A negative radius reads as zero; the others are left alone.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(0.0, 0.0, 20.0, 10.0), [-3.0, 2.0, 2.0, 2.0], &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].radii, [0.0, 2.0, 2.0, 2.0]), "{:?}", out[0].radii);
}

#[test]
fn a_rounded_stroke_pads_its_quad() {
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 20.0, 20.0), [3.0; 4], &stroked(4.0));
  let out = walked(b);
  assert_eq!(out.len(), STROKE_INSTANCES);
  let i = &out[0];
  let grow = 2.0 + ROUNDED_QUAD_PAD;
  assert!(approx(i.rect, [10.0 - grow, 10.0 - grow, 20.0 + 2.0 * grow, 20.0 + 2.0 * grow]), "{:?}", i.rect);
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  assert_eq!(i.offset_kind_stroke[3], 4.0);
  assert!(approx(i.radii, [3.0; 4]), "{:?}", i.radii);
}

#[test]
fn stroke_and_fill_grows_the_radii() {
  let mut both = opaque();
  both.set_draw_style(DrawStyle::StrokeAndFill).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 20.0, 20.0), [4.0, 0.0, 4.0, 0.0], &both);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  assert_eq!(i.offset_kind_stroke[3], 0.0);
  let grow = 1.0 + ROUNDED_QUAD_PAD;
  assert!(approx(i.rect, [10.0 - grow, 10.0 - grow, 20.0 + 2.0 * grow, 20.0 + 2.0 * grow]), "{:?}", i.rect);
  // A sharp corner stays sharp: the miter join's outer corner.
  assert!(approx(i.radii, [5.0, 0.0, 5.0, 0.0]), "{:?}", i.radii);
}

#[test]
fn a_rounded_clip_lands_in_device_space() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 10.0).scale(2.0, 2.0).clip_rounded_rect(
    &rect(0.0, 0.0, 20.0, 10.0),
    &RoundingRadii::circular([3.0; 4]),
    ClipOperation::Intersect,
  );
  b.draw_rect(&rect(-10.0, -10.0, TARGET, TARGET), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rclip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.rclip);
  assert!(approx(i.rclip_radii, [6.0; 4]), "{:?}", i.rclip_radii);
  // The box clips to the rect too.
  assert!(approx(i.clip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.clip);
}

#[test]
fn a_nested_rounded_clip_degrades_to_a_box() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(10.0, 10.0, 60.0, 60.0), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect)
    .clip_rounded_rect(&rect(20.0, 20.0, 20.0, 20.0), &RoundingRadii::circular([2.0; 4]), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, TARGET, TARGET), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.clip, [20.0, 20.0, 40.0, 40.0]), "{:?}", i.clip);
  assert!(approx(i.rclip, [10.0, 10.0, 70.0, 70.0]), "{:?}", i.rclip);
  assert!(approx(i.rclip_radii, [5.0; 4]), "{:?}", i.rclip_radii);
}

#[test]
fn a_rounded_clip_under_rotation_is_a_box() {
  let rotation = Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(45.0));
  let mut b = DisplayListBuilder::new(None);
  b.translate(50.0, 50.0)
    .transform(&rotation)
    .clip_rounded_rect(&rect(-10.0, -10.0, 20.0, 20.0), &RoundingRadii::circular([4.0; 4]), ClipOperation::Intersect)
    .draw_rect(&rect(-50.0, -50.0, TARGET, TARGET), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert_eq!(i.rclip_radii, [0.0; 4]);
  // The rotated square's bounding box: half its diagonal from the center.
  let reach = 10.0 * std::f32::consts::SQRT_2;
  let want = [50.0 - reach, 50.0 - reach, 50.0 + reach, 50.0 + reach];
  assert!(i.clip.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < MAPPED_EPS), "{:?}", i.clip);
}

#[test]
fn an_image_source_textures_the_shape() {
  // Texel (0, 0) at local (10, 10), two local pixels per texel.
  let t = test_texture(7, ISize::new(200, 100));
  let to_local = Matrix::new_2d(2.0, 0.0, 0.0, 2.0, 10.0, 10.0);
  let paint = image_paint(&t, TextureSampling::Linear, Some(&to_local));
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 40.0, 20.0), [4.0; 4], &paint);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT + KIND_TEXTURED);
  // The padded quad's corners through the inverse map, over the size.
  let pad = ROUNDED_QUAD_PAD;
  let want = [(-pad / 2.0) / 200.0, (-pad / 2.0) / 100.0, (20.0 + pad / 2.0) / 200.0, (10.0 + pad / 2.0) / 100.0];
  assert!(approx(i.tex, want), "{:?}", i.tex);
  assert!(approx(i.color, [1.0; 4]), "{:?}", i.color);
  assert_eq!(out.batches, vec![Batch { start: 0, count: 1, texture: Some((name(&t), TextureSampling::Linear)) }]);

  // A plain rect with the same paint samples it too.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 40.0, 20.0), &paint);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(out[0].offset_kind_stroke[2], KIND_RECT + KIND_TEXTURED);
  assert!(approx(out[0].tex, [0.0, 0.0, 0.1, 0.1]), "{:?}", out[0].tex);
}

#[test]
fn a_rotated_image_source_draws_the_color() {
  let t = test_texture(7, ISize::new(200, 100));
  let rotated =
    Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(30.0)).then_translate(euclid::vec3(10.0, 10.0, 0.0));
  let mut paint = image_paint(&t, TextureSampling::Linear, Some(&rotated));
  paint.set_color(Color::new_srgba(1.0, 0.5, 0.0, 0.5));
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 40.0, 20.0), [4.0; 4], &paint);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  assert!(approx(i.color, [0.5, 0.25, 0.0, 0.5]), "{:?}", i.color);
  assert_eq!(i.tex, [0.0; 4]);
  assert_eq!(out.batches, vec![Batch { start: 0, count: 1, texture: None }]);
}
