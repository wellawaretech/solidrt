// A paint's mask blur (renderer/walk.rs `push_shadow`, `masked`): a plain
// rect or rounded rect fill as one analytic shadow instance in a shadow
// batch, a plain fill under its own difference clip as a shadow carrying
// the clip as its cutout (`cutout_shadow`) and anything else at a
// difference clip through the stencil, every other masked op as an
// implicit layer with a `Decal` blur of the sigma, a masked shadow folding
// into a group's alpha, a sigma below the recording's floor drawing hard,
// the op log keeping the mask filter, and the shadow program's reach
// pinned to the walk's (renderer/exec.rs SHADOW_FRAGMENT). No GL context.

use crate::oplog::Op as LogOp;
use crate::prelude::*;
use crate::renderer::blur::{blur_reach, DeviceBlur};
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{
  walk, BatchProgram, Instance, Walked, HARD_SHADOW_SIGMA, KIND_SHADOW, SHADOW_REACH_SIGMAS,
};
use crate::renderer::{report, MASK_SIGMA_MIN, SHADOW_FRAGMENT};
use crate::{
  BlendMode, ClipOperation, Color, ColorFilter, ColorMatrix, ColorSource, DisplayListBuilder, DrawStyle, FillType,
  ISize,
};
use crate::{MaskFilter, Paint, PathBuilder, Point, Rect, RoundingRadii, Size, TileMode};
use euclid::default::{Box2D, Point2D};

// How far a computed value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// The target every walk here draws into, in device pixels.
const TARGET: f32 = 100.0;

// The shadow's sigma, local units: a reach of 6.
const SIGMA: f32 = 2.0;

// The shadow's straight color.
const RED: f32 = 1.0;
const GREEN: f32 = 0.5;
const ALPHA: f32 = 0.5;

// The group alpha of the fold case.
const HALF: f32 = 0.5;

// The stroke width of the stroked rect case.
const STROKE: f32 = 2.0;

// The radii of the rounded cases, CSS order; no side's two sum past it.
const RADII: [f32; 4] = [4.0, 6.0, 8.0, 2.0];

// The rounded clip's radius.
const CLIP_RADIUS: f32 = 10.0;

// The uniform scale of the local-units cases.
const SCALE: f32 = 2.0;

// The translation of the mapped cutout case.
const OFFSET_X: f32 = 5.0;
const OFFSET_Y: f32 = 7.0;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn ibox(x0: i32, y0: i32, x1: i32, y1: i32) -> Box2D<i32> {
  Box2D::new(Point2D::new(x0, y0), Point2D::new(x1, y1))
}

fn walked(b: DisplayListBuilder) -> Walked {
  let list = b.build().expect("a builder always builds a list");
  walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
}

fn programs(out: &Walked) -> Vec<BatchProgram> {
  out.batches.iter().map(|b| b.program).collect()
}

fn approx(got: [f32; 4], want: [f32; 4]) -> bool {
  got.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < EPS)
}

// The shadow color with a mask blur of `sigma`.
fn masked(sigma: f32) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(RED, GREEN, 0.0, ALPHA)).set_mask_filter(&MaskFilter::blur(sigma));
  paint
}

fn opaque() -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 1.0, 1.0));
  paint
}

// The one layer a walk drew, with its `Decal` blur of SIGMA (the identity
// map: local units are device pixels).
fn assert_masked_layer(out: &Walked) {
  assert_eq!(out.layers.len(), 1);
  let layer = &out.layers[0];
  assert!(!layer.folded);
  assert_eq!(layer.blur, Some(DeviceBlur { sigma_x: SIGMA, sigma_y: SIGMA, tile: TileMode::Decal }));
  assert!(layer.composite.color.iter().all(|c| (c - 1.0).abs() < EPS), "{:?}", layer.composite.color);
}

// The shadows a walk emitted.
fn shadows(out: &Walked) -> Vec<&Instance> {
  out.instances.iter().filter(|i| i.offset_kind_stroke[2] == KIND_SHADOW).collect()
}

#[test]
fn a_masked_rounded_rect_fill_is_one_analytic_shadow() {
  let shape = rect(20.0, 20.0, 40.0, 30.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&shape, &RoundingRadii::circular(RADII), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert!(out.layers.is_empty());
  assert_eq!(out.instances.len(), 1);
  let i = &out.instances[0];
  let reach = SHADOW_REACH_SIGMAS * SIGMA;
  // Not rounded to whole pixels: 3 sigma exactly, per side.
  assert!(approx(i.rect, [20.0 - reach, 20.0 - reach, 40.0 + 2.0 * reach, 30.0 + 2.0 * reach]), "{:?}", i.rect);
  assert!(approx(i.tex, [20.0, 20.0, 40.0, 30.0]), "{:?}", i.tex);
  assert_eq!(i.offset_kind_stroke[2], KIND_SHADOW);
  assert_eq!(i.offset_kind_stroke[3], SIGMA);
  assert!(approx(i.radii, RADII), "{:?}", i.radii);
  assert!(approx(i.color, [RED * ALPHA, GREEN * ALPHA, 0.0, ALPHA]), "{:?}", i.color);
  assert_eq!(i.rclip_radii, [0.0; 4]);
  let batch = &out.batches[0];
  assert_eq!((batch.count, batch.texture, batch.blend), (1, None, BlendMode::SourceOver));
}

#[test]
fn a_masked_rect_fill_is_a_shadow_with_zero_radii() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(20.0, 20.0, 40.0, 30.0), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert_eq!(out.instances[0].radii, [0.0; 4]);
}

#[test]
fn a_shadow_under_a_rounded_clip_carries_the_clip() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(
    &rect(10.0, 10.0, 80.0, 80.0),
    &RoundingRadii::circular([CLIP_RADIUS; 4]),
    ClipOperation::Intersect,
  );
  b.draw_rounded_rect(&rect(30.0, 30.0, 20.0, 20.0), &RoundingRadii::circular(RADII), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  let i = &out.instances[0];
  assert_eq!(i.rclip, [10.0, 10.0, 90.0, 90.0]);
  assert_eq!(i.rclip_radii, [CLIP_RADIUS; 4]);
  assert_eq!(i.clip, [10.0, 10.0, 90.0, 90.0]);
}

#[test]
fn two_shadows_share_a_batch_and_a_fill_between_them_opens_its_own() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &masked(SIGMA));
  b.draw_rect(&rect(50.0, 10.0, 20.0, 20.0), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert_eq!(out.batches[0].count, 2);

  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &masked(SIGMA));
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque());
  b.draw_rect(&rect(50.0, 10.0, 20.0, 20.0), &masked(SIGMA));
  let out = walked(b);
  // The fill never rides in the shadow batch, nor the shadow in the fill's.
  assert_eq!(programs(&out), vec![BatchProgram::Shadow, BatchProgram::Fill, BatchProgram::Shadow]);
  assert_eq!(shadows(&out).len(), 2);
}

#[test]
fn a_fill_clear_of_a_shadow_joins_the_fill_batch_before_it() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &opaque());
  b.draw_rect(&rect(40.0, 40.0, 20.0, 20.0), &masked(SIGMA));
  // Clear of the shadow's box (34 to 66): it goes before it.
  b.draw_rect(&rect(80.0, 0.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Fill, BatchProgram::Shadow]);
  assert_eq!(out.batches[0].count, 2);

  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &opaque());
  b.draw_rect(&rect(40.0, 40.0, 20.0, 20.0), &masked(SIGMA));
  // Inside the shadow's reach, past its shape: it stays after it.
  b.draw_rect(&rect(62.0, 40.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Fill, BatchProgram::Shadow, BatchProgram::Fill]);
}

#[test]
fn a_masked_oval_is_a_blurred_layer_around_its_shape() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_oval(&rect(30.0, 30.0, 20.0, 20.0), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Shape, BatchProgram::LayerEnd]);
  assert_masked_layer(&out);
  let reach = blur_reach(SIGMA) as i32;
  assert_eq!(out.layers[0].rect, ibox(30 - reach, 30 - reach, 50 + reach, 50 + reach));
  assert_eq!(out.layers[0].clamp, None);
  // The oval inside draws hard, at the paint's color.
  let oval = &out.instances[0];
  assert!(approx(oval.color, [RED * ALPHA, GREEN * ALPHA, 0.0, ALPHA]), "{:?}", oval.color);
  assert!(shadows(&out).is_empty());
}

#[test]
fn a_masked_path_a_stroked_rect_and_a_sourced_rect_draw_through_a_layer() {
  let mut pb = PathBuilder::default();
  pb.move_to(Point::new(10.0, 10.0)).line_to(Point::new(40.0, 10.0)).line_to(Point::new(10.0, 40.0)).close();
  let path = pb.build(FillType::NonZero);
  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&path, &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Mesh, BatchProgram::LayerEnd]);
  assert_masked_layer(&out);

  let mut stroked = masked(SIGMA);
  stroked.set_draw_style(DrawStyle::Stroke).set_stroke_width(STROKE);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &stroked);
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Shape, BatchProgram::LayerEnd]);
  assert_masked_layer(&out);

  let stops = [Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)];
  let source =
    ColorSource::linear_gradient(Point::zero(), Point::new(TARGET, 0.0), &stops, &[0.0, 1.0], TileMode::Clamp, None);
  let mut sourced = masked(SIGMA);
  sourced.set_color_source(&source);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &sourced);
  let out = walked(b);
  assert_eq!(programs(&out).first(), Some(&BatchProgram::LayerBegin));
  assert_eq!(programs(&out).last(), Some(&BatchProgram::LayerEnd));
  assert_masked_layer(&out);
  assert!(shadows(&out).is_empty());
}

#[test]
fn a_masked_rect_under_a_shader_blend_is_one_layer_with_both() {
  let mut paint = masked(SIGMA);
  paint.set_blend_mode(BlendMode::Multiply);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &paint);
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Fill, BatchProgram::LayerEnd]);
  assert_masked_layer(&out);
  assert_eq!(out.layers[0].blend, BlendMode::Multiply);
  // The rect inside draws source-over.
  assert_eq!(out.batches[1].blend, BlendMode::SourceOver);
}

#[test]
fn a_culled_masked_oval_opens_no_layer() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 10.0, 10.0), &opaque());
  b.draw_oval(&rect(200.0, 200.0, 20.0, 20.0), &masked(SIGMA));
  b.draw_rect(&rect(30.0, 10.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Fill]);
  assert!(out.layers.is_empty());
}

#[test]
fn a_shadow_inside_a_faded_group_folds_its_alpha() {
  let mut group = Paint::default();
  group.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF));
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&group));
  b.draw_rounded_rect(&rect(30.0, 30.0, 20.0, 20.0), &RoundingRadii::circular(RADII), &masked(SIGMA));
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert_eq!(out.layers.len(), 1);
  assert!(out.layers[0].folded);
  let want = [RED * ALPHA * HALF, GREEN * ALPHA * HALF, 0.0, ALPHA * HALF];
  assert!(approx(out.instances[0].color, want), "{:?}", out.instances[0].color);
}

#[test]
fn a_sigma_below_the_floor_draws_the_plain_fill() {
  for sigma in [MASK_SIGMA_MIN * 0.5, -SIGMA, f32::NAN] {
    let mut b = DisplayListBuilder::new(None);
    b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &masked(sigma));
    let out = walked(b);
    assert_eq!(programs(&out), vec![BatchProgram::Fill], "{sigma}");
    assert!(shadows(&out).is_empty() && out.layers.is_empty(), "{sigma}");
  }
}

#[test]
fn the_op_log_keeps_the_mask_filter() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &masked(SIGMA));
  let list = b.build().expect("a builder always builds a list");
  assert!(list.ops()[0].paint().expect("a draw has a paint").mask_filter);
}

#[test]
fn the_shadow_program_reaches_as_far_as_the_walk() {
  let declaration = format!("const float SHADOW_REACH = {SHADOW_REACH_SIGMAS:?};");
  assert!(SHADOW_FRAGMENT.contains(&declaration), "{declaration}");
}

#[test]
fn a_shadow_under_its_rounded_cutout_is_one_instance_with_no_stencil() {
  let cut = rect(20.0, 20.0, 40.0, 30.0);
  let radii = RoundingRadii::circular(RADII);
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rounded_rect(&cut, &radii, ClipOperation::Difference);
  b.draw_rounded_rect(&rect(24.0, 26.0, 40.0, 30.0), &radii, &masked(SIGMA));
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert_eq!(out.batches[0].stencil, 0);
  assert!(out.clip_draws.is_empty());
  assert_eq!(report(&out).clips, 0);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[3], SIGMA);
  // The cutout relative to the shape rect's origin, and its radii.
  assert!(approx(i.grad_x, [-4.0, -6.0, 40.0, 30.0]), "{:?}", i.grad_x);
  assert!(approx(i.grad_y, RADII), "{:?}", i.grad_y);
  assert_eq!(i.persp[3], 0.0);
}

#[test]
fn a_shadow_under_its_rect_cutout_has_zero_cutout_radii() {
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Difference);
  b.draw_rect(&rect(34.0, 36.0, 20.0, 20.0), &masked(SIGMA));
  b.restore();
  // A depth-0 shadow after the restore joins the same batch.
  b.draw_rect(&rect(60.0, 60.0, 20.0, 20.0), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert_eq!((out.batches[0].count, out.batches[0].stencil), (2, 0));
  assert!(out.clip_draws.is_empty());
  let i = &out.instances[0];
  assert!(approx(i.grad_x, [-4.0, -6.0, 20.0, 20.0]), "{:?}", i.grad_x);
  assert_eq!(i.grad_y, [0.0; 4]);
  // The shadow without a cutout carries none.
  assert_eq!((out.instances[1].grad_x, out.instances[1].grad_y), ([0.0; 4], [0.0; 4]));
}

#[test]
fn a_hard_fill_under_a_cutout_is_a_shadow_of_the_hard_sigma() {
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Difference);
  b.draw_rect(&rect(34.0, 36.0, 20.0, 20.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[2], KIND_SHADOW);
  assert_eq!(i.offset_kind_stroke[3], HARD_SHADOW_SIGMA);
  assert!(out.clip_draws.is_empty());

  // Without the cutout a hard fill stays a plain fill.
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(34.0, 36.0, 20.0, 20.0), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Fill]);
  assert!(shadows(&out).is_empty());
}

// The difference clip took the stencil: its box raise and shape lower, and
// every draw batch before the restore at depth 1.
fn assert_stencil_clip(out: &Walked) {
  let clips = out.batches.iter().filter(|b| b.program == BatchProgram::Clip).count();
  // The raise, the lower and the restore's reset.
  assert_eq!(clips, 3, "{:?}", programs(out));
  assert_eq!(out.clip_draws.len(), 3);
  let draws: Vec<u8> = out.batches.iter().filter(|b| b.program != BatchProgram::Clip).map(|b| b.stencil).collect();
  assert!(!draws.is_empty() && draws.iter().all(|d| *d == 1), "{draws:?}");
  assert!(shadows(out).iter().all(|i| i.grad_x == [0.0; 4] && i.grad_y == [0.0; 4]));
}

#[test]
fn a_difference_clip_over_anything_else_takes_the_stencil() {
  let cut = rect(30.0, 30.0, 20.0, 20.0);
  let shape = rect(34.0, 36.0, 20.0, 20.0);
  let mut stroked = opaque();
  stroked.set_draw_style(DrawStyle::Stroke).set_stroke_width(STROKE);

  // Two draws before the restore.
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&cut, ClipOperation::Difference);
  b.draw_rect(&shape, &masked(SIGMA));
  b.draw_rect(&shape, &masked(SIGMA));
  b.restore();
  assert_stencil_clip(&walked(b));

  // An oval.
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&cut, ClipOperation::Difference);
  b.draw_oval(&shape, &opaque());
  b.restore();
  assert_stencil_clip(&walked(b));

  // A stroked rect.
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&cut, ClipOperation::Difference);
  b.draw_rect(&shape, &stroked);
  b.restore();
  assert_stencil_clip(&walked(b));

  // A transform between the clip and the draw.
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&cut, ClipOperation::Difference);
  b.translate(1.0, 1.0);
  b.draw_rect(&shape, &masked(SIGMA));
  b.restore();
  assert_stencil_clip(&walked(b));
}

#[test]
fn a_cutout_under_a_translate_and_a_scale_stays_in_the_shapes_space() {
  let mut b = DisplayListBuilder::new(None);
  b.translate(OFFSET_X, OFFSET_Y);
  b.scale(SCALE, SCALE);
  b.save();
  b.clip_rect(&rect(10.0, 10.0, 10.0, 10.0), ClipOperation::Difference);
  b.draw_rect(&rect(12.0, 13.0, 10.0, 10.0), &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert!(out.clip_draws.is_empty());
  let i = &out.instances[0];
  // The cutout relative to the shape's origin, the map on the instance.
  assert!(approx(i.grad_x, [-2.0, -3.0, 10.0, 10.0]), "{:?}", i.grad_x);
  assert!(approx(i.linear, [SCALE, 0.0, 0.0, SCALE]), "{:?}", i.linear);
  assert!((i.offset_kind_stroke[0] - OFFSET_X).abs() < EPS && (i.offset_kind_stroke[1] - OFFSET_Y).abs() < EPS);
  // The hard sigma is a quarter device pixel: in local units, over the scale.
  assert!((i.offset_kind_stroke[3] - HARD_SHADOW_SIGMA / SCALE).abs() < EPS, "{:?}", i.offset_kind_stroke);
}

// The difference clip took the stencil (its raise, its lower and the
// restore's reset) and no shadow carries a cutout; for the arms that may
// draw through a layer, whose content draws at depth 0.
fn assert_stencil_clip_draws(out: &Walked) {
  assert_eq!(out.clip_draws.len(), 3, "{:?}", programs(out));
  assert!(shadows(out).iter().all(|i| i.grad_x == [0.0; 4] && i.grad_y == [0.0; 4]));
}

#[test]
fn a_blend_a_source_or_a_color_filter_at_a_difference_clip_takes_the_stencil() {
  let cut = rect(30.0, 30.0, 20.0, 20.0);
  let shape = rect(34.0, 36.0, 20.0, 20.0);
  let mut multiply = opaque();
  multiply.set_blend_mode(BlendMode::Multiply);
  let stops = [Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)];
  let source =
    ColorSource::linear_gradient(Point::zero(), Point::new(TARGET, 0.0), &stops, &[0.0, 1.0], TileMode::Clamp, None);
  let mut sourced = opaque();
  sourced.set_color_source(&source);
  let mut filtered = opaque();
  // Any matrix: its presence alone decides.
  filtered.set_color_filter(&ColorFilter::matrix(&ColorMatrix { m: [1.0; 20] }));
  for paint in [multiply, sourced, filtered] {
    let mut b = DisplayListBuilder::new(None);
    b.save();
    b.clip_rect(&cut, ClipOperation::Difference);
    b.draw_rect(&shape, &paint);
    b.restore();
    assert_stencil_clip_draws(&walked(b));
  }
}

#[test]
fn a_culled_cutout_shadow_pushes_nothing_and_the_restore_returns_to_depth_0() {
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&rect(200.0, 200.0, 20.0, 20.0), ClipOperation::Difference);
  b.draw_rect(&rect(204.0, 206.0, 20.0, 20.0), &masked(SIGMA));
  b.restore();
  b.draw_rect(&rect(10.0, 10.0, 10.0, 10.0), &opaque());
  let out = walked(b);
  assert!(out.clip_draws.is_empty());
  assert!(shadows(&out).is_empty());
  assert_eq!(out.instances.len(), 1);
  assert_eq!((out.batches.len(), out.batches[0].program, out.batches[0].stencil), (1, BatchProgram::Fill, 0));
}

#[test]
fn a_nested_lists_restore_without_its_save_leaves_the_clip_to_the_stencil() {
  let mut inner = DisplayListBuilder::new(None);
  inner.clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Difference);
  inner.draw_rect(&rect(34.0, 36.0, 20.0, 20.0), &masked(SIGMA));
  inner.restore();
  inner.draw_rect(&rect(60.0, 60.0, 20.0, 20.0), &masked(SIGMA));
  let inner = inner.build().expect("a builder always builds a list");
  let mut b = DisplayListBuilder::new(None);
  b.draw_display_list(&inner, 1.0);
  let out = walked(b);
  // The restore pops nothing: the clip stays in effect to the list's end,
  // so both shadows draw at depth 1, with no cutout.
  assert_stencil_clip_draws(&out);
  let shadow_depths: Vec<u8> =
    out.batches.iter().filter(|b| b.program == BatchProgram::Shadow).map(|b| b.stencil).collect();
  assert_eq!(shadow_depths, vec![1]);
  assert_eq!(shadows(&out).len(), 2);
}

#[test]
fn a_cutout_shadow_folds_into_a_faded_group_unless_a_fill_overlaps_it() {
  let mut group = Paint::default();
  group.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF));
  let card = rect(30.0, 30.0, 20.0, 20.0);
  let radii = RoundingRadii::circular(RADII);

  // A shadowed card: the cutout shadow and the card's opaque fill over it.
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&group));
  b.save();
  b.clip_rounded_rect(&card, &radii, ClipOperation::Difference);
  b.draw_rounded_rect(&rect(32.0, 34.0, 20.0, 20.0), &radii, &masked(SIGMA));
  b.restore();
  b.draw_rounded_rect(&card, &radii, &opaque());
  b.restore();
  let out = walked(b);
  assert_eq!(out.layers.len(), 1);
  assert!(!out.layers[0].folded);
  assert!(out.clip_draws.is_empty());

  // The cutout shadow alone folds: no clip batch blocks it.
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&group));
  b.save();
  b.clip_rounded_rect(&card, &radii, ClipOperation::Difference);
  b.draw_rounded_rect(&rect(32.0, 34.0, 20.0, 20.0), &radii, &masked(SIGMA));
  b.restore();
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  assert!(out.layers[0].folded);
  let want = [RED * ALPHA * HALF, GREEN * ALPHA * HALF, 0.0, ALPHA * HALF];
  assert!(approx(out.instances[0].color, want), "{:?}", out.instances[0].color);
}

#[test]
fn the_op_log_keeps_the_cutout_clip_and_its_draw() {
  let cut = rect(30.0, 30.0, 20.0, 20.0);
  let mut b = DisplayListBuilder::new(None);
  b.save();
  b.clip_rect(&cut, ClipOperation::Difference);
  b.draw_rect(&rect(34.0, 36.0, 20.0, 20.0), &masked(SIGMA));
  b.restore();
  let list = b.build().expect("a builder always builds a list");
  let ops = list.ops();
  assert_eq!(ops.len(), 4);
  assert_eq!(ops[1], LogOp::ClipRect { rect: cut, op: ClipOperation::Difference });
  assert!(matches!(ops[2], LogOp::DrawRect { .. }));
  assert!(ops[2].paint().expect("a draw has a paint").mask_filter);
}

#[test]
fn a_shadow_keeps_its_sigma_in_local_units_under_a_scale() {
  let mut b = DisplayListBuilder::new(None);
  b.scale(SCALE, SCALE);
  b.draw_rect(&rect(10.0, 10.0, 10.0, 10.0), &masked(SIGMA));
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::Shadow]);
  let i = &out.instances[0];
  assert_eq!(i.offset_kind_stroke[3], SIGMA);
  let reach = SHADOW_REACH_SIGMAS * SIGMA;
  assert!(approx(i.rect, [10.0 - reach, 10.0 - reach, 10.0 + 2.0 * reach, 10.0 + 2.0 * reach]), "{:?}", i.rect);
  assert!(approx(i.linear, [SCALE, 0.0, 0.0, SCALE]), "{:?}", i.linear);

  let mut b = DisplayListBuilder::new(None);
  b.scale(SCALE, SCALE);
  b.draw_oval(&rect(10.0, 10.0, 10.0, 10.0), &masked(SIGMA));
  let out = walked(b);
  let blur = out.layers[0].blur.expect("the masked oval's layer blurs");
  assert!((blur.sigma_x - SCALE * SIGMA).abs() < EPS && (blur.sigma_y - SCALE * SIGMA).abs() < EPS, "{blur:?}");
}

#[test]
fn a_masked_rect_under_a_fixed_function_blend_is_a_layer_of_that_mode() {
  let mut paint = masked(SIGMA);
  paint.set_blend_mode(BlendMode::Plus);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &paint);
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Fill, BatchProgram::LayerEnd]);
  assert_masked_layer(&out);
  assert_eq!(out.layers[0].blend, BlendMode::Plus);
  assert_eq!(out.batches[1].blend, BlendMode::SourceOver);
}
