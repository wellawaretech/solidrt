// The stencil clips (renderer/walk.rs `clip`): which clips take the
// device box, which the per-instance rounded clip and which the stencil;
// the raise, lower and restore draws and the stencil depth every batch
// tests against, with no GL context.

use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::report;
use crate::renderer::walk::{walk, BatchProgram, ClipDraw, ClipShape, Instance, Projective, Walked, KIND_RECT};
use crate::{ClipOperation, Color, DisplayList, DisplayListBuilder, ISize, Matrix, Paint, Point, Rect};
use crate::{RoundingRadii, Size};
use euclid::default::Box2D;
use std::sync::Arc;

// How far a walked value may sit from the expected one, for points mapped
// through a rotation at a magnitude of tens of pixels.
const EPS: f32 = 1e-3;

// The target every test walks into, in device pixels.
const TARGET: f32 = 100.0;

// The rotation the rotated clips are under, about the target's center.
const DEGREES: f32 = 30.0;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn opaque() -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
  paint
}

fn build(b: DisplayListBuilder) -> DisplayList {
  b.build().expect("a builder always builds a list")
}

fn walked_list(list: &DisplayList) -> Walked {
  walk(list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
}

fn walked(b: DisplayListBuilder) -> Walked {
  walked_list(&build(b))
}

fn approx(got: [f32; 4], want: [f32; 4]) -> bool {
  got.iter().zip(want.iter()).all(|(a, b)| (a - b).abs() < EPS)
}

fn box_of(b: &Box2D<f32>) -> [f32; 4] {
  [b.min.x, b.min.y, b.max.x, b.max.y]
}

// The rotation by DEGREES about the target's center.
fn rotated() -> Matrix {
  let c = TARGET / 2.0;
  Matrix::translation(-c, -c, 0.0)
    .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(DEGREES)))
    .then(&Matrix::translation(c, c, 0.0))
}

// The square the rotated cases clip to, centered on the target.
fn square() -> Rect {
  rect(30.0, 30.0, 40.0, 40.0)
}

// A full-target rect: a draw the clip box does not reject.
fn cover() -> Rect {
  rect(0.0, 0.0, TARGET, TARGET)
}

// The programs and stencil depths of a walk's batches, in order.
fn programs(out: &Walked) -> Vec<(BatchProgram, u8)> {
  out.batches.iter().map(|b| (b.program, b.stencil)).collect()
}

// The quad a clip draw raises or lowers; panics on a mesh.
fn quad(shape: &ClipShape) -> &Instance {
  match shape {
    ClipShape::Quad(quad) => quad,
    ClipShape::Mesh(_) => panic!("want a quad, got a mesh"),
  }
}

// A quad's rect as device x0, y0, x1, y1 under the identity map.
fn device_rect(quad: &Instance) -> [f32; 4] {
  assert_eq!(quad.linear, [1.0, 0.0, 0.0, 1.0], "a device quad is under the identity map");
  assert_eq!([quad.offset_kind_stroke[0], quad.offset_kind_stroke[1]], [0.0, 0.0]);
  let [x, y, w, h] = quad.rect;
  [x, y, x + w, y + h]
}

#[test]
fn a_rotated_rect_clip_raises_its_quad_and_restores_it() {
  let mut b = DisplayListBuilder::new(None);
  b.save()
    .transform(&rotated())
    .clip_rect(&square(), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque())
    .restore()
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert_eq!(
    programs(&out),
    vec![(BatchProgram::Clip, 0), (BatchProgram::Fill, 1), (BatchProgram::Clip, 0), (BatchProgram::Fill, 0)]
  );
  let ClipDraw::Raise { reference: 0, shape } = &out.clip_draws[0] else { panic!("{:?}", out.clip_draws[0]) };
  let raised = quad(shape);
  // The rect under the full rotation: a plain rect fill instance.
  assert_eq!(raised.rect, [30.0, 30.0, 40.0, 40.0]);
  assert_eq!(raised.offset_kind_stroke[2], KIND_RECT);
  let p = Projective::from_matrix(&rotated());
  assert_eq!(raised.linear, p.linear);
  // The box narrows to the rotated square's bounds.
  let bounds = box_of(&p.bounds(&square()).expect("in front of the eye"));
  assert!(approx(out.instances[0].clip, bounds), "{:?}", out.instances[0].clip);
  let ClipDraw::Restore { to: 0, area } = &out.clip_draws[1] else { panic!("{:?}", out.clip_draws[1]) };
  assert!(approx(device_rect(area), bounds), "{:?}", area.rect);
  // After the restore the box is the target again.
  assert_eq!(out.instances[1].clip, [0.0, 0.0, TARGET, TARGET]);
}

#[test]
fn a_rotated_rounded_clip_raises_a_retained_mesh() {
  let mut b = DisplayListBuilder::new(None);
  b.transform(&rotated())
    .clip_rounded_rect(&square(), &RoundingRadii::circular([8.0; 4]), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque());
  let list = build(b);
  let out = walked_list(&list);
  assert_eq!(programs(&out), vec![(BatchProgram::Clip, 0), (BatchProgram::Fill, 1)]);
  let ClipDraw::Raise { reference: 0, shape: ClipShape::Mesh(draw) } = &out.clip_draws[0] else {
    panic!("{:?}", out.clip_draws[0])
  };
  assert_eq!(draw.map, Projective::from_matrix(&rotated()));
  assert!(draw.gradient.is_none() && draw.texture.is_none() && draw.rounded.is_none());
  // The mesh is the rounded rect in its own space.
  let bounds = draw.mesh.bounds;
  assert!(approx(box_of(&bounds), [30.0, 30.0, 70.0, 70.0]), "{bounds:?}");
  assert_eq!(out.tessellated, 1);
  // No per-instance rounded clip: the stencil does it.
  assert_eq!(out.instances[0].rclip_radii, [0.0; 4]);

  // The same list again: the op's path hands out the same mesh, from its
  // cache.
  let again = walked_list(&list);
  assert_eq!(again.tessellated, 0);
  let ClipDraw::Raise { shape: ClipShape::Mesh(second), .. } = &again.clip_draws[0] else {
    panic!("{:?}", again.clip_draws[0])
  };
  assert!(Arc::ptr_eq(&draw.mesh, &second.mesh), "the retained mesh is handed out again");
}

#[test]
fn an_oval_clip_raises_its_mesh_and_narrows_the_box() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_oval(&rect(20.0, 10.0, 40.0, 60.0), ClipOperation::Intersect).draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![(BatchProgram::Clip, 0), (BatchProgram::Fill, 1)]);
  let ClipDraw::Raise { reference: 0, shape: ClipShape::Mesh(draw) } = &out.clip_draws[0] else {
    panic!("{:?}", out.clip_draws[0])
  };
  let bounds = draw.mesh.bounds;
  assert!(approx(box_of(&bounds), [20.0, 10.0, 60.0, 70.0]), "{bounds:?}");
  assert_eq!(out.instances[0].clip, [20.0, 10.0, 60.0, 70.0]);
}

#[test]
fn a_difference_clip_raises_the_box_and_lowers_its_shape() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(10.0, 10.0, 80.0, 60.0), ClipOperation::Intersect)
    .save()
    .clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Difference)
    .draw_rect(&cover(), &opaque())
    .restore();
  let out = walked(b);
  assert_eq!(
    programs(&out),
    vec![(BatchProgram::Clip, 0), (BatchProgram::Clip, 0), (BatchProgram::Fill, 1), (BatchProgram::Clip, 0)]
  );
  let the_box = [10.0, 10.0, 90.0, 70.0];
  let ClipDraw::Raise { reference: 0, shape } = &out.clip_draws[0] else { panic!("{:?}", out.clip_draws[0]) };
  assert_eq!(device_rect(quad(shape)), the_box);
  let ClipDraw::Lower { reference: 1, shape } = &out.clip_draws[1] else { panic!("{:?}", out.clip_draws[1]) };
  assert_eq!(quad(shape).rect, [30.0, 30.0, 20.0, 20.0]);
  // A difference never narrows the box.
  assert_eq!(out.instances[0].clip, the_box);
  let ClipDraw::Restore { to: 0, area } = &out.clip_draws[2] else { panic!("{:?}", out.clip_draws[2]) };
  assert_eq!(device_rect(area), the_box);
}

#[test]
fn nested_stencil_clips_count_up_and_restore_step_by_step() {
  let mut b = DisplayListBuilder::new(None);
  b.save()
    .transform(&rotated())
    .clip_rect(&square(), ClipOperation::Intersect)
    .save()
    .clip_oval(&square(), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque())
    .restore()
    .draw_rect(&cover(), &opaque())
    .restore()
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert_eq!(
    programs(&out),
    vec![
      (BatchProgram::Clip, 0),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 2),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 1),
      (BatchProgram::Clip, 0),
      (BatchProgram::Fill, 0),
    ]
  );
  assert!(matches!(out.clip_draws[0], ClipDraw::Raise { reference: 0, .. }));
  assert!(matches!(out.clip_draws[1], ClipDraw::Raise { reference: 1, .. }));
  assert!(matches!(out.clip_draws[2], ClipDraw::Restore { to: 1, .. }));
  assert!(matches!(out.clip_draws[3], ClipDraw::Restore { to: 0, .. }));
}

#[test]
fn a_nested_list_that_leaves_a_stencil_clip_is_restored_at_its_end() {
  let mut inner = DisplayListBuilder::new(None);
  inner.clip_oval(&square(), ClipOperation::Intersect).draw_rect(&cover(), &opaque());
  let inner = build(inner);
  let mut b = DisplayListBuilder::new(None);
  b.draw_display_list(&inner, 1.0).draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert_eq!(
    programs(&out),
    vec![(BatchProgram::Clip, 0), (BatchProgram::Fill, 1), (BatchProgram::Clip, 0), (BatchProgram::Fill, 0)]
  );
  let ClipDraw::Restore { to: 0, area } = &out.clip_draws[1] else { panic!("{:?}", out.clip_draws[1]) };
  assert_eq!(device_rect(area), [30.0, 30.0, 70.0, 70.0]);
}

#[test]
fn a_restore_covers_a_raise_that_reached_past_the_box() {
  // An outer stencil clip at depth 1, an inner rotated rect reaching past
  // the outer box: the inner's raise is not held to the box, so its
  // restore covers the inner's whole bounds, not the narrowed box.
  let mut b = DisplayListBuilder::new(None);
  b.clip_oval(&rect(0.0, 0.0, 60.0, TARGET), ClipOperation::Intersect)
    .save()
    .transform(&rotated())
    .clip_rect(&square(), ClipOperation::Intersect)
    .restore()
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  let ClipDraw::Restore { to: 1, area } = &out.clip_draws[2] else { panic!("{:?}", out.clip_draws[2]) };
  let bounds = box_of(&Projective::from_matrix(&rotated()).bounds(&square()).expect("in front of the eye"));
  let union = [0.0, bounds[1].min(0.0), bounds[2].max(60.0), TARGET];
  assert!(approx(device_rect(area), union), "{:?} {bounds:?}", area.rect);
  assert!(bounds[2] > 60.0, "the inner raise reaches past the outer box");
}

#[test]
fn a_draw_under_a_stencil_clip_outside_its_box_is_culled() {
  let mut b = DisplayListBuilder::new(None);
  b.transform(&rotated())
    .clip_rect(&square(), ClipOperation::Intersect)
    .draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &opaque());
  let out = walked(b);
  assert!(out.instances.is_empty());
  assert_eq!(programs(&out), vec![(BatchProgram::Clip, 0)]);
}

#[test]
fn an_axis_aligned_rounded_clip_rides_on_the_instance() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&square(), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert!(out.clip_draws.is_empty());
  assert_eq!(programs(&out), vec![(BatchProgram::Shape, 0)]);
  assert_eq!(out.instances[0].rclip_radii, [5.0; 4]);
}

#[test]
fn a_rounded_clip_inside_another_goes_through_the_stencil() {
  let mut b = DisplayListBuilder::new(None);
  b.clip_rounded_rect(&rect(10.0, 10.0, 60.0, 60.0), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect)
    .clip_rounded_rect(&rect(20.0, 20.0, 20.0, 20.0), &RoundingRadii::circular([2.0; 4]), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert_eq!(programs(&out), vec![(BatchProgram::Clip, 0), (BatchProgram::Shape, 1)]);
  assert!(matches!(out.clip_draws[0], ClipDraw::Raise { reference: 0, shape: ClipShape::Mesh(_) }));
  let i = &out.instances[0];
  // The box is the inner rect; the per-instance clip stays the outer one.
  assert_eq!(i.clip, [20.0, 20.0, 40.0, 40.0]);
  assert_eq!(i.rclip, [10.0, 10.0, 70.0, 70.0]);
  assert_eq!(i.rclip_radii, [5.0; 4]);
}

#[test]
fn a_rounded_clip_under_a_non_uniform_scale_goes_through_the_stencil() {
  let mut b = DisplayListBuilder::new(None);
  b.scale(2.0, 1.0)
    .clip_rounded_rect(&rect(10.0, 10.0, 20.0, 20.0), &RoundingRadii::circular([4.0; 4]), ClipOperation::Intersect)
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  assert!(matches!(out.clip_draws[0], ClipDraw::Raise { reference: 0, shape: ClipShape::Mesh(_) }));
  assert_eq!(out.instances[0].rclip_radii, [0.0; 4]);
  assert_eq!(out.instances[0].clip, [20.0, 10.0, 60.0, 30.0]);
}

#[test]
fn a_clip_with_a_corner_behind_the_eye_is_raised_and_leaves_the_box() {
  // A quarter turn under a close eye: w' = 1 + x / 10, behind the eye for
  // x below -10.
  #[rustfmt::skip]
  let flip = Matrix::new(
    0.0, 0.0, -1.0, 0.0,
    0.0, 1.0, 0.0,  0.0,
    1.0, 0.0, 0.0,  0.0,
    0.0, 0.0, 0.0,  1.0,
  )
  .then(&Matrix::perspective(10.0));
  let clipped = rect(-300.0, 0.0, 250.0, 40.0);
  assert!(Projective::from_matrix(&flip).bounds(&clipped).is_none());
  let mut b = DisplayListBuilder::new(None);
  b.transform(&flip).clip_rect(&clipped, ClipOperation::Intersect).draw_rect(&clipped, &opaque());
  let out = walked(b);
  assert!(matches!(out.clip_draws[0], ClipDraw::Raise { reference: 0, shape: ClipShape::Quad(_) }));
  assert_eq!(programs(&out), vec![(BatchProgram::Clip, 0), (BatchProgram::Fill, 1)]);
  assert_eq!(out.instances[0].clip, [0.0, 0.0, TARGET, TARGET], "the box is left as it was");
}

#[test]
fn the_draw_report_counts_the_clip_draws() {
  let mut b = DisplayListBuilder::new(None);
  b.save()
    .clip_oval(&square(), ClipOperation::Difference)
    .draw_rect(&cover(), &opaque())
    .restore()
    .draw_rect(&cover(), &opaque());
  let out = walked(b);
  let r = report(&out);
  // A raise of the box, a lower of the oval, a restore; two fill batches.
  assert_eq!((r.clips, r.draw_calls, r.instances), (3, 5, 2));
}
