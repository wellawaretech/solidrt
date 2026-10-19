//! The submission budget's planner and gauge (gl::slicing), pure: how a
//! draw is cut into slices, how the slices are sized from the measured
//! rate, the topology stepping, and what a one-slice render's bound does.

use std::time::Instant;

use crate::gl::{
  reads_instance_id, Bound, Issue, RateMeter, SliceAxis, SliceGauge, SlicePlan, SLICE_MIN_WORK,
  SUBMISSION_BUDGET_MICROS,
};
use crate::gpu::vocab::{DrawRange, Topology};

fn range(vertices: i32, instances: i32) -> DrawRange {
  DrawRange { first_vertex: 0, vertex_count: vertices, instance_count: instances }
}

// A rate of one microsecond per vertex: 250k vertices per slice.
const ONE_MICRO: Option<f64> = Some(1.0);
// A rate no draw reaches: every slice wants the whole draw.
const FREE: Option<f64> = Some(1e-12);

#[test]
fn draws_under_the_work_floor_are_never_planned() {
  let under = (SLICE_MIN_WORK - 1) as i32;
  assert!(SlicePlan::plan(range(under, 1), Topology::Triangles, false).is_none());
  assert!(SlicePlan::plan(range(3, under / 3), Topology::Triangles, false).is_none());
  assert!(SlicePlan::plan(range(96, 1024), Topology::Triangles, false).is_some());
}

#[test]
fn an_instanced_draw_slices_over_instances_unless_the_stage_reads_the_id() {
  let plan = SlicePlan::plan(range(96, 49140), Topology::Triangles, false).expect("planned");
  assert_eq!(plan.axis, SliceAxis::Instances);
  assert_eq!((plan.units, plan.unit_work, plan.step, plan.overlap), (49140, 96, 1, 0));
  assert_eq!(plan.work(), 96 * 49140);
  // The morph-weights row lookup reads gl_InstanceID: the vertex range
  // is the axis, every instance drawn per slice.
  let plan = SlicePlan::plan(range(96, 49140), Topology::Triangles, true).expect("planned");
  assert_eq!(plan.axis, SliceAxis::Vertices);
  assert_eq!((plan.units, plan.unit_work, plan.step, plan.overlap), (96, 49140, 3, 0));
  // A plain draw slices over its vertices.
  let plan = SlicePlan::plan(range(3_000_000, 1), Topology::Triangles, false).expect("planned");
  assert_eq!(plan.axis, SliceAxis::Vertices);
  assert_eq!(plan.unit_work, 1);
}

#[test]
fn a_single_primitive_over_many_instances_cannot_be_cut() {
  // Three vertices reading gl_InstanceID: two slices would need six.
  assert!(SlicePlan::plan(range(3, 1_000_000), Topology::Triangles, true).is_none());
  // Two triangles can: one each.
  let plan = SlicePlan::plan(range(6, 1_000_000), Topology::Triangles, true).expect("planned");
  assert!(plan.sliceable());
  assert_eq!(plan.take(0, ONE_MICRO, None), 3);
  assert_eq!(plan.take(3, ONE_MICRO, Some(3)), 3);
}

#[test]
fn the_topology_sets_the_step_and_the_overlap() {
  let step = |t: Topology| {
    let p = SlicePlan::plan(range(1000, 1000), t, true).expect("planned");
    (p.step, p.overlap, p.min_take())
  };
  assert_eq!(step(Topology::Points), (1, 0, 1));
  assert_eq!(step(Topology::Lines), (2, 0, 2));
  assert_eq!(step(Topology::Triangles), (3, 0, 3));
  assert_eq!(step(Topology::LineStrip), (1, 1, 2));
  assert_eq!(step(Topology::TriangleStrip), (2, 2, 4));
}

#[test]
fn slices_are_sized_to_the_target_at_the_rate_in_whole_primitives() {
  // 1000 vertices x 1000 instances over the vertex axis: a unit is 1000
  // vertices, so one microsecond per vertex wants 250 units.
  let plan = |t: Topology| SlicePlan::plan(range(1000, 1000), t, true).expect("planned");
  assert_eq!(plan(Topology::Points).take(0, ONE_MICRO, None), 250);
  assert_eq!(plan(Topology::Lines).take(0, ONE_MICRO, None), 250);
  assert_eq!(plan(Topology::Triangles).take(0, ONE_MICRO, None), 249);
  assert_eq!(plan(Topology::LineStrip).take(0, ONE_MICRO, None), 250);
  assert_eq!(plan(Topology::TriangleStrip).take(0, ONE_MICRO, None), 250);
  // A strip's next slice starts back by the overlap.
  assert_eq!(plan(Topology::TriangleStrip).advance(0, 250), 248);
  assert_eq!(plan(Topology::LineStrip).advance(0, 250), 249);
  assert_eq!(plan(Topology::Triangles).advance(0, 249), 249);
  // The last slice ends the draw: no overlap is owed past the end.
  assert_eq!(plan(Topology::TriangleStrip).advance(960, 40), 1000);
  assert_eq!(plan(Topology::LineStrip).advance(999, 1), 1000);
  // A triangle strip's slices stay even, so every slice starts on the
  // draw's winding parity.
  let strip = plan(Topology::TriangleStrip);
  let mut at = 0;
  let mut prev = None;
  while at < strip.units {
    let take = strip.take(at, Some(3.0), prev);
    assert!(at % 2 == 0, "slice at {at} starts on the wrong parity");
    if strip.advance(at, take) < strip.units {
      assert!(take % 2 == 0, "slice of {take} at {at} is odd");
    }
    prev = Some(take);
    at = strip.advance(at, take);
  }
}

#[test]
fn the_probe_starts_small_and_grows_by_the_rule() {
  let plan = SlicePlan::plan(range(96, 49140), Topology::Triangles, false).expect("planned");
  // No rate: the probe's work, in units of 96 vertices.
  let first = plan.take(0, None, None);
  assert_eq!(first, SLICE_MIN_WORK / 96);
  // A free GPU wants everything; growth caps it at twice the previous.
  let second = plan.take(first, FREE, Some(first));
  assert_eq!(second, 2 * first);
  // A slow slice shrinks the next one to the target, unbounded downward.
  let slow = Some(10.0);
  assert_eq!(plan.take(first + second, slow, Some(second)), 250_000 / 10 / 96);
  // Never under a primitive, whatever the rate says.
  assert_eq!(plan.take(0, Some(1e9), None), 1);
}

#[test]
fn the_tail_joins_the_last_slice_rather_than_leaving_a_stub() {
  let plan = SlicePlan::plan(range(96, 1000), Topology::Triangles, false).expect("planned");
  // Everything fits: the whole draw is the slice.
  assert_eq!(plan.take(0, FREE, None), 1000);
  // What would remain after a 999-unit slice is one unit, a slice of its
  // own on the instance axis - allowed - but a vertex-axis stub under a
  // primitive is not.
  let tris = SlicePlan::plan(range(1000, 1000), Topology::Triangles, true).expect("planned");
  // 1000 vertices: a rate wanting about 1000 vertices sizes a slice of
  // 999 (whole triangles), which would leave a one-vertex stub - so the
  // slice takes the whole draw.
  let rate_for_1000 = Some(250_000.0 / (1000.5 * 1000.0));
  assert_eq!(tris.take(0, rate_for_1000, None), 1000);
  // A want of 996 leaves four, room for a triangle: two slices.
  let rate_for_996 = Some(250_000.0 / (996.5 * 1000.0));
  assert_eq!(tris.take(0, rate_for_996, None), 996);
}

#[test]
fn a_render_is_whole_under_the_merge_line_and_sliced_without_a_rate() {
  let plan = SlicePlan::plan(range(96, 49140), Topology::Triangles, false).expect("planned");
  assert_eq!(plan.issue(None), Issue::Sliced);
  // Half the budget over the whole work: whole.
  let fits = Some(SUBMISSION_BUDGET_MICROS as f64 / 2.0 / plan.work() as f64);
  assert_eq!(plan.issue(fits), Issue::Whole);
  // Just over: sliced.
  let over = Some(SUBMISSION_BUDGET_MICROS as f64 / 2.0 / plan.work() as f64 * 1.01);
  assert_eq!(plan.issue(over), Issue::Sliced);
}

#[test]
fn the_gauge_remembers_the_worst_slice_and_a_bound_past_the_budget() {
  let gauge = SliceGauge::new();
  assert_eq!(gauge.rate(), None);
  assert_eq!(gauge.last_render(), (0, 0));
  gauge.measured(0.5, 7, 320_000);
  assert_eq!(gauge.rate(), Some(0.5));
  assert_eq!(gauge.last_render(), (7, 320_000));
  // A one-slice render: slices 1, the stale bound handed back.
  let fence = glow::NativeFence(std::ptr::null_mut());
  assert!(gauge.whole(Some(Bound { fence, issued: Instant::now(), work: 1_000_000 })).is_none());
  assert_eq!(gauge.last_render().0, 1);
  assert!(gauge.whole(Some(Bound { fence, issued: Instant::now(), work: 1_000_000 })).is_some());
  assert!(gauge.take_bound().is_some());
  assert!(gauge.bound().is_none());
  // A bound under the budget keeps the exact rate; one over it raises the
  // rate to the bound's, never lowers it.
  gauge.bound_read(SUBMISSION_BUDGET_MICROS, 1_000_000);
  assert_eq!(gauge.rate(), Some(0.5));
  assert_eq!(gauge.last_render(), (1, SUBMISSION_BUDGET_MICROS));
  gauge.bound_read(SUBMISSION_BUDGET_MICROS * 2, 1_000_000);
  assert_eq!(gauge.rate(), Some(1.0));
  gauge.bound_read(SUBMISSION_BUDGET_MICROS + 1, 1_000_000);
  assert_eq!(gauge.rate(), Some(1.0));
  // The first sliced render logs once.
  assert!(gauge.log_once());
  assert!(!gauge.log_once());
}

#[test]
fn the_rate_meter_ignores_the_fixed_cost_of_short_slices() {
  let mut meter = RateMeter::default();
  assert_eq!(meter.rate(), None);
  // Two long slices of a dense and a light region: the dense one's rate.
  meter.add(200_000, 100_000);
  meter.add(50_000, 100_000);
  assert_eq!(meter.rate(), Some(2.0));
  // A two-instance tail that measures the submission round trip alone
  // (500 us for 12 vertices, a rate of 41 per vertex) must not become the
  // draw's rate.
  meter.add(500, 12);
  assert_eq!(meter.rate(), Some(2.0));
  // Short slices alone (a fast GPU's probe) count through the total.
  let mut fast = RateMeter::default();
  fast.add(800, 65_536);
  fast.add(900, 131_072);
  fast.add(500, 12);
  let total = 2200.0 / (65_536.0 + 131_072.0 + 12.0);
  assert_eq!(fast.rate(), Some(total));
}

#[test]
fn the_instance_id_scan_reads_the_token_outside_comments() {
  assert!(reads_instance_id("void main() { int row = uMorphRow + gl_InstanceID; }"));
  assert!(reads_instance_id("ivec2 t = ivec2(0, gl_InstanceID);"));
  assert!(!reads_instance_id("// gl_InstanceID is not read here\nvoid main() {}"));
  assert!(!reads_instance_id("/* gl_InstanceID */ void main() {}"));
  assert!(!reads_instance_id("int gl_InstanceID2 = 0; int my_gl_InstanceID = 1;"));
  assert!(!reads_instance_id("in uvec4 iIds0; void main() { uint id = iIds0.x; }"));
}
