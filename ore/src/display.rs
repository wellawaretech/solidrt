//! The display vocabulary: facts about the display a window is on, as plain
//! data. Read from the platform at the window, a change event comes with
//! the event pump.

/// Orientation of the display the window is on (DisplayOrientation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
  Unknown,
  Landscape,
  LandscapeFlipped,
  Portrait,
  PortraitFlipped,
}

/// A rectangle in integer units (the window's safe area, in logical
/// points): its top-left corner and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
  pub x: i32,
  pub y: i32,
  pub width: i32,
  pub height: i32,
}
