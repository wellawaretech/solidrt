use crate::event::Modifiers;
use crate::rendertree::PointerKey;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

// Facts about the physical input device: the last known position per pointer
// and the current modifier state. These outlive any app or scripting-engine
// instance - the device doesn't reset just because the app is being swapped -
// so a host keeps one InputState for the life of the window. Anything whose
// meaning depends on a particular app's tree (hovered node paths) belongs to
// that app's layer instead, so it is dropped when the app is replaced.
//
// Pointer state is keyed by PointerKey ((PointerType, u64)) so mouse / touch
// / pen can coexist; nothing in this type assumes a single active pointer.
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