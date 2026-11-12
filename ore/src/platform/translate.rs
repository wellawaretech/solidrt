//! SDL's events as ore's (`PlatformEvent`): the arms of the pump's
//! translation, the modifier and button mapping, the back triggers, the
//! window-unit conversions, the pointer-lock coordinate freeze and, on
//! Android, the touch side channel's samples. Needs the window: the scale,
//! the size and the safe area are read live per event.

use sdl3::event::Event as SdlEvent;
use sdl3::keyboard::Scancode;

use super::keymap::{mod_state, w3c_code, w3c_key};
use crate::event::PlatformEvent;
use crate::input::{Modifiers, PointerType};
use crate::window::Window;

/// Scroll distance per wheel step, in the browser's pixel delta units: SDL
/// reports steps (fractional on precise devices), a DOM wheel event pixels.
const WHEEL_DELTA_PER_STEP: f32 = 100.0;

// SDL's modifier bits as the modifier state an event carries: either side's
// key sets the flag.
fn modifiers_from_sdl(m: sdl3::keyboard::Mod) -> Modifiers {
  use sdl3::keyboard::Mod;
  Modifiers {
    shift: m.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
    ctrl: m.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD),
    alt: m.intersects(Mod::LALTMOD | Mod::RALTMOD),
    meta: m.intersects(Mod::LGUIMOD | Mod::RGUIMOD),
  }
}

// Maps SDL mouse buttons to web-standard MouseEvent.button codes:
// 0=left, 1=middle, 2=right, 3=back (X1), 4=forward (X2).
// Unknown returns None so the caller can drop the event.
fn map_mouse_button(b: sdl3::mouse::MouseButton) -> Option<u8> {
  use sdl3::mouse::MouseButton::*;
  match b {
    Left => Some(0),
    Middle => Some(1),
    Right => Some(2),
    X1 => Some(3),
    X2 => Some(4),
    Unknown => None,
  }
}

// The user's back triggers, normalized to PlatformEvent::Back before the
// generic key path so apps never additionally see them as keydown/keyup:
// AC_BACK on every platform (on Android the system back button and back
// swipe, the only signal with SDL_ANDROID_TRAP_BACK_BUTTON=1; on desktop the
// keyboard's BrowserBack media key arrives as the same scancode), plus the
// client-owned chord Ctrl+Shift+Backspace (Cmd+Shift+Backspace on macOS).
// The gamepad "back" (select) button is the third trigger, normalized on the
// pad snapshot path (see gamepad.rs take_back_edge).
fn is_back_trigger(scancode: Option<Scancode>, keymod: sdl3::keyboard::Mod) -> bool {
  if scancode == Some(Scancode::AcBack) {
    return true;
  }
  let m = modifiers_from_sdl(keymod);
  let chord_mod = if cfg!(target_os = "macos") { m.meta } else { m.ctrl };
  chord_mod && m.shift && scancode == Some(Scancode::Backspace)
}

/// One SDL event as at most one platform event. Events with no meaning
/// upward (the pump's own wake, a custom event, anything unmapped) come back
/// None.
pub(super) fn translate(sdl_event: SdlEvent, window: &Window) -> Option<PlatformEvent> {
  match sdl_event {
    SdlEvent::Quit { .. } => Some(PlatformEvent::Quit),
    // iOS applicationWillTerminate (Android sends it alongside Quit at
    // destroy, where the second copy is moot: Quit ends the process).
    SdlEvent::AppTerminating { .. } => Some(PlatformEvent::Quit),
    SdlEvent::KeyDown { keycode, scancode, keymod, repeat, .. } => {
      if is_back_trigger(scancode, keymod) {
        // Repeats collapse: holding the trigger is one request.
        return if repeat { None } else { Some(PlatformEvent::Back) };
      }
      Some(PlatformEvent::Key {
        down: true,
        key: w3c_key(keycode, scancode, keymod),
        code: w3c_code(scancode),
        modifiers: modifiers_from_sdl(keymod),
        repeat,
      })
    }
    SdlEvent::KeyUp { keycode, scancode, keymod, repeat, .. } => {
      // Swallow the trigger's release too. Best-effort for the chord: with the
      // modifiers already released, a stray keyup "Backspace" reaches apps,
      // which is harmless (nothing acts on an unmatched keyup).
      if is_back_trigger(scancode, keymod) {
        return None;
      }
      Some(PlatformEvent::Key {
        down: false,
        key: w3c_key(keycode, scancode, keymod),
        code: w3c_code(scancode),
        modifiers: modifiers_from_sdl(keymod),
        repeat,
      })
    }
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::FocusGained, .. } => Some(PlatformEvent::WindowFocus),
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::FocusLost, .. } => Some(PlatformEvent::WindowBlur),
    // Lifecycle: Android pause/resume arrives on the app path, desktop
    // minimize/restore on the window path. DID (not WILL) on both Android
    // sides: the transition is only fact once it completed, and on resume the
    // recreated EGL surface exists by then. Occluded is deliberately not
    // mapped (noisy on macOS, and partial occlusion is not "hidden").
    SdlEvent::AppDidEnterBackground { .. } => Some(PlatformEvent::Visibility { visible: false }),
    SdlEvent::AppDidEnterForeground { .. } => Some(PlatformEvent::Visibility { visible: true }),
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::Minimized, .. }
    | SdlEvent::Window { win_event: sdl3::event::WindowEvent::Hidden, .. } => {
      Some(PlatformEvent::Visibility { visible: false })
    }
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::Restored, .. }
    | SdlEvent::Window { win_event: sdl3::event::WindowEvent::Shown, .. } => {
      Some(PlatformEvent::Visibility { visible: true })
    }
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::Exposed, .. } => Some(PlatformEvent::Exposed),
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::PixelSizeChanged(w, h), .. } => {
      Some(PlatformEvent::Resize {
        pixel_size: (w.max(0) as u32, h.max(0) as u32),
        display_scale: window.display_scale(),
        safe_area: window.safe_area(),
      })
    }
    // SDL reports mouse coordinates in window units, which are logical points
    // on some backends (macOS, Wayland) and physical pixels on others
    // (Android, X11). The upward logical space is always
    // size_in_pixels / display_scale (as a resize reports it), so mouse
    // coordinates convert by mouse_scale: the ratio between the two spaces,
    // 1.0 where SDL already reports logical points.
    SdlEvent::MouseMotion { which, x, y, xrel, yrel, .. } => {
      let k = mouse_scale(window);
      Some(PlatformEvent::PointerMove {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        x: x * k,
        y: y * k,
        rel: Some((xrel * k, yrel * k)),
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::MouseButtonDown { which, mouse_btn, x, y, .. } => {
      let k = mouse_scale(window);
      let button = map_mouse_button(mouse_btn)?;
      Some(PlatformEvent::PointerDown {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x: x * k,
        y: y * k,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::MouseButtonUp { which, mouse_btn, x, y, .. } => {
      let k = mouse_scale(window);
      let button = map_mouse_button(mouse_btn)?;
      Some(PlatformEvent::PointerUp {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x: x * k,
        y: y * k,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::MouseWheel { which, x, y, direction, mouse_x, mouse_y, .. } => {
      let k = mouse_scale(window);
      let flipped = matches!(direction, sdl3::mouse::MouseWheelDirection::Flipped);
      let sign = if flipped { 1.0 } else { -1.0 };
      Some(PlatformEvent::Wheel {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        x: mouse_x * k,
        y: mouse_y * k,
        delta_x: sign * x * WHEEL_DELTA_PER_STEP,
        delta_y: sign * y * WHEEL_DELTA_PER_STEP,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    // SDL touch coordinates are normalized [0, 1]; scale to logical pixels.
    // touch_id distinguishes multiple touch surfaces, finger_id distinguishes
    // simultaneous touches on one surface. We key on finger_id and rely on
    // pointer_type=Touch to disambiguate from mouse; if multi-surface touch
    // matters later, pointer_id can be (touch_id << 32) | finger_id.
    SdlEvent::FingerDown { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(PlatformEvent::PointerDown {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        button: 0,
        x: x * lw,
        y: y * lh,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::FingerMotion { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(PlatformEvent::PointerMove {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        x: x * lw,
        y: y * lh,
        rel: None,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::FingerUp { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(PlatformEvent::PointerUp {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        button: 0,
        x: x * lw,
        y: y * lh,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::FingerCanceled { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(PlatformEvent::PointerCancel {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        x: x * lw,
        y: y * lh,
        modifiers: modifiers_from_sdl(mod_state()),
      })
    }
    SdlEvent::TextInput { text, .. } => Some(PlatformEvent::TextInput { text }),
    // Any display's orientation event triggers a re-query of the window's own
    // display (authoritative, and sidesteps matching display ids); a rotation
    // elsewhere re-emits an unchanged value, which subscribers dedupe.
    SdlEvent::Display { display_event: sdl3::event::DisplayEvent::Orientation(_), .. } => {
      Some(PlatformEvent::DisplayOrientation { orientation: window.orientation() })
    }
    // Like the camera events below, these have no sdl3 crate Event variants and
    // arrive as Unknown, recovered by raw type id.
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_SYSTEM_THEME_CHANGED.0 => {
      Some(PlatformEvent::SystemThemeChanged)
    }
    SdlEvent::Unknown { type_, .. }
      if type_ == sdl3::sys::events::SDL_EVENT_KEYBOARD_ADDED.0
        || type_ == sdl3::sys::events::SDL_EVENT_KEYBOARD_REMOVED.0
        || type_ == sdl3::sys::events::SDL_EVENT_MOUSE_ADDED.0
        || type_ == sdl3::sys::events::SDL_EVENT_MOUSE_REMOVED.0 =>
    {
      Some(PlatformEvent::InputDevices(crate::window::input_devices()))
    }
    // The sdl3 crate has no Event variants for camera device events, so they
    // arrive as Unknown and are recovered by raw type id. Approved/denied
    // (0x1402/0x1403) stay ignored: the camera open polls permission state
    // itself.
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_CAMERA_DEVICE_ADDED.0 => {
      Some(PlatformEvent::CameraDeviceChange { added: true })
    }
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_CAMERA_DEVICE_REMOVED.0 => {
      Some(PlatformEvent::CameraDeviceChange { added: false })
    }
    // SDL's drop path doubles as its URL delivery (Cocoa's kAEGetURL handler
    // and UIKit's openURL send the URL string as a drop file; the Android
    // activity forwards a warm intent's data the same way). A payload with a
    // scheme is a link; a real file drop is a path and stays unhandled.
    SdlEvent::DropFile { filename, .. } => super::link_from_drop(&filename).map(|link| PlatformEvent::Link { link }),
    // The pump's wake (super::Wake) and any other custom event fall through
    // here: the wake only ends the wait, its work is the caller's.
    _ => None,
  }
}

// A finger sample of Android's own touch path (see touch.rs) as the event
// SDL's finger events translate to above, which is where every other
// platform's touch comes from.
#[cfg(target_os = "android")]
pub(super) fn translate_touch(sample: &crate::touch::TouchSample, window: &Window) -> PlatformEvent {
  use crate::touch::TouchKind;
  let (lw, lh) = touch_window_logical_size(window);
  let (pointer_id, pointer_type) = (sample.finger_id, PointerType::Touch);
  let (x, y) = (sample.x * lw, sample.y * lh);
  let modifiers = modifiers_from_sdl(mod_state());
  match sample.kind {
    TouchKind::Down => PlatformEvent::PointerDown { pointer_id, pointer_type, button: 0, x, y, modifiers },
    TouchKind::Move => PlatformEvent::PointerMove { pointer_id, pointer_type, x, y, rel: None, modifiers },
    TouchKind::Up => PlatformEvent::PointerUp { pointer_id, pointer_type, button: 0, x, y, modifiers },
    TouchKind::Cancel => PlatformEvent::PointerCancel { pointer_id, pointer_type, x, y, modifiers },
  }
}

fn touch_window_logical_size(window: &Window) -> (f32, f32) {
  let scale = window.display_scale();
  let (pw, ph) = window.size_in_pixels();
  (pw as f32 / scale, ph as f32 / scale)
}

// SDL window units -> logical points, for mouse coordinates. On backends
// where SDL window units are already logical (macOS, Wayland) the ratio is
// 1.0; where they are physical pixels (Android, X11) it collapses to
// 1 / display_scale. Derived from the same sizes a resize reports, so mouse
// and layout can never disagree by construction.
fn mouse_scale(window: &Window) -> f32 {
  let (w, _) = window.size();
  let (pw, _) = window.size_in_pixels();
  if w == 0 || pw == 0 {
    return 1.0;
  }
  (pw as f32 / window.display_scale()) / w as f32
}

/// The pointer-lock coordinate freeze (web parity): SDL does NOT freeze x/y
/// in relative mode - it reports a window-clamped simulated position - so
/// hit testing would follow an invisible point. While locked, mouse events
/// report the lock point instead; motion continues via rel.
#[derive(Default)]
pub(super) struct PointerFreeze {
  // The lock point while the pointer is locked.
  frozen: Option<(f32, f32)>,
  // The mouse position as last seen unlocked: the next lock's freeze point.
  last_mouse: (f32, f32),
}

impl PointerFreeze {
  /// Follow the window's pointer lock as it is now: a fresh lock freezes at
  /// the last unlocked mouse position, an unlock clears the freeze.
  pub(super) fn follow(&mut self, locked: bool) {
    if !locked {
      self.frozen = None;
    } else if self.frozen.is_none() {
      self.frozen = Some(self.last_mouse);
    }
  }

  /// While locked, a mouse event carries the lock point; unlocked, its
  /// position is remembered as the next lock's freeze point.
  pub(super) fn apply(&mut self, event: &mut PlatformEvent) {
    match event {
      PlatformEvent::PointerMove { pointer_type: PointerType::Mouse, x, y, .. }
      | PlatformEvent::PointerDown { pointer_type: PointerType::Mouse, x, y, .. }
      | PlatformEvent::PointerUp { pointer_type: PointerType::Mouse, x, y, .. }
      | PlatformEvent::Wheel { pointer_type: PointerType::Mouse, x, y, .. } => match self.frozen {
        Some((fx, fy)) => {
          *x = fx;
          *y = fy;
        }
        None => self.last_mouse = (*x, *y),
      },
      _ => {}
    }
  }
}
