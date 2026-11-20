// Anvil's pure walk (renderer/walk.rs): a list built through the crate's
// types, the way the rendertree builds one, walked into the instances the
// three programs draw, with no GL context. The transform stack, the
// device-space clip, the rejection of draws outside it, nested lists and
// the premultiplied color, the texture quads and their batches are all
// decided here, before any GL work, as are the rounded rects' radii and
// quads, the rounded clip in device space and the image-filled shapes'
// uv, the glyph runs' instances and their batches, the plain rect fills'
// batches, the rounded fills' batches, the paint shader draws' quads and
// batches, and the draw report and the scissor the executor takes from a
// walk. The deferred deletion
// queue a texture or a paint shader handle drops onto is tested here too,
// with no renderer, as is the paint shader's fragment source assembly.

use crate::contract::DrawReport;
use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{
  walk, Batch, BatchProgram, GradientClass, Instance, PaintDraw, Projective, Walked, CAP_BUTT, CAP_ROUND, CAP_SQUARE,
  FILL_FOLD_AREA, KIND_GLYPH, KIND_LINE, KIND_OVAL, KIND_PAINT, KIND_RECT, KIND_ROUNDED_RECT, KIND_TEXTURED,
  ROUNDED_QUAD_PAD, TILE_CLAMP,
};
use crate::renderer::{
  paint_fragment_source, report, test_paint_shader, test_texture, DeletionQueue, Freed, PAINT_FRAGMENT_EPILOGUE,
  SOURCE_HEADER,
};
use crate::{
  ClipOperation, Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, ISize, Matrix, Paint, PaintShader,
  Point, Rect, RoundingRadii, ShaderInput, ShaderParam, Size, StrokeCap, Texture, TextureSampling, TextureWrap,
  TileMode,
};
use crate::{CoverageMode, CoveragePolicy, GlyphQuad, GradientKind, RampGradient, RAMP_WIDTH};
use euclid::default::{Box2D, Point2D};
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
  walk(&build(b), ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
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

// A batch of shapes and texture quads: no glyph policy, no gradient.
fn shapes(start: usize, count: usize, texture: Option<(glow::NativeTexture, TextureSampling)>) -> Batch {
  Batch { start, count, program: BatchProgram::Shape, texture, policy: None, gradient: None, paint: None, stencil: 0 }
}

// A batch of plain rect fills: no texture, no glyph policy, no gradient.
fn fills(start: usize, count: usize) -> Batch {
  Batch {
    start,
    count,
    program: BatchProgram::Fill,
    texture: None,
    policy: None,
    gradient: None,
    paint: None,
    stencil: 0,
  }
}

// A batch of rounded fills: no texture, no glyph policy, of the gradient
// class `gradient` or solid (None).
fn roundeds(start: usize, count: usize, gradient: Option<GradientClass>) -> Batch {
  Batch { start, count, program: BatchProgram::Rounded, texture: None, policy: None, gradient, paint: None, stencil: 0 }
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

fn scissor(b: DisplayListBuilder) -> Option<Box2D<i32>> {
  walked_all(b).scissor
}

fn pixels(x0: i32, y0: i32, x1: i32, y1: i32) -> Option<Box2D<i32>> {
  Some(Box2D::new(Point2D::new(x0, y0), Point2D::new(x1, y1)))
}

#[test]
fn a_root_clip_is_the_scissor_rounded_outward() {
  // A patch frame's root clip around its draws, off the pixel grid.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(10.5, 20.25, 30.0, 40.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 100.0, 100.0), &opaque())
    .draw_rect(&rect(15.0, 25.0, 5.0, 5.0), &opaque());
  assert_eq!(scissor(b), pixels(10, 20, 41, 61));
}

#[test]
fn an_unclipped_list_scissors_to_the_target() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 5.0, 5.0), &opaque());
  assert_eq!(scissor(b), pixels(0, 0, TARGET as i32, TARGET as i32));
}

#[test]
fn a_list_that_emits_nothing_has_no_scissor() {
  assert_eq!(scissor(DisplayListBuilder::new(None)), None);

  // Everything rejected by the clip is nothing emitted, too.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_rect(&rect(20.0, 20.0, 5.0, 5.0), &opaque());
  assert_eq!(scissor(b), None);
}

#[test]
fn two_clips_scissor_to_their_bounding_box() {
  let mut b = DisplayListBuilder::new(None);
  b.save()
    .clip_rect(&rect(10.0, 10.0, 10.0, 10.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 100.0, 100.0), &opaque())
    .restore()
    .save()
    .clip_rect(&rect(60.0, 70.0, 20.0, 5.0), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 100.0, 100.0), &opaque())
    .restore();
  assert_eq!(scissor(b), pixels(10, 10, 80, 75));
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
  // The texel rows the shape program samples by give the same corners as
  // `tex`: a 2D map draws as it did.
  let uv = |x: f32, y: f32| {
    let at = |row: [f32; 4]| row[0] * x + row[1] * y + row[2];
    (at(i.grad_x), at(i.grad_y))
  };
  let (near, far) = (uv(10.0, 10.0), uv(50.0, 30.0));
  assert!(approx([near.0, near.1, far.0, far.1], i.tex), "{near:?} {far:?}");
  assert!(approx(i.color, [1.0; 4]), "{:?}", i.color);
  assert_eq!(i.offset_kind_stroke[2], KIND_RECT + KIND_TEXTURED);
  assert_eq!(i.offset_kind_stroke[3], 0.0);
  assert_eq!(out.batches, vec![shapes(0, 1, Some((name(&t), TextureSampling::Linear)))]);
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
  // A plain rect with no batch open is a fill run; a rect filled with `a`
  // as its image source joins `a`'s run, and a small plain rect after it
  // rides along in `c`'s.
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
    vec![fills(0, 1), shapes(1, 3, Some((name(&a), linear))), shapes(4, 2, Some((name(&c), linear)))]
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
    vec![shapes(0, 1, Some((name(&a), linear))), shapes(1, 1, Some((name(&a), TextureSampling::Nearest))),]
  );
}

#[test]
fn a_draw_report_counts_instances_and_batches() {
  let a = test_texture(1, ISize::new(10, 10));
  let c = test_texture(2, ISize::new(10, 10));
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let linear = TextureSampling::Linear;
  // Two rects, a texture quad and a rect: the two rects are a fill run,
  // the last rides along in the texture's run, so four instances draw in
  // two calls.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque())
    .draw_rect(&quad, &opaque())
    .draw_texture_rect(&a, &quad, &quad, linear, None)
    .draw_rect(&quad, &opaque());
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 4);
  assert_eq!(out.batches.len(), 2);
  assert_eq!(report(&out), DrawReport { instances: 4, draw_calls: 2, ..DrawReport::default() });

  // Ovals before a texture take it into their run: one call.
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque()).draw_oval(&quad, &opaque()).draw_texture_rect(&a, &quad, &quad, linear, None);
  let out = walked_all(b);
  assert_eq!(report(&out), DrawReport { instances: 3, draw_calls: 1, ..DrawReport::default() });

  // A second texture is a second call.
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque())
    .draw_texture_rect(&a, &quad, &quad, linear, None)
    .draw_texture_rect(&c, &quad, &quad, linear, None);
  let out = walked_all(b);
  assert_eq!(out.batches.len(), 2);
  assert_eq!(report(&out), DrawReport { instances: 3, draw_calls: 2, ..DrawReport::default() });

  // An empty list reports zeros.
  let out = walked_all(DisplayListBuilder::new(None));
  assert_eq!(report(&out), DrawReport::default());
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
  assert_eq!(queue.drain(), vec![Freed::Texture(five)]);
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
  // Large enough to split (`split_rounded`): the rim carries the rounded
  // clip, the plain pieces none.
  let out = walked(b);
  let rim: Vec<&Instance> = out.iter().filter(|i| i.rclip_radii != [0.0; 4]).collect();
  assert!(!rim.is_empty());
  assert!(out.iter().any(|i| i.rclip_radii == [0.0; 4]), "no plain piece");
  for i in rim {
    assert!(approx(i.rclip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.rclip);
    assert!(approx(i.rclip_radii, [6.0; 4]), "{:?}", i.rclip_radii);
  }
  // The box clips to the rect too, on every piece.
  for i in &out {
    assert!(approx(i.clip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.clip);
  }
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
  assert_eq!(out.batches, vec![shapes(0, 1, Some((name(&t), TextureSampling::Linear)))]);

  // A plain rect with the same paint samples it too.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 40.0, 20.0), &paint);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(out[0].offset_kind_stroke[2], KIND_RECT + KIND_TEXTURED);
  assert!(approx(out[0].tex, [0.0, 0.0, 0.1, 0.1]), "{:?}", out[0].tex);
}

#[test]
fn a_rotated_image_source_textures_the_shape() {
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
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT + KIND_TEXTURED);
  // The texel is premultiplied: the alpha in all four channels.
  assert!(approx(i.color, [0.5; 4]), "{:?}", i.color);
  // The texel rows on the instance send a local point through the inverse
  // of the rotated map, over the size: the map's own image of a texel
  // lands back on it.
  let texel = Point::new(30.0, 12.0);
  let local = rotated.to_2d().transform_point(texel);
  let uv = |row: [f32; 4]| row[0] * local.x + row[1] * local.y + row[2];
  let (u, v) = (uv(i.grad_x), uv(i.grad_y));
  assert!((u - texel.x / 200.0).abs() < EPS && (v - texel.y / 100.0).abs() < EPS, "{u} {v}");
  // No gradient: the kind is 0, and so is the tile.
  assert_eq!((i.grad_x[3], i.grad_y[3], i.persp[3]), (0.0, 0.0, 0.0));
  assert_eq!(out.batches, vec![shapes(0, 1, Some((name(&t), TextureSampling::Linear)))]);
}

#[test]
fn a_perspective_image_source_draws_the_color() {
  let t = test_texture(7, ISize::new(200, 100));
  let perspective = Matrix::perspective(100.0).then(&Matrix::rotation(0.0, 1.0, 0.0, euclid::Angle::degrees(30.0)));
  assert!(!perspective.is_2d());
  let mut paint = image_paint(&t, TextureSampling::Linear, Some(&perspective));
  paint.set_color(Color::new_srgba(1.0, 0.5, 0.0, 0.5));
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, rect(10.0, 10.0, 40.0, 20.0), [4.0; 4], &paint);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_ROUNDED_RECT);
  assert!(approx(i.color, [0.5, 0.25, 0.0, 0.5]), "{:?}", i.color);
  // Untextured and unstroked: a rounded fill, its whole quad in `tex`.
  assert_eq!(i.tex, i.rect);
  assert_eq!(out.batches, vec![roundeds(0, 1, None)]);
}

// The view's card flip: rotate_y by `degrees` about the origin, then the
// perspective foreshorten at `distance`, as View::compose_user composes
// them (alloy's kinds/view.rs).
fn card_flip(degrees: f32, distance: f32) -> Matrix {
  let (s, co) = degrees.to_radians().sin_cos();
  #[rustfmt::skip]
  let ry = Matrix::new(
    co,  0.0, -s,  0.0,
    0.0, 1.0, 0.0, 0.0,
    s,   0.0, co,  0.0,
    0.0, 0.0, 0.0, 1.0,
  );
  ry.then(&Matrix::perspective(distance))
}

#[test]
fn a_perspective_map_rides_on_the_instance() {
  let (degrees, distance) = (60.0f32, 800.0);
  let mut b = DisplayListBuilder::new(None);
  b.transform(&card_flip(degrees, distance)).draw_rect(&rect(0.0, 0.0, 60.0, 40.0), &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  // By hand: (x, y, 0, 1) through the flip is (x cos, y, -x sin, 1); the
  // foreshorten makes w' = 1 + x sin / d.
  let (s, co) = degrees.to_radians().sin_cos();
  assert!(approx(i.persp, [s / distance, 0.0, 1.0, 0.0]), "{:?}", i.persp);
  assert!(approx(i.linear, [co, 0.0, 0.0, 1.0]), "{:?}", i.linear);
  let p = Projective::from_matrix(&card_flip(degrees, distance));
  assert!(!p.is_affine());
  let project = |x: f32, y: f32| {
    let w = 1.0 + x * s / distance;
    (x * co / w, y / w)
  };
  let corners = [project(0.0, 0.0), project(60.0, 0.0), project(0.0, 40.0), project(60.0, 40.0)];
  let min = corners.iter().fold((f32::MAX, f32::MAX), |m, c| (m.0.min(c.0), m.1.min(c.1)));
  let max = corners.iter().fold((f32::MIN, f32::MIN), |m, c| (m.0.max(c.0), m.1.max(c.1)));
  let bounds = p.bounds(&rect(0.0, 0.0, 60.0, 40.0)).expect("every corner is in front of the eye");
  let got = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
  assert!(approx(got, [min.0, min.1, max.0, max.1]), "{got:?}");
  let far = p.project(Point::new(60.0, 40.0)).expect("in front of the eye");
  assert!((far.x - corners[3].0).abs() < EPS && (far.y - corners[3].1).abs() < EPS, "{far:?}");
}

#[test]
fn a_draw_with_a_corner_behind_the_eye_is_not_culled() {
  // A quarter turn under a close eye: w' = 1 + x / 10, behind the eye for
  // x below -10.
  let flip = card_flip(90.0, 10.0);
  let quad = rect(-300.0, 0.0, 250.0, 40.0);
  let p = Projective::from_matrix(&flip);
  assert!(p.bounds(&quad).is_none());
  assert!(p.project(Point::new(-300.0, 0.0)).is_none());
  let mut b = DisplayListBuilder::new(None);
  b.transform(&flip).draw_rect(&quad, &opaque());
  let out = walked(b);
  assert_eq!(out.len(), 1);
  // Its clip is the state's box: the target.
  assert_eq!(out[0].clip, [0.0, 0.0, TARGET, TARGET]);
}

#[test]
fn a_2d_map_carries_the_unit_projective_row() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(5.0, 5.0).draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert_eq!(out[0].persp, [0.0, 0.0, 1.0, 0.0]);
  assert_eq!((out[0].grad_x, out[0].grad_y), ([0.0; 4], [0.0; 4]));
}

// A glyph quad at (x, y), 4 x 6, sampling the atlas cell at (sx, 0).
fn glyph(x: f32, y: f32, sx: f32) -> GlyphQuad {
  GlyphQuad { dst: [x, y, 4.0, 6.0], src: [sx, 0.0, 4.0, 6.0] }
}

// Two glyphs side by side at y = 10, from x.
fn word(x: f32) -> Arc<[GlyphQuad]> {
  Arc::from([glyph(x, 10.0, 0.0), glyph(x + 5.0, 10.0, 4.0)])
}

// The color of a white glyph run.
const WHITE: [f32; 4] = [1.0; 4];

// A red-to-blue linear gradient across the first hundred pixels.
fn gradient() -> RampGradient {
  RampGradient {
    kind: GradientKind::Linear,
    x_row: [1.0 / TARGET, 0.0, 0.0],
    y_row: [0.0; 3],
    tile: TileMode::Clamp,
    ramp: vec![[255, 0, 0, 255]; RAMP_WIDTH],
  }
}

// A glyph batch over `atlas`, sampled linearly, under `policy`.
fn glyphs(start: usize, count: usize, atlas: &Texture, policy: CoveragePolicy) -> Batch {
  let texture = Some((name(atlas), TextureSampling::Linear));
  Batch {
    start,
    count,
    program: BatchProgram::Glyph,
    texture,
    policy: Some(policy),
    gradient: None,
    paint: None,
    stencil: 0,
  }
}

#[test]
fn a_glyph_run_walks_into_glyph_instances() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let color = [1.0, 0.5, 0.0, 0.5];
  let policy = CoveragePolicy::default();
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).draw_glyphs(&atlas, &Arc::from([glyph(2.0, 3.0, 16.0)]), color, policy, None);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  // The rect is the quad's dst in the op's space, under the state's map.
  assert!(approx(i.rect, [2.0, 3.0, 4.0, 6.0]), "{:?}", i.rect);
  assert!(approx(i.offset_kind_stroke, [10.0, 20.0, KIND_GLYPH, 0.0]), "{:?}", i.offset_kind_stroke);
  // The uv is src over the atlas's size, v growing downward.
  assert!(approx(i.tex, [0.25, 0.0, 20.0 / 64.0, 6.0 / 32.0]), "{:?}", i.tex);
  // Straight alpha: the policy reads the color as the run gave it.
  assert!(approx(i.color, color), "{:?}", i.color);
  assert!(approx(i.clip, [0.0, 0.0, TARGET, TARGET]), "{:?}", i.clip);
  assert_eq!(out.batches, vec![glyphs(0, 1, &atlas, policy)]);
}

#[test]
fn glyph_runs_are_rejected_whole_and_per_quad() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let policy = CoveragePolicy::default();
  // A whole run outside the clip yields nothing, not even a batch.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 20.0, 20.0), ClipOperation::Intersect).draw_glyphs(
    &atlas,
    &word(40.0),
    WHITE,
    policy,
    None,
  );
  let out = walked_all(b);
  assert!(out.instances.is_empty());
  assert!(out.batches.is_empty());

  // A run half inside keeps the quads inside: the first glyph spans
  // 16..20, the second 21..25 past the clip's right edge at 20.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 20.0, 20.0), ClipOperation::Intersect).draw_glyphs(
    &atlas,
    &word(16.0),
    WHITE,
    policy,
    None,
  );
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert!(approx(out[0].rect, [16.0, 10.0, 4.0, 6.0]), "{:?}", out[0].rect);

  // A fully transparent run is skipped whole; an empty run draws nothing.
  let mut b = DisplayListBuilder::new(None);
  let clear = [1.0, 1.0, 1.0, 0.0];
  let empty: Arc<[GlyphQuad]> = Arc::from([]);
  b.draw_glyphs(&atlas, &Arc::from([glyph(0.0, 0.0, 0.0)]), clear, policy, None)
    .draw_glyphs(&atlas, &Arc::from([glyph(5.0, 0.0, 4.0)]), WHITE, policy, None)
    .draw_glyphs(&atlas, &empty, WHITE, policy, None);
  let out = walked(b);
  assert_eq!(out.len(), 1);
  assert_eq!(out[0].rect[0], 5.0);
}

#[test]
fn glyph_runs_and_shapes_take_separate_batches() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let policy = CoveragePolicy::default();
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  // A glyph run between two rects: the run draws through the glyph
  // program, so it is a batch of its own over the atlas, between two
  // fill batches. The second rect overlaps a glyph, so it cannot join the
  // first fill batch in front of the run (the join rule of `Walked::push`).
  let over = rect(0.0, 8.0, 10.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_glyphs(&atlas, &word(0.0), WHITE, policy, None).draw_rect(&over, &opaque());
  let out = walked_all(b);
  assert_eq!(out.batches, vec![fills(0, 1), glyphs(1, 2, &atlas, policy), fills(3, 1)]);
  let (glyph_quads, shape_quads) = (2, 2);
  assert_eq!(report(&out), DrawReport { instances: glyph_quads + shape_quads, draw_calls: 3, ..DrawReport::default() });

  // Two runs with the same policy and nothing between them: one batch.
  let mut b = DisplayListBuilder::new(None);
  b.draw_glyphs(&atlas, &word(0.0), WHITE, policy, None).draw_glyphs(&atlas, &word(20.0), WHITE, policy, None);
  let out = walked_all(b);
  assert_eq!(out.batches, vec![glyphs(0, 4, &atlas, policy)]);

  // A policy change splits the run.
  let naive = CoveragePolicy { mode: CoverageMode::Naive, ..policy };
  let mut b = DisplayListBuilder::new(None);
  b.draw_glyphs(&atlas, &word(0.0), WHITE, policy, None).draw_glyphs(&atlas, &word(20.0), WHITE, naive, None);
  let out = walked_all(b);
  assert_eq!(out.batches, vec![glyphs(0, 2, &atlas, policy), glyphs(2, 2, &atlas, naive)]);
}

#[test]
fn glyph_runs_join_whatever_their_gradient() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let policy = CoveragePolicy::default();
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let g = Arc::new(gradient());
  let mut b = DisplayListBuilder::new(None);
  b.draw_glyphs(&atlas, &word(0.0), WHITE, policy, None)
    .draw_glyphs(&atlas, &word(20.0), WHITE, policy, Some(&g))
    .draw_glyphs(&atlas, &word(40.0), WHITE, policy, None)
    .draw_rect(&quad, &opaque())
    .draw_glyphs(&atlas, &word(60.0), WHITE, policy, None);
  let out = walked_all(b);
  assert_eq!(out.instances.len(), 9);
  // The glyph program reads the gradient per instance: the solid and the
  // gradient runs share one batch; the rect is a fill batch of its own
  // between the glyph runs.
  assert_eq!(out.batches, vec![glyphs(0, 6, &atlas, policy), fills(6, 1), glyphs(7, 2, &atlas, policy)]);
  // The gradient run's quads carry its own rows (the quads are in the
  // op's space), its ramp row, its kind and, in the projective row's last
  // float, its tile (clamp, 0); the solid ones zeros.
  for i in &out.instances[2..4] {
    assert_eq!(i.grad_x, [g.x_row[0], g.x_row[1], g.x_row[2], 0.0]);
    assert_eq!(i.grad_y, [g.y_row[0], g.y_row[1], g.y_row[2], GradientKind::Linear as i32 as f32]);
    assert_eq!(i.persp, [0.0, 0.0, 1.0, TILE_CLAMP]);
  }
  for i in out.instances[0..2].iter().chain(&out.instances[4..6]) {
    assert_eq!((i.grad_x, i.grad_y), ([0.0; 4], [0.0; 4]));
    assert_eq!(i.persp[3], 0.0);
  }
}

#[test]
fn an_image_between_glyph_runs_splits_them() {
  let atlas = test_texture(3, ISize::new(64, 32));
  let image = test_texture(4, ISize::new(10, 10));
  let policy = CoveragePolicy::default();
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let linear = TextureSampling::Linear;
  let mut b = DisplayListBuilder::new(None);
  b.draw_glyphs(&atlas, &word(0.0), WHITE, policy, None)
    .draw_texture_rect(&image, &quad, &quad, linear, None)
    .draw_glyphs(&atlas, &word(20.0), WHITE, policy, None);
  let out = walked_all(b);
  assert_eq!(
    out.batches,
    vec![glyphs(0, 2, &atlas, policy), shapes(2, 1, Some((name(&image), linear))), glyphs(3, 2, &atlas, policy)]
  );
}

#[test]
fn a_patch_clip_rejects_the_grid_outside_it() {
  // A partial-repaint frame as gl/draw.rs records it: a clip to the patch
  // around the frame list, whose root scales, clips to the window with the
  // rendertree's "infinite" extents on an axis, and nests a boundary's list
  // of a grid of rects; only the rects under the patch survive.
  const COLS: usize = 40;
  const ROWS: usize = 20;
  let mut grid = DisplayListBuilder::new(None);
  for i in 0..COLS * ROWS {
    let (x, y) = ((i % COLS) as f32 * 10.0, (i / COLS) as f32 * 10.0);
    grid.draw_rect(&rect(x, y, 9.0, 9.0), &opaque());
  }
  let grid = build(grid);
  let mut frame = DisplayListBuilder::new(None);
  frame.scale(1.0, 1.0);
  frame.save();
  frame.clip_rect(&rect(-1.0e7, 0.0, 2.0e7, 720.0), ClipOperation::Intersect);
  frame.save();
  frame.translate(0.0, 0.0);
  frame.draw_display_list(&grid, 1.0);
  frame.restore();
  frame.draw_rect(&rect(120.0, 280.0, 20.0, 20.0), &opaque());
  frame.restore();
  let frame = build(frame);
  let mut patch = DisplayListBuilder::new(None);
  patch.clip_rect(&rect(112.0, 280.0, 28.0, 20.0), ClipOperation::Intersect);
  patch.draw_display_list(&frame, 1.0);
  let out = walk(&build(patch), ISize::new(1280, 720), &mut RampTable::new());
  // The mover alone: the grid ends at y = 199.
  assert_eq!(out.instances.len(), 1, "{} instances survived the patch", out.instances.len());
}

#[test]
fn plain_rects_are_a_fill_batch() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_rect(&quad, &opaque()).draw_rect(&quad, &opaque());
  assert_eq!(walked_all(b).batches, vec![fills(0, 3)]);

  // A stroke-and-fill rect is one fill outset by half the width: plain.
  let mut both = opaque();
  both.set_draw_style(DrawStyle::StrokeAndFill).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_rect(&quad, &both);
  assert_eq!(walked_all(b).batches, vec![fills(0, 2)]);

  // An oval after a fill run opens a shape batch.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_oval(&quad, &opaque());
  assert_eq!(walked_all(b).batches, vec![fills(0, 1), shapes(1, 1, None)]);
}

#[test]
fn a_small_plain_rect_rides_along_in_a_shape_batch() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque()).draw_rect(&quad, &opaque());
  assert_eq!(walked_all(b).batches, vec![shapes(0, 2, None)]);

  // A large one opens a fill batch: 400 x 400 is past FILL_FOLD_AREA.
  let large = rect(0.0, 0.0, 400.0, 400.0);
  assert!(large.size.width * large.size.height >= FILL_FOLD_AREA);
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque()).draw_rect(&large, &opaque());
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, None), fills(1, 1)]);

  // The area is in device pixels: the same small rect under a scale that
  // makes it as large opens a fill batch too.
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque()).scale(40.0, 40.0).draw_rect(&quad, &opaque());
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, None), fills(1, 1)]);
}

#[test]
fn a_rect_the_fill_program_cannot_draw_is_a_shape() {
  let quad = rect(10.0, 10.0, 20.0, 20.0);
  // Under a rounded clip it is a rounded fill, not a shape
  // (`a_plain_rect_under_a_rounded_clip_is_a_rounded_fill`).

  // Stroked.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &stroked(2.0));
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, None)]);

  // Filled with an image.
  let t = test_texture(7, ISize::new(10, 10));
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &image_paint(&t, TextureSampling::Linear, None));
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, Some((name(&t), TextureSampling::Linear)))]);

  // After a fill run, such a rect opens a shape batch.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_rect(&quad, &stroked(2.0));
  assert_eq!(walked_all(b).batches, vec![fills(0, 1), shapes(1, 1, None)]);
}

#[test]
fn a_plain_rect_under_a_rounded_clip_is_a_rounded_fill() {
  // The rounded program's coverage is the rounded clip's: the rect's own
  // edges are MSAA's, so the fill program's lean path cannot take it, and
  // the shape program's kind branches and discard are not needed for it.
  let quad = rect(10.0, 10.0, 20.0, 20.0);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(0.0, 0.0, 50.0, 50.0), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect)
    .draw_rect(&quad, &opaque())
    .draw_rect(&quad, &opaque());
  let out = walked_all(b);
  assert_eq!(out.instances[0].offset_kind_stroke[2], KIND_RECT);
  assert_eq!(out.batches, vec![roundeds(0, 2, None)]);

  // A rounded rect under the same clip joins the run.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(0.0, 0.0, 50.0, 50.0), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect)
    .draw_rect(&quad, &opaque());
  rounded(&mut b, quad, [3.0; 4], &opaque());
  assert_eq!(walked_all(b).batches, vec![roundeds(0, 2, None)]);
}

#[test]
fn rounded_rect_fills_are_a_rounded_batch() {
  let quad = rect(10.0, 10.0, 20.0, 20.0);
  // Alone.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &opaque());
  assert_eq!(walked_all(b).batches, vec![roundeds(0, 1, None)]);

  // Two in a row share the batch.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &opaque());
  rounded(&mut b, quad, [2.0, 0.0, 2.0, 0.0], &opaque());
  assert_eq!(walked_all(b).batches, vec![roundeds(0, 2, None)]);

  // A plain fill between them does not join: it opens a fill batch.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &opaque());
  b.draw_rect(&quad, &opaque());
  rounded(&mut b, quad, [4.0; 4], &opaque());
  assert_eq!(walked_all(b).batches, vec![roundeds(0, 1, None), fills(1, 1), roundeds(2, 1, None)]);

  // A rounded stroke is a shape.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &stroked(2.0));
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, None)]);

  // A textured rounded rect is a shape.
  let t = test_texture(7, ISize::new(10, 10));
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &image_paint(&t, TextureSampling::Linear, None));
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, Some((name(&t), TextureSampling::Linear)))]);

  // A stroke after a rounded fill opens a shape batch.
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &opaque());
  rounded(&mut b, quad, [4.0; 4], &stroked(2.0));
  assert_eq!(walked_all(b).batches, vec![roundeds(0, 1, None), shapes(1, 1, None)]);
}

#[test]
fn a_small_rounded_rect_rides_along_in_a_shape_batch() {
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque());
  rounded(&mut b, quad, [2.0; 4], &opaque());
  assert_eq!(walked_all(b).batches, vec![shapes(0, 2, None)]);

  // A large one opens a rounded batch: 400 x 400 is past FILL_FOLD_AREA.
  // It splits (past ROUNDED_SPLIT_AREA too): its rim pieces, small as
  // they are, never ride along (the shape program measures a rounded rect
  // from its quad, not the whole one in `tex`), and its plain pieces open
  // a fill batch after them. Only the pieces inside the 100 x 100 target
  // are pushed: the top-left corner, the two edge bands beside it, and the
  // two plain pieces they frame.
  let large = rect(0.0, 0.0, 400.0, 400.0);
  assert!(large.size.width * large.size.height >= FILL_FOLD_AREA);
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&quad, &opaque());
  rounded(&mut b, large, [8.0; 4], &opaque());
  assert_eq!(walked_all(b).batches, vec![shapes(0, 1, None), roundeds(1, 3, None), fills(4, 2)]);
}

#[test]
fn a_gradient_rounded_rect_is_a_gradient_rounded_batch() {
  let quad = rect(10.0, 10.0, 20.0, 20.0);
  let mut graded = opaque();
  graded.set_color_source(&ColorSource::linear_gradient(
    Point::zero(),
    Point::new(TARGET, 0.0),
    &[Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  ));
  let mut b = DisplayListBuilder::new(None);
  rounded(&mut b, quad, [4.0; 4], &graded);
  rounded(&mut b, quad, [4.0; 4], &graded);
  // A solid one does not join it.
  rounded(&mut b, quad, [4.0; 4], &opaque());
  assert_eq!(
    walked_all(b).batches,
    vec![
      roundeds(0, 2, Some(GradientClass { kind: GradientKind::Linear as i32 as f32, tile: TILE_CLAMP })),
      roundeds(2, 1, None)
    ]
  );
}

#[test]
fn a_grid_of_rounded_panels_draws_in_one_call() {
  const COLS: usize = 5;
  const ROWS: usize = 4;
  // Each panel's cell, in local pixels.
  const CELL: f32 = 20.0;
  let mut b = DisplayListBuilder::new(None);
  for i in 0..COLS * ROWS {
    let (x, y) = ((i % COLS) as f32 * CELL, (i / COLS) as f32 * CELL);
    rounded(&mut b, rect(x + 1.0, y + 1.0, CELL - 2.0, CELL - 2.0), [3.0; 4], &opaque());
  }
  let out = walked_all(b);
  assert_eq!(out.batches, vec![roundeds(0, COLS * ROWS, None)]);
  assert_eq!(report(&out).draw_calls, 1);
}

// One param and one input, shared as a draw_shader takes them.
fn shader_args(t: &Texture) -> (Arc<[ShaderParam]>, Arc<[ShaderInput]>) {
  let params: Arc<[ShaderParam]> = Arc::new([ShaderParam { name: "uTime".to_string(), value: vec![1.5] }]);
  let inputs: Arc<[ShaderInput]> = Arc::new([ShaderInput {
    name: "uNoise".to_string(),
    texture: t.clone(),
    sampling: TextureSampling::Linear,
    wrap: TextureWrap::Repeat,
  }]);
  (params, inputs)
}

// A paint batch of the one instance at `start`.
fn paint_batch(
  start: usize,
  shader: &PaintShader,
  params: &Arc<[ShaderParam]>,
  inputs: &Arc<[ShaderInput]>,
  resolution: [f32; 2],
) -> Batch {
  let paint = PaintDraw { shader: shader.clone(), params: params.clone(), inputs: inputs.clone(), resolution };
  Batch {
    start,
    count: 1,
    program: BatchProgram::Paint,
    texture: None,
    policy: None,
    gradient: None,
    paint: Some(paint),
    stencil: 0,
  }
}

#[test]
fn a_paint_shader_draws_one_quad_in_a_batch_of_its_own() {
  let shader = test_paint_shader();
  let t = test_texture(7, ISize::new(10, 10));
  let (params, inputs) = shader_args(&t);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 60.0, 50.0), ClipOperation::Intersect).translate(10.0, 20.0).scale(2.0, 3.0).draw_shader(
    &rect(1.0, 2.0, 12.0, 8.0),
    &shader,
    &params,
    &inputs,
  );
  let list = build(b);
  assert_eq!(list.ops().last(), Some(&crate::oplog::Op::DrawShader { rect: rect(1.0, 2.0, 12.0, 8.0) }));
  let out = walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new());
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  // Placed as a texture quad is: the rect, the map, the unit square.
  assert!(approx(i.rect, [1.0, 2.0, 12.0, 8.0]), "{:?}", i.rect);
  assert!(approx(i.linear, [2.0, 0.0, 0.0, 3.0]), "{:?}", i.linear);
  assert!(approx(i.offset_kind_stroke, [10.0, 20.0, KIND_PAINT, 0.0]), "{:?}", i.offset_kind_stroke);
  assert!(approx(i.tex, [0.0, 0.0, 1.0, 1.0]), "{:?}", i.tex);
  assert!(approx(i.clip, [0.0, 0.0, 60.0, 50.0]), "{:?}", i.clip);
  // The resolution is the quad in device pixels: 12 x 8 under a 2 x 3 scale.
  assert_eq!(out.batches, vec![paint_batch(0, &shader, &params, &inputs, [24.0, 24.0])]);
  assert_eq!(report(&out), DrawReport { instances: 1, draw_calls: 1, ..DrawReport::default() });
}

#[test]
fn a_rotated_paint_shader_measures_its_edges_in_device_pixels() {
  let shader = test_paint_shader();
  let (params, inputs): (Arc<[ShaderParam]>, Arc<[ShaderInput]>) = (Arc::new([]), Arc::new([]));
  let rotation = Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(30.0));
  let mut b = DisplayListBuilder::new(None);
  b.translate(50.0, 50.0).transform(&rotation).scale(2.0, 2.0).draw_shader(
    &rect(-5.0, -5.0, 10.0, 6.0),
    &shader,
    &params,
    &inputs,
  );
  let out = walked_all(b);
  let resolution = out.batches[0].paint.as_ref().expect("a paint batch carries its draw").resolution;
  assert!((resolution[0] - 20.0).abs() < MAPPED_EPS && (resolution[1] - 12.0).abs() < MAPPED_EPS, "{resolution:?}");
}

#[test]
fn a_paint_shader_ends_the_batch_before_and_after_it() {
  let shader = test_paint_shader();
  let (params, inputs): (Arc<[ShaderParam]>, Arc<[ShaderInput]>) = (Arc::new([]), Arc::new([]));
  let quad = rect(0.0, 0.0, 10.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&quad, &opaque()).draw_shader(&quad, &shader, &params, &inputs).draw_rect(&quad, &opaque());
  let paint = paint_batch(1, &shader, &params, &inputs, [10.0, 10.0]);
  assert_eq!(walked_all(b).batches, vec![fills(0, 1), paint, fills(2, 1)]);

  // Two paint draws, even of one shader, are two batches.
  let mut b = DisplayListBuilder::new(None);
  b.draw_shader(&quad, &shader, &params, &inputs).draw_shader(&quad, &shader, &params, &inputs);
  let batches = walked_all(b).batches;
  assert_eq!(
    batches,
    vec![
      paint_batch(0, &shader, &params, &inputs, [10.0, 10.0]),
      paint_batch(1, &shader, &params, &inputs, [10.0, 10.0])
    ]
  );
}

#[test]
fn a_paint_shader_outside_the_clip_is_rejected() {
  let shader = test_paint_shader();
  let (params, inputs): (Arc<[ShaderParam]>, Arc<[ShaderInput]>) = (Arc::new([]), Arc::new([]));
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 10.0, 10.0), ClipOperation::Intersect).draw_shader(
    &rect(20.0, 20.0, 5.0, 5.0),
    &shader,
    &params,
    &inputs,
  );
  let out = walked_all(b);
  assert!(out.instances.is_empty());
  assert!(out.batches.is_empty());
}

#[test]
fn a_paint_shader_under_a_rounded_clip_carries_it_in_device_space() {
  let shader = test_paint_shader();
  let (params, inputs): (Arc<[ShaderParam]>, Arc<[ShaderInput]>) = (Arc::new([]), Arc::new([]));
  let box_ = rect(0.0, 0.0, 20.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 10.0)
    .scale(2.0, 2.0)
    .save()
    .clip_rounded_rect(&box_, &RoundingRadii::circular([3.0; 4]), ClipOperation::Intersect)
    .draw_shader(&box_, &shader, &params, &inputs)
    .restore();
  let out = walked(b);
  assert_eq!(out.len(), 1);
  let i = &out[0];
  assert!(approx(i.rclip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.rclip);
  assert!(approx(i.rclip_radii, [6.0; 4]), "{:?}", i.rclip_radii);
  assert!(approx(i.clip, [10.0, 10.0, 50.0, 30.0]), "{:?}", i.clip);
}

#[test]
fn a_dropped_paint_shader_queues_its_program_once() {
  assert!(test_paint_shader().uniforms().is_empty());
  let queue = Arc::new(DeletionQueue::new());
  let a = PaintShader::over(queue.clone(), 9);
  let b = a.clone();
  drop(a);
  assert!(queue.drain().is_empty());
  drop(b);
  let nine = glow::NativeProgram(std::num::NonZeroU32::new(9).expect("nine is not zero"));
  assert_eq!(queue.drain(), vec![Freed::Program(nine)]);
  assert!(queue.drain().is_empty());
}

#[test]
fn a_paint_source_with_a_version_line_is_refused() {
  let refused = paint_fragment_source("#version 300 es\nvoid main() { fragColor = vec4(1.0); }");
  assert!(refused.is_err_and(|e| e.contains("#version")));
  // The directive, not the word: a comment mentioning it is fine.
  let commented = paint_fragment_source("// no #version here\nvoid main() { fragColor = vec4(1.0); }");
  assert!(commented.is_ok());
}

#[test]
fn a_paint_source_is_wrapped_in_the_header_and_the_epilogue() {
  let src = "void main() { fragColor = vec4(vUV, 0.0, 1.0); }";
  let full = paint_fragment_source(src).expect("a source without #version assembles");
  assert!(full.starts_with(SOURCE_HEADER), "{full}");
  assert!(full.ends_with(PAINT_FRAGMENT_EPILOGUE), "{full}");
  // The app's source, its main renamed, on line 1 by the directive.
  assert!(full.contains(&format!("#line 1\n{src}\n#undef main\n")), "{full}");
}
