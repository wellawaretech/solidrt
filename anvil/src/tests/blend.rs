// Blend modes (renderer/blend.rs, and the walk's `blended`): the fifteen
// fixed-function modes as a batch key with their factor pairs, the fourteen
// shader modes as an implicit layer around the op, Destination as no draw,
// the modes that clear under a zero source drawing a transparent op; and the
// destination copy's read rect (renderer/exec.rs `blit_rect`). No GL
// context.

use crate::oplog::Op as LogOp;
use crate::prelude::*;
use crate::renderer::blend::{gl_factors, keeps_destination, shader_mode};
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{walk, BatchProgram, Walked};
use crate::renderer::{blit_rect, report, test_texture, Frame, COMPOSITE_BLEND};
use crate::{BlendMode, Color, DisplayListBuilder, FillType, ISize, Paint, PathBuilder, Point, Rect, RoundingRadii};
use crate::{ClipOperation, DrawStyle, Size, TextureSampling};
use euclid::default::{Box2D, Point2D};

// The target every walk here draws into, in device pixels.
const TARGET: f32 = 100.0;

// The texture the texture quad case samples: its GL name and side.
const TEXTURE_NAME: u32 = 9;
const TEXTURE_SIDE: i64 = 16;

// The group alpha of the opacity layer case.
const HALF: f32 = 0.5;

// The stroke width of the stroked rect case.
const STROKE: f32 = 4.0;

// A rounded rect's corner radius, and its side: large enough that a
// source-over fill of it splits (renderer/walk.rs ROUNDED_SPLIT_AREA).
const RADIUS: f32 = 8.0;
const LARGE: f32 = 90.0;

// Every mode, in Skia's order.
const ALL: [BlendMode; 29] = [
  BlendMode::Clear,
  BlendMode::Source,
  BlendMode::Destination,
  BlendMode::SourceOver,
  BlendMode::DestinationOver,
  BlendMode::SourceIn,
  BlendMode::DestinationIn,
  BlendMode::SourceOut,
  BlendMode::DestinationOut,
  BlendMode::SourceATop,
  BlendMode::DestinationATop,
  BlendMode::Xor,
  BlendMode::Plus,
  BlendMode::Modulate,
  BlendMode::Screen,
  BlendMode::Overlay,
  BlendMode::Darken,
  BlendMode::Lighten,
  BlendMode::ColorDodge,
  BlendMode::ColorBurn,
  BlendMode::HardLight,
  BlendMode::SoftLight,
  BlendMode::Difference,
  BlendMode::Exclusion,
  BlendMode::Multiply,
  BlendMode::Hue,
  BlendMode::Saturation,
  BlendMode::Color,
  BlendMode::Luminosity,
];

// The shader modes with the names their GLSL constants carry.
const SHADER_MODES: [(BlendMode, &str); 14] = [
  (BlendMode::Overlay, "BLEND_OVERLAY"),
  (BlendMode::Darken, "BLEND_DARKEN"),
  (BlendMode::Lighten, "BLEND_LIGHTEN"),
  (BlendMode::ColorDodge, "BLEND_COLOR_DODGE"),
  (BlendMode::ColorBurn, "BLEND_COLOR_BURN"),
  (BlendMode::HardLight, "BLEND_HARD_LIGHT"),
  (BlendMode::SoftLight, "BLEND_SOFT_LIGHT"),
  (BlendMode::Difference, "BLEND_DIFFERENCE"),
  (BlendMode::Exclusion, "BLEND_EXCLUSION"),
  (BlendMode::Multiply, "BLEND_MULTIPLY"),
  (BlendMode::Hue, "BLEND_HUE"),
  (BlendMode::Saturation, "BLEND_SATURATION"),
  (BlendMode::Color, "BLEND_COLOR"),
  (BlendMode::Luminosity, "BLEND_LUMINOSITY"),
];

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn ibox(x0: i32, y0: i32, x1: i32, y1: i32) -> Box2D<i32> {
  Box2D::new(Point2D::new(x0, y0), Point2D::new(x1, y1))
}

// An opaque red paint blending by `mode`.
fn paint(mode: BlendMode) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0)).set_blend_mode(mode);
  paint
}

fn walked(b: DisplayListBuilder) -> Walked {
  let list = b.build().expect("a builder always builds a list");
  walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
}

// The programs, counts and blend modes of a walk's batches, in order.
fn batches(out: &Walked) -> Vec<(BatchProgram, usize, BlendMode)> {
  out.batches.iter().map(|b| (b.program, b.count, b.blend)).collect()
}

#[test]
fn the_fixed_function_modes_are_their_factor_pairs() {
  use glow::{
    DST_ALPHA, ONE, ONE_MINUS_DST_ALPHA, ONE_MINUS_SRC_ALPHA, ONE_MINUS_SRC_COLOR, SRC_ALPHA, SRC_COLOR, ZERO,
  };
  let want = [
    Some((ZERO, ZERO)),
    Some((ONE, ZERO)),
    Some((ZERO, ONE)),
    Some((ONE, ONE_MINUS_SRC_ALPHA)),
    Some((ONE_MINUS_DST_ALPHA, ONE)),
    Some((DST_ALPHA, ZERO)),
    Some((ZERO, SRC_ALPHA)),
    Some((ONE_MINUS_DST_ALPHA, ZERO)),
    Some((ZERO, ONE_MINUS_SRC_ALPHA)),
    Some((DST_ALPHA, ONE_MINUS_SRC_ALPHA)),
    Some((ONE_MINUS_DST_ALPHA, SRC_ALPHA)),
    Some((ONE_MINUS_DST_ALPHA, ONE_MINUS_SRC_ALPHA)),
    Some((ONE, ONE)),
    Some((ZERO, SRC_COLOR)),
    Some((ONE, ONE_MINUS_SRC_COLOR)),
  ];
  for (mode, want) in ALL.iter().zip(want.iter().copied().chain(std::iter::repeat(None))) {
    assert_eq!(gl_factors(*mode), want, "{mode:?}");
  }
}

#[test]
fn the_shader_modes_are_their_discriminants_in_rust_and_glsl() {
  for mode in ALL {
    let shader = SHADER_MODES.iter().any(|(m, _)| *m == mode);
    assert_eq!(shader_mode(mode).is_some(), shader, "{mode:?}");
    assert_eq!(gl_factors(mode).is_none(), shader, "{mode:?}");
  }
  for (mode, name) in SHADER_MODES {
    assert_eq!(shader_mode(mode), Some(mode as i32), "{mode:?}");
    let declaration = format!("const int {name} = {};", mode as i32);
    assert!(COMPOSITE_BLEND.contains(&declaration), "{declaration}");
  }
}

#[test]
fn only_the_modes_that_clear_under_a_zero_source_draw_a_transparent_op() {
  let clearing = [
    BlendMode::Clear,
    BlendMode::Source,
    BlendMode::SourceIn,
    BlendMode::DestinationIn,
    BlendMode::SourceOut,
    BlendMode::DestinationATop,
    BlendMode::Modulate,
  ];
  for mode in ALL {
    assert_eq!(keeps_destination(mode), !clearing.contains(&mode), "{mode:?}");
  }
  // A transparent rect draws exactly under a fixed-function mode that clears
  // under a zero source; nothing (no batch, no layer) for Destination, the
  // other fixed-function modes and every shader mode.
  for mode in ALL {
    let mut transparent = paint(mode);
    transparent.set_color(Color::TRANSPARENT);
    let mut b = DisplayListBuilder::new(None);
    b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &transparent);
    let out = walked(b);
    let drawn = gl_factors(mode).is_some() && !keeps_destination(mode);
    assert_eq!(out.instances.len(), drawn as usize, "{mode:?}");
    assert_eq!(out.batches.len(), drawn as usize, "{mode:?}");
    assert!(out.layers.is_empty(), "{mode:?}");
  }
  // A texture quad faded to nothing under Clear still clears.
  let texture = test_texture(TEXTURE_NAME, ISize::new(TEXTURE_SIDE, TEXTURE_SIDE));
  let side = TEXTURE_SIDE as f32;
  let mut transparent = paint(BlendMode::Clear);
  transparent.set_color(Color::TRANSPARENT);
  let mut b = DisplayListBuilder::new(None);
  b.draw_texture_rect(
    &texture,
    &rect(0.0, 0.0, side, side),
    &rect(10.0, 10.0, 20.0, 20.0),
    TextureSampling::Linear,
    Some(&transparent),
  );
  let out = walked(b);
  assert_eq!(batches(&out), [(BatchProgram::Shape, 1, BlendMode::Clear)]);
}

#[test]
fn a_plus_rect_is_a_shape_batch_of_its_own_mode() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Plus));
  b.draw_rect(&rect(15.0, 15.0, 20.0, 20.0), &paint(BlendMode::Plus));
  b.draw_rect(&rect(20.0, 20.0, 20.0, 20.0), &paint(BlendMode::SourceOver));
  let out = walked(b);
  // Not the lean fill program: its zero fragments outside the clip would
  // write under another mode. Two Plus rects share one draw; a source-over
  // rect after them opens its own.
  assert_eq!(
    batches(&out),
    [(BatchProgram::Shape, 2, BlendMode::Plus), (BatchProgram::Fill, 1, BlendMode::SourceOver)]
  );
}

#[test]
fn a_fill_does_not_join_before_a_batch_of_another_mode() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &paint(BlendMode::SourceOver));
  b.draw_rect(&rect(20.0, 20.0, 10.0, 10.0), &paint(BlendMode::Plus));
  b.draw_rect(&rect(50.0, 50.0, 10.0, 10.0), &paint(BlendMode::SourceOver));
  let out = walked(b);
  // The third rect is disjoint from the second and could join the first
  // batch safely; the walk keeps the blend check as a simplification for
  // clarity, not a painter's-order necessity, so it opens its own.
  assert_eq!(
    batches(&out),
    [
      (BatchProgram::Fill, 1, BlendMode::SourceOver),
      (BatchProgram::Shape, 1, BlendMode::Plus),
      (BatchProgram::Fill, 1, BlendMode::SourceOver)
    ]
  );
}

#[test]
fn a_destination_draw_emits_nothing() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Destination));
  b.draw_oval(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Destination));
  b.draw_line(Point::new(0.0, 0.0), Point::new(50.0, 50.0), &paint(BlendMode::Destination));
  let out = walked(b);
  assert!(out.instances.is_empty());
  assert!(out.batches.is_empty());
  assert!(out.layers.is_empty());
}

#[test]
fn a_multiply_rect_draws_in_a_layer_of_its_own() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Multiply));
  let out = walked(b);
  // Inside the layer the rect draws source-over over transparent pixels:
  // the lean fill program is exact there.
  assert_eq!(
    batches(&out),
    [
      (BatchProgram::LayerBegin, 0, BlendMode::SourceOver),
      (BatchProgram::Fill, 1, BlendMode::SourceOver),
      (BatchProgram::LayerEnd, 0, BlendMode::SourceOver)
    ]
  );
  assert_eq!(out.layers.len(), 1);
  let layer = &out.layers[0];
  assert_eq!(layer.blend, BlendMode::Multiply);
  assert!(!layer.folded);
  assert_eq!(layer.rect, ibox(10, 10, 30, 30));
  // The composite does not fade: the alpha is 1.
  assert_eq!(layer.composite.color, [1.0; 4]);
  let r = report(&out);
  assert_eq!((r.layers, r.draw_calls), (1, 2), "the fill batch and the composite");
}

#[test]
fn a_layer_holding_another_fixed_mode_does_not_fold() {
  let mut half = Paint::default();
  half.set_color(Color::new_srgba(0.0, 0.0, 0.0, 0.5));
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&half));
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Clear));
  b.restore();
  let out = walked(b);
  // Folded, the Clear would clear the target where it cleared the layer's
  // transparent pixels.
  assert!(!out.layers[0].folded);
  assert_eq!(out.batches[1].blend, BlendMode::Clear);
}

#[test]
fn a_large_rounded_fill_under_another_mode_does_not_split() {
  let radii = RoundingRadii::circular([RADIUS; 4]);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rounded_rect(&rect(5.0, 5.0, LARGE, LARGE), &radii, &paint(BlendMode::Screen));
  let out = walked(b);
  assert_eq!(batches(&out), [(BatchProgram::Shape, 1, BlendMode::Screen)]);
  assert_eq!(out.instances.len(), 1);
}

#[test]
fn a_path_blends_on_its_mesh_batch_or_in_a_layer() {
  let mut pb = PathBuilder::default();
  pb.move_to(Point::new(10.0, 10.0)).line_to(Point::new(40.0, 10.0)).line_to(Point::new(10.0, 40.0)).close();
  let path = pb.build(FillType::NonZero);
  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&path, &paint(BlendMode::Screen));
  let out = walked(b);
  assert_eq!(batches(&out), [(BatchProgram::Mesh, 1, BlendMode::Screen)]);
  assert_eq!(out.mesh_draws[0].blend, BlendMode::Screen);

  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&path, &paint(BlendMode::Hue));
  let out = walked(b);
  assert_eq!(
    batches(&out),
    [
      (BatchProgram::LayerBegin, 0, BlendMode::SourceOver),
      (BatchProgram::Mesh, 1, BlendMode::SourceOver),
      (BatchProgram::LayerEnd, 0, BlendMode::SourceOver)
    ]
  );
  assert_eq!(out.layers[0].blend, BlendMode::Hue);
  assert_eq!(out.layers[0].rect, ibox(10, 10, 40, 40));
}

#[test]
fn a_texture_quad_takes_its_paints_mode() {
  let texture = test_texture(TEXTURE_NAME, ISize::new(TEXTURE_SIDE, TEXTURE_SIDE));
  let side = TEXTURE_SIDE as f32;
  let mut b = DisplayListBuilder::new(None);
  b.draw_texture_rect(
    &texture,
    &rect(0.0, 0.0, side, side),
    &rect(10.0, 10.0, 20.0, 20.0),
    TextureSampling::Linear,
    Some(&paint(BlendMode::DestinationOut)),
  );
  b.draw_texture_rect(
    &texture,
    &rect(0.0, 0.0, side, side),
    &rect(40.0, 40.0, 20.0, 20.0),
    TextureSampling::Linear,
    None,
  );
  let out = walked(b);
  // Without a paint the quad draws source-over, a batch of its own.
  assert_eq!(
    batches(&out),
    [(BatchProgram::Shape, 1, BlendMode::DestinationOut), (BatchProgram::Shape, 1, BlendMode::SourceOver)]
  );
}

#[test]
fn the_destination_copy_reads_the_layer_rect_in_window_coordinates() {
  // The target: device y down from the top, window y up from the bottom.
  let root = Frame { fbo: None, origin: (0, 0), size: (100, 80), root: true };
  assert_eq!(blit_rect(&root, ibox(10, 20, 40, 30)), (10, 50, 30, 10));
  // A layer frame at device (20, 10), 50 by 40: its device rect at window
  // [0, 50] x [0, 40], its top row at window y 40.
  let nested = Frame { fbo: None, origin: (20, 10), size: (50, 40), root: false };
  assert_eq!(blit_rect(&nested, ibox(30, 15, 45, 25)), (10, 25, 15, 10));
  // The whole frame is its whole window.
  assert_eq!(blit_rect(&nested, ibox(20, 10, 70, 50)), (0, 0, 50, 40));
}

#[test]
fn the_op_log_records_the_blend_mode() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 1.0, 1.0), &paint(BlendMode::Multiply));
  let list = b.build().expect("the list builds");
  let LogOp::DrawRect { paint, .. } = &list.ops()[0] else { panic!("the rect logs as a rect") };
  assert_eq!(paint.blend_mode, BlendMode::Multiply);
}

#[test]
fn a_shader_mode_op_inside_an_opacity_layer_nests_its_layer() {
  let mut half = Paint::default();
  half.set_color(Color::new_srgba(0.0, 0.0, 0.0, HALF));
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&half));
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &paint(BlendMode::Multiply));
  b.restore();
  let out = walked(b);
  let programs: Vec<(BatchProgram, usize)> = out.batches.iter().map(|b| (b.program, b.start)).collect();
  assert_eq!(
    programs,
    [
      (BatchProgram::LayerBegin, 0),
      (BatchProgram::LayerBegin, 1),
      (BatchProgram::Fill, 0),
      (BatchProgram::LayerEnd, 1),
      (BatchProgram::LayerEnd, 0)
    ]
  );
  let (outer, inner) = (&out.layers[0], &out.layers[1]);
  assert!(!outer.folded);
  assert_eq!(outer.blend, BlendMode::SourceOver);
  assert_eq!(inner.blend, BlendMode::Multiply);
  assert!(outer.rect.contains_box(&inner.rect), "{:?} {:?}", outer.rect, inner.rect);
  assert_eq!(out.batches[3].stencil, 0);
}

#[test]
fn a_shader_mode_layer_composites_under_the_clips_in_effect() {
  let radii = RoundingRadii::circular([RADIUS; 4]);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(5.0, 5.0, LARGE, LARGE), &radii, ClipOperation::Intersect);
  // A difference clip always goes through the stencil: depth 1.
  b.clip_rect(&rect(40.0, 40.0, 10.0, 10.0), ClipOperation::Difference);
  b.draw_rect(&rect(10.0, 10.0, 60.0, 60.0), &paint(BlendMode::Darken));
  let out = walked(b);
  let end = out.batches.last().expect("the layer's end marker is last");
  assert_eq!(end.program, BatchProgram::LayerEnd);
  assert_eq!(end.stencil, 1);
  let layer = &out.layers[end.start];
  assert_eq!(layer.stencil, 1);
  assert_eq!(layer.composite.rclip_radii, [RADIUS; 4]);
  assert_eq!(layer.composite.rclip, [5.0, 5.0, 5.0 + LARGE, 5.0 + LARGE]);
  // Inside, the content draws with no stencil and no rounded clip.
  let content = &out.batches[out.batches.len() - 2];
  assert_eq!(content.stencil, 0);
  assert_eq!(out.instances[content.start].rclip_radii, [0.0; 4]);
}

#[test]
fn a_culled_shader_mode_op_opens_no_layer() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 40.0, 40.0), ClipOperation::Intersect);
  b.draw_rect(&rect(50.0, 50.0, 10.0, 10.0), &paint(BlendMode::Multiply));
  let out = walked(b);
  assert!(out.batches.is_empty());
  assert!(out.layers.is_empty());

  // Between two fills it does not split their run.
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 0.0, 40.0, 40.0), ClipOperation::Intersect);
  b.draw_rect(&rect(0.0, 0.0, 10.0, 10.0), &paint(BlendMode::SourceOver));
  b.draw_rect(&rect(50.0, 50.0, 10.0, 10.0), &paint(BlendMode::Multiply));
  b.draw_rect(&rect(20.0, 20.0, 10.0, 10.0), &paint(BlendMode::SourceOver));
  let out = walked(b);
  assert_eq!(batches(&out), [(BatchProgram::Fill, 2, BlendMode::SourceOver)]);
  assert!(out.layers.is_empty());
}

#[test]
fn a_stroked_rect_under_multiply_takes_its_outset_box_as_its_layer() {
  let mut stroked = paint(BlendMode::Multiply);
  stroked.set_draw_style(DrawStyle::Stroke).set_stroke_width(STROKE);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &stroked);
  let out = walked(b);
  assert_eq!(out.layers.len(), 1);
  let half = (STROKE / 2.0) as i32;
  assert_eq!(out.layers[0].rect, ibox(10 - half, 10 - half, 30 + half, 30 + half));
  assert_eq!(out.layers[0].blend, BlendMode::Multiply);
  assert_eq!(batches(&out)[1], (BatchProgram::Shape, 1, BlendMode::SourceOver));
}
