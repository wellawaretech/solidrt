use crate::renderer::{ISize, Rect};

use ore::cursor::{Cursor, CursorFrame};
use ore::display::Orientation;
use ore::event::PlatformEvent;
use ore::gamepad::GamepadState;
use ore::input::{Modifiers, PointerType};
use ore::lifecycle::SuspendHold;
use ore::power::PowerInfo;
use ore::text_input::TextInputOptions;
use ore::theme::SystemTheme;
use ore::window::Window;

pub enum AlloyCommand {
  EmitInitEvents,
  // Register the demand gate's frame-request latch (rendertree
  // Platform::frame_request_handle) so the platform loop can self-schedule
  // the repaints its surface lifecycle requires (expose, resize settling,
  // return to visibility; see liveness.rs). Send once at startup. Without
  // it, lifecycle rebinds still happen but the resume/expose repaint
  // degrades to the raster thread's present-failure fallback.
  SetFrameRequestLatch(std::sync::Arc<std::sync::atomic::AtomicBool>),
  // Whether the UI thread is in the middle of JS work (the embedder's
  // engine sets it around each closure it runs). The other half of the
  // idle-tick gate: a frame being built will present and bring its own
  // frame signal, so a Tick emitted meanwhile is a second signal for one
  // present - a JS-bound app under vsync pacing ran two frames per present
  // because of it (okf/backlog/vsync-locked-js-bound-double-signal.md).
  SetUiBusyFlag(std::sync::Arc<std::sync::atomic::AtomicBool>),
  SetTitle(String),
  // Bring the window to the front: a link handed over by a second instance
  // (ore::link's hand-off) should show the screen it opened, not stay behind
  // whatever started that instance.
  RaiseWindow,
  // Window icon from straight-alpha RGBA8 pixels (width * height * 4 bytes).
  // Platforms without window icons (macOS) ignore it.
  SetIcon { width: u32, height: u32, rgba: Vec<u8> },
  SetFullscreen(bool),
  // Frame-release policy (see vsync::FramePacing). The embedder derives it
  // from the input-modality facts (InputDevices) and re-sends on hotplug;
  // the loop applies it immediately, releasing any vsync-deferred presents
  // when leaving VsyncLocked.
  SetFramePacing(crate::vsync::FramePacing),
  // Cadence-hold policy in force (see cadence::CadenceHold): whether frame
  // intervals below the refresh rate are held to a whole number of
  // refreshes. The embedder chooses it (the layers of default, app choice
  // and override are its own) and sends it when it changes; each send
  // restarts the controller, unheld under Auto. The reset ends a learned
  // hold and keeps the policy. Each change answers with
  // AlloyEvent::CadenceHold.
  SetCadenceHold(crate::cadence::CadenceHold),
  ResetCadenceHold,
  // The mouse cursor to show (see Cursor). The loop keeps every cursor it
  // has created alive for as long as SDL may show it (cursor.rs).
  SetCursor(Cursor),
  // Register an image cursor under the embedder's id for Cursor::Custom(id):
  // one frame for a static cursor, several for an animated one (SDL plays
  // them; a platform without animated cursors shows the first frame). The
  // hotspot is in the 1x image's pixels.
  CreateCursor { id: u64, frames: Vec<CursorFrame>, hot_x: u32, hot_y: u32 },
  // Destroy a registered image cursor; if it is showing, SDL reverts to the
  // default cursor.
  DropCursor(u64),
  // A synthetic gamepad (the dev tools' `input` query): connect, hold a
  // state, disconnect. Applied by the pad owner on the loop's thread, which
  // is where SDL's pads live, so a synthetic pad takes a slot and reaches
  // the snapshot exactly as a physical one does (see gamepad::Gamepads).
  Gamepad(ore::gamepad::GamepadCommand),
  // Relative mouse mode (pointer lock): SDL hides the cursor, confines it
  // to the window, and mouse motion reports hardware deltas while absolute
  // positions freeze. The loop answers with AlloyEvent::PointerLock carrying
  // the applied state.
  SetPointerLock(bool),
  SetTextInputActive(bool, TextInputOptions),
  // A system file dialog (dialog.rs): shown from the loop thread with the
  // app window as its parent, answered through the responder once the user
  // is done, from whatever thread the platform answers on. A cancel is an
  // empty answer. The stepped loop has no surface to show one on and
  // answers unavailable at once. Whether a dialog shows at all is the
  // session's call (dialog::Dialogs): a test's answer takes its place.
  ShowDialog { request: ore::dialog::DialogRequest, respond: ore::dialog::DialogResponder },
  // Leave the app at the OS level without ending it: SDL's minimize, which
  // on Android routes to Activity.moveTaskToBack - what the system itself
  // does for back at the root since Android 12 (the process stays, the
  // suspend hook has run through the event watch on the way) - and on
  // desktop minimizes the window.
  Background,
  // The headless window's logical size and display scale from now on (see
  // stepped::HeadlessWindow): the window framebuffer becomes `width *
  // scale` by `height * scale` pixels and the app is told the resize, with
  // that scale. Stepped loop only; the interactive loop's window is sized
  // by the user and ignores it. The products must be whole pixels (the
  // sender validates). The next EmitInitEvents restores the run's window.
  SetHeadlessWindow { width: u32, height: u32, scale: f32 },
}

/// An event on the channel to the embedder, with when it happened: the
/// platform's time for an input event (see `EventSender::send_at`), the
/// send for everything else.
pub struct Arrival {
  pub event: AlloyEvent,
  pub at: std::time::Instant,
}

/// The sending half of the event channel to the embedder.
#[derive(Clone)]
pub struct EventSender(std::sync::mpsc::Sender<Arrival>);

impl EventSender {
  pub(crate) fn new(tx: std::sync::mpsc::Sender<Arrival>) -> Self {
    EventSender(tx)
  }

  /// Send an event that happens now. Err when the receiver is gone.
  pub fn send(&self, event: AlloyEvent) -> Result<(), ()> {
    self.send_at(event, std::time::Instant::now())
  }

  /// Send an event that happened at `at`: an input event with a time of
  /// its own (the platform's event time, a touch sample's time), which is
  /// what its `timeStamp` comes from however long it waits in a queue.
  pub fn send_at(&self, event: AlloyEvent, at: std::time::Instant) -> Result<(), ()> {
    self.0.send(Arrival { event, at }).map_err(|_| ())
  }
}

#[derive(Clone)]
pub enum AlloyEvent {
  // The GL context was lost and the engine rebuilt every GPU resource on a
  // fresh one (okf/done/gpu-context-loss.md): `handles` are the new
  // renderer adoptions per texture id for the UI thread's registry (its old
  // ones belong to the dead context), `lost` the ids whose content the app
  // alone can re-supply, `guilty` whether the driver attributed the reset to
  // this context's own work (a pass past its hang timeout; the app's load
  // to lower, since the engine stops recovering after a few such resets in
  // a row), `losses` how many losses this process has had. The UI thread
  // installs the handles, drops its paint caches, resyncs the raster thread,
  // and tells the app.
  ContextRestored {
    handles: Vec<(u64, crate::renderer::Texture)>,
    lost: crate::gpu::LostContent,
    guilty: bool,
    losses: u64,
  },
  // The platform is ending this app instance: desktop window close or
  // SIGTERM, Android onDestroy, iOS applicationWillTerminate. A command, not
  // a request (compare Back): the embedder runs the app's quit hook under a
  // deadline and then ends the process.
  Quit,
  // The app is being suspended and may be killed without further notice:
  // WILL_ENTER_BACKGROUND, delivered at push time by the event watch (the
  // queued copy is not translated, so a resume never replays it). Carries the
  // hold that keeps the platform running while the app's suspend handlers
  // execute; see SuspendHold. Never fires on desktop: a minimized desktop app
  // is not killed, so there is nothing to report.
  Suspend {
    hold: SuspendHold,
  },
  // The user's back intent: Android's back button/gesture (AC_BACK), or the
  // desktop chord (translated by ore, see ore::event::PlatformEvent::Back).
  // Unlike Quit this is a request, not a command: it surfaces to the app as
  // the preventable `back` event (in-app navigation handles it there), and
  // only an unprevented dispatch (or an unresponsive engine) falls through
  // to the default action: exit.
  Back,
  // A link handed to the running app from outside: a custom-scheme URL the
  // OS routed here (macOS and iOS deliver it through SDL's drop path, an
  // Android intent through the activity's onNewIntent; translated by ore,
  // see ore::event::PlatformEvent::Link), or one a dev tool injected. The
  // raw string, untouched: what it means is the app's to decide, and it is
  // untrusted input. The link a process was started with is not an event
  // but a launch fact (see the embedder).
  Link {
    link: String,
  },
  WindowFocus,
  WindowBlur,
  // App/window visibility state: false when backgrounded (Android
  // DID_ENTER_BACKGROUND) or minimized/hidden on desktop, true on the way
  // back. State transitions only, never damage (that is Exposed); platforms
  // may report the same transition through both the app and window paths, so
  // consumers must tolerate repeats of the same value.
  Visibility {
    visible: bool,
  },
  // Relative mouse mode (pointer lock) state as applied by the loop: the
  // answer to SetPointerLock, and the hook for platform-initiated drops if
  // one ever surfaces. Sticky window fact downstream.
  PointerLock {
    locked: bool,
  },
  // The cadence-hold policy in force and the hold (refreshes per frame, 1 =
  // none), whenever either changes; replayed by EmitInitEvents. Sticky
  // fact downstream.
  CadenceHold {
    policy: crate::cadence::CadenceHold,
    hold: u32,
  },
  // The window surface needs a repaint without any visibility change: SDL
  // WINDOW_EXPOSED (damage, or a recreated swapchain surface whose contents
  // are undefined). Repaint trigger only; never surfaces to JS.
  Exposed,
  // One key transition, already translated to the W3C UI Events vocabulary
  // by ore (ore::event::PlatformEvent::Key): `key` is the logical value ("a",
  // "!", "Enter"), `code` the physical key ("KeyA", "NumpadEnter"). SDL's
  // keycodes and scancodes never leave ore.
  Key {
    down: bool,
    key: String,
    code: &'static str,
    modifiers: Modifiers,
    repeat: bool,
  },
  Resize {
    size: ISize,
    safe_area: Rect,
    display_scale: f32,
  },
  // A frame was presented. `refreshes` is how many display refreshes passed
  // since the previous frame signal (FrameRendered or Tick), as counted by
  // the main loop (see present.rs): one at full rate, more when the frame
  // took longer than a period. It is the fact the app timeline advances by;
  // what a count worth a suspension means is the embedder's policy.
  // `present_at` is when the frame this signal asks for is expected to
  // reach the screen: the signal's reference instant plus the cadence
  // hold's periods (the virtual frame time headless), the deadline a
  // frame's video content is latched against. `reference` is the instant
  // the refreshes were counted from (the vsync under vsync-locked pacing, a
  // swap's return otherwise), the frame time touch is resampled to (see
  // resample.rs). `grid` is where the count places the signal: the
  // counter's anchor plus the counted refreshes, which advances by exactly
  // `refreshes` periods from signal to signal, so a timeline that advances
  // by the count can place an instant between signals against it.
  FrameRendered {
    frame: u64,
    fps: u32,
    refreshes: u32,
    present_at: std::time::Instant,
    reference: std::time::Instant,
    grid: std::time::Instant,
  },
  // Idle tick: emitted at the refresh cadence when no display list has arrived
  // for a full refresh period, so the UI thread keeps running its per-frame
  // logic (timers, signal flush, camera pump) while nothing is presented.
  // `frame` is the present counter, i.e. one past the last FrameRendered's
  // frame: the index the next present will get. The rest as above.
  Tick {
    frame: u64,
    fps: u32,
    refreshes: u32,
    present_at: std::time::Instant,
    reference: std::time::Instant,
    grid: std::time::Instant,
  },
  // Display refresh rate in Hz. Its own event (independent of frames): emitted
  // at startup and whenever the rate changes (e.g. Android 90 <-> 60Hz).
  DisplayRefreshRate {
    hz: f32,
  },
  // A producer's moves do not travel: the pump consumes them into the
  // resampler at translation (see resample.rs), and every other producer of
  // pointer events must do the same at its send site. `rel` is the hardware
  // motion delta when the device reports one (mouse `xrel`/`yrel`, in the
  // same logical units as `x`/`y`); None for producers without one (touch,
  // synthetic input), whose movement is derived from positions instead.
  // The one move that does travel is the resampler's own: the flush ahead
  // of an up, with `rel` the movement since the last dispatched move.
  PointerMove {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    rel: Option<(f32, f32)>,
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
  // The pointer's other end: the system took the touch away (a system
  // gesture, a palm rejection, the window losing the gesture). The last
  // event for its pointer, carrying where the platform last saw it and no
  // button; it routes like an up, and nothing of the pointer survives it.
  PointerCancel {
    pointer_id: u64,
    pointer_type: PointerType,
    x: f32,
    y: f32,
    modifiers: Modifiers,
  },
  TextInput {
    text: String,
  },
  PowerStatus {
    info: PowerInfo,
  },
  // Emitted when the on-screen keyboard visibility or size changes. SDL does
  // not provide an event for this, so it is detected by polling
  // SDL_ScreenKeyboardShown and the platform-reported IME inset each loop
  // iteration. `height` is the keyboard's overlap with the window in logical
  // pixels (0 when hidden or unsupported); the window is fullscreen so it is
  // not resized for the keyboard, and the app uses this to lift its content.
  KeyboardVisibility {
    shown: bool,
    height: f32,
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
  // OS-level dark/light preference. Emitted at init and whenever the OS theme
  // changes; Unknown on platforms that do not report one.
  SystemTheme {
    theme: SystemTheme,
  },
  // Connected input device classes. Presence, not traffic: a connected mouse
  // that never moves still reports true. Emitted at init and on keyboard/mouse
  // hotplug; SDL has no touch hotplug events, so touch is re-queried on those
  // same occasions.
  InputDevices {
    keyboard: bool,
    mouse: bool,
    touch: bool,
    screen_keyboard: bool,
  },
  // Orientation of the display the window is on. Emitted at init and on
  // rotation.
  DisplayOrientation {
    orientation: Orientation,
  },
  // Full connected-gamepad state, emitted whenever any pad connects,
  // disconnects, or changes a button/axis (coalesced to at most one per main
  // loop iteration), plus once at init. Slots are stable for a pad's whole
  // connection; a disconnect leaves a None hole that the next connect reuses.
  Gamepads {
    pads: Vec<Option<GamepadState>>,
  },
  // Camera hotplug. Carries no device id (subscribers re-enumerate via
  // Context::list_cameras()). SDL only delivers these once the camera
  // subsystem is initialized, i.e. after the first list/open call.
  //
  // SDL 3.4.8 hotplug coverage is uneven; this arm handles both add and remove
  // and simply fires whatever SDL delivers:
  //   - Android: add + remove both work.
  //   - Linux: add works; remove is broken in both backends -- pipewire never
  //     calls SDL_CameraDisconnected, v4l2 mis-gates removals on a device class
  //     that is 0 on remove (a one-line fix, written up in okf/upstream/ and
  //     not filed upstream yet). We take SDL's own backend order, v4l2 first
  //     and pipewire as its fallback: see camera_subsystem_init.
  //   - macOS/Windows: no camera hotplug at all (upstream FIXMEs, not wired),
  //     so neither add nor remove arrives.
  CameraDeviceChange {
    added: bool,
  },
}

pub(crate) fn current_system_theme_event() -> AlloyEvent {
  AlloyEvent::SystemTheme { theme: ore::theme::system_theme() }
}

pub(crate) fn current_input_devices_event() -> AlloyEvent {
  input_devices_event(ore::window::input_devices())
}

fn input_devices_event(d: ore::input::InputDevices) -> AlloyEvent {
  AlloyEvent::InputDevices { keyboard: d.keyboard, mouse: d.mouse, touch: d.touch, screen_keyboard: d.screen_keyboard }
}

pub(crate) fn current_orientation_event(window: &Window) -> AlloyEvent {
  AlloyEvent::DisplayOrientation { orientation: window.orientation() }
}

pub(crate) fn current_resize_event(window: &Window) -> AlloyEvent {
  resize_event(window.size_in_pixels(), window.display_scale(), window.safe_area())
}

// The resize for a drawable of `pixel_size` physical pixels at `scale`: the
// logical size is the pixel size over the scale, the safe area is already in
// logical points. One place, so the event ore's pump reports and the one
// read from the window here cannot disagree.
fn resize_event((w, h): (u32, u32), scale: f32, r: ore::display::Rect) -> AlloyEvent {
  AlloyEvent::Resize {
    size: ISize::new((w as f32 / scale) as i64, (h as f32 / scale) as i64),
    safe_area: Rect::new(
      crate::renderer::Point::new(r.x as f32, r.y as f32),
      crate::renderer::Size::new(r.width as f32, r.height as f32),
    ),
    display_scale: scale,
  }
}

// The headless mode's resize event: the headless window's logical size and
// its scale (see stepped::HeadlessWindow), nothing of the host display. A
// headless window has no insets, so the safe area is the full window.
pub(crate) fn headless_resize_event(window: &crate::stepped::HeadlessWindow) -> AlloyEvent {
  let (w, h) = (window.width, window.height);
  AlloyEvent::Resize {
    size: ISize::new(w as i64, h as i64),
    safe_area: Rect::new(crate::renderer::Point::new(0.0, 0.0), crate::renderer::Size::new(w as f32, h as f32)),
    display_scale: window.scale,
  }
}

// The headless mode's init bundle: what the stepped loop answers
// EmitInitEvents with, for every engine a headless host builds, pinned to
// host-independent values. The refresh rate is the rate the host steps at,
// so a frame callback's `rate` matches the virtual clock's step. The resize
// goes last: the app bootstraps its first frame on the first resize it sees,
// so everything sent before it is in place when that frame is built. Theme,
// input devices, orientation and pointer lock are not sent: a headless
// window has none, and the embedder's defaults cover their absence.
pub(crate) fn headless_init_events(window: &crate::stepped::HeadlessWindow, fps: u32) -> [AlloyEvent; 2] {
  [AlloyEvent::DisplayRefreshRate { hz: fps as f32 }, headless_resize_event(window)]
}

/// The user-originated input a dev-tool mute drops (see the mute site in
/// app.rs): presses, moves, wheel, text and back. Releases are left out on
/// purpose - a button or key held when the mute began must still let go.
pub(crate) fn is_muted_input(event: &AlloyEvent) -> bool {
  match event {
    AlloyEvent::PointerMove { .. }
    | AlloyEvent::PointerDown { .. }
    | AlloyEvent::Wheel { .. }
    | AlloyEvent::TextInput { .. }
    | AlloyEvent::Back => true,
    AlloyEvent::Key { down, .. } => *down,
    _ => false,
  }
}

// The platform's events, as ore's pump translated them, on the embedder's
// channel: variant to variant, plain data carried over.
impl From<PlatformEvent> for AlloyEvent {
  fn from(e: PlatformEvent) -> AlloyEvent {
    match e {
      PlatformEvent::Quit => AlloyEvent::Quit,
      PlatformEvent::Back => AlloyEvent::Back,
      PlatformEvent::Link { link } => AlloyEvent::Link { link },
      PlatformEvent::WindowFocus => AlloyEvent::WindowFocus,
      PlatformEvent::WindowBlur => AlloyEvent::WindowBlur,
      PlatformEvent::Visibility { visible } => AlloyEvent::Visibility { visible },
      PlatformEvent::Exposed => AlloyEvent::Exposed,
      PlatformEvent::Key { down, key, code, modifiers, repeat } => {
        AlloyEvent::Key { down, key, code, modifiers, repeat }
      }
      PlatformEvent::Resize { pixel_size, display_scale, safe_area } => {
        resize_event(pixel_size, display_scale, safe_area)
      }
      PlatformEvent::PointerMove { pointer_id, pointer_type, x, y, rel, modifiers } => {
        AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, rel, modifiers }
      }
      PlatformEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
        AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers }
      }
      PlatformEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
        AlloyEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers }
      }
      PlatformEvent::PointerCancel { pointer_id, pointer_type, x, y, modifiers } => {
        AlloyEvent::PointerCancel { pointer_id, pointer_type, x, y, modifiers }
      }
      PlatformEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
        AlloyEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers }
      }
      PlatformEvent::TextInput { text } => AlloyEvent::TextInput { text },
      PlatformEvent::DisplayOrientation { orientation } => AlloyEvent::DisplayOrientation { orientation },
      PlatformEvent::SystemThemeChanged => current_system_theme_event(),
      PlatformEvent::InputDevices(d) => input_devices_event(d),
      PlatformEvent::DisplayRefreshRate { hz } => AlloyEvent::DisplayRefreshRate { hz },
      PlatformEvent::KeyboardVisibility { shown, height } => AlloyEvent::KeyboardVisibility { shown, height },
      PlatformEvent::PowerStatus { info } => AlloyEvent::PowerStatus { info },
      PlatformEvent::CameraDeviceChange { added } => AlloyEvent::CameraDeviceChange { added },
    }
  }
}
