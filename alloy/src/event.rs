use impellers::{ISize, Rect};
use sdl3::event::Event as SdlEvent;

use crate::sdl_utils;

pub enum AlloyCommand {
  EmitInitEvents,
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

#[derive(Clone)]
pub enum AlloyEvent {
  Quit,
  KeyDown {
    keycode: Option<sdl3::keyboard::Keycode>,
    scancode: Option<sdl3::keyboard::Scancode>,
    modifiers: Modifiers,
  },
  KeyUp {
    keycode: Option<sdl3::keyboard::Keycode>,
    scancode: Option<sdl3::keyboard::Scancode>,
    modifiers: Modifiers,
  },
  Resize {
    size: ISize,
    safe_area: Rect,
    display_scale: f32,
  },
  FrameRendered { frame: u64 },
  PointerMove {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  PointerDown {
    pointer_id: u64,
    pointer_type: PointerType,
    button: u8,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  PointerUp {
    pointer_id: u64,
    pointer_type: PointerType,
    button: u8,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  // delta_x / delta_y use browser convention: positive delta_y means
  // content should scroll down (wheel rolled toward the user). SDL's
  // direction=Flipped is normalized away at translation time.
  Wheel {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
    modifiers: Modifiers,
  },
}

pub(crate) fn current_resize_event(window: &sdl3::video::Window) -> AlloyEvent {
  let (w, h) = window.size_in_pixels();
  let scale = sdl_utils::window_display_scale(window);
  let r = sdl_utils::window_safe_area(window);
  AlloyEvent::Resize {
    size: ISize::new((w as f32 / scale) as i64, (h as f32 / scale) as i64),
    safe_area: Rect::new(
      impellers::Point::new(r.x as f32, r.y as f32),
      impellers::Size::new(r.w as f32, r.h as f32),
    ),
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
    SdlEvent::Window {
      win_event: sdl3::event::WindowEvent::PixelSizeChanged(w, h),
      ..
    } => {
      let display_scale = sdl_utils::window_display_scale(window);
      let size = ISize::new((w as f32 / display_scale) as i64, (h as f32 / display_scale) as i64);
      let r = sdl_utils::window_safe_area(window);
      let safe_area = Rect::new(
        impellers::Point::new(r.x as f32, r.y as f32),
        impellers::Size::new(r.w as f32, r.h as f32),
      );
      Some(AlloyEvent::Resize { size, safe_area, display_scale })
    }
    SdlEvent::MouseMotion { which, x, y, .. } => {
      let scale = sdl_utils::window_display_scale(window);
      Some(AlloyEvent::PointerMove {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        x: x / scale,
        y: y / scale,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::MouseButtonDown { which, mouse_btn, x, y, .. } => {
      let button = map_mouse_button(mouse_btn)?;
      let scale = sdl_utils::window_display_scale(window);
      Some(AlloyEvent::PointerDown {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x: x / scale,
        y: y / scale,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::MouseButtonUp { which, mouse_btn, x, y, .. } => {
      let button = map_mouse_button(mouse_btn)?;
      let scale = sdl_utils::window_display_scale(window);
      Some(AlloyEvent::PointerUp {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        button,
        x: x / scale,
        y: y / scale,
        modifiers: sdl_utils::mod_state().into(),
      })
    }
    SdlEvent::MouseWheel { which, x, y, direction, mouse_x, mouse_y, .. } => {
      let scale = sdl_utils::window_display_scale(window);
      let flipped = matches!(direction, sdl3::mouse::MouseWheelDirection::Flipped);
      let sign = if flipped { 1.0 } else { -1.0 };
      Some(AlloyEvent::Wheel {
        pointer_id: which as u64,
        pointer_type: PointerType::Mouse,
        x: mouse_x / scale,
        y: mouse_y / scale,
        delta_x: sign * x,
        delta_y: sign * y,
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
    _ => None,
  }
}

fn touch_window_logical_size(window: &sdl3::video::Window) -> (f32, f32) {
  let scale = sdl_utils::window_display_scale(window);
  let (pw, ph) = window.size_in_pixels();
  (pw as f32 / scale, ph as f32 / scale)
}