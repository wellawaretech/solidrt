//! Tree-side transition plumbing: starting enter/exit tracks against nodes,
//! the stagger ladder, the JS write interception, and the per-frame advance
//! that settles tracks and applies damage. The element-side state (configs,
//! tracks, property mapping) lives in `rendertree::transitions`; the node
//! map and damage application this drives live in the parent module.

use super::RenderTree;
use crate::rendertree::transitions::{AnimValue, PendingWrite};
use crate::rendertree::{AnimProp, Damage};

impl RenderTree {
  /// Mount-time enter animations: a per-property `from` in the node's
  /// transition declaration snaps the property to `from` and starts a track
  /// toward the value it mounted with (with the entry's delay honored - the
  /// element sits at `from` until the hold expires). Fires on the node's
  /// first attach only; a move or reorder re-runs nothing. A property whose
  /// mounted value is unreadable (no explicit value, a gradient) skips its
  /// enter animation and simply shows the mounted state.
  pub(super) fn apply_enter_transitions(&mut self, node_id: u64) {
    let entries: Vec<(AnimProp, crate::rendertree::TransitionEntry)> = {
      let Some(el) = self.nodes.get_mut(&node_id) else { return };
      if el.entered {
        return;
      }
      el.entered = true;
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
      let Some(from) = entry.from else { continue };
      let Some(target) = self.nodes.get(&node_id).and_then(|el| el.anim_value(prop)) else { continue };
      if std::mem::discriminant(&from) != std::mem::discriminant(&target) {
        continue;
      }
      let damage = self.nodes.get_mut(&node_id).map(|el| el.set_anim_value(prop, from)).unwrap_or(Damage::None);
      self.apply_damage(node_id, damage);
      let delay_ms = entry.delay_ms + stagger;
      if delay_ms > 0.0 {
        let at_ms = now + delay_ms as f64;
        self.transitions.schedule(PendingWrite { node: node_id, prop, to: target, spec: entry.spec, at_ms });
      } else {
        self.transitions.retarget(node_id, prop, from, target, entry.spec);
      }
    }
  }


  /// Start the exit animation instead of detaching, when the node declares
  /// `exit` values and at least one of them has somewhere to move. Returns
  /// whether the node is now exiting (still linked). A node whose exit
  /// values all already hold (or are unreadable) detaches instantly - an
  /// exit that animates nothing must not defer the removal.
  pub(super) fn begin_exit(&mut self, parent_id: u64, node_id: u64) -> bool {
    let entries: Vec<(AnimProp, crate::rendertree::TransitionEntry)> = match self.nodes.get(&node_id) {
      Some(el) if !el.exiting && el.parent == Some(parent_id) => match &el.transitions {
        Some(t) => t.props.iter().filter(|(_, e)| e.exit.is_some()).cloned().collect(),
        None => return false,
      },
      _ => return false,
    };
    if entries.is_empty() {
      return false;
    }
    // One stagger index per node, shared by all its exiting properties.
    let stagger = self.stagger_delay_for(node_id, true);
    let now = self.transitions.now_ms;
    let mut started = false;
    for (prop, entry) in entries {
      let Some(to) = entry.exit else { continue };
      let Some(current) = self.nodes.get(&node_id).and_then(|el| el.anim_value(prop)) else { continue };
      if std::mem::discriminant(&current) != std::mem::discriminant(&to) {
        continue;
      }
      let delay_ms = entry.delay_ms + stagger;
      if delay_ms > 0.0 {
        let at_ms = now + delay_ms as f64;
        self.transitions.schedule(PendingWrite { node: node_id, prop, to, spec: entry.spec, at_ms });
        started = true;
      } else {
        started |= self.transitions.retarget(node_id, prop, current, to, entry.spec);
      }
    }
    if started {
      if let Some(el) = self.nodes.get_mut(&node_id) {
        el.exiting = true;
      }
    }
    started
  }

  /// The properties of a node's transition declaration that carry an `exit`
  /// value - the set whose tracks gate the exiting node's free.
  fn exit_props(&self, node_id: u64) -> Vec<AnimProp> {
    self
      .nodes
      .get(&node_id)
      .and_then(|el| el.transitions.as_ref())
      .map(|t| t.props.iter().filter(|(_, e)| e.exit.is_some()).map(|(p, _)| *p).collect())
      .unwrap_or_default()
  }

  /// A re-insert reached an exiting node: the removal turned out to be a
  /// move. Drop the exit tracks (the node holds its current values; later
  /// writes take over as usual) and clear the marks.
  pub(super) fn abandon_exit(&mut self, node_id: u64) {
    let exiting = self.nodes.get(&node_id).map(|el| el.exiting).unwrap_or(false);
    if !exiting {
      return;
    }
    let props = self.exit_props(node_id);
    self.transitions.cancel_props(node_id, &props);
    if let Some(el) = self.nodes.get_mut(&node_id) {
      el.exiting = false;
      el.doomed = false;
    }
  }

  /// The last exit track of an exiting node settled: complete the removal
  /// that was deferred at detach - unlink, and free if the deferred destroy
  /// already ran (the renderer's sweep found the node exiting).
  fn finish_exit(&mut self, node_id: u64) {
    self.transitions.cancel_node(node_id);
    let Some(el) = self.nodes.get_mut(&node_id) else { return };
    el.exiting = false;
    let doomed = el.doomed;
    let parent = el.parent;
    if let Some(parent_id) = parent {
      if self.nodes.contains_key(&parent_id) {
        self.detach_node_now(parent_id, node_id);
      }
    }
    if doomed {
      self.delete_recursive(node_id);
      self.bump_revision();
    }
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
  /// Initial values never animate: a node not yet inserted (no parent) has
  /// never been painted, so its mount-time writes snap. This is what keeps
  /// `transition` listed before other props in JSX from animating the mount.
  /// (Enter animations opt in explicitly via `from`; see `insert_node`.)
  pub fn transition_write(&mut self, id: u64, prop: AnimProp, value: Option<AnimValue>) -> bool {
    let animate = value.and_then(|to| {
      let el = self.nodes.get(&id)?;
      if el.parent.is_none() {
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
          self.transitions.retarget(id, prop, current, to, entry.spec);
        }
        true
      }
      None => {
        self.transitions.cancel(id, prop);
        false
      }
    }
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
    if self.transitions.is_empty() {
      return false;
    }
    let now = self.transitions.now_ms;
    // Exiting nodes whose exit-track gate may have emptied this pass; each
    // is checked (and freed when the gate is empty) after the advance.
    let mut exit_checks: Vec<u64> = Vec::new();
    // Delayed writes whose hold expired apply now, exactly as a JS write
    // this frame would: retarget from the property's present value. State
    // may have shifted during the hold (a gradient took over, the node
    // died); a write that no longer applies is dropped silently.
    for w in self.transitions.take_due(now) {
      let current = self.nodes.get(&w.node).and_then(|el| el.anim_value(w.prop));
      let mut running = false;
      if let Some(current) = current {
        if std::mem::discriminant(&current) == std::mem::discriminant(&w.to) {
          running = self.transitions.retarget(w.node, w.prop, current, w.to, w.spec);
        }
      }
      // A due exit write that starts no track (value already there, state
      // shifted) may have been the last thing keeping the node around.
      if !running && self.nodes.get(&w.node).map(|el| el.exiting).unwrap_or(false) {
        exit_checks.push(w.node);
      }
    }
    let (mut tracks, dt) = self.transitions.begin_advance();
    if dt <= 0.0 && exit_checks.is_empty() {
      self.transitions.end_advance(tracks);
      return true;
    }
    let mut damages: Vec<(u64, Damage)> = Vec::with_capacity(tracks.len());
    tracks.retain_mut(|t| {
      if !self.nodes.contains_key(&t.node) {
        return false;
      }
      let (value, settled) = t.advance(now, dt);
      let damage = self.nodes.get_mut(&t.node).map(|el| el.set_anim_value(t.prop, value)).unwrap_or(Damage::None);
      damages.push((t.node, damage));
      if settled {
        // Exiting nodes settle into their free, not into onTransitionEnd:
        // the component that could observe the event is already disposed.
        if self.nodes.get(&t.node).map(|el| el.exiting).unwrap_or(false) {
          exit_checks.push(t.node);
        } else {
          self.transitions.settled.push((t.node, t.prop));
        }
      }
      !settled
    });
    self.apply_damage_batch(&damages);
    self.transitions.end_advance(tracks);
    exit_checks.dedup();
    for node_id in exit_checks {
      let exiting = self.nodes.get(&node_id).map(|el| el.exiting).unwrap_or(false);
      if exiting && !self.transitions.any_running(node_id, &self.exit_props(node_id)) {
        self.finish_exit(node_id);
      }
    }
    !self.transitions.is_empty()
  }

}
