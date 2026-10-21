// The residency gate: explicit regions and grids wanted per viewer with
// hysteresis, wanted by any viewer, reported once per change, enters paced
// nearest first under one budget.

use crate::spatial::RegionEvent::{Enter, Leave};
use crate::spatial::RegionRef::{Cell, Node};
use crate::spatial::{
  compose, GridSpec, Mat4, Mat4f, RegionEvent, RegionMargins, RegionSpec, RegionState, SinkWriter, Spatial,
  DEFAULT_RESIDENCY_BUDGET, IDENTITY, VIEW_KEY_BASE,
};

const Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
const ONE: [f64; 3] = [1.0, 1.0, 1.0];
const UNIT: [f64; 6] = [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5];

/// Nothing the gate does reaches a sink; the writer only has to exist.
struct Sink;

impl SinkWriter for Sink {
  fn write_emitter(
    &mut self,
    _voice: u64,
    _output: &crate::spatial::EmitterOutput,
    _ramp: crate::spatial::EmitterRamp,
  ) -> bool {
    true
  }
  fn write_params(&mut self, _: u64, _: u64, _: &Mat4f, _: Option<&Mat4f>) -> bool {
    true
  }
  fn write_count(&mut self, _: u64, _: u64, _: u32) -> bool {
    true
  }
  fn write_fade(&mut self, _: u64, _: u64, _: [f32; 2]) -> bool {
    true
  }
  fn write_order(&mut self, _: u64, _: &[u64]) -> bool {
    true
  }
  fn shift_params(&mut self, _: u64, _: &[u64], _: [f32; 3]) -> bool {
    true
  }
  fn write_order_view(&mut self, _: u64, _: u64, _: [f32; 16]) -> bool {
    true
  }
  fn write_shared(&mut self, _: u64, _: &str, _: &[f32]) -> bool {
    true
  }
  fn write_param(&mut self, _: u64, _: Option<u64>, _: &str, _: f32) -> bool {
    true
  }

  fn write_instances(&mut self, _: u64, _: u32, _: u32, _: &[f32]) -> bool {
    true
  }
  fn write_texture(&mut self, _: u64, _: &[f32]) -> bool {
    true
  }
}

/// One flush and the events it produced (one delivery per flush, as the
/// frame does).
fn step(s: &mut Spatial) -> Vec<RegionEvent> {
  s.flush(&mut Sink);
  s.take_region_events()
}

fn margins(enter: f64, leave: f64) -> Option<RegionSpec> {
  Some(RegionSpec { margins: RegionMargins { enter, leave }, ..RegionSpec::default() })
}

/// A region spec with a weight and a priority beside its margins.
fn weighted(enter: f64, leave: f64, weight: f64, priority: i32) -> Option<RegionSpec> {
  Some(RegionSpec { margins: RegionMargins { enter, leave }, weight, priority })
}

/// Under an identity projection the view alone is the view-projection and
/// an identity one is the unit cube in world space (x, y, z in -1..1), so
/// `(cube_at(x), IDENTITY)` is a viewer whose volume spans x-1..x+1 with
/// its eye at (x, 0, 0) looking down -z.
fn cube_at(x: f64) -> Option<(Mat4, Mat4)> {
  Some((compose([-x, 0.0, 0.0], Q, ONE), IDENTITY))
}

/// A unit-box region at `x` with the given margins.
fn region_at(s: &mut Spatial, x: f64, enter: f64, leave: f64) -> u64 {
  let n = s.create([x, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(n, Some(UNIT)).expect("bounds");
  s.set_region(n, margins(enter, leave)).expect("region");
  n
}

/// An orthographic 2d camera over `w` x `h` world units with its top-left
/// at (`x`, `y`), looking down -z from z = 0 (the plane z in -1..1): the
/// view a tile layer's composite pushes.
fn camera_2d(x: f64, y: f64, w: f64, h: f64) -> Option<(Mat4, Mat4)> {
  let mut proj = [0.0f64; 16];
  proj[0] = 2.0 / w;
  proj[5] = -2.0 / h;
  proj[10] = -1.0;
  proj[15] = 1.0;
  // Clip x = 2 (vx - x) / w - 1: the view translates the rect's center to
  // the origin.
  let view = compose([-(x + w / 2.0), -(y + h / 2.0), 0.0], Q, ONE);
  Some((view, proj))
}

/// A perspective viewer at `eye` looking down -z with a 90 degree fov,
/// near 0.1 and far `far`.
fn perspective_at(eye: [f64; 3], far: f64) -> Option<(Mat4, Mat4)> {
  let near = 0.1;
  let mut proj = [0.0f64; 16];
  proj[0] = 1.0;
  proj[5] = 1.0;
  proj[10] = -(far + near) / (far - near);
  proj[11] = -1.0;
  proj[14] = -2.0 * far * near / (far - near);
  let view = compose([-eye[0], -eye[1], -eye[2]], Q, ONE);
  Some((view, proj))
}

/// The grid a 2d tile layer declares: 10 x 10 chunks in its plane, z
/// unbounded, origin at the world's top-left.
fn plane_grid(enter: f64, leave: f64) -> GridSpec {
  GridSpec {
    origin: [0.0, 0.0, 0.0],
    cell: [10.0, 10.0, 0.0],
    margins: RegionMargins { enter, leave },
    bounds: None,
    wrap: None,
    weight: 1.0,
    limit: None,
    priority: 0,
    label: None,
    root: None,
  }
}

fn cells(events: &[RegionEvent], grid: u64, entered: bool) -> Vec<[i32; 3]> {
  let mut out: Vec<[i32; 3]> = events
    .iter()
    .filter_map(|e| match (e, entered) {
      (Enter(Cell { grid: g, cell }), true) | (Leave(Cell { grid: g, cell }), false) if *g == grid => Some(*cell),
      _ => None,
    })
    .collect();
  out.sort();
  out
}

#[test]
fn a_region_enters_within_enter_and_leaves_beyond_leave_with_hysteresis() {
  let mut s = Spatial::new();
  let r = region_at(&mut s, 0.0, 1.0, 3.0);
  // The volume spans 9..11, the box 8.5 away from its near edge: beyond enter.
  s.set_view(1, cube_at(10.0));
  assert!(step(&mut s).is_empty());
  // Volume 1.5..3.5: the box's far edge at 0.5 is 1.0 away: within enter.
  s.set_view(1, cube_at(2.5));
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
  // 2.0 away: past enter, inside leave - the band holds it.
  s.set_view(1, cube_at(3.5));
  assert!(step(&mut s).is_empty());
  // 3.5 away: beyond leave.
  s.set_view(1, cube_at(5.0));
  assert_eq!(step(&mut s), vec![Leave(Node(r))]);
  // Back into the band only: not re-entered (hysteresis is one-sided).
  s.set_view(1, cube_at(3.5));
  assert!(step(&mut s).is_empty());
}

#[test]
fn a_still_world_reports_nothing() {
  let mut s = Spatial::new();
  let r = region_at(&mut s, 0.0, 1.0, 2.0);
  s.set_view(1, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
  for _ in 0..3 {
    assert!(step(&mut s).is_empty());
  }
  // The same view written again is no change either.
  s.set_view(1, cube_at(0.0));
  assert!(step(&mut s).is_empty());
}

#[test]
fn a_region_is_wanted_while_any_viewer_wants_it() {
  let mut s = Spatial::new();
  let r = region_at(&mut s, 0.0, 0.5, 0.5);
  s.set_view(1, cube_at(0.0));
  s.set_view(2, cube_at(0.5));
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
  // The first viewer turns away; the second still sees it.
  s.set_view(1, cube_at(20.0));
  assert!(step(&mut s).is_empty());
  // The second viewer stops: nobody wants it.
  s.set_view(2, None);
  assert_eq!(step(&mut s), vec![Leave(Node(r))]);
}

#[test]
fn a_region_moves_with_its_parent() {
  let mut s = Spatial::new();
  let parent = s.create([0.0; 3], Q, ONE, true);
  let r = s.create([0.0; 3], Q, ONE, true);
  s.set_parent(r, Some(parent)).expect("parent");
  s.set_bounds(r, Some(UNIT)).expect("bounds");
  s.set_region(r, margins(0.5, 1.0)).expect("region");
  s.set_view(1, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
  s.set_transform(parent, [10.0, 0.0, 0.0], Q, ONE).expect("move");
  assert_eq!(step(&mut s), vec![Leave(Node(r))]);
  s.set_transform(parent, [1.0, 0.0, 0.0], Q, ONE).expect("move back");
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
}

#[test]
fn ending_or_destroying_a_region_leaves_only_if_it_entered() {
  let mut s = Spatial::new();
  let a = region_at(&mut s, 0.0, 0.5, 1.0);
  let b = region_at(&mut s, 0.0, 0.5, 1.0);
  s.set_view(1, cube_at(0.0));
  let mut first = step(&mut s);
  first.sort_by_key(|e| format!("{e:?}"));
  assert_eq!(first.len(), 2);
  s.set_region(a, None).expect("end");
  assert_eq!(s.take_region_events(), vec![Leave(Node(a))]);
  s.destroy(b).expect("destroy");
  assert_eq!(s.take_region_events(), vec![Leave(Node(b))]);
  // A region still pending (budget 0 holds every enter) goes silently.
  s.set_residency_budget(0);
  let c = region_at(&mut s, 0.0, 0.5, 1.0);
  assert!(step(&mut s).is_empty());
  s.destroy(c).expect("destroy pending");
  assert!(step(&mut s).is_empty());
  // Ending and re-declaring a region that never entered: still nothing.
  s.set_residency_budget(DEFAULT_RESIDENCY_BUDGET);
  let d = region_at(&mut s, 0.0, 0.5, 1.0);
  assert_eq!(step(&mut s), vec![Enter(Node(d))]);
}

#[test]
fn set_region_validates() {
  let mut s = Spatial::new();
  let boxless = s.create([0.0; 3], Q, ONE, true);
  assert!(s.set_region(boxless, margins(1.0, 2.0)).is_err(), "a region needs a box");
  let n = s.create([0.0; 3], Q, ONE, true);
  s.set_bounds(n, Some(UNIT)).expect("bounds");
  assert!(s.set_region(n, margins(2.0, 1.0)).is_err(), "leave below enter");
  assert!(s.set_region(n, margins(-1.0, 1.0)).is_err(), "negative enter");
  assert!(s.set_region(n, margins(f64::NAN, 1.0)).is_err(), "nan");
  assert!(s.set_region(n, margins(1.0, 1.0)).is_ok(), "leave == enter is a hard edge, allowed");
  assert!(s.set_region(n, None).is_ok());
  assert!(s.set_region(n, None).is_ok(), "ending twice is a no-op");
  s.destroy(n).expect("destroy");
  assert!(s.set_region(n, margins(1.0, 2.0)).is_err(), "a dead node");
}

#[test]
fn grid_cells_under_an_orthographic_view_enter_and_a_pan_swaps_them() {
  let mut s = Spatial::new();
  let g = s.create_grid(plane_grid(0.0, 0.0)).expect("grid");
  let key = s.alloc_view_key();
  // A 20 x 20 viewport over the world's corner: cells 0..1 x 0..1.
  s.set_view(key, camera_2d(0.0, 0.0, 20.0, 20.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, g, true), vec![[0, 0, 0], [0, 1, 0], [1, 0, 0], [1, 1, 0]]);
  assert!(cells(&events, g, false).is_empty());
  // Pan one cell right: column 0 leaves, column 2 enters.
  s.set_view(key, camera_2d(10.0, 0.0, 20.0, 20.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, g, false), vec![[0, 0, 0], [0, 1, 0]]);
  assert_eq!(cells(&events, g, true), vec![[2, 0, 0], [2, 1, 0]]);
  // Leaves come before enters in a pass.
  let first_enter = events.iter().position(|e| matches!(e, Enter(_))).expect("an enter");
  assert!(events[..first_enter].iter().all(|e| matches!(e, Leave(_))));
  // The viewer stops: everything leaves.
  s.set_view(key, None);
  let events = step(&mut s);
  assert_eq!(cells(&events, g, false), vec![[1, 0, 0], [1, 1, 0], [2, 0, 0], [2, 1, 0]]);
}

#[test]
fn grid_margins_reach_past_the_view_with_hysteresis() {
  let mut s = Spatial::new();
  // Enter reaches 5 past the view (half a cell), leave 15 (a cell and a half).
  let g = s.create_grid(plane_grid(5.0, 15.0)).expect("grid");
  let key = s.alloc_view_key();
  // View 10..20: cells 1 in view, 0 and 2 within enter.
  s.set_view(key, camera_2d(10.0, 10.0, 10.0, 10.0));
  let events = step(&mut s);
  let xs: Vec<i32> = cells(&events, g, true).iter().map(|c| c[0]).collect();
  assert!(xs.contains(&0) && xs.contains(&1) && xs.contains(&2) && !xs.contains(&3), "{xs:?}");
  // Pan right by one cell: cell 0 is now 10 past the view's left edge:
  // past enter, within leave, kept. Cell 4 at 10 past the right edge:
  // beyond enter, not taken.
  s.set_view(key, camera_2d(20.0, 10.0, 10.0, 10.0));
  let events = step(&mut s);
  assert!(cells(&events, g, false).is_empty(), "{events:?}");
  let xs: Vec<i32> = cells(&events, g, true).iter().map(|c| c[0]).collect();
  assert!(xs.contains(&3) && !xs.contains(&4), "{xs:?}");
  // Pan again: cell 0 is 20 past: beyond leave.
  s.set_view(key, camera_2d(30.0, 10.0, 10.0, 10.0));
  let events = step(&mut s);
  let left: Vec<i32> = cells(&events, g, false).iter().map(|c| c[0]).collect();
  assert_eq!(left, vec![0, 0, 0]);
}

#[test]
fn grid_cells_beyond_the_depth_reach_stay_out_under_a_perspective_view() {
  let mut s = Spatial::new();
  // Many cells at once: the pacing is another test's.
  s.set_residency_budget(u32::MAX);
  // Columns over XZ, 10 wide, y unbounded; load 25 deep, keep to 35.
  let g = s
    .create_grid(GridSpec {
      origin: [0.0, 0.0, 0.0],
      cell: [10.0, 0.0, 10.0],
      margins: RegionMargins { enter: 25.0, leave: 35.0 },
      bounds: None,
      wrap: None,
      weight: 1.0,
      limit: None,
      priority: 0,
      label: None,
      root: None,
    })
    .expect("grid");
  // Eye at z = 100 looking down -z with a far plane at 1000: the frustum
  // covers the whole grid, the depth reach does not.
  s.set_view(1, perspective_at([5.0, 0.0, 100.0], 1000.0));
  let events = step(&mut s);
  let entered = cells(&events, g, true);
  let zs: Vec<i32> = entered.iter().map(|c| c[2]).collect();
  // The nearest column ahead starts at z = 90 (index 9), 10 deep: within
  // 25. Index 7 (z 70..80) is 20 deep: in. Index 6 (60..70) is 30: out.
  assert!(zs.contains(&9) && zs.contains(&7) && !zs.contains(&6), "{entered:?}");
  // Nothing behind the eye beyond the margin: index 12 (120..130) is 20
  // behind, the near plane pushed out by 25 reaches it; 13 does not.
  assert!(zs.contains(&12) && !zs.contains(&13), "{entered:?}");
  // Every entered cell is within reach along y (unbounded: index 0).
  assert!(entered.iter().all(|c| c[1] == 0));
  // Step forward by 5: index 6 is now 25 deep, enters; 12 is 25 behind
  // and still within the near reach (the test is <=), nothing leaves yet.
  s.set_view(1, perspective_at([5.0, 0.0, 95.0], 1000.0));
  let events = step(&mut s);
  assert!(cells(&events, g, true).iter().any(|c| c[2] == 6), "{events:?}");
}

#[test]
fn grid_bounds_exclude_cells_and_wrap_reports_canonical_cells_once() {
  let mut s = Spatial::new();
  let mut spec = plane_grid(0.0, 0.0);
  spec.bounds = Some([0, 0, 0, 1, 0, 0]);
  let bounded = s.create_grid(spec).expect("bounded grid");
  let mut spec = plane_grid(0.0, 0.0);
  spec.wrap = Some([3, 0, 0]);
  let wrapped = s.create_grid(spec).expect("wrapped grid");
  let key = s.alloc_view_key();
  // 60 wide: raw columns 0..5 of the plane, rows 0..1.
  s.set_view(key, camera_2d(0.0, 0.0, 60.0, 20.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, bounded, true), vec![[0, 0, 0], [1, 0, 0]]);
  assert_eq!(
    cells(&events, wrapped, true),
    vec![[0, 0, 0], [0, 1, 0], [1, 0, 0], [1, 1, 0], [2, 0, 0], [2, 1, 0]],
    "six raw columns fold onto three canonical ones, each once"
  );
  // Pan to raw columns 3..8: the canonical set is unchanged, nothing moves.
  s.set_view(key, camera_2d(30.0, 0.0, 60.0, 20.0));
  let events = step(&mut s);
  assert!(cells(&events, wrapped, true).is_empty() && cells(&events, wrapped, false).is_empty(), "{events:?}");
  // A viewer entirely outside the bounds wants no bounded cell.
  assert_eq!(cells(&events, bounded, false), vec![[0, 0, 0], [1, 0, 0]]);
}

#[test]
fn destroying_a_grid_leaves_every_resident_cell() {
  let mut s = Spatial::new();
  let g = s.create_grid(plane_grid(0.0, 0.0)).expect("grid");
  let key = s.alloc_view_key();
  s.set_view(key, camera_2d(0.0, 0.0, 20.0, 10.0));
  assert_eq!(cells(&step(&mut s), g, true), vec![[0, 0, 0], [1, 0, 0]]);
  s.destroy_grid(g).expect("destroy");
  assert_eq!(cells(&s.take_region_events(), g, false), vec![[0, 0, 0], [1, 0, 0]]);
  assert!(s.destroy_grid(g).is_err(), "gone");
  assert!(step(&mut s).is_empty());
}

#[test]
fn create_grid_validates() {
  let mut s = Spatial::new();
  let mut spec = plane_grid(1.0, 2.0);
  spec.cell = [-1.0, 10.0, 0.0];
  assert!(s.create_grid(spec).is_err(), "negative cell");
  let mut spec = plane_grid(2.0, 1.0);
  spec.cell = [10.0, 10.0, 0.0];
  assert!(s.create_grid(spec).is_err(), "leave below enter");
  let mut spec = plane_grid(1.0, 2.0);
  spec.bounds = Some([2, 0, 0, 1, 0, 0]);
  assert!(s.create_grid(spec).is_err(), "inverted bounds");
  let mut spec = plane_grid(1.0, 2.0);
  spec.wrap = Some([-1, 0, 0]);
  assert!(s.create_grid(spec).is_err(), "negative period");
  assert!(s.create_grid(plane_grid(1.0, 2.0)).is_ok());
  assert!(s.destroy_grid(99).is_err(), "unknown grid");
}

#[test]
fn enters_are_paced_nearest_first_under_one_budget() {
  let mut s = Spatial::new();
  s.set_residency_budget(2);
  // Five regions along x; the eye at the origin.
  let ids: Vec<u64> = [4.0, 1.0, 3.0, 0.0, 2.0].iter().map(|&x| region_at(&mut s, x, 10.0, 10.0)).collect();
  s.set_view(1, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(ids[3])), Enter(Node(ids[1]))], "the two nearest first");
  assert!(s.residency_demand(), "enters still pending");
  assert_eq!(step(&mut s), vec![Enter(Node(ids[4])), Enter(Node(ids[2]))]);
  assert_eq!(step(&mut s), vec![Enter(Node(ids[0]))]);
  assert!(!s.residency_demand());
  assert!(step(&mut s).is_empty());
}

#[test]
fn the_budget_spans_every_flush_between_two_deliveries() {
  let mut s = Spatial::new();
  s.set_residency_budget(2);
  for x in 0..5 {
    region_at(&mut s, x as f64, 10.0, 10.0);
  }
  s.set_view(1, cube_at(0.0));
  // Three flushes (a JS-side flush, the frame's, another) before one take.
  s.flush(&mut Sink);
  s.flush(&mut Sink);
  s.flush(&mut Sink);
  assert_eq!(s.take_region_events().len(), 2);
  assert_eq!(step(&mut s).len(), 2);
  assert_eq!(step(&mut s).len(), 1);
}

#[test]
fn a_pending_region_the_viewer_turned_away_from_never_enters() {
  let mut s = Spatial::new();
  s.set_residency_budget(1);
  let near = region_at(&mut s, 0.0, 0.5, 0.5);
  let far = region_at(&mut s, 1.0, 0.5, 0.5);
  s.set_view(1, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(near))]);
  // Before the far one's turn, the viewer leaves both behind: one leave
  // for the entered one, nothing for the pending one.
  s.set_view(1, cube_at(20.0));
  assert_eq!(step(&mut s), vec![Leave(Node(near))]);
  assert!(step(&mut s).is_empty());
  let _ = far;
}

#[test]
fn pending_grid_cells_sort_by_their_center_distance() {
  let mut s = Spatial::new();
  s.set_residency_budget(1);
  let g = s.create_grid(plane_grid(0.0, 0.0)).expect("grid");
  let key = s.alloc_view_key();
  // A viewport over cells 0..2 x 0, the eye at x = 25: cell 2 is nearest.
  s.set_view(key, camera_2d(10.0, 0.0, 30.0, 10.0));
  assert_eq!(cells(&step(&mut s), g, true), vec![[2, 0, 0]]);
  // Then 1 and 3 at equal distance, then nothing.
  let next = cells(&step(&mut s), g, true);
  assert!(next == vec![[1, 0, 0]] || next == vec![[3, 0, 0]], "{next:?}");
  let next = cells(&step(&mut s), g, true);
  assert_eq!(next.len(), 1);
  let rest = step(&mut s);
  assert!(rest.is_empty(), "{rest:?}");
}

#[test]
fn view_keys_count_up_from_the_reserved_base() {
  let mut s = Spatial::new();
  let a = s.alloc_view_key();
  let b = s.alloc_view_key();
  assert_eq!(a, VIEW_KEY_BASE);
  assert_eq!(b, VIEW_KEY_BASE + 1);
  // A key is a viewer like any target.
  let r = region_at(&mut s, 0.0, 0.5, 0.5);
  s.set_view(a, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(r))]);
}

/// A grid of 10 x 10 cells with its own limit in cells (weight 1).
fn limited_grid(limit: f64) -> GridSpec {
  GridSpec { limit: Some(limit), ..plane_grid(0.0, 10.0) }
}

#[test]
fn a_grid_limit_evicts_the_least_urgent_resident_for_a_more_urgent_candidate() {
  let mut s = Spatial::new();
  let g = s.create_grid(limited_grid(2.0)).expect("grid");
  let key = s.alloc_view_key();
  // A viewport over x 10..35 (cells 1, 2 and half of 3), the eye at
  // x = 22.5: cells 2 and 1 enter, the furthest (cell 3) stalls.
  s.set_view(key, camera_2d(10.0, 0.0, 25.0, 10.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, g, true), vec![[1, 0, 0], [2, 0, 0]]);
  assert!(!s.residency_demand(), "a stalled cell demands no frame");
  let stats = s.residency_stats();
  assert_eq!((stats.resident, stats.pending, stats.stalled, stats.wanted), (2, 1, 1, 3));
  assert_eq!(stats.weight, 2.0);
  assert!(step(&mut s).is_empty(), "nothing changes for a still view");
  // The eye moves to x = 5 (cell 0's center): cell 0 is now the most
  // urgent and cell 2, within leave, the least; the limit evicts cell 2
  // before cell 0 enters, and cell -1 (a leave away) stalls behind cell 1.
  s.set_view(key, camera_2d(-7.5, 0.0, 25.0, 10.0));
  let events = step(&mut s);
  assert_eq!(events, vec![Leave(Cell { grid: g, cell: [2, 0, 0] }), Enter(Cell { grid: g, cell: [0, 0, 0] })]);
  let stats = s.residency_stats();
  assert_eq!((stats.enters, stats.leaves, stats.stalled), (3, 1, 1));
}

#[test]
fn the_scene_limit_weighs_every_grid_with_a_minimal_eviction_and_a_weight_change_re_weighs() {
  let mut s = Spatial::new();
  s.set_residency_limit(Some(30.0)).expect("limit");
  // Two grids over the same plane: a cheap one, and one at ten per cell
  // whose band is twice as wide, so its cells are twice as urgent at the
  // same distance (no ties below).
  let cheap = s.create_grid(plane_grid(0.0, 10.0)).expect("grid");
  let dear = s.create_grid(GridSpec { weight: 10.0, ..plane_grid(0.0, 20.0) }).expect("grid");
  let key = s.alloc_view_key();
  // A viewport over cells 0..1 x 0: four cells wanted, weighing 22 in all.
  s.set_view(key, camera_2d(0.0, 0.0, 20.0, 10.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, cheap, true).len() + cells(&events, dear, true).len(), 4);
  assert_eq!(s.residency_stats().weight, 22.0);
  // The dear grid's cells now weigh 20 each: 42 resident over a cap of 30.
  // The next pass re-weighs: the resident set only changes when a more
  // urgent candidate arrives, so nothing leaves yet.
  s.set_grid_weight(dear, 20.0).expect("weight");
  assert!(step(&mut s).is_empty());
  // Pan one cell right (eye at x = 20): cell 2 of each grid wants in. The
  // dear one needs 32 freed and the cells less urgent than it weigh 22:
  // it stalls, nothing evicted for it. The cheap one needs 13: the least
  // urgent residents are cheap 0 (1) then dear 0 (20), and with both
  // picked cheap 0 turns out unnecessary, so dear 0 alone leaves.
  s.set_view(key, camera_2d(10.0, 0.0, 20.0, 10.0));
  let events = step(&mut s);
  assert!(cells(&events, cheap, false).is_empty());
  assert_eq!(cells(&events, dear, false), vec![[0, 0, 0]]);
  assert_eq!(cells(&events, cheap, true), vec![[2, 0, 0]]);
  assert!(cells(&events, dear, true).is_empty());
  assert_eq!(s.residency_stats().weight, 23.0);
  // The evicted cell is pending again (within leave of the view); the
  // next pass finds it and the dear cell 2 both unplaceable: stalled,
  // no frame demanded.
  assert!(s.residency_demand());
  assert!(step(&mut s).is_empty());
  assert_eq!(s.residency_stats().stalled, 2);
  assert!(!s.residency_demand());
}

#[test]
fn limits_validate() {
  let mut s = Spatial::new();
  assert!(s.create_grid(GridSpec { weight: -1.0, ..plane_grid(0.0, 0.0) }).is_err());
  assert!(s.create_grid(GridSpec { limit: Some(0.0), ..plane_grid(0.0, 0.0) }).is_err());
  assert!(s.set_residency_limit(Some(f64::NAN)).is_err());
  let g = s.create_grid(plane_grid(0.0, 0.0)).expect("grid");
  assert!(s.set_grid_weight(g, f64::INFINITY).is_err());
  assert!(s.set_grid_weight(g + 1, 1.0).is_err());
}

#[test]
fn a_zoomed_orthographic_viewer_far_from_the_origin_enumerates_only_the_cells_under_it() {
  // A 2d camera's view matrix carries its zoom as a scale: the eye must
  // still be the camera's position, or the enumeration box spans from a
  // zoom's worth short of it (10^5 units here) to the view.
  let mut s = Spatial::new();
  s.set_residency_budget(u32::MAX);
  let g = s.create_grid(plane_grid(0.0, 0.0)).expect("grid");
  let key = s.alloc_view_key();
  // Zoom 0.5 over a 20 x 20 viewport at (10^6, 0): 40 x 40 world units,
  // cells 100000..100003 x 0..3, sixteen of them.
  let zoom = 0.5f64;
  let mut view = [0.0f64; 16];
  view[0] = zoom;
  view[5] = zoom;
  view[10] = 1.0;
  view[12] = -zoom * 1_000_000.0 - 10.0;
  view[13] = -10.0;
  view[15] = 1.0;
  let mut proj = [0.0f64; 16];
  proj[0] = 2.0 / 20.0;
  proj[5] = -2.0 / 20.0;
  proj[10] = -1.0;
  proj[15] = 1.0;
  s.set_view(key, Some((view, proj)));
  let entered = cells(&step(&mut s), g, true);
  assert_eq!(entered.len(), 16, "{entered:?}");
  assert!(entered.iter().all(|c| (100_000..100_004).contains(&c[0]) && (0..4).contains(&c[1])), "{entered:?}");
}

#[test]
fn explicit_regions_weigh_under_the_scene_limit_and_are_evicted_least_needed_first() {
  let mut s = Spatial::new();
  s.set_residency_limit(Some(25.0)).expect("limit");
  // Three unit-box regions along x, 10 each, under a cube viewer at the
  // origin reaching x in -1..1 pushed out by a 20 enter: all three wanted.
  let near = s.create([1.5, 0.0, 0.0], Q, ONE, true);
  let mid = s.create([5.0, 0.0, 0.0], Q, ONE, true);
  let far = s.create([15.0, 0.0, 0.0], Q, ONE, true);
  for n in [near, mid, far] {
    s.set_bounds(n, Some(UNIT)).expect("bounds");
    s.set_region(n, weighted(20.0, 20.0, 10.0, 0)).expect("region");
  }
  s.set_view(1, cube_at(0.0));
  let events = step(&mut s);
  // Nearest first; the third does not fit: stalled.
  assert_eq!(events, vec![Enter(Node(near)), Enter(Node(mid))]);
  let stats = s.residency_stats();
  assert_eq!((stats.resident, stats.stalled, stats.weight), (2, 1, 20.0));
  assert_eq!(s.region_state(far).expect("node"), Some(RegionState::Stalled));
  assert_eq!(s.region_state(near).expect("node"), Some(RegionState::Resident));
  assert!(!s.residency_demand());
  // The near region measures 2 once built (its estimate was 10): the
  // resident weight drops to 12 and the far one (10) fits at the next
  // pass without an eviction.
  s.set_region_weight(near, 2.0).expect("weight");
  let events = step(&mut s);
  assert_eq!(events, vec![Enter(Node(far))]);
  assert_eq!(s.residency_stats().weight, 22.0);
  // A fourth region, nearer than `far` and heavier than the slack: the
  // least needed resident (far, 10) is evicted for it.
  let fourth = s.create([3.0, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(fourth, Some(UNIT)).expect("bounds");
  s.set_region(fourth, weighted(20.0, 20.0, 8.0, 0)).expect("region");
  let events = step(&mut s);
  assert_eq!(events, vec![Leave(Node(far)), Enter(Node(fourth))]);
  assert_eq!(s.region_state(far).expect("node"), Some(RegionState::Pending));
  assert_eq!(s.residency_stats().weight, 20.0);
}

#[test]
fn priority_orders_enters_before_distance_and_protects_from_eviction() {
  let mut s = Spatial::new();
  s.set_residency_budget(1);
  // A near low-priority region and a far high-priority one: the far one
  // enters first.
  let near = s.create([1.0, 0.0, 0.0], Q, ONE, true);
  let far = s.create([10.0, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(near, Some(UNIT)).expect("bounds");
  s.set_bounds(far, Some(UNIT)).expect("bounds");
  s.set_region(near, weighted(20.0, 20.0, 1.0, 0)).expect("region");
  s.set_region(far, weighted(20.0, 20.0, 1.0, 1)).expect("region");
  s.set_view(1, cube_at(0.0));
  assert_eq!(step(&mut s), vec![Enter(Node(far))]);
  assert_eq!(step(&mut s), vec![Enter(Node(near))]);
  // A scene limit of 2 and a third region at priority 0 nearer than
  // `near`: it may evict `near` (same priority, further) but never `far`
  // (higher priority), and `near` alone covers the need.
  s.set_residency_limit(Some(2.0)).expect("limit");
  let third = s.create([0.5, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(third, Some(UNIT)).expect("bounds");
  s.set_region(third, weighted(20.0, 20.0, 1.0, 0)).expect("region");
  assert_eq!(step(&mut s), vec![Leave(Node(near)), Enter(Node(third))]);
  // A fourth at priority 0, further than `third`: nothing less needed
  // than it is resident but `near`, which is pending; stalled.
  let fourth = s.create([0.7, 0.0, 0.0], Q, ONE, true);
  s.set_bounds(fourth, Some(UNIT)).expect("bounds");
  s.set_region(fourth, weighted(20.0, 20.0, 1.0, 0)).expect("region");
  assert!(step(&mut s).is_empty());
  assert_eq!(s.region_state(fourth).expect("node"), Some(RegionState::Stalled));
  assert_eq!(s.region_state(far).expect("node"), Some(RegionState::Resident));
}

#[test]
fn a_measured_cell_weight_replaces_the_estimate_and_a_grid_priority_orders_grids() {
  let mut s = Spatial::new();
  s.set_residency_budget(1);
  // Two grids over the same plane, the second at a higher priority: its
  // cell enters first though both are at the same distance.
  let low = s.create_grid(plane_grid(0.0, 10.0)).expect("grid");
  let high = s.create_grid(GridSpec { priority: 1, ..plane_grid(0.0, 10.0) }).expect("grid");
  let key = s.alloc_view_key();
  s.set_view(key, camera_2d(0.0, 0.0, 10.0, 10.0));
  assert_eq!(step(&mut s), vec![Enter(Cell { grid: high, cell: [0, 0, 0] })]);
  assert_eq!(s.cell_state(low, [0, 0, 0]).expect("grid"), Some(RegionState::Pending));
  assert_eq!(step(&mut s), vec![Enter(Cell { grid: low, cell: [0, 0, 0] })]);
  assert_eq!(s.cell_state(low, [0, 0, 0]).expect("grid"), Some(RegionState::Resident));
  assert_eq!(s.cell_state(low, [5, 5, 0]).expect("grid"), None);
  assert!(s.cell_state(low + 7, [0, 0, 0]).is_err());
  // The handler measured the low cell at 40 (the estimate was 1): the
  // resident weight follows, and a scene limit of 30 evicts it for the
  // next wanted cell.
  s.set_cell_weight(low, [0, 0, 0], 40.0).expect("weight");
  assert!(s.set_cell_weight(low, [5, 5, 0], 1.0).is_err(), "a cell nobody wants holds no weight");
  assert!(s.set_cell_weight(low, [0, 0, 0], -1.0).is_err());
  assert_eq!(s.residency_stats().weight, 41.0);
  s.set_residency_limit(Some(30.0)).expect("limit");
  s.set_view(key, camera_2d(5.0, 0.0, 10.0, 10.0));
  let events = step(&mut s);
  assert_eq!(cells(&events, low, false), vec![[0, 0, 0]]);
  assert_eq!(events.iter().filter(|e| matches!(e, Enter(_))).count(), 1);
}

#[test]
fn an_enter_and_its_leave_within_one_delivery_collapse_to_nothing() {
  let mut s = Spatial::new();
  let near = region_at(&mut s, 0.0, 0.5, 1.0);
  let far = region_at(&mut s, 10.0, 0.5, 1.0);
  // A viewer over `near` for one flush, then over `far` before the
  // frame takes the events: `near` entered and left in between, which
  // is no change; `far` enters.
  s.set_view(1, cube_at(0.0));
  s.flush(&mut Sink);
  s.set_view(1, cube_at(10.0));
  s.flush(&mut Sink);
  assert_eq!(s.take_region_events(), vec![Enter(Node(far))]);
  let stats = s.residency_stats();
  assert_eq!((stats.enters, stats.leaves, stats.resident), (1, 0, 1));
  // Enter, leave, enter within one window: one enter.
  s.set_view(1, cube_at(0.0));
  s.flush(&mut Sink);
  s.set_view(1, cube_at(10.0));
  s.flush(&mut Sink);
  s.set_view(1, cube_at(0.0));
  s.flush(&mut Sink);
  assert_eq!(s.take_region_events(), vec![Leave(Node(far)), Enter(Node(near))]);
  assert_eq!(s.residency_stats().enters, 2);
}

#[test]
fn a_viewer_scoped_to_a_world_wants_only_that_worlds_grids_and_regions() {
  let mut s = Spatial::new();
  let world_a = s.create([0.0; 3], Q, ONE, true);
  let world_b = s.create([0.0; 3], Q, ONE, true);
  // Cells straddling y = 0 (origin y -5), so a unit cube at y = 0 wants
  // one cell per grid.
  let grid_a =
    s.create_grid(GridSpec { root: Some(world_a), origin: [0.0, -5.0, 0.0], ..plane_grid(0.0, 5.0) }).expect("grid a");
  let grid_b =
    s.create_grid(GridSpec { root: Some(world_b), origin: [0.0, -5.0, 0.0], ..plane_grid(0.0, 5.0) }).expect("grid b");
  let open = s.create_grid(GridSpec { origin: [100.0, -5.0, 0.0], ..plane_grid(0.0, 5.0) }).expect("open grid");
  // An explicit region under world a, and one under no world.
  let in_a = region_at(&mut s, 5.0, 0.0, 5.0);
  s.set_parent(in_a, Some(world_a)).expect("parent");
  let loose = region_at(&mut s, 105.0, 0.0, 5.0);
  // A viewer of world a over the origin: a's cell and a's region, not
  // b's; the open grid is wanted by every viewer, so its cell at 100 is
  // not in reach and the loose region neither.
  s.set_view(1, cube_at(5.0));
  s.set_view_root(1, Some(world_a));
  let events = step(&mut s);
  assert!(events.iter().any(|e| matches!(e, Enter(Cell { grid, .. }) if *grid == grid_a)), "a's cell: {events:?}");
  assert!(!events.iter().any(|e| matches!(e, Enter(Cell { grid, .. }) if *grid == grid_b)), "not b's: {events:?}");
  assert!(events.iter().any(|e| matches!(e, Enter(Node(n)) if *n == in_a)), "a's region: {events:?}");
  assert!(!events.iter().any(|e| matches!(e, Enter(Node(n)) if *n == loose)));
  // The same viewer over the open grid and the loose region wants them.
  s.set_view(1, cube_at(105.0));
  let events = step(&mut s);
  assert!(events.iter().any(|e| matches!(e, Enter(Cell { grid, .. }) if *grid == open)), "open grid: {events:?}");
  assert!(events.iter().any(|e| matches!(e, Enter(Node(n)) if *n == loose)), "loose region: {events:?}");
  // An unscoped viewer over the origin wants both worlds' cells.
  s.set_view(2, cube_at(5.0));
  let events = step(&mut s);
  assert!(
    events.iter().any(|e| matches!(e, Enter(Cell { grid, .. }) if *grid == grid_b)),
    "unscoped wants b: {events:?}"
  );
  // Re-scoping viewer 2 to world b drops a's region and keeps b's cell.
  s.set_view_root(2, Some(world_b));
  s.set_view(1, None);
  let events = step(&mut s);
  assert!(events.iter().any(|e| matches!(e, Leave(Node(n)) if *n == in_a)), "a's region leaves: {events:?}");
  assert!(
    events.iter().any(|e| matches!(e, Leave(Cell { grid, .. }) if *grid == grid_a)),
    "a's cell leaves: {events:?}"
  );
  assert!(s.cell_state(grid_b, [0, 0, 0]).expect("grid").is_some(), "b's cell stays wanted");
}

#[test]
fn a_cell_approached_again_is_admitted_by_what_it_measured_last_time() {
  let mut s = Spatial::new();
  // A grid whose estimate admits anything; the cell at the origin
  // measures 40 once built, over a limit of 30.
  let grid = s
    .create_grid(GridSpec { weight: 1.0, limit: Some(30.0), origin: [0.0, -5.0, 0.0], ..plane_grid(0.0, 5.0) })
    .expect("grid");
  s.set_view(1, cube_at(5.0));
  let events = step(&mut s);
  assert_eq!(events, vec![Enter(Cell { grid, cell: [0, 0, 0] })], "admitted by the estimate");
  s.set_cell_weight(grid, [0, 0, 0], 40.0).expect("weight");
  // Away and back: the cell is known to weigh 40, which the limit never
  // admits, so it waits stalled instead of entering to be evicted.
  s.set_view(1, cube_at(500.0));
  let events = step(&mut s);
  assert!(events.iter().any(|e| matches!(e, Leave(Cell { .. }))), "left: {events:?}");
  s.set_view(1, cube_at(5.0));
  let events = step(&mut s);
  assert!(!events.iter().any(|e| matches!(e, Enter(Cell { .. }))), "not re-admitted: {events:?}");
  assert_eq!(s.cell_state(grid, [0, 0, 0]).expect("grid"), Some(RegionState::Stalled));
  // A wider limit admits it, at its remembered weight.
  s.set_grid_limit(grid, Some(50.0)).expect("limit");
  let events = step(&mut s);
  assert!(events.iter().any(|e| matches!(e, Enter(Cell { .. }))), "admitted at 40 under 50: {events:?}");
  assert_eq!(s.residency_stats().weight, 40.0);
}
