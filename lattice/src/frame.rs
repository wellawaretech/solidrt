use alloy::{Modifiers, PointerType};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

pub type PointerKey = (PointerType, u64);

// Profiling counters, read by the debug overlay. They live as thread-locals on
// the single JS execution thread (where the render handler, setProperty and
// draw all run), so the JS side never makes a timing call: native stamps the
// values around the work and the overlay reads them. Zero added FFI crossings.
thread_local! {
  // Instant captured just before the "render" event is emitted to JS. Read at
  // draw() entry, the delta is the JS render handler (onFrame + flush).
  pub static RENDER_START: Cell<Option<Instant>> = Cell::new(None);
}

pub enum InputEvent {
  PointerMove { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, modifiers: Modifiers },
  PointerDown { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  PointerUp { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  Wheel { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, delta_x: f32, delta_y: f32, modifiers: Modifiers },
}

// Per-frame state is split into two structs by lifetime, not by topic.
//
// InputState  - facts about the physical input device. Persists across
//               engine reloads, because the device doesn't reset just
//               because the JS bundle is being swapped.
//
// EngineState - anything whose meaning depends on the current engine's
//               render tree. Recreated on every reload, so its contents
//               are automatically dropped when the engine is replaced.
//
// Rule of thumb: if you would be surprised that this still applied
// after a reload, it belongs in EngineState. In particular, anything
// carrying a node id is EngineState - node ids become dangling on
// reload.
//
// Pointer state is keyed by (PointerType, u64) so mouse / touch / pen
// can coexist; nothing in this file assumes a single active pointer.

pub struct InputState {
  pointers: RefCell<HashMap<PointerKey, (f32, f32)>>,
  modifiers: Cell<Modifiers>,
}

// Safety: InputState is only accessed on the UI thread.
unsafe impl Send for InputState {}
unsafe impl Sync for InputState {}

impl InputState {
  pub fn new() -> Self {
    Self { pointers: RefCell::new(HashMap::new()), modifiers: Cell::new(Modifiers::default()) }
  }

  pub fn set_pointer_pos(&self, key: PointerKey, x: f32, y: f32) {
    self.pointers.borrow_mut().insert(key, (x, y));
  }

  pub fn remove_pointer(&self, key: PointerKey) {
    self.pointers.borrow_mut().remove(&key);
  }

  pub fn pointers(&self) -> Vec<(PointerKey, (f32, f32))> {
    self.pointers.borrow().iter().map(|(k, v)| (*k, *v)).collect()
  }

  pub fn set_modifiers(&self, m: Modifiers) {
    self.modifiers.set(m);
  }

  pub fn modifiers(&self) -> Modifiers {
    self.modifiers.get()
  }
}

pub struct EngineState {
  hovered_paths: RefCell<HashMap<PointerKey, Vec<u64>>>,
}

// Safety: EngineState is only accessed on the UI thread.
unsafe impl Send for EngineState {}
unsafe impl Sync for EngineState {}

impl EngineState {
  pub fn new() -> Self {
    Self { hovered_paths: RefCell::new(HashMap::new()) }
  }

  pub fn hovered_path(&self, key: PointerKey) -> Vec<u64> {
    self.hovered_paths.borrow().get(&key).cloned().unwrap_or_default()
  }

  pub fn set_hovered_path(&self, key: PointerKey, path: Vec<u64>) {
    self.hovered_paths.borrow_mut().insert(key, path);
  }

  pub fn remove_hovered_path(&self, key: PointerKey) {
    self.hovered_paths.borrow_mut().remove(&key);
  }
}
