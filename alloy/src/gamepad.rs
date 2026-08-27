use sdl3::event::Event as SdlEvent;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::joystick::{HatState, Joystick, JoystickId};
// GamepadSubsystem is only exported at the crate root (sdl3::gamepad::* is private)
use sdl3::{GamepadSubsystem, JoystickSubsystem};

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

// Names for raw (unmapped) joystick buttons, by index in W3C standard-mapping
// order. These are positional guesses (the standard layout is what browsers
// remap *into*, not natural HID order), but a single guessed vocabulary means
// consumers never need a raw-vs-mapped code path; `mapped: false` on the
// snapshot discloses the uncertainty.
const RAW_BUTTONS: [&str; 17] = [
  "south",
  "east",
  "west",
  "north",
  "leftShoulder",
  "rightShoulder",
  "leftTrigger",
  "rightTrigger",
  "back",
  "start",
  "leftStick",
  "rightStick",
  "dpadUp",
  "dpadDown",
  "dpadLeft",
  "dpadRight",
  "guide",
];

// Overflow names for raw button indices past the standard layout.
const RAW_EXTRA_BUTTONS: [&str; 15] = [
  "button17", "button18", "button19", "button20", "button21", "button22", "button23", "button24", "button25",
  "button26", "button27", "button28", "button29", "button30", "button31",
];

fn raw_button_name(index: u32) -> Option<&'static str> {
  let i = index as usize;
  RAW_BUTTONS.get(i).or_else(|| RAW_EXTRA_BUTTONS.get(i - RAW_BUTTONS.len())).copied()
}

const RAW_AXES: [&str; 8] = ["leftX", "leftY", "rightX", "rightY", "axis4", "axis5", "axis6", "axis7"];

// Retro sticks and arcade adapters often report their d-pad as a hat rather
// than buttons; fold hat 0 into the dpad names so consumers see one shape.
fn hat_names(state: HatState) -> &'static [&'static str] {
  match state {
    HatState::Centered => &[],
    HatState::Up => &["dpadUp"],
    HatState::Down => &["dpadDown"],
    HatState::Left => &["dpadLeft"],
    HatState::Right => &["dpadRight"],
    HatState::RightUp => &["dpadRight", "dpadUp"],
    HatState::RightDown => &["dpadRight", "dpadDown"],
    HatState::LeftUp => &["dpadLeft", "dpadUp"],
    HatState::LeftDown => &["dpadLeft", "dpadDown"],
  }
}

fn axis_value(raw: i16) -> f32 {
  (raw as f32 / 32767.0).clamp(-1.0, 1.0)
}

// A connected pad: opened through SDL's gamepad API when the device has a
// controller-database mapping (semantic button positions are then reliable),
// or through the raw joystick API otherwise - SDL refuses to guess a layout
// it doesn't know, but for lua64's purposes a plain 2-button stick is a pad.
//
// A mapped pad also keeps a raw joystick handle to the same device (SDL
// refcounts the open): Android's auto-mapping is built from a hasKeys probe
// that TV remotes routinely fail for their Back key, so the mapping can lack
// the back entry even though the press arrives as raw button 4 - the Android
// driver sends gamepad-button enum values as joystick button indices. The
// raw handle lets take_back_edge read past that gap.
enum Pad {
  Mapped { pad: Gamepad, joystick: Option<Joystick> },
  Raw(Joystick),
}

// SDL_GAMEPAD_BUTTON_BACK: on the Android joystick driver, raw button
// indices are the gamepad-button enum values.
#[cfg(target_os = "android")]
const RAW_BACK_INDEX: u32 = 4;

impl Pad {
  fn id(&self) -> u32 {
    match self {
      Pad::Mapped { pad, .. } => pad.id().ok().map_or(0, u32::from),
      Pad::Raw(joystick) => joystick.id(),
    }
  }

  fn state(&self) -> GamepadState {
    match self {
      Pad::Mapped { pad, .. } => GamepadState {
        id: self.id(),
        name: pad.name().unwrap_or_default(),
        // The full truth, including "back": that button doubles as the
        // client-owned back trigger (see take_back_edge), but the snapshot
        // stays faithful - consumers get facts, the back event carries the
        // intent.
        buttons: BUTTONS.iter().copied().filter(|&b| pad.button(b)).map(button_name).collect(),
        axes: AXES.iter().map(|&(axis, name)| (name, axis_value(pad.axis(axis)))).collect(),
        mapped: true,
      },
      Pad::Raw(joystick) => {
        let mut buttons: Vec<&'static str> = Vec::new();
        let mut press = |name: &'static str| {
          if !buttons.contains(&name) {
            buttons.push(name);
          }
        };
        for i in 0..joystick.num_buttons() {
          if joystick.button(i).unwrap_or(false) {
            if let Some(name) = raw_button_name(i) {
              press(name);
            }
          }
        }
        if joystick.num_hats() > 0 {
          if let Ok(hat) = joystick.hat(0) {
            for name in hat_names(hat) {
              press(name);
            }
          }
        }
        let axes = (0..joystick.num_axes())
          .filter_map(|i| {
            let name = *RAW_AXES.get(i as usize)?;
            Some((name, axis_value(joystick.axis(i).unwrap_or(0))))
          })
          .collect();
        GamepadState { id: self.id(), name: joystick.name(), buttons, axes, mapped: false }
      }
    }
  }
}

// Opened pads in stable slots (web Gamepad API style): a pad keeps its
// index for its whole connection, disconnecting leaves a hole, and the next
// connect fills the lowest free slot. Stable indices are what lets a game
// keep "player 2" attached to the same physical pad across the session.
//
// Everything is driven by the Joy* event family: gamepad events are
// synthesized from joystick events (SDL_GamepadEventWatcher), so joystick
// events cover mapped and unmapped devices alike, and SDL emits Added events
// for already-connected devices when the subsystem initializes, so
// open-on-Added covers initial population too.
pub(crate) struct Gamepads {
  gamepad: GamepadSubsystem,
  joystick: JoystickSubsystem,
  slots: Vec<Option<Pad>>,
  dirty: bool,
  back_down: bool,
  // The dev-tool user-input mute (an agent measuring or testing; see the
  // mute site in app.rs). Pad state is level-read, so the mute cannot drop events the
  // way the key/pointer path does: instead, entering it emits one neutral
  // state (every pad still listed, nothing pressed, sticks at rest) - the
  // pad version of "releases still pass" - and nothing more until it lifts,
  // when the real state goes out again. No back edge while muted.
  muted: bool,
}

impl Gamepads {
  pub fn new(sdl: &sdl3::Sdl) -> Option<Gamepads> {
    let gamepad = match sdl.gamepad() {
      Ok(subsystem) => subsystem,
      Err(e) => {
        log::warn!("[alloy] gamepad subsystem unavailable: {e}");
        return None;
      }
    };
    let joystick = match sdl.joystick() {
      Ok(subsystem) => subsystem,
      Err(e) => {
        log::warn!("[alloy] joystick subsystem unavailable: {e}");
        return None;
      }
    };
    Some(Gamepads { gamepad, joystick, slots: Vec::new(), dirty: false, back_down: false, muted: false })
  }

  // Track connection changes and mark state dirty on any pad activity. The
  // per-event payloads are ignored: the snapshot re-reads current state from
  // the open handles, which cannot drift from SDL's view.
  pub fn handle_event(&mut self, e: &SdlEvent) {
    match e {
      SdlEvent::JoyDeviceAdded { which, .. } => {
        if self.slot_of(*which).is_some() {
          return; // already open (SDL can re-announce)
        }
        let id = JoystickId::new(*which);
        let opened = if self.gamepad.is_gamepad(id) {
          self
            .gamepad
            .open(id)
            .map(|pad| Pad::Mapped { pad, joystick: self.joystick.open(id).ok() })
            .map_err(|e| e.to_string())
        } else {
          self.joystick.open(id).map(Pad::Raw).map_err(|e| e.to_string())
        };
        match opened {
          Ok(pad) => {
            let kind = match &pad {
              Pad::Mapped { .. } => "mapped",
              Pad::Raw(_) => "raw",
            };
            log::info!("[alloy] pad connected ({kind}): {}", pad.state().name);
            let free = self.slots.iter().position(|s| s.is_none());
            match free {
              Some(i) => self.slots[i] = Some(pad),
              None => self.slots.push(Some(pad)),
            }
            self.dirty = true;
          }
          Err(err) => log::warn!("[alloy] pad open failed: {err}"),
        }
      }
      SdlEvent::JoyDeviceRemoved { which, .. } => {
        if let Some(i) = self.slot_of(*which) {
          self.slots[i] = None;
          self.dirty = true;
        }
      }
      SdlEvent::JoyButtonDown { .. }
      | SdlEvent::JoyButtonUp { .. }
      | SdlEvent::JoyAxisMotion { .. }
      | SdlEvent::JoyHatMotion { .. } => {
        // Muted, activity is not news: the neutral state already went out.
        if !self.muted {
          self.dirty = true;
        }
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
    let mut event = self.snapshot_event();
    if self.muted {
      if let AlloyEvent::Gamepads { pads } = &mut event {
        for pad in pads.iter_mut().flatten() {
          pad.buttons.clear();
          for axis in &mut pad.axes {
            axis.1 = 0.0;
          }
        }
      }
    }
    Some(event)
  }

  /// Apply the user-input mute (see `muted`). A change in either direction
  /// marks the state dirty: entering sends the neutral state, leaving the
  /// real one.
  pub fn set_muted(&mut self, muted: bool) {
    if muted != self.muted {
      self.muted = muted;
      self.dirty = true;
    }
  }

  pub fn snapshot_event(&self) -> AlloyEvent {
    let pads = self.slots.iter().map(|slot| slot.as_ref().map(Pad::state)).collect();
    AlloyEvent::Gamepads { pads }
  }

  // The gamepad "back" (select) button is a client-owned back trigger, the
  // pad-side sibling of AC_BACK on the key path: a press edge on any mapped
  // pad becomes AlloyEvent::Back, and the button never appears in the JS
  // snapshot (see Pad::state). Mapped pads only: on a raw HID pad "back" is a
  // positional guess, too uncertain to hang an exit-the-app intent on. State
  // is level-read per iteration, so a press outlasting one loop drain (any
  // human press does) is never missed and holding is one request.
  pub fn take_back_edge(&mut self) -> bool {
    let down = self.slots.iter().flatten().any(|p| match p {
      Pad::Mapped { pad, joystick } => {
        if pad.button(Button::Back) {
          return true;
        }
        // Android: read the raw button too - the auto-mapping may lack the
        // back entry for TV remotes (see the Pad doc comment). Elsewhere raw
        // indices are device-arbitrary, so only the mapping is trusted.
        #[cfg(target_os = "android")]
        {
          joystick.as_ref().is_some_and(|j| j.button(RAW_BACK_INDEX).unwrap_or(false))
        }
        #[cfg(not(target_os = "android"))]
        {
          let _ = joystick;
          false
        }
      }
      Pad::Raw(_) => false,
    });
    let edge = down && !self.back_down;
    // Level tracking continues while muted, so a press held across the mute
    // is still one request, never an edge on the way out.
    self.back_down = down;
    edge && !self.muted
  }

  fn slot_of(&self, id: u32) -> Option<usize> {
    self.slots.iter().position(|s| s.as_ref().is_some_and(|p| p.id() == id))
  }
}
