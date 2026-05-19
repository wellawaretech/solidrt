use alloy::{Modifiers, PointerType};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub type PointerKey = (PointerType, u64);

pub enum InputEvent {
  PointerMove {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  PointerDown {
    pointer_id: u64,
    pointer_type: PointerType,
    button: u8,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
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
// carrying a node id, or a coordinate that was aimed at a specific
// tree, is EngineState - node ids become dangling on reload, and
// queued coordinates were aimed at a tree that no longer exists.
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
    Self {
      pointers: RefCell::new(HashMap::new()),
      modifiers: Cell::new(Modifiers::default()),
    }
  }

  pub fn set_pointer_pos(&self, key: PointerKey, x: f32, y: f32) {
    self.pointers.borrow_mut().insert(key, (x, y));
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
  input_queue: RefCell<Vec<InputEvent>>,
}

// Safety: EngineState is only accessed on the UI thread.
unsafe impl Send for EngineState {}
unsafe impl Sync for EngineState {}

impl EngineState {
  pub fn new() -> Self {
    Self {
      hovered_paths: RefCell::new(HashMap::new()),
      input_queue: RefCell::new(Vec::new()),
    }
  }

  pub fn hovered_path(&self, key: PointerKey) -> Vec<u64> {
    self.hovered_paths.borrow().get(&key).cloned().unwrap_or_default()
  }

  pub fn set_hovered_path(&self, key: PointerKey, path: Vec<u64>) {
    self.hovered_paths.borrow_mut().insert(key, path);
  }

  pub fn push_input(&self, event: InputEvent) {
    self.input_queue.borrow_mut().push(event);
  }

  pub fn drain_input(&self) -> Vec<InputEvent> {
    self.input_queue.borrow_mut().drain(..).collect()
  }
}