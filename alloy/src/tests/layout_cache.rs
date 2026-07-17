use taffy::geometry::{Line, Point, Size};
use taffy::style::AvailableSpace;
use taffy::tree::{LayoutInput, LayoutOutput, RunMode};
use taffy::{RequestedAxis, SizingMode};

use crate::rendertree::LayoutCache;

fn compute_size_input(known_width: Option<f32>, parent_width: Option<f32>) -> LayoutInput {
  LayoutInput {
    run_mode: RunMode::ComputeSize,
    sizing_mode: SizingMode::InherentSize,
    axis: RequestedAxis::Vertical,
    known_dimensions: Size { width: known_width, height: None },
    parent_size: Size { width: parent_width, height: None },
    available_space: Size { width: AvailableSpace::Definite(900.0), height: AvailableSpace::MaxContent },
    vertical_margins_are_collapsible: Line::FALSE,
  }
}

fn output(width: f32) -> LayoutOutput {
  LayoutOutput::from_outer_size(Size { width, height: 20.0 })
}

// The regression this cache exists for: one flex pass probes a child with the
// same input shape under different parent sizes. Taffy's one-slot-per-shape
// cache clobbers on each store; ours must keep all variants live.
#[test]
fn keeps_same_shape_entries_with_different_parent_sizes() {
  let mut cache = LayoutCache::new();
  let unresolved = compute_size_input(Some(900.0), None);
  let resolved = compute_size_input(Some(900.0), Some(900.0));

  cache.store(&unresolved, output(900.0));
  cache.store(&resolved, output(900.0));

  assert!(cache.get(&unresolved).is_some());
  assert!(cache.get(&resolved).is_some());
}

#[test]
fn distinct_inputs_do_not_collide() {
  let mut cache = LayoutCache::new();
  cache.store(&compute_size_input(Some(900.0), None), output(900.0));

  assert!(cache.get(&compute_size_input(Some(800.0), None)).is_none());
  assert!(cache.get(&compute_size_input(Some(900.0), Some(900.0))).is_none());
  assert!(cache.get(&compute_size_input(None, None)).is_none());
}

#[test]
fn same_key_store_updates_in_place() {
  let mut cache = LayoutCache::new();
  let input = compute_size_input(Some(900.0), None);
  cache.store(&input, output(900.0));
  cache.store(&input, output(555.0));

  let hit = cache.get(&input).expect("stored input should hit");
  assert_eq!(hit.size.width, 555.0);
}

#[test]
fn ring_evicts_oldest_when_full() {
  let mut cache = LayoutCache::new();
  for i in 0..17 {
    cache.store(&compute_size_input(Some(i as f32), None), output(i as f32));
  }

  // 17 distinct keys through a 16-slot ring: the first is gone, the rest hit.
  assert!(cache.get(&compute_size_input(Some(0.0), None)).is_none());
  assert!(cache.get(&compute_size_input(Some(16.0), None)).is_some());
  assert!(cache.get(&compute_size_input(Some(1.0), None)).is_some());
}

#[test]
fn final_layout_entry_round_trips() {
  let mut cache = LayoutCache::new();
  let mut input = compute_size_input(Some(900.0), Some(900.0));
  input.run_mode = RunMode::PerformLayout;
  input.axis = RequestedAxis::Both;

  let mut out = output(900.0);
  out.first_baselines = Point { x: None, y: Some(14.0) };
  cache.store(&input, out);

  let hit = cache.get(&input).expect("final layout should hit");
  assert_eq!(hit.first_baselines.y, Some(14.0));

  // A different parent height must miss: the final key matches the full input.
  let mut other = input;
  other.parent_size.height = Some(500.0);
  assert!(cache.get(&other).is_none());
}

#[test]
fn clear_empties_the_cache() {
  let mut cache = LayoutCache::new();
  let input = compute_size_input(Some(900.0), None);
  assert!(cache.is_empty());

  cache.store(&input, output(900.0));
  assert!(!cache.is_empty());

  cache.clear();
  assert!(cache.is_empty());
  assert!(cache.get(&input).is_none());
}
