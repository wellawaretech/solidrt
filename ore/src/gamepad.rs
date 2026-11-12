//! The gamepad vocabulary: one connected pad's state as a snapshot reports
//! it, and the commands a synthetic pad is driven by. Plain data; the pads
//! themselves come with the event pump.

/// One connected gamepad's current state. `buttons` holds the names of the
/// currently-pressed buttons (SDL3 positional names: "south", "dpadUp", ...);
/// `axes` holds every axis as (name, value) with sticks in -1..1 and triggers
/// in 0..1. `mapped` is false for joysticks without an SDL controller-database
/// entry: those still report, but their names are positional guesses following
/// the W3C standard-mapping button/axis order, not device-verified.
#[derive(Clone)]
pub struct GamepadState {
  pub id: u32,
  pub name: String,
  pub buttons: Vec<&'static str>,
  pub axes: Vec<(&'static str, f32)>,
  pub mapped: bool,
}

/// What the dev tools ask of the pad owner. Names are the static button and
/// axis vocabulary a pad reports, checked by the sender against the pad
/// owner's static names.
#[derive(Clone, Debug, PartialEq)]
pub enum GamepadCommand {
  /// Seat a synthetic pad: in `slot`, or the lowest free one as SDL's do.
  Connect {
    slot: Option<usize>,
    name: String,
  },
  /// Hold this state until the next Set: level, as a physical pad's is.
  Set {
    slot: usize,
    buttons: Vec<&'static str>,
    axes: Vec<(&'static str, f32)>,
  },
  Disconnect {
    slot: usize,
  },
}
