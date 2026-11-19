// The ramp atlas's rows (renderer/ramps.rs): which gradient holds which
// row across frames, the rows an upload owes, and the cap, with no GL
// context.

use crate::renderer::ramps::{RampTable, RAMP_ROWS};
use crate::{Color, GradientKind, RampGradient, TileMode};
use std::sync::Arc;

// A gradient of its own: a fresh Arc, so a distinct one to the table.
fn gradient() -> Arc<RampGradient> {
  let red = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
  Arc::new(RampGradient {
    kind: GradientKind::Linear,
    x_row: [1.0, 0.0, 0.0],
    y_row: [0.0; 3],
    tile: TileMode::Clamp,
    ramp: RampGradient::ramp(&[red], &[0.0]).expect("one stop"),
  })
}

#[test]
fn the_same_gradient_keeps_its_row() {
  let mut table = RampTable::new();
  table.begin_frame();
  let g = gradient();
  let row = table.row(&g).expect("a free row");
  assert_eq!(table.row(&g), Some(row));
  assert_eq!(table.row(&g.clone()), Some(row), "a clone of the Arc is the same gradient");
  assert_eq!(table.take_dirty(), vec![row]);
  assert_eq!(table.ramp(row), &g.ramp[..]);
}

#[test]
fn two_gradients_take_two_rows() {
  let mut table = RampTable::new();
  table.begin_frame();
  let (a, b) = (gradient(), gradient());
  let (ra, rb) = (table.row(&a).expect("a free row"), table.row(&b).expect("a free row"));
  assert_ne!(ra, rb);
  assert_eq!(table.take_dirty(), vec![ra, rb]);
}

#[test]
fn a_row_reused_in_a_later_frame_is_not_dirty() {
  let mut table = RampTable::new();
  let g = gradient();
  table.begin_frame();
  let row = table.row(&g).expect("a free row");
  assert_eq!(table.take_dirty(), vec![row]);
  table.begin_frame();
  assert_eq!(table.row(&g), Some(row));
  assert!(table.take_dirty().is_empty());
}

#[test]
fn the_victim_is_the_oldest_row_unused_this_frame() {
  // The oldest row is not the lowest: a first-free or first-evictable
  // choice would take row 0.
  const OLDEST: usize = 3;
  let mut table = RampTable::new();
  let all: Vec<_> = (0..RAMP_ROWS).map(|_| gradient()).collect();
  table.begin_frame();
  for g in &all {
    table.row(g).expect("a free row");
  }
  // The next frame uses every row but OLDEST.
  table.begin_frame();
  for (row, g) in all.iter().enumerate().filter(|(row, _)| *row != OLDEST) {
    assert_eq!(table.row(g), Some(row as u16));
  }
  table.take_dirty();
  // The frame after it: every row is evictable, OLDEST is the oldest.
  table.begin_frame();
  assert_eq!(table.row(&gradient()), Some(OLDEST as u16));
  assert_eq!(table.take_dirty(), vec![OLDEST as u16]);
}

#[test]
fn a_row_used_this_frame_is_never_evicted() {
  let mut table = RampTable::new();
  let all: Vec<_> = (0..RAMP_ROWS).map(|_| gradient()).collect();
  table.begin_frame();
  for g in &all {
    table.row(g).expect("a free row");
  }
  // Past the capacity in the same frame: none.
  assert_eq!(table.row(&gradient()), None);
  // Every row still holds its gradient.
  for (row, g) in all.iter().enumerate() {
    assert_eq!(table.row(g), Some(row as u16));
  }
}

#[test]
fn take_dirty_drains() {
  let mut table = RampTable::new();
  table.begin_frame();
  table.row(&gradient()).expect("a free row");
  assert_eq!(table.take_dirty().len(), 1);
  assert!(table.take_dirty().is_empty());
}
