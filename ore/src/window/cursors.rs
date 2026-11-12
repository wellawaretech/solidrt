//! The window's cursor state: every SDL cursor it has created, kept alive
//! for as long as SDL may show it (`OwnedCursor`), and the application of a
//! resolved `crate::cursor::Cursor`. SDL's cursors are main thread only,
//! and so is everything here.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use sdl3::sys::mouse::{
  SDL_CreateAnimatedCursor, SDL_CreateColorCursor, SDL_CreateSystemCursor, SDL_Cursor, SDL_CursorFrameInfo,
  SDL_DestroyCursor, SDL_HideCursor, SDL_SetCursor, SDL_ShowCursor, SDL_SystemCursor,
};
use sdl3::sys::surface::{SDL_AddSurfaceAlternateImage, SDL_DestroySurface, SDL_Surface};

use super::surface::rgba_surface;
use crate::cursor::{Cursor, CursorFrame, CursorShape};

/// The cursors the window created (system shapes on first use, image
/// cursors on register), owned for as long as SDL may show them.
#[derive(Default)]
pub(crate) struct Cursors {
  system: HashMap<CursorShape, OwnedCursor>,
  custom: HashMap<u64, OwnedCursor>,
}

impl Cursors {
  /// Show `cursor`. A system shape is created on first use; an unregistered
  /// custom id shows the default shape (and warns), so the pointer never
  /// sticks on whatever was shown before.
  pub(crate) fn apply(&mut self, cursor: Cursor) {
    let owned = match cursor {
      Cursor::Hidden => {
        show_cursor(false);
        return;
      }
      Cursor::System(shape) => system_cursor(&mut self.system, shape),
      Cursor::Custom(id) => match self.custom.get(&id) {
        Some(owned) => Some(owned),
        None => {
          log::warn!("[ore] cursor {id} is not registered; showing the default cursor");
          system_cursor(&mut self.system, CursorShape::Default)
        }
      },
    };
    if let Some(owned) = owned {
      set_cursor(owned);
    }
    show_cursor(true);
  }

  pub(crate) fn register(&mut self, id: u64, frames: &[CursorFrame], hot_x: u32, hot_y: u32) {
    match create_image_cursor(frames, hot_x, hot_y) {
      Ok(owned) => {
        self.custom.insert(id, owned);
      }
      Err(e) => log::warn!("[ore] cursor {id} not created: {e}"),
    }
  }

  pub(crate) fn unregister(&mut self, id: u64) {
    self.custom.remove(&id);
  }
}

/// An SDL cursor this process created, destroyed with it. SDL keeps showing
/// a set cursor until another is set, and destroying the current one reverts
/// to the default, so the owner outlives every use (the window's cursor
/// cache, `Cursors`).
pub(crate) struct OwnedCursor(*mut SDL_Cursor);

impl Drop for OwnedCursor {
  fn drop(&mut self) {
    // SAFETY: a cursor SDL created for us, destroyed once, here; the cache
    // drops before the window's video subsystem handle (see Window).
    unsafe { SDL_DestroyCursor(self.0) };
  }
}

fn owned_cursor(raw: *mut SDL_Cursor) -> Result<OwnedCursor, String> {
  if raw.is_null() {
    Err(crate::sdl_error())
  } else {
    Ok(OwnedCursor(raw))
  }
}

fn create_system_cursor(shape: SDL_SystemCursor) -> Result<OwnedCursor, String> {
  // SAFETY: takes a plain enum value; null on failure.
  owned_cursor(unsafe { SDL_CreateSystemCursor(shape) })
}

/// A cursor from straight-alpha RGBA8 frames (crate::cursor::CursorFrame):
/// one frame is a color cursor, more an animated one. SDL copies every image
/// during the call; the platform picks a frame's HiDPI alternate by size
/// ratio.
pub(crate) fn create_image_cursor(frames: &[CursorFrame], hot_x: u32, hot_y: u32) -> Result<OwnedCursor, String> {
  if frames.is_empty() {
    return Err("cursor has no frames".to_string());
  }
  let mut surfaces: Vec<*mut SDL_Surface> = Vec::with_capacity(frames.len());
  let mut built = Ok(());
  for frame in frames {
    match frame_surface(frame) {
      Ok(surface) => surfaces.push(surface),
      Err(e) => {
        built = Err(e);
        break;
      }
    }
  }
  let cursor = built.and_then(|()| {
    let raw = if surfaces.len() == 1 {
      // SAFETY: a live surface; SDL copies its pixels during the call.
      unsafe { SDL_CreateColorCursor(surfaces[0], hot_x as i32, hot_y as i32) }
    } else {
      let mut infos: Vec<SDL_CursorFrameInfo> = surfaces
        .iter()
        .zip(frames)
        .map(|(&surface, frame)| SDL_CursorFrameInfo { surface, duration: frame.duration_ms })
        .collect();
      // SAFETY: `infos` holds `infos.len()` entries over live surfaces and
      // outlives the call; SDL copies every image during it.
      unsafe { SDL_CreateAnimatedCursor(infos.as_mut_ptr(), infos.len() as i32, hot_x as i32, hot_y as i32) }
    };
    owned_cursor(raw)
  });
  for surface in surfaces {
    // SAFETY: each surface was created by frame_surface and is destroyed
    // once, here.
    unsafe { SDL_DestroySurface(surface) };
  }
  cursor
}

// The frame's 1x surface with its alternates attached. An attachment holds
// its own reference, so each alternate is released here; the pixels behind
// every surface stay borrowed from the frame until the base is destroyed.
fn frame_surface(frame: &CursorFrame) -> Result<*mut SDL_Surface, String> {
  let (first, alternates) = frame.images.split_first().ok_or_else(|| "cursor frame has no image".to_string())?;
  let base = rgba_surface(first.width, first.height, &first.rgba)?;
  for image in alternates {
    let attached = rgba_surface(image.width, image.height, &image.rgba).and_then(|alternate| {
      // SAFETY: both surfaces are live; the attachment takes its own
      // reference, so the alternate is released right after.
      let ok = unsafe { SDL_AddSurfaceAlternateImage(base, alternate) };
      // SAFETY: our reference to the alternate, released once.
      unsafe { SDL_DestroySurface(alternate) };
      if ok {
        Ok(())
      } else {
        Err(crate::sdl_error())
      }
    });
    if let Err(e) = attached {
      // SAFETY: the base surface, released once on this error path.
      unsafe { SDL_DestroySurface(base) };
      return Err(e);
    }
  }
  Ok(base)
}

/// Show `cursor` from now on (SDL keeps the pointer until another is set).
fn set_cursor(cursor: &OwnedCursor) -> bool {
  // SAFETY: a live cursor, kept alive by its owner while SDL shows it.
  unsafe { SDL_SetCursor(cursor.0) }
}

fn show_cursor(visible: bool) {
  // SAFETY: both take no arguments.
  unsafe {
    if visible {
      SDL_ShowCursor();
    } else {
      SDL_HideCursor();
    }
  }
}

// The cached system cursor for `shape`, created on first use; None (once
// warned) when the platform has no cursor for it.
fn system_cursor(cache: &mut HashMap<CursorShape, OwnedCursor>, shape: CursorShape) -> Option<&OwnedCursor> {
  match cache.entry(shape) {
    Entry::Occupied(entry) => Some(&*entry.into_mut()),
    Entry::Vacant(entry) => match create_system_cursor(system_cursor_id(shape)) {
      Ok(owned) => Some(&*entry.insert(owned)),
      Err(e) => {
        log::warn!("[ore] system cursor {shape:?} unavailable: {e}");
        None
      }
    },
  }
}

// The SDL system cursor for a CSS shape.
fn system_cursor_id(shape: CursorShape) -> SDL_SystemCursor {
  use SDL_SystemCursor as Sdl;
  match shape {
    CursorShape::Default => Sdl::DEFAULT,
    CursorShape::Text => Sdl::TEXT,
    CursorShape::Wait => Sdl::WAIT,
    CursorShape::Crosshair => Sdl::CROSSHAIR,
    CursorShape::Progress => Sdl::PROGRESS,
    CursorShape::NwseResize => Sdl::NWSE_RESIZE,
    CursorShape::NeswResize => Sdl::NESW_RESIZE,
    CursorShape::EwResize => Sdl::EW_RESIZE,
    CursorShape::NsResize => Sdl::NS_RESIZE,
    CursorShape::Move => Sdl::MOVE,
    CursorShape::NotAllowed => Sdl::NOT_ALLOWED,
    CursorShape::Pointer => Sdl::POINTER,
    CursorShape::NwResize => Sdl::NW_RESIZE,
    CursorShape::NResize => Sdl::N_RESIZE,
    CursorShape::NeResize => Sdl::NE_RESIZE,
    CursorShape::EResize => Sdl::E_RESIZE,
    CursorShape::SeResize => Sdl::SE_RESIZE,
    CursorShape::SResize => Sdl::S_RESIZE,
    CursorShape::SwResize => Sdl::SW_RESIZE,
    CursorShape::WResize => Sdl::W_RESIZE,
  }
}
