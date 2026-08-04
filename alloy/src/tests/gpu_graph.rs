use std::collections::{HashMap, HashSet};

use crate::context::samples_transitively;
use crate::raster::propagation_order;

fn edges(list: &[(u64, &[u64])]) -> HashMap<u64, Vec<u64>> {
  list.iter().map(|(id, sources)| (*id, sources.to_vec())).collect()
}

fn dirty(ids: &[u64]) -> HashSet<u64> {
  ids.iter().copied().collect()
}

fn sources(list: &[(u64, &[u64])]) -> HashMap<u64, HashMap<(u64, String), u64>> {
  list
    .iter()
    .map(|(id, srcs)| (*id, srcs.iter().enumerate().map(|(i, s)| ((0, format!("u{i}")), *s)).collect()))
    .collect()
}

fn barriers(ids: &[u64]) -> HashSet<u64> {
  ids.iter().copied().collect()
}

#[test]
fn chain_propagates_in_order() {
  // a -> b -> c: dirtying the head re-renders the whole chain, head first.
  let g = edges(&[(1, &[]), (2, &[1]), (3, &[2])]);
  assert_eq!(propagation_order(&dirty(&[1]), &g), (vec![1, 2, 3], vec![]));
}

#[test]
fn pixel_source_renders_consumers_only() {
  // Id 10 is a plain texture (no edges entry): it never renders itself, but
  // the targets sampling it do, in chain order.
  let g = edges(&[(2, &[10]), (3, &[2])]);
  assert_eq!(propagation_order(&dirty(&[10]), &g), (vec![2, 3], vec![]));
}

#[test]
fn mid_chain_dirty_skips_upstream() {
  let g = edges(&[(1, &[]), (2, &[1]), (3, &[2])]);
  assert_eq!(propagation_order(&dirty(&[2]), &g), (vec![2, 3], vec![]));
}

#[test]
fn diamond_renders_join_once_after_both_arms() {
  // 1 -> 2, 1 -> 3, {2,3} -> 4: the join renders once, after both arms.
  let g = edges(&[(1, &[]), (2, &[1]), (3, &[1]), (4, &[2, 3])]);
  assert_eq!(propagation_order(&dirty(&[1]), &g), (vec![1, 2, 3, 4], vec![]));
}

#[test]
fn unaffected_targets_stay_out() {
  let g = edges(&[(1, &[]), (2, &[1]), (5, &[6]), (6, &[])]);
  assert_eq!(propagation_order(&dirty(&[1]), &g), (vec![1, 2], vec![]));
}

#[test]
fn duplicate_bindings_to_one_source_render_once() {
  // Two uniforms of target 2 both sample source 1.
  let g = edges(&[(1, &[]), (2, &[1, 1])]);
  assert_eq!(propagation_order(&dirty(&[1]), &g), (vec![1, 2], vec![]));
}

#[test]
fn empty_dirty_is_empty() {
  let g = edges(&[(1, &[]), (2, &[1])]);
  assert_eq!(propagation_order(&dirty(&[]), &g), (vec![], vec![]));
}

#[test]
fn cycle_members_come_back_separately() {
  // 2 <-> 3 cannot be ordered; the acyclic prefix (1) and suffix fed only by
  // it still order. 4 samples the cycle, so it lands in the cyclic set too:
  // no member of a cycle's downstream can claim a settled input.
  let g = edges(&[(1, &[]), (2, &[1, 3]), (3, &[2]), (4, &[3])]);
  let (order, cyclic) = propagation_order(&dirty(&[1]), &g);
  assert_eq!(order, vec![1]);
  assert_eq!(cyclic, vec![2, 3, 4]);
}

#[test]
fn self_loop_is_cyclic() {
  let g = edges(&[(1, &[1])]);
  assert_eq!(propagation_order(&dirty(&[1]), &g), (vec![], vec![1]));
}

#[test]
fn reaches_direct_and_transitive() {
  let s = sources(&[(2, &[1]), (3, &[2])]);
  assert!(samples_transitively(&s, &barriers(&[]), 3, 1));
  assert!(samples_transitively(&s, &barriers(&[]), 2, 1));
  assert!(!samples_transitively(&s, &barriers(&[]), 1, 3));
}

#[test]
fn reaches_is_inclusive() {
  // from == to is the self-binding rejection.
  let s = sources(&[]);
  assert!(samples_transitively(&s, &barriers(&[]), 7, 7));
}

#[test]
fn unrelated_ids_do_not_reach() {
  let s = sources(&[(2, &[1]), (4, &[3])]);
  assert!(!samples_transitively(&s, &barriers(&[]), 2, 3));
}

#[test]
fn barrier_breaks_the_path() {
  // 3 samples 2 samples 1; with 2 manual, 3 no longer reaches 1 for cycle
  // purposes (the flush never renders 2, so no flush loop can close there).
  let s = sources(&[(2, &[1]), (3, &[2])]);
  assert!(samples_transitively(&s, &barriers(&[]), 3, 1));
  assert!(!samples_transitively(&s, &barriers(&[2]), 3, 1));
}

#[test]
fn barrier_at_the_start_blocks_expansion() {
  // The new source itself being manual already breaks any cycle it would
  // close: its own edges are never flush-ordered.
  let s = sources(&[(2, &[1])]);
  assert!(!samples_transitively(&s, &barriers(&[2]), 2, 1));
}

#[test]
fn barrier_endpoint_still_hits() {
  // Reaching `to` is a hit even when `to` is a barrier: the check is "is
  // there a path", barriers only stop paths from continuing THROUGH a node.
  // (In update_shader_textures a manual `to` skips the walk entirely.)
  let s = sources(&[(2, &[1])]);
  assert!(samples_transitively(&s, &barriers(&[1]), 2, 1));
}

#[test]
fn pingpong_via_barriers_is_legal() {
  // The ping-pong shape: A(10) and B(11) sample each other, both manual.
  // Binding either direction must not count as a flush cycle.
  let s = sources(&[(10, &[11])]);
  assert!(!samples_transitively(&s, &barriers(&[10, 11]), 10, 11));
}
