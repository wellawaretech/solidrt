use std::cell::Cell;

pub struct FrameState {
  pointer_pos: Cell<(f32, f32)>,
}

// Safety: FrameState is only accessed on the UI thread.
unsafe impl Send for FrameState {}
unsafe impl Sync for FrameState {}

impl FrameState {
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