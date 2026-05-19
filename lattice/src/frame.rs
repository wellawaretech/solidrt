use std::cell::{Cell, RefCell};

pub struct FrameState {
  pointer_pos: Cell<(f32, f32)>,
  hovered_path: RefCell<Vec<u64>>,
}

// Safety: FrameState is only accessed on the UI thread.
unsafe impl Send for FrameState {}
unsafe impl Sync for FrameState {}

impl FrameState {
  pub fn new() -> Self {
    Self {
      pointer_pos: Cell::new((0.0, 0.0)),
      hovered_path: RefCell::new(Vec::new()),
    }
  }

  pub fn pointer_pos(&self) -> (f32, f32) {
    self.pointer_pos.get()
  }

  pub fn set_pointer_pos(&self, x: f32, y: f32) {
    self.pointer_pos.set((x, y));
  }

  pub fn hovered_path(&self) -> Vec<u64> {
    self.hovered_path.borrow().clone()
  }

  pub fn set_hovered_path(&self, path: Vec<u64>) {
    *self.hovered_path.borrow_mut() = path;
  }
}