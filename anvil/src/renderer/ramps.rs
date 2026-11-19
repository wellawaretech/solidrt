//! The gradient ramp atlas's rows: which gradient holds which row of the
//! one RAMP_WIDTH x RAMP_ROWS ramp texture, kept across frames, so a frame
//! that draws the gradients of the frame before uploads nothing and a
//! gradient batch never rewrites a texture another batch samples. Pure, no
//! GL: the walk assigns the rows (`walk::walk`), the executor uploads the
//! rows assigned since its last upload (`take_dirty`), the tests read the
//! table with no context (`tests/ramps.rs`).

use crate::RampGradient;
use std::sync::Arc;

/// Rows of the ramp atlas: 256 KiB of rgba8 at RAMP_WIDTH; a frame with
/// more distinct gradients draws the rest solid.
pub const RAMP_ROWS: u16 = 256;

/// The rows of the ramp atlas and the gradient each holds, with the frame
/// each was last used in. A gradient is its `Arc`: two draws of one shared
/// gradient take one row.
pub struct RampTable {
  /// One entry per row; None for a row no gradient has taken yet.
  entries: Vec<Option<Entry>>,
  /// The current frame, counted by `begin_frame`. A frame of the table is
  /// one `Executor::draw` call: a window frame with offscreen layer draws
  /// begins several.
  frame: u64,
  /// The rows (re)assigned since the last `take_dirty`, in assignment
  /// order: their texels are not uploaded yet.
  dirty: Vec<u16>,
}

/// A row's gradient. The entry holds the `Arc`, so the gradient's address
/// cannot be reused by another gradient while the row is cached.
struct Entry {
  gradient: Arc<RampGradient>,
  last_used: u64,
}

impl Default for RampTable {
  fn default() -> Self {
    Self::new()
  }
}

impl RampTable {
  /// An empty table: every row free.
  pub fn new() -> Self {
    RampTable { entries: (0..RAMP_ROWS).map(|_| None).collect(), frame: 0, dirty: Vec::new() }
  }

  /// Start a frame, one `Executor::draw` call (not a window frame, which
  /// may draw several lists): the rows used before it become evictable.
  pub fn begin_frame(&mut self) {
    self.frame += 1;
  }

  /// The row `gradient` draws from this frame: the row it already holds,
  /// else a free row, else the least recently used row not used this
  /// frame; None when every row is taken this frame. A new assignment
  /// marks the row dirty.
  pub fn row(&mut self, gradient: &Arc<RampGradient>) -> Option<u16> {
    let frame = self.frame;
    let held = self.entries.iter().position(|e| e.as_ref().is_some_and(|e| Arc::ptr_eq(&e.gradient, gradient)));
    if let Some(index) = held {
      if let Some(entry) = self.entries[index].as_mut() {
        entry.last_used = frame;
      }
      return Some(index as u16);
    }
    let free = self.entries.iter().position(|e| e.is_none());
    let victim = free.or_else(|| {
      self
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.as_ref().map(|e| (i, e.last_used)))
        .filter(|(_, last_used)| *last_used < frame)
        .min_by_key(|(_, last_used)| *last_used)
        .map(|(i, _)| i)
    })?;
    self.entries[victim] = Some(Entry { gradient: gradient.clone(), last_used: frame });
    let row = victim as u16;
    self.dirty.push(row);
    Some(row)
  }

  /// The rows (re)assigned since the last call, leaving none.
  pub fn take_dirty(&mut self) -> Vec<u16> {
    std::mem::take(&mut self.dirty)
  }

  /// The texels of `row`'s gradient; empty for a row no gradient holds.
  pub fn ramp(&self, row: u16) -> &[[u8; 4]] {
    match self.entries.get(row as usize) {
      Some(Some(entry)) => &entry.gradient.ramp,
      _ => &[],
    }
  }
}
