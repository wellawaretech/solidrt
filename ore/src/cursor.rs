//! The mouse cursor vocabulary, plain data: which cursor to show and the
//! images an embedder registers for its own. Part of ore's window part (the
//! `window` feature); showing one is the window's.

/// The mouse cursor, in the CSS `cursor` vocabulary through the SolidRT lens:
/// a platform shape (the CSS keywords SDL has a system cursor for), an image
/// the embedder registered, or hidden (CSS `none`). SDL's cursor type is
/// not part of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cursor {
  System(CursorShape),
  Custom(u64),
  Hidden,
}

impl Default for Cursor {
  fn default() -> Self {
    Cursor::System(CursorShape::Default)
  }
}

/// The platform-provided cursor shapes, named as CSS names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorShape {
  Default,
  Text,
  Wait,
  Crosshair,
  Progress,
  NwseResize,
  NeswResize,
  EwResize,
  NsResize,
  Move,
  NotAllowed,
  Pointer,
  NwResize,
  NResize,
  NeResize,
  EResize,
  SeResize,
  SResize,
  SwResize,
  WResize,
}

/// One image of a cursor frame: straight-alpha RGBA8, tightly packed. The
/// first image of a frame is the 1x representation; any others are HiDPI
/// alternates the platform picks per display scale by their size ratio to the
/// first (a 64x64 alternate of a 32x32 base serves 2x).
#[derive(Clone, Debug)]
pub struct CursorImage {
  pub width: u32,
  pub height: u32,
  pub rgba: Vec<u8>,
}

/// One frame of a cursor: its images and how long it shows, in milliseconds
/// (0 holds the frame, which ends a one-shot animation on it; a static cursor
/// is one frame). Every frame of a cursor has the same 1x size.
#[derive(Clone, Debug)]
pub struct CursorFrame {
  pub images: Vec<CursorImage>,
  pub duration_ms: u32,
}
