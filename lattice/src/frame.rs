use alloy::Modifiers;
use flux::gui::input::PointerKey;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

// Profiling counters, read by the debug overlay. They live as thread-locals on
// the single JS execution thread (where the render handler, setProperty and
// draw all run), so the JS side never makes a timing call: native stamps the
// values around the work and the overlay reads them. Zero added FFI crossings.
thread_local! {
  // Instant captured just before the "render" event is emitted to JS. Read at
  // draw() entry, the delta is the JS render handler (onFrame + flush).
  pub static RENDER_START: Cell<Option<Instant>> = Cell::new(None);
}

// Facts about the physical input device. Persists across engine reloads,
// because the device doesn't reset just because the JS bundle is being
// swapped. Anything whose meaning depends on the current engine's render
// tree (hovered node paths) lives in flux::gui::input instead, as engine
// userdata, so it is dropped automatically when the engine is replaced.
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
