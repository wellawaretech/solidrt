use std::collections::{HashMap, HashSet};

use super::Context;

impl Context {
  /// Record that texture `id`'s pixels changed (or will, at the next dirty
  /// flush) behind its unchanged id, plus everything downstream: every
  /// flush-rendered target sampling it re-renders, transitively (see
  /// `content_closure`). Inserting an already-noted id is a no-op, so a
  /// per-frame write burst on one target costs one closure walk. Drained by
  /// `take_content_changes`.
  pub(super) fn note_content(&self, id: u64) {
    let mut changes = self.content_changes.borrow_mut();
    if !changes.insert(id) {
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
    if self.manual_targets.borrow().contains(&id) {
      return;
    }
    self.note_content(id);
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

  /// Drain the texture ids whose pixels changed since the last drain. The
  /// frame build takes these before its clean check and applies them as
  /// damage on the snapshot boundaries that baked those pixels
  /// (`RenderTree::texture_content_changed`); everything else keeps live
  /// texture references and needs no damage for a content change.
  pub fn take_content_changes(&self) -> HashSet<u64> {
    std::mem::take(&mut *self.content_changes.borrow_mut())
  }
}

/// Whether `to` is reachable from `from` (inclusive: `from == to` is a hit)
/// by following sampler edges in `sources` (target id -> its source id per
/// (draw entry, uniform name) binding) without passing through a node in
/// `barriers`: the sampling-cycle test behind every bind path. Barriers are
/// the manual targets - the flush never renders one, so a path through one
/// can never be part of a flush-ordered feedback loop and does not count.
/// Pure over the id graph, so it unit-tests without a Context.
pub(crate) fn samples_transitively(
  sources: &HashMap<u64, HashMap<(u64, String), u64>>,
  barriers: &HashSet<u64>,
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
      if let Some(srcs) = sources.get(&node) {
        stack.extend(srcs.values().copied());
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
  changes: &mut HashSet<u64>,
) {
  let mut stack = vec![root];
  while let Some(id) = stack.pop() {
    for (target, bindings) in sources.iter() {
      if manual.contains(target) || changes.contains(target) {
        continue;
      }
      if bindings.values().any(|src| *src == id) {
        changes.insert(*target);
        stack.push(*target);
      }
    }
  }
}
