use impellers::{ISize, Rect};
use sdl3::event::Event as SdlEvent;

use crate::sdl_utils;

pub enum AlloyCommand {
  EmitInitEvents,
  SetTitle(String),
  SetFullscreen(bool),
  SetCursor(sdl3::mouse::SystemCursor),
  SetCursorVisible(bool),
  SetTextInputActive(bool),
}

// Pointer kind. Combined with a u64 pointer_id, uniquely identifies an
// active pointer. Mouse and touch IDs come from disjoint SDL ID spaces,
// so they share a numeric range only by accident; pointer_type
// discriminates them.
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

// Keyboard modifier state at the time of an event. `meta` is Cmd on
// macOS, Win on Windows, Super on Linux. Matches the names browsers
// expose via KeyboardEvent / PointerEvent (shiftKey, ctrlKey, ...).
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
  pub shift: bool,
  pub ctrl: bool,
  pub alt: bool,
  pub meta: bool,
}

impl From<sdl3::keyboard::Mod> for Modifiers {
  fn from(m: sdl3::keyboard::Mod) -> Self {
    use sdl3::keyboard::Mod;
    Self {
      shift: m.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
      ctrl: m.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD),
      alt: m.intersects(Mod::LALTMOD | Mod::RALTMOD),
      meta: m.intersects(Mod::LGUIMOD | Mod::RGUIMOD),
    }
  }
}

// One connected gamepad's current state. `buttons` holds the names of the
// currently-pressed buttons (SDL3 positional names: "south", "dpadUp", ...);
// `axes` holds every axis as (name, value) with sticks in -1..1 and triggers
// in 0..1.
#[derive(Clone)]
pub struct GamepadState {
  pub id: u32,
  pub name: String,
  pub buttons: Vec<&'static str>,
  pub axes: Vec<(&'static str, f32)>,
}

#[derive(Clone)]
pub enum AlloyEvent {
  Quit,
  WindowFocus,
  WindowBlur,
  KeyDown { keycode: Option<sdl3::keyboard::Keycode>, scancode: Option<sdl3::keyboard::Scancode>, modifiers: Modifiers },
  KeyUp { keycode: Option<sdl3::keyboard::Keycode>, scancode: Option<sdl3::keyboard::Scancode>, modifiers: Modifiers },
  Resize { size: ISize, safe_area: Rect, display_scale: f32 },
  // `time` is raw wall-clock seconds since render-thread start, sampled right
  // after present. Intentionally unsmoothed: pacing is userspace policy.
  FrameRendered { frame: u64, fps: u32, time: f64 },
  // Idle tick: emitted at the refresh cadence when no display list has arrived
  // for a full refresh period, so the UI thread keeps running its per-frame
  // logic (timers, signal flush, camera pump) while nothing is presented.
  // `frame` is the present counter, i.e. one past the last FrameRendered's
  // frame: the index the next present will get.
  Tick { frame: u64, fps: u32 },
  // Display refresh rate in Hz. Its own event (independent of frames): emitted
  // at startup and whenever the rate changes (e.g. Android 90 <-> 60Hz).
  DisplayRefreshRate { hz: f32 },
  PointerMove { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, modifiers: Modifiers },
  PointerDown { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  PointerUp { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  TextInput { text: String },
  PowerStatus { info: sdl_utils::PowerInfo },
  // Emitted when the on-screen keyboard visibility or size changes. SDL does
  // not provide an event for this, so it is detected by polling
  // SDL_ScreenKeyboardShown and the platform-reported IME inset each loop
  // iteration. `height` is the keyboard's overlap with the window in logical
  // pixels (0 when hidden or unsupported); the window is fullscreen so it is
  // not resized for the keyboard, and the app uses this to lift its content.
  KeyboardVisibility { shown: bool, height: f32 },
  // delta_x / delta_y use browser convention: positive delta_y means
  // content should scroll down (wheel rolled toward the user). SDL's
  // direction=Flipped is normalized away at translation time.
  Wheel { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, delta_x: f32, delta_y: f32, modifiers: Modifiers },
  // OS-level dark/light preference. Emitted at init and whenever the OS theme
  // changes; Unknown on platforms that do not report one.
  SystemTheme { theme: sdl_utils::SystemTheme },
  // Connected input device classes. Presence, not traffic: a connected mouse
  // that never moves still reports true. Emitted at init and on keyboard/mouse
  // hotplug; SDL has no touch hotplug events, so touch is re-queried on those
  // same occasions.
  InputDevices { keyboard: bool, mouse: bool, touch: bool },
  // Orientation of the display the window is on. Emitted at init and on
  // rotation.
  DisplayOrientation { orientation: sdl3::video::Orientation },
  // Full connected-gamepad state, emitted whenever any pad connects,
  // disconnects, or changes a button/axis (coalesced to at most one per main
  // loop iteration), plus once at init. Slots are stable for a pad's whole
  // connection; a disconnect leaves a None hole that the next connect reuses.
  Gamepads { pads: Vec<Option<GamepadState>> },
  // Camera hotplug. Carries no device id (subscribers re-enumerate via
  // camera::list_cameras()). SDL only delivers these once the camera
  // subsystem is initialized, i.e. after the first list/open call.
  //
  // SDL 3.4.8 hotplug coverage is uneven; this arm handles both add and remove
  // and simply fires whatever SDL delivers:
  //   - Android: add + remove both work.
  //   - Linux: add works; remove is broken in both backends -- pipewire never
  //     calls SDL_CameraDisconnected, v4l2 mis-gates removals on a device class
  //     that is 0 on remove (a filed one-line upstream fix; we force v4l2 in
  //     camera_subsystem_init to pick it up automatically once SDL ships it).
  //   - macOS/Windows: no camera hotplug at all (upstream FIXMEs, not wired),
  //     so neither add nor remove arrives.
  CameraDeviceChange { added: bool },
}

pub(crate) fn current_system_theme_event() -> AlloyEvent {
  AlloyEvent::SystemTheme { theme: sdl_utils::system_theme() }
}

pub(crate) fn current_input_devices_event() -> AlloyEvent {
  AlloyEvent::InputDevices {
    keyboard: sdl_utils::has_keyboard(),
    mouse: sdl_utils::has_mouse(),
    touch: !sdl3::touch::num_touch_devices().is_empty(),
  }
}

pub(crate) fn current_orientation_event(window: &sdl3::video::Window) -> AlloyEvent {
  let orientation = window
    .get_display()
    .map(|d| sdl3::video::Orientation::from_ll(d.get_orientation()))
    .unwrap_or(sdl3::video::Orientation::Unknown);
  AlloyEvent::DisplayOrientation { orientation }
}

pub(crate) fn current_resize_event(window: &sdl3::video::Window) -> AlloyEvent {
  let (w, h) = window.size_in_pixels();
  let scale = sdl_utils::window_display_scale(window);
  let r = sdl_utils::window_safe_area(window);
  AlloyEvent::Resize {
    size: ISize::new((w as f32 / scale) as i64, (h as f32 / scale) as i64),
    safe_area: Rect::new(impellers::Point::new(r.x as f32, r.y as f32), impellers::Size::new(r.w as f32, r.h as f32)),
    display_scale: scale,
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

pub(crate) fn translate_event(sdl_event: SdlEvent, window: &sdl3::video::Window) -> Option<AlloyEvent> {
  match sdl_event {
    SdlEvent::Quit { .. } => Some(AlloyEvent::Quit),
    SdlEvent::KeyDown { keycode, scancode, keymod, .. } => {
      Some(AlloyEvent::KeyDown { keycode, scancode, modifiers: keymod.into() })
    }
    SdlEvent::KeyUp { keycode, scancode, keymod, .. } => {
      Some(AlloyEvent::KeyUp { keycode, scancode, modifiers: keymod.into() })
    }
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::FocusGained, .. } => Some(AlloyEvent::WindowFocus),
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::FocusLost, .. } => Some(AlloyEvent::WindowBlur),
    SdlEvent::Window { win_event: sdl3::event::WindowEvent::PixelSizeChanged(w, h), .. } => {
      let display_scale = sdl_utils::window_display_scale(window);
      let size = ISize::new((w as f32 / display_scale) as i64, (h as f32 / display_scale) as i64);
      let r = sdl_utils::window_safe_area(window);
      let safe_area =
        Rect::new(impellers::Point::new(r.x as f32, r.y as f32), impellers::Size::new(r.w as f32, r.h as f32));
      Some(AlloyEvent::Resize { size, safe_area, display_scale })
    }
    // SDL3 reports mouse coordinates in the window's logical coordinate space
    // (the same units as SDL_GetWindowSize), which is what the layout/hit tree
    // uses, so they are passed through unscaled. Dividing by display_scale would
    // over-shrink the pointer on a fractional-scaled display (a no-op at 1.0).
    SdlEvent::MouseMotion { which, x, y, .. } => Some(AlloyEvent::PointerMove {
      pointer_id: which as u64,
      pointer_type: PointerType::Mouse,
      x,
      y,
      modifiers: sdl_utils::mod_state().into(),
    }),
    SdlEvent::MouseButtonDown { which, mouse_btn, x, y, .. } => {
      let button = map_mouse_button(mouse_btn)?;
      Some(AlloyEvent::PointerDown {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x,
        y,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::MouseButtonUp { which, mouse_btn, x, y, .. } => {
      let button = map_mouse_button(mouse_btn)?;
      Some(AlloyEvent::PointerUp {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x,
        y,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::MouseWheel { which, x, y, direction, mouse_x, mouse_y, .. } => {
      let flipped = matches!(direction, sdl3::mouse::MouseWheelDirection::Flipped);
      let sign = if flipped { 1.0 } else { -1.0 };
      Some(AlloyEvent::Wheel {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        x: mouse_x,
        y: mouse_y,
        delta_x: sign * x * 100.0,
        delta_y: sign * y * 100.0,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    // SDL touch coordinates are normalized [0, 1]; scale to logical pixels.
    // touch_id distinguishes multiple touch surfaces, finger_id distinguishes
    // simultaneous touches on one surface. We key on finger_id and rely on
    // pointer_type=Touch to disambiguate from mouse; if multi-surface touch
    // matters later, pointer_id can be (touch_id << 32) | finger_id.
    SdlEvent::FingerDown { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(AlloyEvent::PointerDown {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        button: 0,
        x: x * lw,
        y: y * lh,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::FingerMotion { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(AlloyEvent::PointerMove {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        x: x * lw,
        y: y * lh,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::FingerUp { finger_id, x, y, .. } => {
      let (lw, lh) = touch_window_logical_size(window);
      Some(AlloyEvent::PointerUp {
        pointer_id: finger_id,
        pointer_type: PointerType::Touch,
        button: 0,
        x: x * lw,
        y: y * lh,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::TextInput { text, .. } => Some(AlloyEvent::TextInput { text }),
    // Any display's orientation event triggers a re-query of the window's own
    // display (authoritative, and sidesteps matching display ids); a rotation
    // elsewhere re-emits an unchanged value, which subscribers dedupe.
    SdlEvent::Display { display_event: sdl3::event::DisplayEvent::Orientation(_), .. } => {
      Some(current_orientation_event(window))
    }
    // Like the camera events below, these have no sdl3 crate Event variants and
    // arrive as Unknown, recovered by raw type id.
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_SYSTEM_THEME_CHANGED.0 => {
      Some(current_system_theme_event())
    }
    SdlEvent::Unknown { type_, .. }
      if type_ == sdl3::sys::events::SDL_EVENT_KEYBOARD_ADDED.0
        || type_ == sdl3::sys::events::SDL_EVENT_KEYBOARD_REMOVED.0
        || type_ == sdl3::sys::events::SDL_EVENT_MOUSE_ADDED.0
        || type_ == sdl3::sys::events::SDL_EVENT_MOUSE_REMOVED.0 =>
    {
      Some(current_input_devices_event())
    }
    // The sdl3 crate has no Event variants for camera device events, so they
    // arrive as Unknown and are recovered by raw type id. Approved/denied
    // (0x1402/0x1403) stay ignored: open_camera polls permission state itself.
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_CAMERA_DEVICE_ADDED.0 => {
      Some(AlloyEvent::CameraDeviceChange { added: true })
    }
    SdlEvent::Unknown { type_, .. } if type_ == sdl3::sys::events::SDL_EVENT_CAMERA_DEVICE_REMOVED.0 => {
      Some(AlloyEvent::CameraDeviceChange { added: false })
    }
    _ => None,
  }
}

fn touch_window_logical_size(window: &sdl3::video::Window) -> (f32, f32) {
  let scale = sdl_utils::window_display_scale(window);
  let (pw, ph) = window.size_in_pixels();
  (pw as f32 / scale, ph as f32 / scale)
}
