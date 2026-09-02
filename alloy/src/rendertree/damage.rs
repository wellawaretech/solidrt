//! Partial repaint, UI side (okf/done/partial-repaint.md): the ledger that
//! accumulates per-node damage between frames and resolves it into a
//! frame's FrameDamage. One owner: both resolution paths - the paint walk
//! (composite::paint_phase) and the walk-less reuse path
//! (composite::resolve_reuse_damage) - end in a `resolve_*` here, so the
//! backdrop widening always runs before the window clamp by construction,
//! not by call-site convention.

use crate::rendertree::cull;
use crate::rendertree::{BackdropRegion, FrameDamage, Rect};
use std::collections::HashSet;

// Damage accumulation cap: past this many damaged nodes in one frame the
// frame is treated as fully damaged instead of paying per-node rect math (a
// resize-scale relayout damages nearly every node, and a near-window union
// is worth no less than a full repaint).
const MAX_DAMAGED_NODES: usize = 256;

/// The tree's damage state. Accumulation: every damage and structural
/// mutation path funnels into `note` (RenderTree::note_damage); a resolve
/// drains it with `take` and settles the union through `resolve_walk` or
/// `resolve_reuse`.
pub(crate) struct DamageLedger {
  // Nodes whose on-screen pixels may differ from the last painted frame;
  // resolved against the elements' last_extent cells by the caller.
  damaged: HashSet<u64>,
  damaged_full: bool,
  // The damage the last painted frame resolved to, for stats and the
  // raster pass. A reuse frame does not write it: nothing was painted.
  frame_damage: FrameDamage,
  // Backdrop-filter regions as of the last paint walk (window space + blur
  // reach). Both resolves widen any damage rect touching one to the whole
  // panel, walk or no walk.
  backdrop_regions: Vec<BackdropRegion>,
}

impl DamageLedger {
  /// Starts fully damaged: nothing has been painted yet.
  pub(crate) fn new() -> Self {
    Self { damaged: HashSet::new(), damaged_full: true, frame_damage: FrameDamage::Full, backdrop_regions: Vec::new() }
  }

  /// Note that a node's on-screen pixels may differ from the last painted
  /// frame; past the cap the frame degrades to full damage.
  pub(crate) fn note(&mut self, id: u64) {
    if self.damaged_full {
      return;
    }
    if self.damaged.len() >= MAX_DAMAGED_NODES {
      self.all();
      return;
    }
    self.damaged.insert(id);
  }

  /// Degrade the frame being accumulated to full damage (resize, re-root).
  pub(crate) fn all(&mut self) {
    self.damaged_full = true;
    self.damaged.clear();
  }

  /// Drain the accumulated damage for the frame being resolved: the damaged
  /// node ids, and whether the frame is fully damaged regardless of them.
  pub(crate) fn take(&mut self) -> (Vec<u64>, bool) {
    let full = self.damaged_full;
    self.damaged_full = false;
    (self.damaged.drain().collect(), full)
  }

  /// The damage the last painted frame resolved to (`resolve_walk` writes it).
  pub(crate) fn frame_damage(&self) -> FrameDamage {
    self.frame_damage
  }

  /// Resolve a paint walk's damage union: adopt the walk's fresh backdrop
  /// regions, widen by them, clamp to the window, and record the result as
  /// the painted frame's damage.
  pub(crate) fn resolve_walk(
    &mut self,
    damage: cull::Extent,
    full: bool,
    regions: Vec<BackdropRegion>,
    window: Rect,
  ) -> FrameDamage {
    self.backdrop_regions = regions;
    let resolved = self.resolve(damage, full, window);
    self.frame_damage = resolved;
    resolved
  }

  /// Resolve WITHOUT a walk (the present-only reuse path): the tree is
  /// unchanged, so the retained regions from the last walk still hold.
  /// `frame_damage` keeps the last painted frame's value.
  pub(crate) fn resolve_reuse(&self, damage: cull::Extent, full: bool, window: Rect) -> FrameDamage {
    self.resolve(damage, full, window)
  }

  fn resolve(&self, damage: cull::Extent, full: bool, window: Rect) -> FrameDamage {
    clamp_damage(expand_damage_for_backdrops(damage, &self.backdrop_regions), full, window)
  }
}

// A backdrop panel re-filters what lies beneath it, so a change within the
// blur's reach of a panel changes the panel's own pixels too - the repaint
// rect must grow to cover the whole panel, or the blit leaves its edge
// stale. An unmappable region (non-2D transform) makes any damage full-frame
// rather than risking that. Iterates because one panel's growth can reach
// another; bounded by the region count.
pub(crate) fn expand_damage_for_backdrops(damage: cull::Extent, regions: &[BackdropRegion]) -> cull::Extent {
  if regions.is_empty() {
    return damage;
  }
  let mut current = match damage {
    cull::Extent::Bounded(r) => r,
    other => return other,
  };
  if regions.iter().any(Option::is_none) {
    return cull::Extent::Unbounded;
  }
  for _ in 0..regions.len() {
    let mut grew = false;
    for entry in regions {
      let Some((region, reach)) = entry else { continue };
      if current.intersects(&region.inflate(*reach, *reach)) && !current.contains_rect(region) {
        current = current.union(region);
        grew = true;
      }
    }
    if !grew {
      break;
    }
  }
  cull::Extent::Bounded(current)
}

// A resolved damage union cut to the window and to the FrameDamage form.
fn clamp_damage(damage: cull::Extent, full: bool, window_rect: Rect) -> FrameDamage {
  if full {
    return FrameDamage::Full;
  }
  match damage {
    cull::Extent::Empty => FrameDamage::None,
    cull::Extent::Unbounded => FrameDamage::Full,
    cull::Extent::Bounded(r) => match r.intersection(&window_rect) {
      // Damage entirely outside the window changes no visible pixel.
      None => FrameDamage::None,
      Some(clamped) if clamped.contains_rect(&window_rect) => FrameDamage::Full,
      Some(clamped) => FrameDamage::Rect(clamped),
    },
  }
}
