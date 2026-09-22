// The loop's cursor state: every SDL cursor it has created, kept alive for
// as long as SDL may show it (sdl_utils::OwnedCursor), and the application
// of a resolved Cursor. Lives on the loop thread, where SDL's cursors do.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::sdl_utils::{self, OwnedCursor};
use crate::{Cursor, CursorFrame, CursorShape};

#[derive(Default)]
pub struct Cursors {
  system: HashMap<CursorShape, OwnedCursor>,
  custom: HashMap<u64, OwnedCursor>,
}

impl Cursors {
  /// Show `cursor`. A system shape is created on first use; an unregistered
  /// custom id shows the default shape (and warns), so the pointer never
  /// sticks on whatever was shown before.
  pub fn apply(&mut self, cursor: Cursor) {
    let owned = match cursor {
      Cursor::Hidden => {
        sdl_utils::show_cursor(false);
        return;
      }
      Cursor::System(shape) => system_cursor(&mut self.system, shape),
      Cursor::Custom(id) => match self.custom.get(&id) {
        Some(owned) => Some(owned),
        None => {
          log::warn!("[alloy] cursor {id} is not registered; showing the default cursor");
          system_cursor(&mut self.system, CursorShape::Default)
        }
      },
    };
    if let Some(owned) = owned {
      sdl_utils::set_cursor(owned);
    }
    sdl_utils::show_cursor(true);
  }

  pub fn register(&mut self, id: u64, frames: &[CursorFrame], hot_x: u32, hot_y: u32) {
    match sdl_utils::create_image_cursor(frames, hot_x, hot_y) {
      Ok(owned) => {
        self.custom.insert(id, owned);
      }
      Err(e) => log::warn!("[alloy] cursor {id} not created: {e}"),
    }
  }

  pub fn unregister(&mut self, id: u64) {
    self.custom.remove(&id);
  }
}

// The cached system cursor for `shape`, created on first use; None (once
// warned) when the platform has no cursor for it.
fn system_cursor(cache: &mut HashMap<CursorShape, OwnedCursor>, shape: CursorShape) -> Option<&OwnedCursor> {
  match cache.entry(shape) {
    Entry::Occupied(entry) => Some(&*entry.into_mut()),
    Entry::Vacant(entry) => match sdl_utils::create_system_cursor(shape.to_sdl()) {
      Ok(owned) => Some(&*entry.insert(owned)),
      Err(e) => {
        log::warn!("[alloy] system cursor {shape:?} unavailable: {e}");
        None
      }
    },
  }
}
