//! Tree-side transition plumbing: starting enter/exit tracks against nodes,
//! the stagger ladder, the JS write interception, and the per-frame advance
//! that settles tracks and applies damage. The element-side state (configs,
//! tracks, property mapping) lives in `rendertree::transitions`; the node
//! map and damage application this drives live in the parent module.

use super::RenderTree;
use crate::rendertree::transitions::{AnimValue, PendingWrite};
use crate::rendertree::{AnimProp, Damage, Point, Slide, Vector};

impl RenderTree {
  /// Mount-time enter animations: a per-property `from` in the node's
  /// transition declaration snaps the property to `from` and starts a track
  /// toward the value it mounted with (with the entry's delay honored - the
  /// element sits at `from` until the hold expires). Runs from the advance
  /// of the frame that attached the node (insert_node queues it), after the
  /// tick's script work and before the paint: whatever the mount tick wrote
  /// after the insert - the config itself, the mounted values - has landed
  /// and snapped, and the node's first painted frame is at `from`. Fires
  /// on the node's first attach only; a move or reorder re-runs nothing. A
  /// property whose mounted value is unreadable (no explicit value, a
  /// gradient) skips its enter animation and simply shows the mounted
  /// state. A node detached again before the advance enters at its next
  /// attach; one already on its way out (removed the same tick, under an
  /// exit root) spends its enter and leaves from its mounted state, so the
  /// enter pass never retargets an exit track back toward the mount.
  fn apply_enter_transitions(&mut self, node_id: u64) {
    let leaving = self.exit_root_of(node_id).is_some();
    let entries: Vec<(AnimProp, crate::rendertree::TransitionEntry)> = {
      let Some(el) = self.nodes.get_mut(&node_id) else { return };
      if el.entered || el.parent.is_none() {
        return;
      }
      el.entered = true;
      if leaving {
        return;
      }
      match &el.transitions {
        Some(t) => t.props.iter().filter(|(_, e)| e.from.is_some()).cloned().collect(),
        None => return,
      }
    };
    if entries.is_empty() {
      return;
    }
    // One stagger index per node, shared by all its entering properties, so
    // a multi-property enter moves as one item of the cascade.
    let stagger = self.stagger_delay_for(node_id, false);
    let now = self.transitions.now_ms;
    for (prop, entry) in entries {
      let Some(enter) = entry.from else { continue };
      let from = enter.value;
      let Some(target) = self.nodes.get(&node_id).and_then(|el| el.anim_value(prop)) else { continue };
      if std::mem::discriminant(&from) != std::mem::discriminant(&target) {
        continue;
      }
      let damage = self.nodes.get_mut(&node_id).map(|el| el.set_anim_value(prop, from)).unwrap_or(Damage::None);
      self.apply_damage(node_id, damage);
      let delay_ms = enter.delay_ms + stagger;
      if delay_ms > 0.0 {
        let at_ms = now + delay_ms as f64;
        self.transitions.schedule(PendingWrite { node: node_id, prop, to: target, spec: enter.spec, at_ms });
      } else {
        self.transitions.retarget(node_id, prop, from, target, enter.spec, now);
      }
    }
  }

  /// The exit root that owns `node_id`'s removal, if any: the nearest
  /// ancestor-or-self marked `exiting`. Structure is membership - a node
  /// under an exit root leaves with it, whatever its own tracks do - so
  /// there is no per-member state to keep in step, and every event on the
  /// way out (a settle, a destroy, a second detach, a re-insert, an owed
  /// enter) finds its cascade through this climb.
  pub fn exit_root_of(&self, node_id: u64) -> Option<u64> {
    let mut cursor = Some(node_id);
    while let Some(id) = cursor {
      let el = self.nodes.get(&id)?;
      if el.exiting {
        return Some(id);
      }
      cursor = el.parent;
    }
    None
  }

  /// `root` and its descendants in tree order (pre-order, children order).
  /// `skip_nested_exits` leaves out any subtree under an exiting node other
  /// than `root` itself: a cascade of its own, already running.
  fn subtree(&self, root: u64, skip_nested_exits: bool) -> Vec<u64> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
      let Some(el) = self.nodes.get(&id) else { continue };
      if skip_nested_exits && id != root && el.exiting {
        continue;
      }
      out.push(id);
      stack.extend(el.children.iter().rev().copied());
    }
    out
  }

  /// Start the exit animation instead of detaching, when the removed
  /// subtree declares `exit` values and at least one has somewhere to move.
  /// Every node under `node_id` that declares exits starts them, in tree
  /// order so the stagger cascade mirrors the enter's, and `node_id`
  /// becomes the exit root: it stays linked, out of the layout flow and
  /// hit-test invisible with its subtree, until the last of those tracks
  /// settles (`advance_transitions` -> `finish_exit`). Returns whether the
  /// node is now exiting (still linked). A subtree whose exit values all
  /// already hold (or are unreadable) detaches instantly - an exit that
  /// animates nothing must not defer the removal.
  ///
  /// A second removal reaching a cascade in flight (the node is under an
  /// exit root already) starts nothing and keeps the node where it is: the
  /// root's free takes it. A nested root already exiting keeps its own
  /// cascade, skipped by the walk; the outer gate waits for it all the same.
  pub(super) fn begin_exit(&mut self, parent_id: u64, node_id: u64) -> bool {
    match self.nodes.get(&node_id) {
      Some(el) if el.parent == Some(parent_id) => {}
      _ => return false,
    }
    if self.exit_root_of(node_id).is_some() {
      return true;
    }
    let mut started = false;
    for id in self.subtree(node_id, true) {
      started |= self.start_exit_tracks(id);
    }
    if started {
      if let Some(el) = self.nodes.get_mut(&node_id) {
        el.exiting = true;
      }
      self.pop_from_layout(parent_id, node_id);
    }
    started
  }

  /// One node's share of a cascade: a track (or a held write, with the
  /// exit's delay plus the stagger slot) per declared `exit` value that
  /// has somewhere to move. Returns whether anything started.
  fn start_exit_tracks(&mut self, node_id: u64) -> bool {
    let entries: Vec<(AnimProp, crate::rendertree::TransitionEntry)> = match self.nodes.get(&node_id) {
      Some(el) => match &el.transitions {
        Some(t) => t.props.iter().filter(|(_, e)| e.exit.is_some()).cloned().collect(),
        None => return false,
      },
      None => return false,
    };
    if entries.is_empty() {
      return false;
    }
    // One stagger index per node, shared by all its exiting properties.
    let stagger = self.stagger_delay_for(node_id, true);
    let now = self.transitions.now_ms;
    let mut started = false;
    for (prop, entry) in entries {
      let Some(exit) = entry.exit else { continue };
      let to = exit.value;
      let Some(current) = self.nodes.get(&node_id).and_then(|el| el.anim_value(prop)) else { continue };
      if std::mem::discriminant(&current) != std::mem::discriminant(&to) {
        continue;
      }
      let delay_ms = exit.delay_ms + stagger;
      if delay_ms > 0.0 {
        let at_ms = now + delay_ms as f64;
        self.transitions.schedule(PendingWrite { node: node_id, prop, to, spec: exit.spec, at_ms });
        started = true;
      } else {
        started |= self.transitions.retarget(node_id, prop, current, to, exit.spec, now);
      }
    }
    started
  }

  /// The properties of a node's transition declaration that carry an `exit`
  /// value - the set whose tracks gate its exit root's free.
  fn exit_props(&self, node_id: u64) -> Vec<AnimProp> {
    self
      .nodes
      .get(&node_id)
      .and_then(|el| el.transitions.as_ref())
      .map(|t| t.props.iter().filter(|(_, e)| e.exit.is_some()).map(|(p, _)| *p).collect())
      .unwrap_or_default()
  }

  /// The exit root's liveness gate: whether any declared exit under `root`
  /// still runs. The walk includes nested roots, so an outer removal waits
  /// for an inner cascade already in flight instead of cutting it short.
  fn subtree_exit_running(&self, root: u64) -> bool {
    self.subtree(root, false).into_iter().any(|id| self.transitions.any_running(id, &self.exit_props(id)))
  }

  /// A re-insert reached a node on its way out: the removal turned out to
  /// be a move. Drops the exit tracks under the node (it holds its current
  /// values; later writes take over as usual) and clears its root marks. A
  /// nested root exiting on its own keeps its cascade. A descendant the
  /// renderer already destroyed while the cascade ran (`doomed` under a
  /// root) has no proxy left to move, so it is freed here instead of
  /// travelling with the subtree. A member moved out of a root leaves the
  /// root's gate to the next advance, which frees the root if that was its
  /// last running exit.
  pub(super) fn abandon_exit(&mut self, node_id: u64) {
    let Some(root) = self.exit_root_of(node_id) else { return };
    let members = self.subtree(node_id, true);
    for &id in &members {
      let props = self.exit_props(id);
      self.transitions.cancel_props(id, &props);
    }
    if let Some(el) = self.nodes.get_mut(&node_id) {
      el.exiting = false;
      el.doomed = false;
    }
    let orphans: Vec<u64> = members
      .into_iter()
      .filter(|&id| id != node_id && self.nodes.get(&id).map(|el| el.doomed).unwrap_or(false))
      .collect();
    for id in orphans {
      if self.nodes.contains_key(&id) {
        self.destroy_node(id);
      }
    }
    if root != node_id {
      self.transitions.exit_checks.push(root);
    }
  }

  /// The last exit track under an exit root settled: complete the removal
  /// that was deferred at detach - unlink, and free the subtree if the
  /// deferred destroy already ran (the renderer's sweep found the node
  /// exiting). Returns the exit root the node was nested in, if any: its
  /// gate may have just emptied and wants the same check.
  fn finish_exit(&mut self, node_id: u64) -> Option<u64> {
    for id in self.subtree(node_id, false) {
      self.transitions.cancel_node(id);
    }
    let el = self.nodes.get_mut(&node_id)?;
    el.exiting = false;
    let doomed = el.doomed;
    let parent = el.parent;
    let outer = parent.and_then(|p| self.exit_root_of(p));
    if let Some(parent_id) = parent {
      if self.nodes.contains_key(&parent_id) {
        self.detach_node_now(parent_id, node_id);
      }
    }
    if doomed {
      self.delete_recursive(node_id);
      self.bump_revision();
    }
    outer
  }

  /// `edit` for writes decoded from untrusted input (the FFI property path):
  /// on Err nothing is invalidated and the error returns to the caller to
  /// surface as a script error instead of a process abort.
  /// Stamp the animation clock: the app-timeline time (ms) of the frame
  /// about to run, set by the embedder before the frame's script work so
  /// writes (track starts) and the advance agree on time. The paced clock's
  /// pause/scale/step semantics ride in through this value.
  pub fn set_transition_now(&mut self, now_ms: f64) {
    self.transitions.now_ms = now_ms;
    // Stagger indices are per frame: each stamp opens a fresh count.
    self.transitions.reset_stagger();
  }

  /// The extra delay a stagger group imposes on this node's lifecycle event
  /// (enter when `exit` is false, exit when true): `index * stagger_ms` under
  /// the nearest ancestor declaring `stagger`, zero without one. Counting is
  /// per group per frame, in occurrence order.
  fn stagger_delay_for(&mut self, node_id: u64, exit: bool) -> f32 {
    let mut cursor = self.nodes.get(&node_id).and_then(|el| el.parent);
    while let Some(id) = cursor {
      let Some(el) = self.nodes.get(&id) else { break };
      if let Some(stagger_ms) = el.transitions.as_ref().and_then(|t| t.stagger_ms) {
        return self.transitions.stagger_index(id, exit) as f32 * stagger_ms;
      }
      cursor = el.parent;
    }
    0.0
  }

  /// A property write arriving for an animatable property: consume it as a
  /// transition target when this element declares a transition covering the
  /// property, or fall back to the normal (snapping) write path.
  ///
  /// `value` is the numeric target; `None` (a null reset, a non-numeric
  /// value) never animates. Returns true when the write was consumed (a
  /// track now runs, a delayed write is held, or the target already holds);
  /// false means the caller must perform the normal write, and any running
  /// track or held write for the pair has been cancelled so it cannot
  /// overwrite the snap on the next frame.
  ///
  /// Initial values never animate: a write to a node the paint walk has not
  /// entered yet (detached, or attached but not painted since) snaps, so an
  /// element's first painted state is what it holds then and never a fade
  /// from the kind's defaults. The attach check alone is not enough: JSX
  /// inserts a template's children into their parent before the effect that
  /// writes their props runs, so a child's mount-time writes land on an
  /// attached node. The one thing that animates before the first paint is
  /// an explicit enter animation (`from`, see `apply_enter_transitions`),
  /// started at the advance after the mount tick's writes have snapped: a
  /// write that reaches the node between that advance and the paint (a
  /// transition-end handler's) retargets the track instead of snapping it
  /// away.
  pub fn transition_write(&mut self, id: u64, prop: AnimProp, value: Option<AnimValue>) -> bool {
    let animate = value.and_then(|to| {
      let el = self.nodes.get(&id)?;
      if el.parent.is_none() || (!el.painted.get() && !self.transitions.any_running(id, &[prop])) {
        return None;
      }
      let entry = el.transitions.as_ref()?.entry_for(prop)?;
      let current = el.anim_value(prop)?;
      // A kind mismatch (a scalar arriving for the color prop or vice
      // versa) is not animatable; the normal path sorts it out.
      if std::mem::discriminant(&current) != std::mem::discriminant(&to) {
        return None;
      }
      Some((current, to, entry))
    });
    match animate {
      Some((current, to, entry)) => {
        if entry.delay_ms > 0.0 {
          let at_ms = self.transitions.now_ms + entry.delay_ms as f64;
          self.transitions.schedule(PendingWrite { node: id, prop, to, spec: entry.spec, at_ms });
        } else {
          // Last write wins: an immediate write supersedes a held one (the
          // config may have changed since the hold was scheduled).
          self.transitions.unschedule(id, prop);
          let now = self.transitions.now_ms;
          self.transitions.retarget(id, prop, current, to, entry.spec, now);
        }
        true
      }
      None => {
        self.transitions.cancel(id, prop);
        false
      }
    }
  }

  /// Note that a layout pass moved a node declaring a `layout` transition:
  /// the location it had (None for the empty box) is what
  /// `start_layout_slides` slides it from.
  pub(crate) fn note_reflow(&mut self, id: u64, old: Option<Point>) {
    self.reflowed.push((id, old));
  }

  /// Layout slides (okf/backlog/transition-layout-animations.md): after a
  /// layout pass, every declaring node the pass moved slides from where it
  /// was painted to its new solved location. The slide lane is the painted
  /// location itself, so a reflow mid-slide is the ordinary retarget (a
  /// spring keeps position and velocity, a tween restarts from the painted
  /// point), and the paint that follows this pass draws the node where it
  /// was (`Slide::at` snapped, the enter pass's snap-to-from) with motion
  /// starting at the next advance. Nothing slides from nowhere: a node's
  /// first layout, a reparent (the old box is in another parent's frame),
  /// a node not yet painted and taffy's hidden pass (the empty box, either
  /// side) all snap and anchor the node where it now is. Runs from
  /// `layout_phase`, which the frame builder and the paint phase both call:
  /// a post-layout hook that reflows again is picked up, and an unchanged
  /// second run has nothing to drain.
  pub(crate) fn start_layout_slides(&mut self) {
    if self.reflowed.is_empty() {
      return;
    }
    let reflowed = std::mem::take(&mut self.reflowed);
    let now = self.transitions.now_ms;
    for (id, old) in reflowed {
      let Some(el) = self.nodes.get_mut(&id) else { continue };
      let Some(entry) = el.transitions.as_ref().and_then(|t| t.layout) else { continue };
      let Some(layout) = el.layout.as_ref() else { continue };
      let hidden = layout.computed == taffy::Layout::new();
      let to = layout.location();
      let parent = el.parent;
      let painted = el.painted.get();
      let slide = el.slide.get_or_insert_with(Slide::default);
      let anchored = parent.is_some() && slide.under == parent;
      slide.under = parent;
      let from = match old {
        Some(old) if anchored && painted && !hidden => slide.at.unwrap_or(old),
        _ => {
          slide.at = None;
          self.transitions.cancel(id, AnimProp::Layout);
          continue;
        }
      };
      slide.at = Some(from);
      let running = if entry.delay_ms > 0.0 {
        let at_ms = now + entry.delay_ms as f64;
        let write =
          PendingWrite { node: id, prop: AnimProp::Layout, to: AnimValue::Point(to), spec: entry.spec, at_ms };
        self.transitions.schedule(write);
        true
      } else {
        self.transitions.unschedule(id, AnimProp::Layout);
        self.transitions.retarget(id, AnimProp::Layout, AnimValue::Point(from), AnimValue::Point(to), entry.spec, now)
      };
      if !running {
        // Nowhere to move (the painted point is the new box): on it.
        slide.at = None;
      }
    }
  }

  /// Keeps a node's slide state in step with its declaration, after every
  /// edit: declaring `layout` anchors the node under its current parent, so
  /// its next reflow slides; clearing it drops the state and any running
  /// slide, so the node snaps to its solved box and no stale track can
  /// write it again.
  pub(super) fn reconcile_slide(&mut self, node_id: u64) {
    let Some(el) = self.nodes.get_mut(&node_id) else { return };
    let declares = el.transitions.as_ref().is_some_and(|t| t.layout.is_some());
    match (declares, el.slide.is_some()) {
      (true, false) => el.slide = Some(Slide { under: el.parent, at: None }),
      (false, true) => {
        el.slide = None;
        self.transitions.cancel(node_id, AnimProp::Layout);
      }
      _ => {}
    }
  }

  /// The offset a sliding node has still to cover, solved minus painted
  /// location, for the dev tooling's dump; None when it sits on its box.
  pub fn slide_remaining(&self, node_id: u64) -> Option<Vector> {
    let el = self.nodes.get(&node_id)?;
    let at = el.slide?.at?;
    Some(el.layout.as_ref()?.location() - at)
  }

  /// Settled (node, prop) pairs since the last drain, for the embedder's
  /// onTransitionEnd dispatch. Natural settles only; cancelled or
  /// destroyed-node tracks never report.
  pub fn take_settled_transitions(&mut self) -> Vec<(u64, AnimProp)> {
    std::mem::take(&mut self.transitions.settled)
  }

  /// Advance every running track to the stamped animation clock, writing the
  /// interpolated values through the typed setters (damage applies as for
  /// any property write) and dropping settled tracks and tracks whose node
  /// is gone. Returns whether any track is still running - the embedder's
  /// signal to keep requesting frames. A repeated call at an unchanged clock
  /// (the paused path) writes nothing.
  pub fn advance_transitions(&mut self) -> bool {
    // Nodes attached since the last advance start their enter animations
    // first, so a `from` snap never shows and its track advances below.
    for node_id in std::mem::take(&mut self.transitions.entering) {
      self.apply_enter_transitions(node_id);
    }
    // Exit roots whose gate may have emptied this pass; each is checked
    // (and freed when the gate is empty) after the advance. A member moved
    // out mid-cascade queued its root ahead of time (abandon_exit).
    let mut exit_checks: Vec<u64> = std::mem::take(&mut self.transitions.exit_checks);
    if self.transitions.is_empty() && exit_checks.is_empty() {
      return false;
    }
    let now = self.transitions.now_ms;
    // Delayed writes whose hold expired apply now, exactly as a JS write
    // this frame would: retarget from the property's present value. State
    // may have shifted during the hold (a gradient took over, the node
    // died); a write that no longer applies is dropped silently.
    for w in self.transitions.take_due(now) {
      let current = self.nodes.get(&w.node).and_then(|el| el.anim_value(w.prop));
      let mut running = false;
      if let Some(current) = current {
        if std::mem::discriminant(&current) == std::mem::discriminant(&w.to) {
          running = self.transitions.retarget(w.node, w.prop, current, w.to, w.spec, w.at_ms);
        }
      }
      // A due exit write that starts no track (value already there, state
      // shifted) may have been the last thing keeping its root around.
      if !running {
        if let Some(root) = self.exit_root_of(w.node) {
          exit_checks.push(root);
        }
      }
    }
    let (mut tracks, dt) = self.transitions.begin_advance();
    // Every track started at this very clock: nothing can move yet, and
    // the pass stays active for the next frame.
    if dt <= 0.0 && !tracks.is_empty() && exit_checks.is_empty() {
      self.transitions.end_advance(tracks);
      return true;
    }
    let mut damages: Vec<(u64, Damage)> = Vec::with_capacity(tracks.len());
    tracks.retain_mut(|t| {
      if !self.nodes.contains_key(&t.node) {
        return false;
      }
      let (value, settled) = t.advance(now);
      let damage = self.nodes.get_mut(&t.node).map(|el| el.set_anim_value(t.prop, value)).unwrap_or(Damage::None);
      damages.push((t.node, damage));
      if settled {
        // Anything under an exit root settles into the root's free, not
        // into onTransitionEnd: the components that could observe the
        // event are already disposed.
        match self.exit_root_of(t.node) {
          Some(root) => exit_checks.push(root),
          None => self.transitions.settled.push((t.node, t.prop)),
        }
      }
      !settled
    });
    self.apply_damage_batch(&damages);
    self.transitions.end_advance(tracks);
    exit_checks.sort_unstable();
    exit_checks.dedup();
    // A worklist: an inner root finishing may empty the gate of the root it
    // was nested in, which then frees in the same pass.
    while let Some(root) = exit_checks.pop() {
      let exiting = self.nodes.get(&root).map(|el| el.exiting).unwrap_or(false);
      if exiting && !self.subtree_exit_running(root) {
        if let Some(outer) = self.finish_exit(root) {
          exit_checks.push(outer);
        }
      }
    }
    !self.transitions.is_empty()
  }
}
