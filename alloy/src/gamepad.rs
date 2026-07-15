use sdl3::event::Event as SdlEvent;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::joystick::JoystickId;
// GamepadSubsystem is only exported at the crate root (sdl3::gamepad::* is private)
use sdl3::GamepadSubsystem;

use crate::event::{AlloyEvent, GamepadState};

// Semantic names for the JS-facing snapshot. SDL's own string names are the
// controller-mapping-format ones ("a", "dpup", ...), which conflate position
// and label; these follow SDL3's positional enum instead.
fn button_name(b: Button) -> &'static str {
  match b {
    Button::South => "south",
    Button::East => "east",
    Button::West => "west",
    Button::North => "north",
    Button::Back => "back",
    Button::Guide => "guide",
    Button::Start => "start",
    Button::LeftStick => "leftStick",
    Button::RightStick => "rightStick",
    Button::LeftShoulder => "leftShoulder",
    Button::RightShoulder => "rightShoulder",
    Button::DPadUp => "dpadUp",
    Button::DPadDown => "dpadDown",
    Button::DPadLeft => "dpadLeft",
    Button::DPadRight => "dpadRight",
    Button::Misc1 => "misc1",
    Button::Misc2 => "misc2",
    Button::Misc3 => "misc3",
    Button::Misc4 => "misc4",
    Button::Misc5 => "misc5",
    Button::Misc6 => "misc6",
    Button::RightPaddle1 => "rightPaddle1",
    Button::LeftPaddle1 => "leftPaddle1",
    Button::RightPaddle2 => "rightPaddle2",
    Button::LeftPaddle2 => "leftPaddle2",
    Button::Touchpad => "touchpad",
  }
}

const BUTTONS: [Button; 15] = [
  Button::South,
  Button::East,
  Button::West,
  Button::North,
  Button::Back,
  Button::Guide,
  Button::Start,
  Button::LeftStick,
  Button::RightStick,
  Button::LeftShoulder,
  Button::RightShoulder,
  Button::DPadUp,
  Button::DPadDown,
  Button::DPadLeft,
  Button::DPadRight,
];

const AXES: [(Axis, &str); 6] = [
  (Axis::LeftX, "leftX"),
  (Axis::LeftY, "leftY"),
  (Axis::RightX, "rightX"),
  (Axis::RightY, "rightY"),
  (Axis::TriggerLeft, "leftTrigger"),
  (Axis::TriggerRight, "rightTrigger"),
];

// Opened gamepads in stable slots (web Gamepad API style): a pad keeps its
// index for its whole connection, disconnecting leaves a hole, and the next
// connect fills the lowest free slot. Stable indices are what lets a game
// keep "player 2" attached to the same physical pad across the session.
//
// SDL only reports button/axis events for gamepads that have been opened,
// and it emits Added events for already-connected devices when the subsystem
// initializes, so open-on-Added covers initial population too.
pub(crate) struct Gamepads {
  subsystem: GamepadSubsystem,
  slots: Vec<Option<Gamepad>>,
  dirty: bool,
}

impl Gamepads {
  pub fn new(sdl: &sdl3::Sdl) -> Option<Gamepads> {
    match sdl.gamepad() {
      Ok(subsystem) => Some(Gamepads { subsystem, slots: Vec::new(), dirty: false }),
      Err(e) => {
        log::warn!("[alloy] gamepad subsystem unavailable: {e}");
        None
      }
    }
  }

  // Track connection changes and mark state dirty on any pad activity. The
  // per-event payloads are ignored: the snapshot re-reads current state from
  // the open handles, which cannot drift from SDL's view.
  pub fn handle_event(&mut self, e: &SdlEvent) {
    match e {
      SdlEvent::ControllerDeviceAdded { which, .. } => {
        if self.slot_of(*which).is_some() {
          return; // already open (SDL can re-announce on remap)
        }
        match self.subsystem.open(JoystickId::new(*which)) {
          Ok(pad) => {
            let free = self.slots.iter().position(|s| s.is_none());
            match free {
              Some(i) => self.slots[i] = Some(pad),
              None => self.slots.push(Some(pad)),
            }
            self.dirty = true;
          }
          Err(err) => log::warn!("[alloy] gamepad open failed: {err}"),
        }
      }
      SdlEvent::ControllerDeviceRemoved { which, .. } => {
        if let Some(i) = self.slot_of(*which) {
          self.slots[i] = None;
          self.dirty = true;
        }
      }
      SdlEvent::ControllerButtonDown { .. }
      | SdlEvent::ControllerButtonUp { .. }
      | SdlEvent::ControllerAxisMotion { .. } => {
        self.dirty = true;
      }
      _ => {}
    }
  }

  // One snapshot per main-loop iteration, however many pad events arrived:
  // analog sticks can emit several motion events per frame and each JS emit
  // crosses the runtime boundary.
  pub fn take_snapshot_if_dirty(&mut self) -> Option<AlloyEvent> {
    if !self.dirty {
      return None;
    }
    self.dirty = false;
    Some(self.snapshot_event())
  }

  pub fn snapshot_event(&self) -> AlloyEvent {
    let pads = self
      .slots
      .iter()
      .map(|slot| {
        slot.as_ref().map(|pad| GamepadState {
          id: pad.id().ok().map_or(0, u32::from),
          name: pad.name().unwrap_or_default(),
          buttons: BUTTONS.iter().copied().filter(|&b| pad.button(b)).map(button_name).collect(),
          axes: AXES.iter().map(|&(axis, name)| (name, (pad.axis(axis) as f32 / 32767.0).clamp(-1.0, 1.0))).collect(),
        })
      })
      .collect();
    AlloyEvent::Gamepads { pads }
  }

  fn slot_of(&self, id: u32) -> Option<usize> {
    self.slots.iter().position(|s| s.as_ref().is_some_and(|p| p.id().ok().is_some_and(|i| i == id)))
  }
}
