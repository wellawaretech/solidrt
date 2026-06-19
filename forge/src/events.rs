//! Engine-free event-bus core.
//!
//! The scripting-engine-independent half of flux's event mechanism: a
//! string-keyed listener registry generic over the callback type `F`. It holds
//! no policy (it does not know which events exist) and names no scripting-engine
//! types; the marshalling layer (`plugins/flux/events.rs`) instantiates it with
//! the engine's callback handle (`Persistent<Function>` for QuickJS), keeps the
//! engine alive via `PendingOps` using the `is_first`/`is_last` signals the
//! methods return, and restores/calls the callbacks. A second engine reuses this
//! registry with its own `F`. Destined for the `forge` crate (see REDESIGN.md).

use std::collections::HashMap;

// (id, callback, once)
type Listener<F> = (u32, F, bool);

/// A registry of listeners keyed by event name. `F` is the host's callback
/// handle. The host drives all access (it lives behind the host's interior
/// mutability) and acts on the `is_first`/`is_last` booleans the mutating
/// methods return to balance any per-event resource (e.g. a `PendingOps` hold).
pub struct ListenerRegistry<F> {
  map: HashMap<String, Vec<Listener<F>>>,
  next_id: u32,
}

impl<F> Default for ListenerRegistry<F> {
  fn default() -> Self {
    Self { map: HashMap::new(), next_id: 1 }
  }
}

impl<F: Clone> ListenerRegistry<F> {
  /// Register `callback` for `event`. Returns its id (for `remove`) and whether
  /// it is the first listener for that event (so the host can `hold` once per
  /// event).
  pub fn insert(&mut self, event: String, callback: F, once: bool) -> (u32, bool) {
    let id = self.next_id;
    self.next_id += 1;
    let is_first = !self.map.contains_key(&event);
    self.map.entry(event).or_default().push((id, callback, once));
    (id, is_first)
  }

  /// Remove the listener `id` from `event`. Returns true when that emptied the
  /// event (its entry was dropped), so the host can `release` the matching hold.
  /// A no-op (id not present) returns false.
  pub fn remove(&mut self, event: &str, id: u32) -> bool {
    if let Some(cbs) = self.map.get_mut(event) {
      cbs.retain(|(lid, _, _)| *lid != id);
      if cbs.is_empty() {
        self.map.remove(event);
        return true;
      }
    }
    false
  }

  /// Whether `event` currently has any listeners.
  pub fn has_listeners(&self, event: &str) -> bool {
    self.map.get(event).is_some_and(|cbs| !cbs.is_empty())
  }

  /// Snapshot the callbacks for `event` (cloned, so dispatch can release the
  /// registry borrow before calling into them) plus the ids of the once-only
  /// listeners (to `prune` after dispatch).
  pub fn snapshot(&self, event: &str) -> (Vec<F>, Vec<u32>) {
    match self.map.get(event) {
      Some(cbs) => (
        cbs.iter().map(|(_, f, _)| f.clone()).collect(),
        cbs.iter().filter(|(_, _, once)| *once).map(|(id, _, _)| *id).collect(),
      ),
      None => (Vec::new(), Vec::new()),
    }
  }

  /// Remove the once-listeners just fired (by id). Returns true when that emptied
  /// the event, so the host can `release`. `retain` is a no-op for ids a listener
  /// already unsubscribed during dispatch.
  pub fn prune(&mut self, event: &str, once_ids: &[u32]) -> bool {
    if let Some(cbs) = self.map.get_mut(event) {
      cbs.retain(|(id, _, _)| !once_ids.contains(id));
      if cbs.is_empty() {
        self.map.remove(event);
        return true;
      }
    }
    false
  }
}