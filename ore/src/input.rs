//! The input vocabulary: the kinds of pointer and the keyboard modifier
//! state an input event carries, and the W3C UI Events `code` for a key
//! name. Plain data, the same on every platform; the translation from the
//! platform's own events comes with the event pump.

/// Pointer kind. Combined with a u64 pointer_id, uniquely identifies an
/// active pointer. Mouse and touch IDs come from disjoint SDL ID spaces,
/// so they share a numeric range only by accident; pointer_type
/// discriminates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerType {
  Mouse,
  Touch,
  Pen,
}

impl PointerType {
  pub fn as_str(self) -> &'static str {
    match self {
      PointerType::Mouse => "mouse",
      PointerType::Touch => "touch",
      PointerType::Pen => "pen",
    }
  }
}

/// Keyboard modifier state at the time of an event. `meta` is Cmd on
/// macOS, Win on Windows, Super on Linux. Matches the names browsers
/// expose via KeyboardEvent / PointerEvent (shiftKey, ctrlKey, ...).
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
  pub shift: bool,
  pub ctrl: bool,
  pub alt: bool,
  pub meta: bool,
}

/// W3C `KeyboardEvent.code` for a synthetic event that carries only a `key`
/// name: the position that produces that value on a US layout, or
/// "Unidentified" when the key has no single unshifted position (shifted
/// punctuation, non-ASCII, unknown names). Real events derive `code` from the
/// platform's scancode instead.
pub fn w3c_code_for_key(key: &str) -> &'static str {
  const LETTERS: [&str; 26] = [
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL", "KeyM", "KeyN",
    "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX", "KeyY", "KeyZ",
  ];
  const DIGITS: [&str; 10] =
    ["Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9"];
  let mut chars = key.chars();
  if let (Some(c), None) = (chars.next(), chars.next()) {
    return match c {
      'a'..='z' | 'A'..='Z' => LETTERS[(c.to_ascii_lowercase() as u8 - b'a') as usize],
      '0'..='9' => DIGITS[(c as u8 - b'0') as usize],
      ' ' => "Space",
      '-' => "Minus",
      '=' => "Equal",
      '[' => "BracketLeft",
      ']' => "BracketRight",
      '\\' => "Backslash",
      ';' => "Semicolon",
      '\'' => "Quote",
      '`' => "Backquote",
      ',' => "Comma",
      '.' => "Period",
      '/' => "Slash",
      _ => "Unidentified",
    };
  }
  match key {
    "Enter" => "Enter",
    "Escape" => "Escape",
    "Backspace" => "Backspace",
    "Tab" => "Tab",
    "Delete" => "Delete",
    "Insert" => "Insert",
    "Home" => "Home",
    "End" => "End",
    "PageUp" => "PageUp",
    "PageDown" => "PageDown",
    "ArrowUp" => "ArrowUp",
    "ArrowDown" => "ArrowDown",
    "ArrowLeft" => "ArrowLeft",
    "ArrowRight" => "ArrowRight",
    "CapsLock" => "CapsLock",
    "NumLock" => "NumLock",
    "ScrollLock" => "ScrollLock",
    "PrintScreen" => "PrintScreen",
    "Pause" => "Pause",
    "ContextMenu" => "ContextMenu",
    "Help" => "Help",
    "Power" => "Power",
    // Left/right placement is not expressible in `key`; synthesize the left.
    "Shift" => "ShiftLeft",
    "Control" => "ControlLeft",
    "Alt" => "AltLeft",
    "Meta" => "MetaLeft",
    "F1" => "F1",
    "F2" => "F2",
    "F3" => "F3",
    "F4" => "F4",
    "F5" => "F5",
    "F6" => "F6",
    "F7" => "F7",
    "F8" => "F8",
    "F9" => "F9",
    "F10" => "F10",
    "F11" => "F11",
    "F12" => "F12",
    "F13" => "F13",
    "F14" => "F14",
    "F15" => "F15",
    "F16" => "F16",
    "F17" => "F17",
    "F18" => "F18",
    "F19" => "F19",
    "F20" => "F20",
    "F21" => "F21",
    "F22" => "F22",
    "F23" => "F23",
    "F24" => "F24",
    _ => "Unidentified",
  }
}

/// Connected input device classes. Presence, not traffic: a connected mouse
/// that never moves still reports true. Read at init and re-read on
/// keyboard/mouse hotplug; SDL has no touch hotplug events, so touch is
/// re-queried on those same occasions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputDevices {
  pub keyboard: bool,
  pub mouse: bool,
  pub touch: bool,
  pub screen_keyboard: bool,
}
