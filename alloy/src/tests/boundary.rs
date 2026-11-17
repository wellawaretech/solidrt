// A snapshot boundary's texel arithmetic: the dimensions a canvas takes
// (boundary::canvas_texels, what SnapshotKey::texture_dims reads per axis)
// and the whole-texel box a raster made under a running track holds
// (boundary::whole_texels). And the raster budget's arithmetic
// (boundary::RasterBudget): the grant order, the grants, the room left, and
// what a walk defers, over costs given here instead of measured. The rasters
// themselves are the GL example's (alloy/examples/boundary_scale.rs): the
// headless raster here refuses them.
use std::time::Duration;

use crate::rendertree::boundary::{canvas_texels, grant_order, granted, whole_texels, within, Deferral, RasterBudget};
use crate::rendertree::{BoundaryMode, Damage, RenderTree, View};

// A density a box divided by it and multiplied back lands a float noise
// above a whole count at (60 texels at it come out at 60.000004).
const NOISY_SCALE: f32 = 0.741462;
const NOISY_TEXELS: f32 = 60.0;
// A canvas large enough that the noise of a held scale (a fraction of
// 1e-5, text::SCALE_HELD_EPSILON) passes the absolute tolerance.
const LARGE_TEXELS: f32 = 4000.0;
const HELD_NOISE: f32 = 1e-5;
// Densities and sides the whole-texel box is checked over.
const DENSITIES: [f32; 5] = [0.5, 0.75, 1.0, 1.35, 2.0 / 3.0];
const SIDES: [f32; 5] = [0.3, 27.47, 81.07, 100.3, 1234.56];

#[test]
fn a_canvas_a_float_noise_above_whole_texels_takes_those_texels() {
  let side = NOISY_TEXELS / NOISY_SCALE;
  assert!(side * NOISY_SCALE > NOISY_TEXELS, "the case is noisy: {}", side * NOISY_SCALE);
  assert_eq!(canvas_texels(side, NOISY_SCALE), NOISY_TEXELS as u32);
  // Exactly whole, and a hair below whole, are that count too.
  assert_eq!(canvas_texels(NOISY_TEXELS, 1.0), NOISY_TEXELS as u32);
  assert_eq!(canvas_texels(NOISY_TEXELS - 1e-4, 1.0), NOISY_TEXELS as u32);
}

#[test]
fn a_real_sliver_takes_one_more_texel() {
  assert_eq!(canvas_texels(60.3, 1.0), 61);
  assert_eq!(canvas_texels(60.01, 1.0), 61);
  assert_eq!(canvas_texels(0.5, 1.0), 1);
  // Past the relative tolerance at a large count too.
  assert_eq!(canvas_texels(LARGE_TEXELS + 0.5, 1.0), LARGE_TEXELS as u32 + 1);
}

#[test]
fn the_tolerance_grows_with_the_canvas() {
  // A box snapped on a grid held at the raster's density is off by up to
  // that fraction of its texels: past the absolute tolerance at this size,
  // within the relative one.
  let noisy = LARGE_TEXELS * (1.0 + HELD_NOISE);
  assert!(noisy - LARGE_TEXELS > 1e-3, "the noise passes the absolute tolerance");
  assert_eq!(canvas_texels(noisy, 1.0), LARGE_TEXELS as u32);
}

#[test]
fn no_canvas_takes_no_texels() {
  assert_eq!(canvas_texels(0.0, 1.0), 0);
  assert_eq!(canvas_texels(-3.0, 1.0), 0);
  assert_eq!(canvas_texels(10.0, 0.0), 0);
  assert_eq!(canvas_texels(f32::NAN, 1.0), 0);
}

#[test]
fn a_whole_texel_box_takes_exactly_its_rounded_texels() {
  for scale in DENSITIES {
    for side in SIDES {
      let whole = whole_texels(side, scale);
      let texels = (side * scale).round().max(1.0);
      assert_eq!(canvas_texels(whole, scale), texels as u32, "{side} at {scale}");
      // Within half a texel of the side it was made for, or the one-texel
      // minimum.
      assert!((whole - side).abs() * scale <= 0.5 || texels == 1.0, "{side} at {scale}: {whole}");
    }
  }
}

#[test]
fn a_whole_texel_box_is_at_least_one_texel_and_nothing_stays_nothing() {
  assert_eq!(whole_texels(0.2, 1.0), 1.0);
  assert_eq!(canvas_texels(whole_texels(0.2, 0.5), 0.5), 1);
  assert_eq!(whole_texels(0.0, 1.0), 0.0);
  assert_eq!(whole_texels(-2.0, 1.0), -2.0);
  // A scale that is no scale leaves the side as it is.
  assert_eq!(whole_texels(12.5, 0.0), 12.5);
  assert_eq!(whole_texels(12.5, f32::INFINITY), 12.5);
}

// Costs and budgets for the budget's arithmetic, in ms.
const COST_MS: u64 = 3;
const LARGE_COST_MS: u64 = 8;
const BUDGET_MS: u64 = 10;
const CONTENT_MS: u64 = 5;
const CHEAP_COST_MS: u64 = 1;
// Stretches of deferred snapshots: none, a little, and past the motion
// bound.
const NO_STRETCH: f32 = 1.0;
const SOME_STRETCH: f32 = 1.2;
const FAR_STRETCH: f32 = 1.8;
// Node ids for the budget's walks, and the root of their tree.
const ROOT: u64 = 1;
const A: u64 = 10;
const B: u64 = 11;
const C: u64 = 12;
const D: u64 = 13;

fn ms(n: u64) -> Duration {
  Duration::from_millis(n)
}

fn deferral(stretch: f32, cost_ms: u64) -> Deferral {
  Deferral { stretch, cost: ms(cost_ms) }
}

#[test]
fn deferred_snapshots_are_granted_the_most_stretched_first_and_alike_in_their_order() {
  let mut deferred = vec![
    (A, deferral(SOME_STRETCH, COST_MS)),
    (B, deferral(FAR_STRETCH, COST_MS)),
    (C, deferral(NO_STRETCH, COST_MS)),
    (D, deferral(FAR_STRETCH, COST_MS)),
  ];
  grant_order(&mut deferred);
  let order: Vec<u64> = deferred.iter().map(|&(id, _)| id).collect();
  assert_eq!(order, vec![B, D, A, C]);
}

#[test]
fn the_first_grant_is_given_whatever_its_cost_and_the_rest_while_they_fit() {
  let costs = |list: &[u64]| list.iter().map(|&n| ms(n)).collect::<Vec<_>>();
  // Over the budget alone: still granted.
  assert_eq!(granted(costs(&[BUDGET_MS * 2, COST_MS]), Some(ms(BUDGET_MS))), 1);
  // 3 + 3 + 3 is below 10; a fourth would make 12.
  assert_eq!(granted(costs(&[COST_MS; 4]), Some(ms(BUDGET_MS))), 3);
  // The grants are a prefix: a cheap one past one that does not fit waits.
  assert_eq!(granted(costs(&[COST_MS, LARGE_COST_MS, CHEAP_COST_MS]), Some(ms(BUDGET_MS))), 1);
  // A zero budget grants the first alone, no budget grants all of them.
  assert_eq!(granted(costs(&[0, 0, 0]), Some(Duration::ZERO)), 1);
  assert_eq!(granted(costs(&[BUDGET_MS; 3]), None), 3);
  assert_eq!(granted(costs(&[]), Some(ms(BUDGET_MS))), 0);
}

#[test]
fn within_the_budget_is_below_it() {
  assert!(within(ms(BUDGET_MS - 1), Some(ms(BUDGET_MS))));
  assert!(!within(ms(BUDGET_MS), Some(ms(BUDGET_MS))));
  assert!(!within(Duration::ZERO, Some(Duration::ZERO)));
  assert!(within(ms(BUDGET_MS * 100), None));
}

#[test]
fn without_a_budget_no_candidate_is_deferred() {
  let deferred = vec![(A, deferral(FAR_STRETCH, LARGE_COST_MS))];
  let mut budget = RasterBudget::for_walk(None, deferred);
  budget.set_spent(ms(BUDGET_MS * 100));
  assert!(!budget.defers(B, true));
  assert!(!budget.defers(A, true));
  assert!(!budget.defers(C, true));
}

#[test]
fn a_zero_budget_with_no_grants_rasters_the_first_candidate_and_defers_the_next() {
  let mut budget = RasterBudget::for_walk(Some(Duration::ZERO), Vec::new());
  // A visit that is no candidate never defers, and is not the frame's one.
  assert!(!budget.defers(A, false));
  assert!(!budget.defers(B, true));
  budget.set_spent(ms(COST_MS));
  assert!(budget.defers(C, true));
  assert!(budget.defers(D, true));
  assert!(!budget.defers(A, false));
}

#[test]
fn a_zero_budget_grants_the_most_stretched_and_defers_the_rest_again() {
  let deferred = vec![(A, deferral(SOME_STRETCH, COST_MS)), (B, deferral(FAR_STRETCH, COST_MS))];
  let mut budget = RasterBudget::for_walk(Some(Duration::ZERO), deferred);
  // A candidate the walk before did not defer comes first in the walk, and
  // is deferred: the grant is the frame's one.
  assert!(budget.defers(C, true));
  // A was deferred and not granted: deferred again.
  assert!(budget.defers(A, true));
  assert!(!budget.defers(B, true));
}

#[test]
fn the_grants_to_come_count_toward_the_time_spent_until_their_visit_gives_them_back() {
  // A's 8 ms are reserved: B, new, rasters at 0 + 8 below 10.
  let mut budget = RasterBudget::for_walk(Some(ms(BUDGET_MS)), vec![(A, deferral(FAR_STRETCH, LARGE_COST_MS))]);
  assert!(!budget.defers(B, true));
  budget.set_spent(ms(COST_MS));
  // 3 spent and 8 reserved is past 10.
  assert!(budget.defers(C, true));
  // A's raster is no longer due: its visit gives its 8 ms back, and D fits.
  assert!(!budget.defers(A, false));
  assert!(!budget.defers(D, true));
}

#[test]
fn a_raster_for_changed_content_counts_toward_the_time_spent() {
  // The rasters of candidates A, B and C cost 3 ms each, under 10.
  let walk = |content: Duration| {
    let mut budget = RasterBudget::for_walk(Some(ms(BUDGET_MS)), Vec::new());
    budget.set_spent(content);
    [A, B, C]
      .into_iter()
      .map(|id| {
        let defers = budget.defers(id, true);
        if !defers {
          budget.set_spent(budget.spent() + ms(COST_MS));
        }
        defers
      })
      .collect::<Vec<_>>()
  };
  assert_eq!(walk(Duration::ZERO), vec![false, false, false]);
  // After a content raster of 5 ms the first is still owed, the second fits
  // (8), the third does not (11).
  assert_eq!(walk(ms(CONTENT_MS)), vec![false, false, true]);
  // The first candidate rasters whatever was spent before it.
  assert_eq!(walk(ms(BUDGET_MS * 2)), vec![false, true, true]);
}

// A tree of snapshot boundaries A, B and C under a root.
fn snapshot_tree() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.root = Some(ROOT);
  for id in [A, B, C] {
    tree.create_node(id, View::default().with_layout());
    tree.insert_node(ROOT, id, None).expect("insert");
    tree.edit(id, |el| {
      el.repaint_boundary = BoundaryMode::Snapshot;
      Damage::Paint
    });
  }
  tree
}

#[test]
fn the_deferrals_come_back_in_their_order_once_and_only_for_live_snapshot_boundaries() {
  let mut tree = snapshot_tree();
  tree.note_snapshot_deferred(C, deferral(SOME_STRETCH, COST_MS));
  tree.note_snapshot_deferred(A, deferral(SOME_STRETCH, COST_MS));
  tree.note_snapshot_deferred(B, deferral(SOME_STRETCH, COST_MS));
  // A second visit keeps the first place and notes what it saw last.
  tree.note_snapshot_deferred(C, deferral(FAR_STRETCH, LARGE_COST_MS));
  let taken = tree.take_snapshot_deferrals();
  assert_eq!(
    taken,
    vec![
      (C, deferral(FAR_STRETCH, LARGE_COST_MS)),
      (A, deferral(SOME_STRETCH, COST_MS)),
      (B, deferral(SOME_STRETCH, COST_MS))
    ]
  );
  assert!(tree.take_snapshot_deferrals().is_empty(), "taken once");

  // A destroyed node and a node that stopped being a snapshot boundary
  // between two walks are not granted.
  for id in [A, B, C] {
    tree.note_snapshot_deferred(id, deferral(SOME_STRETCH, COST_MS));
  }
  tree.delete_node(ROOT, A);
  tree.edit(B, |el| {
    el.repaint_boundary = BoundaryMode::None;
    Damage::Paint
  });
  assert_eq!(tree.take_snapshot_deferrals(), vec![(C, deferral(SOME_STRETCH, COST_MS))]);
}
