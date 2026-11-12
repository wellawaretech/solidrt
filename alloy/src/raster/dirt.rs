//! The raster thread's pending target dirt (`RasterState::dirty`): per
//! texture id, full dirt or rectangle dirt (D8 of
//! okf/design/draw-order-and-culling.md). Every dirtying that names no
//! rectangle is full, and full always wins: rectangles never narrow a full
//! dirt, and a full dirt drops the rectangles before it. Rectangles sit on a
//! draw target or a sub-target, in that owner's own pixel space, and union
//! until the flush renders the owner. Pure, so it unit-tests without GL.

use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;

use crate::gpu::{DamageRects, PxRect};

/// One id's pending dirt.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dirt {
  /// Everything the id's pixels hold may have changed.
  Full,
  /// Only the pixels inside the rectangles changed.
  Rects(DamageRects),
}

/// The ids whose content changed since the last flush, each with its dirt.
#[derive(Default)]
pub(crate) struct DirtySet {
  pending: HashMap<u64, Dirt>,
}

impl DirtySet {
  pub(crate) fn new() -> DirtySet {
    DirtySet::default()
  }

  /// Dirty `id` in full, dropping any rectangles it held.
  pub(crate) fn insert(&mut self, id: u64) {
    self.pending.insert(id, Dirt::Full);
  }

  /// Dirty every id of `ids` in full.
  pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = u64>) {
    for id in ids {
      self.insert(id);
    }
  }

  /// Dirty `id` inside `rects` only: nothing on an id that is full, the
  /// rectangles on a clean id, merged into those of a rectangle-dirty id
  /// (`DamageRects::add`). Empty rectangles are dropped, so a clean id
  /// given none stays clean.
  pub(crate) fn damage(&mut self, id: u64, rects: &[PxRect]) {
    match self.pending.get_mut(&id) {
      Some(Dirt::Full) => {}
      Some(Dirt::Rects(set)) => set.extend(rects.iter().copied()),
      None => {
        let mut set = DamageRects::new();
        set.extend(rects.iter().copied());
        if !set.is_empty() {
          self.pending.insert(id, Dirt::Rects(set));
        }
      }
    }
  }

  pub(crate) fn remove(&mut self, id: &u64) {
    self.pending.remove(id);
  }

  /// Whether `id` is dirty at all, full or inside rectangles.
  pub(crate) fn contains(&self, id: &u64) -> bool {
    self.pending.contains_key(id)
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.pending.is_empty()
  }

  pub(crate) fn clear(&mut self) {
    self.pending.clear();
  }

  /// Whether `id` is dirty in full.
  pub(crate) fn is_full(&self, id: &u64) -> bool {
    matches!(self.pending.get(id), Some(Dirt::Full))
  }

  /// The rectangles of a rectangle-dirty `id`; None for a clean or full one.
  pub(crate) fn rects(&self, id: &u64) -> Option<&DamageRects> {
    match self.pending.get(id) {
      Some(Dirt::Rects(set)) => Some(set),
      _ => None,
    }
  }

  /// Whether any id is rectangle-dirty.
  pub(crate) fn has_rects(&self) -> bool {
    self.pending.values().any(|d| matches!(d, Dirt::Rects(_)))
  }

  /// The dirty ids, whatever their dirt.
  #[cfg(test)]
  pub(crate) fn ids(&self) -> HashSet<u64> {
    self.pending.keys().copied().collect()
  }
}
