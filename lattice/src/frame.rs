use std::cell::{Cell, RefCell};

pub enum InputEvent {
  PointerMove { x: f32, y: f32 },
  PointerDown { button: u8, x: f32, y: f32 },
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

pub struct InputState {
  pointer_pos: Cell<(f32, f32)>,
}

// Safety: InputState is only accessed on the UI thread.
unsafe impl Send for InputState {}
unsafe impl Sync for InputState {}

impl InputState {
  pub fn new() -> Self {
    Self {
      pointer_pos: Cell::new((0.0, 0.0)),
    }
  }

  pub fn pointer_pos(&self) -> (f32, f32) {
    self.pointer_pos.get()
  }

  pub fn set_pointer_pos(&self, x: f32, y: f32) {
    self.pointer_pos.set((x, y));
  }
}

pub struct EngineState {
  hovered_path: RefCell<Vec<u64>>,
  input_queue: RefCell<Vec<InputEvent>>,
}

// Safety: EngineState is only accessed on the UI thread.
unsafe impl Send for EngineState {}
unsafe impl Sync for EngineState {}

impl EngineState {
  pub fn new() -> Self {
    Self {
      hovered_path: RefCell::new(Vec::new()),
      input_queue: RefCell::new(Vec::new()),
    }
  }

  pub fn hovered_path(&self) -> Vec<u64> {
    self.hovered_path.borrow().clone()
  }

  pub fn set_hovered_path(&self, path: Vec<u64>) {
    *self.hovered_path.borrow_mut() = path;
  }

  pub fn push_input(&self, event: InputEvent) {
    self.input_queue.borrow_mut().push(event);
  }

  pub fn drain_input(&self) -> Vec<InputEvent> {
    self.input_queue.borrow_mut().drain(..).collect()
  }
}