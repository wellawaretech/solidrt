// Scripted input: a timeline of synthetic events replayed by playback mode
// (see `crate::playback`) or captured by a runner's live input recorder (see
// lattice). Stage 1 covers only key transitions; more variants are expected to
// join `ScriptEvent` as scripting grows beyond the keyboard.

use crate::event::{AlloyEvent, Modifiers};

#[derive(Clone, Debug)]
pub struct ScriptEvent {
  pub down: bool,
  // The W3C `key` value apps observe in their handlers ("Enter", "ArrowLeft",
  // "a"), so scripts round-trip through recordings verbatim.
  pub key: String,
}

impl ScriptEvent {
  pub fn to_alloy_event(&self) -> AlloyEvent {
    AlloyEvent::Key {
      down: self.down,
      key: self.key.clone(),
      // Scripts record logical keys only; reconstruct the US-layout position,
      // "Unidentified" where there is no single one.
      code: crate::keymap::w3c_code_for_key(&self.key),
      modifiers: Modifiers::default(),
      repeat: false,
    }
  }

  // None for events that have no scripted representation yet.
  pub fn from_alloy_event(event: &AlloyEvent) -> Option<ScriptEvent> {
    match event {
      AlloyEvent::Key { down, key, .. } => Some(ScriptEvent { down: *down, key: key.clone() }),
      _ => None,
    }
  }
}

pub struct ScriptedAction {
  // Cumulative seconds from the start of the script.
  pub at: f64,
  pub event: ScriptEvent,
}

// Consumes a script's actions in order as elapsed time advances. Assumes
// `actions` is sorted by `at` (the case whether built from cumulative delta
// sums or a live recording).
pub struct ScriptPlayer {
  actions: Vec<ScriptedAction>,
  next: usize,
}

impl ScriptPlayer {
  pub fn new(actions: Vec<ScriptedAction>) -> Self {
    Self { actions, next: 0 }
  }

  // Drains and returns every action due at or before `elapsed_secs`.
  pub fn due(&mut self, elapsed_secs: f64) -> Vec<AlloyEvent> {
    let mut out = Vec::new();
    while self.next < self.actions.len() && self.actions[self.next].at <= elapsed_secs {
      out.push(self.actions[self.next].event.to_alloy_event());
      self.next += 1;
    }
    out
  }
}

impl Default for ScriptPlayer {
  fn default() -> Self {
    Self::new(Vec::new())
  }
}
