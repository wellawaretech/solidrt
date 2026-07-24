// SDL -> W3C keyboard mapping: turn SDL keycodes/scancodes into the `key` /
// `code` values defined by the W3C UI Events spec (KeyboardEvent.key /
// KeyboardEvent.code), so nothing outside this crate ever sees SDL's naming.
// `key` is the logical, layout-dependent value ("a", "!", "Enter"); `code` is
// the physical, layout-independent key position ("KeyA", "Digit1",
// "NumpadEnter").

use sdl3::keyboard::{Keycode, Mod, Scancode};
use sdl3::sys::keycode::SDL_Keymod;

/// W3C `KeyboardEvent.code` for a physical key. "Unidentified" when SDL has no
/// scancode or the key has no standard code value.
pub(crate) fn w3c_code(scancode: Option<Scancode>) -> &'static str {
  use Scancode as S;
  let Some(s) = scancode else { return "Unidentified" };
  match s {
    S::A => "KeyA",
    S::B => "KeyB",
    S::C => "KeyC",
    S::D => "KeyD",
    S::E => "KeyE",
    S::F => "KeyF",
    S::G => "KeyG",
    S::H => "KeyH",
    S::I => "KeyI",
    S::J => "KeyJ",
    S::K => "KeyK",
    S::L => "KeyL",
    S::M => "KeyM",
    S::N => "KeyN",
    S::O => "KeyO",
    S::P => "KeyP",
    S::Q => "KeyQ",
    S::R => "KeyR",
    S::S => "KeyS",
    S::T => "KeyT",
    S::U => "KeyU",
    S::V => "KeyV",
    S::W => "KeyW",
    S::X => "KeyX",
    S::Y => "KeyY",
    S::Z => "KeyZ",
    S::_1 => "Digit1",
    S::_2 => "Digit2",
    S::_3 => "Digit3",
    S::_4 => "Digit4",
    S::_5 => "Digit5",
    S::_6 => "Digit6",
    S::_7 => "Digit7",
    S::_8 => "Digit8",
    S::_9 => "Digit9",
    S::_0 => "Digit0",
    S::Return => "Enter",
    S::Escape => "Escape",
    S::Backspace => "Backspace",
    S::Tab => "Tab",
    S::Space => "Space",
    S::Minus => "Minus",
    S::Equals => "Equal",
    S::LeftBracket => "BracketLeft",
    S::RightBracket => "BracketRight",
    // NonUsHash is the ISO layout's second hash/tilde key; the W3C code
    // tables fold it into "Backslash".
    S::Backslash | S::NonUsHash => "Backslash",
    S::Semicolon => "Semicolon",
    S::Apostrophe => "Quote",
    S::Grave => "Backquote",
    S::Comma => "Comma",
    S::Period => "Period",
    S::Slash => "Slash",
    S::CapsLock => "CapsLock",
    S::F1 => "F1",
    S::F2 => "F2",
    S::F3 => "F3",
    S::F4 => "F4",
    S::F5 => "F5",
    S::F6 => "F6",
    S::F7 => "F7",
    S::F8 => "F8",
    S::F9 => "F9",
    S::F10 => "F10",
    S::F11 => "F11",
    S::F12 => "F12",
    S::F13 => "F13",
    S::F14 => "F14",
    S::F15 => "F15",
    S::F16 => "F16",
    S::F17 => "F17",
    S::F18 => "F18",
    S::F19 => "F19",
    S::F20 => "F20",
    S::F21 => "F21",
    S::F22 => "F22",
    S::F23 => "F23",
    S::F24 => "F24",
    S::PrintScreen => "PrintScreen",
    S::ScrollLock => "ScrollLock",
    S::Pause => "Pause",
    S::Insert => "Insert",
    S::Home => "Home",
    S::PageUp => "PageUp",
    S::Delete => "Delete",
    S::End => "End",
    S::PageDown => "PageDown",
    S::Right => "ArrowRight",
    S::Left => "ArrowLeft",
    S::Down => "ArrowDown",
    S::Up => "ArrowUp",
    S::NumLockClear => "NumLock",
    S::KpDivide => "NumpadDivide",
    S::KpMultiply => "NumpadMultiply",
    S::KpMinus => "NumpadSubtract",
    S::KpPlus => "NumpadAdd",
    S::KpEnter => "NumpadEnter",
    S::Kp1 => "Numpad1",
    S::Kp2 => "Numpad2",
    S::Kp3 => "Numpad3",
    S::Kp4 => "Numpad4",
    S::Kp5 => "Numpad5",
    S::Kp6 => "Numpad6",
    S::Kp7 => "Numpad7",
    S::Kp8 => "Numpad8",
    S::Kp9 => "Numpad9",
    S::Kp0 => "Numpad0",
    S::KpPeriod => "NumpadDecimal",
    S::KpEquals | S::KpEqualsAs400 => "NumpadEqual",
    S::KpComma => "NumpadComma",
    S::NonUsBackslash => "IntlBackslash",
    S::International1 => "IntlRo",
    S::International3 => "IntlYen",
    S::Lang1 => "Lang1",
    S::Lang2 => "Lang2",
    S::Lang3 => "Lang3",
    S::Lang4 => "Lang4",
    S::Lang5 => "Lang5",
    S::Application | S::Menu => "ContextMenu",
    S::Power => "Power",
    S::Help => "Help",
    S::Again => "Again",
    S::Undo => "Undo",
    S::Cut => "Cut",
    S::Copy => "Copy",
    S::Paste => "Paste",
    S::Find => "Find",
    S::Select => "Select",
    S::Mute => "AudioVolumeMute",
    S::VolumeUp => "AudioVolumeUp",
    S::VolumeDown => "AudioVolumeDown",
    S::MediaPlay | S::MediaPlayPause => "MediaPlayPause",
    S::MediaStop => "MediaStop",
    S::MediaNextTrack => "MediaTrackNext",
    S::MediaPreviousTrack => "MediaTrackPrevious",
    S::AcSearch => "BrowserSearch",
    S::AcHome => "BrowserHome",
    S::AcBack => "BrowserBack",
    S::AcForward => "BrowserForward",
    S::AcRefresh => "BrowserRefresh",
    S::LCtrl => "ControlLeft",
    S::LShift => "ShiftLeft",
    S::LAlt => "AltLeft",
    S::LGui => "MetaLeft",
    S::RCtrl => "ControlRight",
    S::RShift => "ShiftRight",
    S::RAlt => "AltRight",
    S::RGui => "MetaRight",
    _ => "Unidentified",
  }
}

/// W3C `KeyboardEvent.key` for a key event. Named keys ("Enter", "ArrowLeft",
/// "Shift") come from the keycode; printable keys resolve through the active
/// keyboard layout with the modifier state applied, so Shift+1 is "!" and
/// letters carry their produced case. "Unidentified" when nothing matches.
pub(crate) fn w3c_key(keycode: Option<Keycode>, scancode: Option<Scancode>, keymod: Mod) -> String {
  if let Some(k) = keycode {
    if let Some(name) = named_key(k, keymod) {
      return name.to_string();
    }
  }
  // Printables: let SDL resolve the physical key through the layout with
  // modifiers applied (SDL_GetKeyFromScancode), then take the character.
  if let Some(s) = scancode {
    if let Some(k) = Keycode::from_scancode(s, SDL_Keymod(keymod.bits()), true) {
      if let Some(ch) = keycode_char(k) {
        return ch.to_string();
      }
    }
  }
  // No scancode (synthetic events): fall back to the keycode's own codepoint.
  if let Some(ch) = keycode.and_then(keycode_char) {
    return ch.to_string();
  }
  "Unidentified".to_string()
}

fn named_key(k: Keycode, keymod: Mod) -> Option<&'static str> {
  use Keycode as K;
  // Numpad keys produce digits with NumLock on and navigation with it off;
  // SDL reports the same keycode either way, so the split happens here.
  let num = keymod.contains(Mod::NUMMOD);
  Some(match k {
    K::Return | K::KpEnter => "Enter",
    K::Tab => "Tab",
    K::Backspace => "Backspace",
    K::Escape => "Escape",
    K::Delete => "Delete",
    K::Insert => "Insert",
    K::Home => "Home",
    K::End => "End",
    K::PageUp => "PageUp",
    K::PageDown => "PageDown",
    K::Up => "ArrowUp",
    K::Down => "ArrowDown",
    K::Left => "ArrowLeft",
    K::Right => "ArrowRight",
    K::CapsLock => "CapsLock",
    K::NumLockClear => "NumLock",
    K::ScrollLock => "ScrollLock",
    K::PrintScreen => "PrintScreen",
    K::Pause => "Pause",
    K::Application | K::Menu => "ContextMenu",
    K::Help => "Help",
    K::Power => "Power",
    // Left/right placement is expressed only in `code`, never in `key`.
    K::LShift | K::RShift => "Shift",
    K::LCtrl | K::RCtrl => "Control",
    K::LAlt | K::RAlt => "Alt",
    K::LGui | K::RGui => "Meta",
    K::Mode => "AltGraph",
    K::F1 => "F1",
    K::F2 => "F2",
    K::F3 => "F3",
    K::F4 => "F4",
    K::F5 => "F5",
    K::F6 => "F6",
    K::F7 => "F7",
    K::F8 => "F8",
    K::F9 => "F9",
    K::F10 => "F10",
    K::F11 => "F11",
    K::F12 => "F12",
    K::F13 => "F13",
    K::F14 => "F14",
    K::F15 => "F15",
    K::F16 => "F16",
    K::F17 => "F17",
    K::F18 => "F18",
    K::F19 => "F19",
    K::F20 => "F20",
    K::F21 => "F21",
    K::F22 => "F22",
    K::F23 => "F23",
    K::F24 => "F24",
    K::Kp0 => {
      if num {
        "0"
      } else {
        "Insert"
      }
    }
    K::Kp1 => {
      if num {
        "1"
      } else {
        "End"
      }
    }
    K::Kp2 => {
      if num {
        "2"
      } else {
        "ArrowDown"
      }
    }
    K::Kp3 => {
      if num {
        "3"
      } else {
        "PageDown"
      }
    }
    K::Kp4 => {
      if num {
        "4"
      } else {
        "ArrowLeft"
      }
    }
    K::Kp5 => {
      if num {
        "5"
      } else {
        "Clear"
      }
    }
    K::Kp6 => {
      if num {
        "6"
      } else {
        "ArrowRight"
      }
    }
    K::Kp7 => {
      if num {
        "7"
      } else {
        "Home"
      }
    }
    K::Kp8 => {
      if num {
        "8"
      } else {
        "ArrowUp"
      }
    }
    K::Kp9 => {
      if num {
        "9"
      } else {
        "PageUp"
      }
    }
    K::KpPeriod => {
      if num {
        "."
      } else {
        "Delete"
      }
    }
    K::KpDivide => "/",
    K::KpMultiply => "*",
    K::KpMinus => "-",
    K::KpPlus => "+",
    K::KpEquals | K::KpEqualsAs400 => "=",
    K::KpComma => ",",
    K::Mute => "AudioVolumeMute",
    K::VolumeUp => "AudioVolumeUp",
    K::VolumeDown => "AudioVolumeDown",
    K::MediaPlay => "MediaPlay",
    K::MediaPlayPause => "MediaPlayPause",
    K::MediaStop => "MediaStop",
    K::MediaNextTrack => "MediaTrackNext",
    K::MediaPreviousTrack => "MediaTrackPrevious",
    K::AcSearch => "BrowserSearch",
    K::AcHome => "BrowserHome",
    K::AcBack => "BrowserBack",
    K::AcForward => "BrowserForward",
    K::AcRefresh => "BrowserRefresh",
    K::Undo => "Undo",
    K::Cut => "Cut",
    K::Copy => "Copy",
    K::Paste => "Paste",
    K::Find => "Find",
    _ => return None,
  })
}

// The character a keycode produces, when it is a printable codepoint. SDL
// keycodes for printables are the unicode value itself; non-character keys
// have the scancode bit (1<<30) or extended bit (1<<29) set instead.
fn keycode_char(k: Keycode) -> Option<char> {
  let v = k as u32;
  if v == 0 || v & 0x6000_0000 != 0 {
    return None;
  }
  char::from_u32(v).filter(|c| !c.is_control())
}
