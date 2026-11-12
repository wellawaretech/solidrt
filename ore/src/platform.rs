//! The platform: SDL, initialized once on the host's main thread, with the
//! hints that must precede its subsystems. The host (lattice) builds it
//! before alloy or forge start anything, so the first `SDL_Init` runs on
//! the main thread. `Platform` is not `Send`: it and everything opened
//! from it stay on that thread. A windowless host builds one and never
//! opens a window; with the `window` feature, `open_window` opens the one
//! window a run has (`crate::window`).

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
/// hints that must precede its subsystems. !Send. A windowless host builds
/// one and never opens a window.
pub struct Platform {
  sdl: sdl3::Sdl,
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

    Ok(Platform { sdl })
  }

  /// Open the run's window (see `crate::window::Display` for the kinds and
  /// the headless chain). Main thread only.
  #[cfg(feature = "window")]
  pub fn open_window(&self, config: crate::window::WindowConfig<'_>) -> Result<crate::window::Window, String> {
    crate::window::open(&self.sdl, config)
  }

  /// Transitional, step 1 only: the SDL handle alloy's pump, event watch,
  /// event sender and gamepads still take. Removed in step 2 of
  /// okf/plans/ore-pump-and-window.md.
  #[doc(hidden)]
  pub fn sdl(&self) -> &sdl3::Sdl {
    &self.sdl
  }
}
