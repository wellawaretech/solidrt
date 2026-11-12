use std::collections::{HashMap, HashSet};

use super::Context;
use crate::gpu::{Dirt, DirtySet, PxRect, TargetDirt};

impl Context {
  /// Record that texture `id`'s pixels changed (or will, at the next dirty
  /// flush) behind its unchanged id, anywhere in it, plus everything
  /// downstream: every flush-rendered target sampling it re-renders,
  /// transitively (see `content_closure`). Noting an already-full id is a
  /// no-op, so a per-frame write burst on one target costs one closure
  /// walk. Drained by `take_content_changes`.
  pub(crate) fn note_content(&self, id: u64) {
    self.note_content_dirt(id, &TargetDirt::Full);
  }

  /// `note_content` with the dirt the change declares: full, or only inside
  /// rectangles of `id`'s own pixels (a draw list's write, the one change
  /// that knows where it lands). The rectangles follow the dirt model's
  /// rules (`DirtySet`): they never narrow a full change and union with the
  /// ones already noted. What samples `id` downstream changes in full
  /// either way, and so does its depth texture.
  pub(super) fn note_content_dirt(&self, id: u64, dirt: &TargetDirt) {
    // A sub-target's pixels are its parent's, its rectangles moved by its
    // own origin there.
    let (id, origin) = match self.sub_targets.borrow().get(&id) {
      Some(tile) => (tile.parent, (tile.x, tile.y)),
      None => (id, (0, 0)),
    };
    let mut changes = self.content_changes.borrow_mut();
    let noted = changes.contains(&id);
    match dirt {
      TargetDirt::Full if changes.is_full(&id) => return,
      TargetDirt::Full => changes.insert(id),
      TargetDirt::Rects(rects) => {
        let moved: Vec<PxRect> = rects.iter().map(|r| r.translated(origin.0, origin.1)).collect();
        changes.damage(id, &moved);
      }
    }
    // The downstream walk ran when the id was first noted; rectangles that
    // are all empty note nothing.
    if noted || !changes.contains(&id) {
      return;
    }
    // A draw target's depth texture changes with its color: the same
    // render writes both.
    if let Some(depth) = self.depth_of(id) {
      changes.insert(depth);
    }
    content_closure(&self.shader_sources.borrow(), &self.manual_targets.borrow(), id, &mut changes);
  }

  /// `note_content` for a mutated target: a manual target's pixels hold
  /// until an explicit render or copy steps it (those note then), so a
  /// params/entry/range write to one is not a content change yet.
  pub(super) fn note_target_content(&self, id: u64) {
    self.note_target_content_dirt(id, &TargetDirt::Full);
  }

  /// `note_target_content` with the dirt a draw list's write declares.
  pub(super) fn note_target_content_dirt(&self, id: u64, dirt: &TargetDirt) {
    if self.manual_targets.borrow().contains(&id) {
      return;
    }
    self.note_content_dirt(id, dirt);
  }

  /// `note_content` for a buffer write: every flush-rendered target drawing
  /// from the buffer re-renders with the new geometry, so each such target's
  /// pixels change content.
  pub(super) fn note_buffer_content(&self, buffer: u64) {
    let affected: Vec<u64> = {
      let targets = self.targets.borrow();
      let manual = self.manual_targets.borrow();
      targets
        .iter()
        .filter(|(id, mirror)| {
          !manual.contains(id)
            && (mirror.buffers.reads(buffer)
              || mirror.entries.as_ref().is_some_and(|l| l.entries.values().any(|e| e.buffers.reads(buffer))))
        })
        .map(|(id, _)| *id)
        .collect()
    };
    for id in affected {
      self.note_content(id);
    }
  }

  /// Drain the texture ids whose pixels changed since the last drain, each
  /// with its dirt: full, or the rectangles of its own pixels the change
  /// stayed inside. The frame build takes these before its clean check and
  /// applies them to the tree (`RenderTree::texture_content_changed`): as
  /// window damage where a node shows the texture, and as a re-bake on the
  /// snapshot boundaries that baked those pixels. Two changes are full
  /// whatever rectangles they hold: every one while the damage debug switch
  /// is on (`set_force_full_renders`), so a wrong rectangle shows as stale
  /// window pixels that go away under the switch, as it does in the target;
  /// and a texture with a mip chain, whose levels spread a changed texel
  /// over more of what a minified draw shows than the rectangle maps to.
  pub fn take_content_changes(&self) -> DirtySet {
    let mut changes = std::mem::take(&mut *self.content_changes.borrow_mut());
    let force = self.force_full_renders.get();
    let widened: Vec<u64> = changes
      .iter()
      .filter(|(id, dirt)| {
        matches!(dirt, Dirt::Rects(_)) && (force || self.textures.get(*id).is_some_and(|e| e.sampler().mipmap))
      })
      .map(|(id, _)| id)
      .collect();
    changes.extend(widened);
    changes
  }
}

/// Whether `to` is reachable from `from` (inclusive: `from == to` is a hit)
/// by following sampler edges in `sources` (target id -> its source id per
/// (draw entry, uniform name) binding) without passing through a node in
/// `barriers`: the sampling-cycle test behind every bind path. Barriers are
/// the manual targets - the flush never renders one, so a path through one
/// can never be part of a flush-ordered feedback loop and does not count.
/// `tiles` (parent -> its sub-targets) folds a tile's edges into its
/// parent's: the parent's pass draws what the tiles bind. Pure over the id
/// graph, so it unit-tests without a Context.
pub(crate) fn samples_transitively(
  sources: &HashMap<u64, HashMap<(u64, String), u64>>,
  barriers: &HashSet<u64>,
  tiles: &HashMap<u64, Vec<u64>>,
  from: u64,
  to: u64,
) -> bool {
  let mut stack = vec![from];
  let mut visited: HashSet<u64> = HashSet::new();
  while let Some(node) = stack.pop() {
    if node == to {
      return true;
    }
    if visited.insert(node) && !barriers.contains(&node) {
      for member in std::iter::once(&node).chain(tiles.get(&node).into_iter().flatten()) {
        if let Some(srcs) = sources.get(member) {
          stack.extend(srcs.values().copied());
        }
      }
    }
  }
  false
}

/// Every texture id bound as a sampler source on any recorded target: the
/// GPU side's references, which the render tree cannot see. A target that is
/// itself pending destroy still counts until it is reclaimed (its record
/// leaves `sources` then); `reclaim_destroyed` iterates so its sources
/// follow in the same sweep.
pub(crate) fn bound_sources(sources: &HashMap<u64, HashMap<(u64, String), u64>>) -> HashSet<u64> {
  sources.values().flat_map(|bindings| bindings.values().copied()).collect()
}

/// Collect into `changes` the flush-rendered targets whose pixels change
/// when `root`'s content does: everything sampling it, transitively, walking
/// the sampler graph upstream-to-downstream. Manual targets stop the walk -
/// the flush never renders one, so its pixels hold until an explicit render
/// steps it (which notes content itself, resuming propagation from there).
/// `root` itself is the caller's call: a stepped manual target counts, a
/// written-but-manual one does not. Pure over the id graph, so it unit-tests
/// without a Context (like `samples_transitively`).
pub(crate) fn content_closure(
  sources: &HashMap<u64, HashMap<(u64, String), u64>>,
  manual: &HashSet<u64>,
  root: u64,
  changes: &mut DirtySet,
) {
  let mut stack = vec![root];
  while let Some(id) = stack.pop() {
    for (target, bindings) in sources.iter() {
      // A target noted with rectangles only (its own list's write) is not
      // done: a changed source changes all of it.
      if manual.contains(target) || changes.is_full(target) {
        continue;
      }
      if bindings.values().any(|src| *src == id) {
        changes.insert(*target);
        stack.push(*target);
      }
    }
  }
}
