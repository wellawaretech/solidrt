// Anvil's pure walk (renderer/anvil/walk.rs): a list built through the
// aliases, the way the rendertree builds one, walked into the instances the
// shape program draws, with no GL context. The transform stack, the
// device-space clip, the rejection of draws outside it, nested lists and
// the premultiplied color are all decided here, before any GL work.

use crate::renderer::anvil::walk::{walk, Instance, KIND_RECT};
use crate::renderer::prelude::*;
use crate::renderer::{
  ClipOperation, Color, DisplayList, DisplayListBuilder, DrawStyle, ISize, Paint, Point, Rect, Size,
};

// How far a walked value may sit from the expected one: float noise.
const EPS: f32 = 1e-5;

// The target every test walks into, in device pixels.
const TARGET: f32 = 100.0;

// The instances a stroke op draws: none until step 4 draws strokes, one
// from then on.
const STROKE_INSTANCES: usize = 0;

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

fn approx(got: [f32; 4], want: [f32; 4]) -> bool {
  got.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < EPS)
}

fn build(b: DisplayListBuilder) -> DisplayList {
  b.build().expect("a builder always builds a list")
}

fn walked(b: DisplayListBuilder) -> Vec<Instance> {
  walk(&build(b), ISize::new(TARGET as i64, TARGET as i64))
}

fn offset(instance: &Instance) -> [f32; 2] {
  [instance.offset_kind_stroke[0], instance.offset_kind_stroke[1]]
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
fn stroke_and_fill_records_both_ops() {
  let mut both = opaque();
  both.set_draw_style(DrawStyle::StrokeAndFill).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &both);
  assert_eq!(walked(b).len(), 1 + STROKE_INSTANCES);

  let mut stroke = opaque();
  stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(10.0, 10.0, 20.0, 20.0), &stroke);
  assert_eq!(walked(b).len(), STROKE_INSTANCES);
}
