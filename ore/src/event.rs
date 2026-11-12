//! The platform's events: what the pump (`Platform::wait`) hands upward,
//! translated from SDL's events, the Android touch side channel and the
//! polled platform facts, already normalized (logical points, the W3C key
//! vocabulary, the back triggers collapsed into one request). SDL's own
//! event types never leave ore. Plain data.

use crate::display::{Orientation, Rect};
use crate::input::{InputDevices, Modifiers, PointerType};
use crate::power::PowerInfo;

#[derive(Clone, Debug)]
pub enum PlatformEvent {
  /// The platform is ending this app instance: desktop window close or
  /// SIGTERM, Android onDestroy, iOS applicationWillTerminate (SDL's Quit
  /// and AppTerminating both normalize to it). A command, not a request
  /// (compare Back): the embedder runs the app's quit hook under a deadline
  /// and then ends the process.
  Quit,
  /// The user's back intent: Android's back button/gesture (AC_BACK), or the
  /// desktop chord Ctrl+Shift+Backspace (Cmd+Shift+Backspace on macOS).
  /// Unlike Quit this is a request, not a command: it surfaces to the app
  /// as the preventable `back` event (in-app navigation handles it there),
  /// and only an unprevented dispatch (or an unresponsive engine) falls
  /// through to the default action: exit. A repeat of the trigger yields
  /// nothing, and so does its release. The gamepad "back" button is the
  /// third trigger, read on the pad snapshot path
  /// (`crate::gamepad::Gamepads::take_back_edge`).
  Back,
  /// A link handed to the running app from outside: a custom-scheme URL the
  /// OS routed here (macOS and iOS deliver it through SDL's drop path, an
  /// Android intent through the activity's onNewIntent). The raw string,
  /// untouched: what it means is the app's to decide, and it is untrusted
  /// input. The link a process was started with is not an event but a
  /// launch fact (see the embedder).
  Link {
    link: String,
  },
  WindowFocus,
  WindowBlur,
  /// App/window visibility state: false when backgrounded (Android
  /// DID_ENTER_BACKGROUND) or minimized/hidden on desktop, true on the way
  /// back. State transitions only, never damage (that is Exposed); platforms
  /// may report the same transition through both the app and window paths,
  /// so consumers must tolerate repeats of the same value.
  Visibility {
    visible: bool,
  },
  /// The window surface needs a repaint without any visibility change: SDL
  /// WINDOW_EXPOSED (damage, or a recreated swapchain surface whose contents
  /// are undefined).
  Exposed,
  /// One key transition, already translated to the W3C UI Events vocabulary:
  /// `key` is the logical value ("a", "!", "Enter"), `code` the physical key
  /// ("KeyA", "NumpadEnter"). SDL's keycodes and scancodes never leave ore.
  Key {
    down: bool,
    key: String,
    code: &'static str,
    modifiers: Modifiers,
    repeat: bool,
  },
  /// The window's drawable changed size: its size in physical pixels, with
  /// the display scale and the safe area (logical points) read from the
  /// window when the event was translated.
  Resize {
    pixel_size: (u32, u32),
    display_scale: f32,
    safe_area: Rect,
  },
  /// A pointer moved, in logical points. `rel` is the hardware motion delta
  /// when the device reports one (mouse `xrel`/`yrel`, in the same logical
  /// units as `x`/`y`); None for producers without one (touch), whose
  /// movement is derived from positions instead. Under pointer lock a
  /// mouse's `x`/`y` stay at the lock point and only `rel` moves.
  PointerMove {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    rel: Option<(f32, f32)>,
    modifiers: Modifiers,
  },
  /// A button went down, in logical points. `button` is the web-standard
  /// MouseEvent.button code (0 for a touch).
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
  /// The pointer's other end: the system took the touch away (a system
  /// gesture, a palm rejection, the window losing the gesture). The last
  /// event for its pointer, carrying where the platform last saw it and no
  /// button; it routes like an up, and nothing of the pointer survives it.
  PointerCancel {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  /// delta_x / delta_y use browser convention: positive delta_y means
  /// content should scroll down (wheel rolled toward the user). SDL's
  /// direction=Flipped is normalized away at translation time.
  Wheel {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
    modifiers: Modifiers,
  },
  TextInput {
    text: String,
  },
  /// Orientation of the display the window is on, re-queried from the
  /// window's own display on any display's orientation event (a rotation
  /// elsewhere re-emits an unchanged value, which subscribers dedupe).
  DisplayOrientation {
    orientation: Orientation,
  },
  /// The OS-level dark/light preference changed; re-query it through
  /// `crate::theme`.
  SystemThemeChanged,
  /// Connected input device classes, on keyboard/mouse hotplug (SDL's) and
  /// on a change of the platform-reported hardware keyboard (Android). See
  /// `crate::input::InputDevices`.
  InputDevices(InputDevices),
  /// Display refresh rate in Hz, whenever it changes (e.g. Android 90 <->
  /// 60Hz): on a display mode event, and from a periodic safety net that
  /// catches a change no event reported.
  DisplayRefreshRate {
    hz: f32,
  },
  /// Emitted when the on-screen keyboard visibility or size changes. SDL does
  /// not provide an event for this, so it is detected by polling
  /// SDL_ScreenKeyboardShown and the platform-reported IME inset on every
  /// wait. `height` is the keyboard's overlap with the window in logical
  /// pixels (0 when hidden or unsupported); the window is fullscreen so it
  /// is not resized for the keyboard, and the app uses this to lift its
  /// content.
  KeyboardVisibility {
    shown: bool,
    height: f32,
  },
  /// The power state and the charge left, polled on a fixed period.
  PowerStatus {
    info: PowerInfo,
  },
  /// Camera hotplug. Carries no device id (subscribers re-enumerate). SDL
  /// only delivers these once the camera subsystem is initialized, i.e.
  /// after the first list/open call.
  ///
  /// SDL 3.4.8 hotplug coverage is uneven; both add and remove are passed
  /// on as SDL delivers them:
  ///   - Android: add + remove both work.
  ///   - Linux: add works; remove is broken in both backends -- pipewire never
  ///     calls SDL_CameraDisconnected, v4l2 mis-gates removals on a device class
  ///     that is 0 on remove (a one-line fix, written up in okf/upstream/ and
  ///     not filed upstream yet). SDL's own backend order is taken, v4l2 first
  ///     and pipewire as its fallback.
  ///   - macOS/Windows: no camera hotplug at all (upstream FIXMEs, not wired),
  ///     so neither add nor remove arrives.
  CameraDeviceChange {
    added: bool,
  },
}
