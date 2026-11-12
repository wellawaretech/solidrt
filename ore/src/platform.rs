//! The platform: SDL, initialized once on the host's main thread, with the
//! hints that must precede its subsystems, and the event pump. The host
//! (lattice) builds it before alloy or forge start anything, so the first
//! `SDL_Init` runs on the main thread. `Platform` is not `Send`: it and
//! everything opened from it stay on that thread. A windowless host builds
//! one and never opens a window; with the `window` feature, `open_window`
//! opens the one window a run has (`crate::window`).
//!
//! The pump is a plain function: `wait` blocks on SDL's queue until an
//! event, a wake (`Waker`, from any thread) or the timeout, and returns the
//! drained events translated and timestamped (`crate::event`). The
//! lifecycle transitions that must not wait for the queue come through
//! `on_lifecycle`, synchronously on the pushing thread. The gamepads open
//! on first use (`gamepads`) and are fed by the pump from then on.

#[cfg(feature = "window")]
mod keymap;
#[cfg(feature = "window")]
mod translate;
#[cfg(feature = "window")]
mod watch;

use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use crate::gamepad::Gamepads;
use crate::lifecycle::{Lifecycle, SuspendHold};

/// How the platform starts.
pub struct PlatformConfig<'a> {
  /// The Wayland app id (X11 class) every window carries; SDL_APP_ID.
  pub app_id: &'a str,
  /// A headless run: no display and no sound (the offscreen video driver
  /// is asked for at open_window; the dummy audio driver is set here, and
  /// SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY so a hidden fallback window is 1:1).
  pub headless: bool,
}

/// The platform, initialized once on the host's main thread: SDL and the
/// hints that must precede its subsystems, and the event pump. !Send. A
/// windowless host builds one and never opens a window.
pub struct Platform {
  // Field order is drop order: the pads and the pump go before the event
  // subsystem handle, and SDL itself last.
  /// The polled platform facts (keyboard, power, refresh rate); made on
  /// the first `wait`, which is the first time the window is at hand.
  #[cfg(feature = "window")]
  watch: Option<watch::Watch>,
  /// The pointer-lock coordinate freeze, followed at every `wait`.
  #[cfg(feature = "window")]
  freeze: translate::PointerFreeze,
  /// None until the first `gamepads` call; then the table, or None for
  /// good where a subsystem was unavailable (the open is not retried).
  gamepads: Option<Option<Gamepads>>,
  /// The one event pump SDL allows, which `wait` blocks on. Taken at init
  /// either way, so a windowless host's pump lands here unchanged.
  #[cfg_attr(not(feature = "window"), allow(dead_code))]
  event_pump: sdl3::EventPump,
  /// Held so the event subsystem (and the wake's registered event type)
  /// stays up for the platform's life.
  _events: sdl3::EventSubsystem,
  /// Wakes a blocked `wait` from any thread (see `Waker`).
  waker: Waker,
  /// SDL's tick base, which its event timestamps count from.
  #[cfg_attr(not(feature = "window"), allow(dead_code))]
  epoch: Instant,
  sdl: sdl3::Sdl,
}

/// Payload-less user event a `Waker` pushes onto the SDL queue, so a
/// blocked `wait` returns immediately instead of at its timeout. It carries
/// nothing and translates to nothing: what it announces (a presented frame,
/// a touch sample) travels over the waker's caller's own channel, which the
/// woken loop drains.
struct Wake;

/// Wakes the pump: pushes a payload-less event onto SDL's queue, so a
/// `wait` blocked on it returns at once instead of at its timeout. Any
/// thread. The waker carries nothing; whatever the wake is for (a
/// presented frame, a vsync, a touch sample) travels over the caller's own
/// channel, which the woken loop drains: send first, then wake.
#[derive(Clone)]
pub struct Waker(Arc<sdl3::event::EventSender>);

impl Waker {
  /// Wake the pump. A failed push (the event subsystem is gone) is
  /// ignored: nothing is waiting any more.
  pub fn wake(&self) {
    self.0.push_custom_event(Wake).ok();
  }
}

impl Platform {
  /// Set the startup hints and initialize SDL. Main thread only, once. An
  /// SDL that does not start is an `Err`; the caller decides.
  pub fn init(config: PlatformConfig<'_>) -> Result<Platform, String> {
    // Must precede the video subsystem: SDL reads it when it creates a window.
    sdl3::hint::set("SDL_APP_ID", config.app_id);

    // Keep touch and mouse streams separate. Without this, SDL synthesizes
    // mouse events from touches (and vice versa) using SDL_TOUCH_MOUSEID
    // as `which`, which would arrive on the mouse arm of the event
    // translation and be misclassified as PointerType::Mouse with a
    // sentinel pointer_id.
    sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
    sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
    // Image cursors follow the display scale where SDL does that itself
    // (Windows; macOS and Wayland always do). Must precede any cursor creation.
    sdl3::hint::set("SDL_MOUSE_DPI_SCALE_CURSORS", "1");
    if config.headless {
      // For the headless fallback to a hidden window (window::open), force
      // 1:1 pixel mapping so the hidden window is exactly the requested
      // size in physical pixels regardless of display scale.
      sdl3::hint::set("SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY", "1");
      // No sound either: SDL's dummy audio driver takes the mix at the
      // device rate and drops it, so the mixer runs (positions advance,
      // playbacks end) on a box without a sound card, and a test run plays
      // nothing through the speakers of one that has. Read when the audio
      // subsystem is first initialized (alloy's audio/device.rs), so it is
      // set here, before SDL starts anything.
      sdl3::hint::set("SDL_AUDIO_DRIVER", "dummy");
    }
    // On Android SDL's event wait (SDL_WaitEventTimeout) never blocks with
    // the poll sentinel enabled: every pump pushes a sentinel event, every
    // pushed event sends the Android lifecycle WAKE, and that WAKE is what
    // the wait's semaphore blocks on, so the wait spins through its whole
    // timeout (SDL 3.4.10 through 3.4.18 and main, measured 100% CPU on an idle app; see
    // okf/upstream/sdl-android-wait-event-poll-sentinel-spin.md). Without
    // the sentinel the pump pushes nothing of its own and the wait blocks.
    // The sentinel only bounds a poll drain against an event flood, and the
    // event loop drains one queue per iteration.
    if cfg!(target_os = "android") {
      sdl3::hint::set("SDL_POLL_SENTINEL", "0");
    }

    let sdl = sdl3::init().map_err(|e| format!("SDL init: {e}"))?;
    // On Android, publish the JavaVM and the activity in ndk-context so
    // JNI-using deps (iroh's network monitoring via flux:p2p) can reach the
    // Android context. A no-op when the host already did.
    #[cfg(target_os = "android")]
    crate::android::init_context();

    let events = sdl.event().map_err(|e| format!("SDL event subsystem: {e}"))?;
    events.register_custom_event::<Wake>().map_err(|e| format!("SDL wake event: {e}"))?;
    let waker = Waker(Arc::new(events.event_sender()));
    // Android's finger samples arrive beside SDL's queue (see touch.rs); a
    // sample queued while the pump sleeps wakes it.
    #[cfg(target_os = "android")]
    crate::touch::set_waker(waker.clone());
    // Android's held redraws arrive the same way (see redraw.rs); set on
    // every platform, where nothing asks, so the loop needs no cfg.
    crate::redraw::set_waker(waker.clone());
    let event_pump = sdl.event_pump().map_err(|e| format!("SDL event pump: {e}"))?;
    let epoch = crate::time::ticks_epoch();

    Ok(Platform {
      #[cfg(feature = "window")]
      watch: None,
      #[cfg(feature = "window")]
      freeze: translate::PointerFreeze::default(),
      gamepads: None,
      event_pump,
      _events: events,
      waker,
      epoch,
      sdl,
    })
  }

  /// A waker onto this platform's pump, for any thread.
  pub fn waker(&self) -> Waker {
    self.waker.clone()
  }

  /// Call `f` on every lifecycle transition (see `Lifecycle`), synchronously
  /// on the thread pushing SDL's app event, before the pump sees it. The
  /// suspend hold is taken inside the platform callback, ahead of `f`. `f`
  /// must be quick and must not call back into the pump. The watch lives
  /// for the process: it is never removed, and `f` is never dropped.
  ///
  /// Through `SDL_AddEventWatch` directly: the sdl3 crate's `add_event_watch`
  /// removes its watch when the returned guard drops, which would have the
  /// platform hold a guard per call, and it rebuilds a crate `Event` for
  /// every event pushed on any thread, where this watch needs only the raw
  /// type.
  pub fn on_lifecycle(&self, f: impl Fn(Lifecycle) + Send + Sync + 'static) {
    let callback: Box<Box<dyn Fn(Lifecycle) + Send + Sync>> = Box::new(Box::new(f));
    let userdata = Box::into_raw(callback).cast::<c_void>();
    // SAFETY: `userdata` is a leaked Box that is never freed while the watch
    // is installed (the watch is never removed, so it lives for the
    // process), and `lifecycle_watch` reads it as exactly that type.
    let added = unsafe { sdl3::sys::events::SDL_AddEventWatch(Some(lifecycle_watch), userdata) };
    if !added {
      log::warn!("[ore] lifecycle watch not installed: {}", crate::sdl_error());
      // SAFETY: SDL refused the watch, so nothing else holds the pointer;
      // reclaimed once, as the Box it was made from.
      drop(unsafe { Box::from_raw(userdata.cast::<Box<dyn Fn(Lifecycle) + Send + Sync>>()) });
    }
  }

  /// The gamepads: SDL's gamepad and joystick subsystems open on the first
  /// call (pads already plugged in surface through the Added events SDL
  /// emits on subsystem init, so call this before the first `wait` that
  /// should see them), and the pump feeds the table from then on. None
  /// where SDL has no gamepad support on this platform (warned once, not
  /// retried).
  pub fn gamepads(&mut self) -> Option<&mut Gamepads> {
    let sdl = &self.sdl;
    self.gamepads.get_or_insert_with(|| Gamepads::new(sdl)).as_mut()
  }

  /// Open the run's window (see `crate::window::Display` for the kinds and
  /// the headless chain). Main thread only.
  #[cfg(feature = "window")]
  pub fn open_window(&self, config: crate::window::WindowConfig<'_>) -> Result<crate::window::Window, String> {
    crate::window::open(&self.sdl, config)
  }

  /// The pump: wait on SDL's queue until an event, a wake or `timeout`
  /// (zero: no wait, only the drain), then drain it, each event translated
  /// and with the instant it happened. Main thread only. In order:
  ///
  /// 1. SDL's events as drained. A display mode change also re-queries the
  ///    refresh rate (a DisplayRefreshRate on change, stamped now); the
  ///    gamepads, once opened, see every event; the rest translate to at
  ///    most one event each, at SDL's event time. While the pointer is
  ///    locked, a mouse event carries the lock point (SDL does not freeze
  ///    x/y in relative mode; see the freeze in translate.rs), read from
  ///    the window's lock at the start of the call.
  /// 2. On Android, the touch side channel's samples, each at its own time.
  /// 3. The polled platform facts' transitions (screen keyboard, hardware
  ///    keyboard, power, the refresh rate safety net), stamped now.
  ///
  /// Behind the `window` feature because translation reads the window's
  /// scale, size and safe area live, per event; the windowless host lifts
  /// the window-free part of the translation out when it is built.
  #[cfg(feature = "window")]
  pub fn wait(
    &mut self,
    timeout: std::time::Duration,
    window: &crate::window::Window,
  ) -> Vec<(crate::event::PlatformEvent, Instant)> {
    let watch = self.watch.get_or_insert_with(|| watch::Watch::new(window));
    self.freeze.follow(window.pointer_locked());
    let first_event = if timeout.is_zero() {
      None
    } else {
      // SDL waits in whole milliseconds; round up so a sub-millisecond
      // remainder does not degrade into a spin on zero-length waits.
      let ms = timeout.as_millis() as u32 + (!timeout.as_micros().is_multiple_of(1000)) as u32;
      self.event_pump.wait_event_timeout_ms(ms)
    };

    let mut events = Vec::new();
    for sdl_event in first_event.into_iter().chain(self.event_pump.poll_iter()) {
      if let sdl3::event::Event::Display { display_event, .. } = &sdl_event {
        use sdl3::event::DisplayEvent;
        if matches!(display_event, DisplayEvent::CurrentModeChanged | DisplayEvent::DesktopModeChanged) {
          if let Some(e) = watch.check_refresh(window) {
            events.push((e, Instant::now()));
          }
        }
      }
      if let Some(Some(g)) = self.gamepads.as_mut() {
        g.handle_event(&sdl_event);
      }
      let at = crate::time::event_instant(self.epoch, sdl_event.get_timestamp());
      if let Some(mut e) = translate::translate(sdl_event, window) {
        self.freeze.apply(&mut e);
        events.push((e, at));
      }
    }
    // Android's finger samples arrive beside SDL's queue, each with its
    // own time (see touch.rs).
    #[cfg(target_os = "android")]
    for sample in crate::touch::drain() {
      events.push((translate::translate_touch(&sample, window), sample.at));
    }
    let now = Instant::now();
    events.extend(watch.poll(window).into_iter().map(|e| (e, now)));
    events
  }
}

// The lifecycle watch's SDL callback (see `Platform::on_lifecycle`): runs on
// whatever thread pushes an event, for every event pushed, so it matches the
// raw type and touches nothing else. Never unwinds into SDL: a panic in the
// caller's closure is caught and logged.
unsafe extern "C" fn lifecycle_watch(userdata: *mut c_void, event: *mut sdl3::sys::events::SDL_Event) -> bool {
  use sdl3::sys::events::{
    SDL_EVENT_DID_ENTER_BACKGROUND, SDL_EVENT_DID_ENTER_FOREGROUND, SDL_EVENT_WILL_ENTER_BACKGROUND,
  };
  // SAFETY: SDL passes the event it is pushing, valid for the call; every
  // variant of the union starts with the type word.
  let event_type = unsafe { (*event).r#type };
  let lifecycle = if event_type == SDL_EVENT_WILL_ENTER_BACKGROUND.0 {
    Lifecycle::WillBackground(SuspendHold::take())
  } else if event_type == SDL_EVENT_DID_ENTER_BACKGROUND.0 {
    Lifecycle::DidBackground
  } else if event_type == SDL_EVENT_DID_ENTER_FOREGROUND.0 {
    Lifecycle::DidForeground
  } else {
    return true;
  };
  // SAFETY: `userdata` is the Box leaked in `on_lifecycle`, never freed
  // while the watch is installed, and only read here (the closure is Sync).
  let f = unsafe { &*userdata.cast::<Box<dyn Fn(Lifecycle) + Send + Sync>>() };
  if std::panic::catch_unwind(AssertUnwindSafe(|| f(lifecycle))).is_err() {
    log::error!("[ore] the lifecycle callback panicked");
  }
  // An event watch's answer is ignored by SDL; the event is kept.
  true
}

// The link in a drop payload, if it is one: a URI scheme per RFC 3986
// (a letter, then letters, digits, `+`, `-` or `.`, then `:`) at least two
// characters long, so a Windows drive path (`C:\...`) is not mistaken for a
// scheme. Paths (`/...`) and bare names have no scheme and return None.
#[cfg_attr(not(feature = "window"), allow(dead_code))]
pub(crate) fn link_from_drop(payload: &str) -> Option<String> {
  let colon = payload.find(':')?;
  let scheme = &payload[..colon];
  let mut chars = scheme.chars();
  let first_is_letter = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
  let rest_valid = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
  if scheme.len() >= 2 && first_is_letter && rest_valid {
    Some(payload.to_string())
  } else {
    None
  }
}
