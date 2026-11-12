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
