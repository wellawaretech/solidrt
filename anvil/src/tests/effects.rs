// A layer's effects (renderer/walk.rs `open_layer`, `close_layer`,
// `blended`; renderer/blur.rs): a backdrop layer's rect, blur and color
// matrix, a `save_layer` blur's reach, a draw's color filter as an
// implicit layer, the gaussian weights, the downsample levels and the pass
// count, the color matrix as the composite's uniforms, and the backdrop
// fill's and the clamp's rects (renderer/exec.rs). No GL context.

use crate::prelude::*;
use crate::renderer::blend::BLEND_SOURCE_OVER;
use crate::renderer::blur::{blur_levels, blur_passes, gaussian_weights, halved, DeviceBlur, BLUR_SIGMA_MIN};
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{walk, BatchProgram, Walked};
use crate::renderer::{clamp_rect, fill_rects, matrix_uniforms, report, Frame, COMPOSITE_BLEND};
use crate::{BlendMode, ClipOperation, Color, ColorFilter, ColorMatrix, DisplayListBuilder, ISize, ImageFilter};
use crate::{Matrix, Paint, Point, Rect, Size, TileMode};
use euclid::default::{Box2D, Point2D};

// How far a computed value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// How far the gaussian's weights may sum from 1.
const SUM_EPS: f32 = 1e-6;

// The target every walk here draws into, in device pixels.
const TARGET: f32 = 100.0;

// The alpha the backdrop case fades by.
const HALF: f32 = 0.5;

// The local sigma of the blur cases.
const SIGMA: f32 = 1.5;

// The local sigma of the decal and clamp layer cases: a reach of 6.
const LAYER_SIGMA: f32 = 2.0;

// The backdrop case's map: a translation, then a scale apart per axis.
const SHIFT: f32 = 10.0;
const SCALE_X: f32 = 2.0;
const SCALE_Y: f32 = 3.0;

// The brightness and the offset of the known matrix.
const BRIGHTNESS: f32 = 2.0;
const OFFSET: f32 = 0.25;

// An off-diagonal coefficient of the known matrix: green into red.
const GREEN_INTO_RED: f32 = 0.5;

// A sigma below BLUR_SIGMA_MIN, as the layer's local units give it.
const TINY_SIGMA: f32 = 0.01;

// The rotation of the uniform-sigma case, in degrees.
const DEGREES: f32 = 45.0;

// The perspective coefficient (m14) of the behind-the-eye case: w' = 1 -
// x / 10, at or below zero from x = 10 on.
const PERSPECTIVE: f32 = -0.1;

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

// Brightness BRIGHTNESS on the color channels, plus OFFSET, alpha kept,
// and GREEN_INTO_RED of green into red.
fn known_matrix() -> ColorMatrix {
  #[rustfmt::skip]
  let m = [
    BRIGHTNESS, GREEN_INTO_RED, 0.0, 0.0, OFFSET,
    0.0, BRIGHTNESS, 0.0, 0.0, OFFSET,
    0.0, 0.0, BRIGHTNESS, 0.0, OFFSET,
    0.0, 0.0, 0.0, 1.0, 0.0,
  ];
  ColorMatrix { m }
}

fn opaque() -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
  paint
}

// A layer paint with a gaussian blur of `sigma` tiled by `tile`.
fn blurring(sigma: f32, tile: TileMode) -> Paint {
  let mut paint = Paint::default();
  paint.set_image_filter(&ImageFilter::gaussian_blur(sigma, sigma, tile));
  paint
}

fn near(a: f32, b: f32) -> bool {
  (a - b).abs() < EPS
}

#[test]
fn a_backdrop_layer_takes_its_bounds_under_the_map_and_the_clip() {
  let mut b = DisplayListBuilder::new(None);
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF)).set_color_filter(&ColorFilter::matrix(&known_matrix()));
  let blur = ImageFilter::gaussian_blur(SIGMA, SIGMA, TileMode::Clamp);
  // Device clip (10, 10) to (60, 60); the bounds map to (20, 25) to (60, 85).
  b.translate(SHIFT, SHIFT).clip_rect(&rect(0.0, 0.0, 50.0, 50.0), ClipOperation::Intersect);
  b.scale(SCALE_X, SCALE_Y);
  b.save_backdrop_layer(&rect(5.0, 5.0, 20.0, 20.0), Some(&paint), Some(&blur)).restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::LayerEnd]);
  assert_eq!(out.layers.len(), 1);
  let layer = &out.layers[0];
  assert!(layer.backdrop && !layer.folded);
  assert_eq!(layer.rect, ibox(20, 25, 60, 60));
  let device = layer.blur.expect("the backdrop carries its blur");
  assert!(near(device.sigma_x, SIGMA * SCALE_X) && near(device.sigma_y, SIGMA * SCALE_Y), "{device:?}");
  assert_eq!(device.tile, TileMode::Clamp);
  assert_eq!(layer.matrix, Some(known_matrix()));
  assert_eq!(layer.blend, BlendMode::SourceOver);
  assert!(layer.composite.color.iter().all(|c| near(*c, HALF)), "{:?}", layer.composite.color);
  let r = report(&out);
  assert_eq!(r.layers, 1);
  assert_eq!(r.blur_passes, blur_passes(SIGMA * SCALE_X, SIGMA * SCALE_Y, true));
  // The fill copy, a halving (4.5 > 4), and a pass per axis.
  assert_eq!(r.blur_passes, 4);
}

#[test]
fn a_backdrop_layer_with_no_effect_is_a_plain_save() {
  let mut b = DisplayListBuilder::new(None);
  b.save_backdrop_layer(&rect(10.0, 10.0, 20.0, 20.0), None, None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque()).restore();
  let out = walked(b);
  assert!(out.layers.is_empty());
  assert_eq!(programs(&out), vec![BatchProgram::Fill]);
}

#[test]
fn a_blur_inflates_the_layer_by_its_reach() {
  let draw = |tile| {
    let mut b = DisplayListBuilder::new(None);
    // The clip cuts the inflated rect's right side at x = 52.
    b.clip_rect(&rect(0.0, 0.0, 52.0, TARGET), ClipOperation::Intersect);
    b.save_layer(&Rect::zero(), Some(&blurring(LAYER_SIGMA, tile)));
    b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &opaque()).restore();
    walked(b)
  };
  let decal = draw(TileMode::Decal);
  assert_eq!(programs(&decal), vec![BatchProgram::LayerBegin, BatchProgram::Fill, BatchProgram::LayerEnd]);
  let layer = &decal.layers[0];
  // ceil(3 * 2) = 6 per side, cut to the clip.
  assert_eq!(layer.rect, ibox(24, 24, 52, 56));
  assert!(!layer.folded && !layer.backdrop);
  assert_eq!(layer.blur.map(|b| b.tile), Some(TileMode::Decal));
  assert_eq!(layer.clamp, None);
  // No halving at sigma 2, a pass per axis.
  assert_eq!(report(&decal).blur_passes, 2);
  // A clamp blur is inflated alike and clamps its first pass to the content.
  let clamp = draw(TileMode::Clamp);
  assert_eq!(clamp.layers[0].rect, ibox(24, 24, 52, 56));
  assert_eq!(clamp.layers[0].clamp, Some(ibox(30, 30, 50, 50)));
  assert!(!clamp.layers[0].folded);
}

#[test]
fn a_blur_below_the_minimum_sigma_draws_no_blur() {
  let draw = |alpha| {
    let mut paint = blurring(TINY_SIGMA, TileMode::Decal);
    paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, alpha));
    let mut b = DisplayListBuilder::new(None);
    b.save_layer(&Rect::zero(), Some(&paint));
    // Two overlapping rects, so an opacity layer cannot fold.
    b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &opaque()).draw_rect(&rect(20.0, 20.0, 20.0, 20.0), &opaque());
    b.restore();
    walked(b)
  };
  let opaque_group = draw(1.0);
  assert!(opaque_group.layers.is_empty());
  assert_eq!(programs(&opaque_group), vec![BatchProgram::Fill]);
  let faded = draw(HALF);
  assert_eq!(faded.layers.len(), 1);
  let layer = &faded.layers[0];
  assert!(layer.blur.is_none() && !layer.folded);
  assert_eq!(layer.rect, ibox(10, 10, 40, 40), "no reach without a blur");
  assert_eq!(report(&faded).blur_passes, 0);
}

#[test]
fn a_backdrop_behind_the_eye_takes_the_clip_box() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(10.0, 10.0, 50.0, 50.0), ClipOperation::Intersect);
  let mut m = Matrix::identity();
  m.m14 = PERSPECTIVE;
  b.transform(&m);
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF));
  b.save_backdrop_layer(&rect(0.0, 0.0, 50.0, 50.0), Some(&paint), None).restore();
  let out = walked(b);
  assert_eq!(out.layers.len(), 1);
  assert!(out.layers[0].backdrop);
  assert_eq!(out.layers[0].rect, ibox(10, 10, 60, 60));
  assert_eq!(report(&out).blur_passes, 1, "the fill copy alone");
}

#[test]
fn a_backdrop_layer_unions_its_content() {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF));
  let mut b = DisplayListBuilder::new(None);
  b.save_backdrop_layer(&rect(10.0, 10.0, 20.0, 20.0), Some(&paint), None);
  b.draw_rect(&rect(40.0, 40.0, 10.0, 10.0), &opaque()).restore();
  let out = walked(b);
  assert_eq!(programs(&out), vec![BatchProgram::LayerBegin, BatchProgram::Fill, BatchProgram::LayerEnd]);
  assert_eq!(out.layers[0].rect, ibox(10, 10, 50, 50));
  assert!(!out.layers[0].folded);
}

#[test]
fn a_rotated_blur_keeps_a_uniform_sigma() {
  let c = TARGET / 2.0;
  let rotation = Matrix::translation(-c, -c, 0.0)
    .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(DEGREES)))
    .then(&Matrix::translation(c, c, 0.0));
  let mut b = DisplayListBuilder::new(None);
  b.transform(&rotation);
  b.save_layer(&Rect::zero(), Some(&blurring(LAYER_SIGMA, TileMode::Decal)));
  b.draw_rect(&rect(40.0, 40.0, 20.0, 20.0), &opaque()).restore();
  let out = walked(b);
  let blur = out.layers[0].blur.expect("the layer carries its blur");
  assert!(near(blur.sigma_x, LAYER_SIGMA) && near(blur.sigma_y, LAYER_SIGMA), "{blur:?}");
}

#[test]
fn the_clamp_rect_lands_in_the_storage_window() {
  // A layer at (24, 24), 32 high; its content (30, 30) to (50, 50) sits 6
  // in from every side, y up.
  assert_eq!(clamp_rect(ibox(24, 24, 52, 56), ibox(30, 30, 50, 50)), [6, 6, 26, 26]);
  // Off-center: the content's top row is the window's highest.
  assert_eq!(clamp_rect(ibox(0, 0, 40, 40), ibox(0, 0, 10, 10)), [0, 30, 10, 40]);
}

#[test]
fn a_draw_with_a_color_filter_draws_in_an_implicit_layer() {
  let draw = |mode| {
    let mut paint = opaque();
    paint.set_color_filter(&ColorFilter::matrix(&known_matrix())).set_blend_mode(mode);
    let mut b = DisplayListBuilder::new(None);
    b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint);
    walked(b)
  };
  let over = draw(BlendMode::SourceOver);
  assert_eq!(programs(&over), vec![BatchProgram::LayerBegin, BatchProgram::Fill, BatchProgram::LayerEnd]);
  let layer = &over.layers[0];
  assert_eq!(layer.matrix, Some(known_matrix()));
  assert_eq!(layer.blend, BlendMode::SourceOver);
  assert_eq!(layer.rect, ibox(10, 10, 30, 30));
  assert!(!layer.folded && layer.blur.is_none() && !layer.backdrop);
  // A fixed-function mode rides on the same one layer, its composite's.
  let plus = draw(BlendMode::Plus);
  assert_eq!(plus.layers.len(), 1);
  assert_eq!(plus.layers[0].blend, BlendMode::Plus);
  assert!(plus.batches.iter().all(|b| b.blend == BlendMode::SourceOver));
  // And so does a shader mode.
  let multiply = draw(BlendMode::Multiply);
  assert_eq!(multiply.layers.len(), 1);
  assert_eq!(multiply.layers[0].blend, BlendMode::Multiply);
  assert_eq!(multiply.layers[0].matrix, Some(known_matrix()));
}

#[test]
fn the_gaussian_weights_are_a_normalized_symmetric_kernel() {
  let weights = gaussian_weights(1.0);
  assert_eq!(weights.len(), 7, "radius 3 at sigma 1");
  let sum: f32 = weights.iter().sum();
  assert!((sum - 1.0).abs() < SUM_EPS, "{sum}");
  for k in 0..weights.len() {
    assert!(near(weights[k], weights[weights.len() - 1 - k]));
  }
  assert!(weights.windows(2).take(3).all(|w| w[0] < w[1]), "rising to the center: {weights:?}");
  let wide = gaussian_weights(4.0);
  assert_eq!(wide.len(), 25);
  assert!((wide.iter().sum::<f32>() - 1.0).abs() < SUM_EPS);
  // Past BLUR_SIGMA_MAX the radius is capped at BLUR_RADIUS_MAX.
  let capped = gaussian_weights(4.5);
  assert_eq!(capped.len(), 25);
  assert!((capped.iter().sum::<f32>() - 1.0).abs() < SUM_EPS);
  assert_eq!(gaussian_weights(BLUR_SIGMA_MIN * 0.5), vec![1.0]);
}

#[test]
fn a_large_blur_halves_its_content_first() {
  assert_eq!(blur_levels(4.0, 0.0), 0);
  assert_eq!(blur_levels(5.0, 0.0), 1);
  assert_eq!(blur_levels(20.0, 0.0), 3);
  assert_eq!(blur_levels(1.0, 20.0), 3, "the larger sigma decides");
  // Each halving, then a pass per axis whose halved sigma still counts.
  assert_eq!(blur_passes(4.0, 4.0, false), 2);
  assert_eq!(blur_passes(4.0, 0.0, false), 1);
  assert_eq!(blur_passes(5.0, 5.0, false), 3);
  assert_eq!(blur_passes(20.0, BLUR_SIGMA_MIN, false), 4, "the y sigma falls below the minimum after the halvings");
  assert_eq!(blur_passes(BLUR_SIGMA_MIN * 0.5, 0.0, false), 0);
  // A backdrop's fill copy counts one more, blur or not.
  assert_eq!(blur_passes(5.0, 5.0, true), 4);
  assert_eq!(blur_passes(0.0, 0.0, true), 1);
  assert_eq!(halved((65, 3), 1), (33, 2));
  assert_eq!(halved((65, 3), 3), (9, 1));
}

#[test]
fn the_color_matrix_splits_into_the_composite_uniforms() {
  let (columns, offset) = matrix_uniforms(&known_matrix());
  // Column-major: entry (row i, column j) at j * 4 + i.
  for (i, want) in [BRIGHTNESS, BRIGHTNESS, BRIGHTNESS, 1.0].into_iter().enumerate() {
    assert_eq!(columns[i * 4 + i], want);
  }
  assert_eq!(columns[4], GREEN_INTO_RED, "green's column, red's row");
  assert_eq!(columns[1], 0.0);
  assert_eq!(offset, [OFFSET, OFFSET, OFFSET, 0.0]);
  // The composite's identity mode is SourceOver's discriminant in both
  // languages.
  assert_eq!(BlendMode::SourceOver as i32, BLEND_SOURCE_OVER);
  assert!(COMPOSITE_BLEND.contains(&format!("const int BLEND_SOURCE_OVER = {BLEND_SOURCE_OVER};")));
}

#[test]
fn a_mirror_or_repeat_blur_clamps() {
  for tile in [TileMode::Mirror, TileMode::Repeat] {
    let mut b = DisplayListBuilder::new(None);
    b.save_layer(&Rect::zero(), Some(&blurring(LAYER_SIGMA, tile)));
    b.draw_rect(&rect(30.0, 30.0, 20.0, 20.0), &opaque()).restore();
    let out = walked(b);
    let blur: Option<DeviceBlur> = out.layers[0].blur;
    assert_eq!(blur.map(|b| b.tile), Some(TileMode::Clamp));
    assert_eq!(out.layers[0].rect, ibox(24, 24, 56, 56), "inflated by the reach");
    assert_eq!(out.layers[0].clamp, Some(ibox(30, 30, 50, 50)));
  }
}

#[test]
fn the_backdrop_fill_is_a_translation_into_the_storage_corner() {
  let rect = ibox(20, 25, 60, 60);
  let root = Frame { fbo: None, origin: (0, 0), size: (TARGET as i32, TARGET as i32), root: true };
  // Window row 100 - 60 = 40 is the rect's bottom edge.
  assert_eq!(fill_rects(&root, rect), ([20, 40, 60, 75], [0, 0, 40, 35]));
  let layer = Frame { fbo: None, origin: (10, 10), size: (70, 70), root: false };
  // Inside a layer at (10, 10): x - 10, and 70 - (60 - 10) = 20.
  assert_eq!(fill_rects(&layer, rect), ([10, 20, 50, 55], [0, 0, 40, 35]));
}
