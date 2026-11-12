---
title: Move SDL out of alloy - the pump, the window, the GL surface and the devices into ore
description: The biggest piece of okf/design/platform-layering.md still in alloy, widened so the end state is compiler-checked - the platform vocabulary types first, then SDL init and hints, the window and the GL surface with its loss recovery, the event pump and its translation, the lifecycle watch, gamepads, the Android touch, vsync and view glue, the dialogs, and the audio, microphone and camera devices move into ore behind a `Platform` (a plain-function pump, a waker, a lifecycle hook) and an owned `Window` (commands, cursors, text input, a GL surface), until alloy keeps only the frame loop, the pacing facts, the stepped loop and the mixer and drops its `sdl3` dependency. Six steps, each a commit that builds; the mixer to forge, forge's OS code and the windowless host stay their own items.
created: 2026-11-12
completed: 2026-11-13
---

# Move SDL out of alloy: the pump, the window, the GL surface and the devices into ore

okf/design/platform-layering.md decided this and called the pump and window
"the biggest single piece, mechanical, not a redesign". okf/done/ore-first-moves.md
moved the window-free pieces and put the first resident into ore's `window`
feature (`ore::theme`), and deliberately left this. Nothing in `backlog/` or
`plans/` covers it. This item is the whole move, as one piece of work with an
internal order (below).

Read the design note first: the thread rules and the main-thread bridge facts
are there and are not repeated. What this item adds is the inventory of what
is in alloy today, the decisions taken on its seams, the order and the checks.

## Scope: why the devices are in

The first cut of this item left the audio, microphone and camera devices in
alloy, "until the mixer is in forge". Two reasons to take them along:

- With them left behind, alloy keeps its `sdl3` dependency and the boundary
  is only checkable by grep. With them moved, the last step removes `sdl3`
  from `alloy/Cargo.toml` and the compiler holds the line from then on.
- The device half does not need the mixer to move. The SDL side is already a
  thin wrapper set in `sdl_utils.rs` (about 55 call sites in
  `audio/device.rs`, `microphone/mod.rs` and `camera/`). The device goes to
  ore and takes a render callback; alloy's mixer plugs into it now and
  forge's later. That is the seam the design note already describes ("they
  meet on rendered frames").

It also removes an interim: the background and foreground audio restarts
become ore's own business inside its lifecycle handling, as the design says,
instead of living in a closure alloy registers.

## Where it stands

- `alloy/src/app.rs` (1321 lines) is the one function the design describes:
  `setup` does `sdl3::init`, the SDL hints, the headless driver fallbacks
  (offscreen, then dummy plus an EGL pbuffer, then a hidden window) and
  `setup_video`; `run` holds the `sdl3::Sdl`, the `Window`, the event pump,
  the event watch, `PlatformWatch`, the gamepads, the window commands and
  `recover_gl`, next to the frame signals that really are alloy's.
- Alloy names `sdl3` or `sdl_utils` in 17 source files, 3 examples and its
  manifest, and `alloy::sdl3` is a public re-export (`alloy/src/lib.rs`).
  `lattice` and `flux` no longer reach through alloy for SDL: since step 0
  `lattice/src/gl_libs.rs` calls `ore::gl_surface::set_library_path` and
  `flux/src/alloy_plugins/record.rs` calls `ore::time::ticks_ns`.
- The window and the GL context are one knot: `setup_video` calls
  `gl::configure_opengl` before it builds the window and
  `DisplayContext::new_opengl(&window)` after, `recover_gl` replaces both in
  one function, and `stepped.rs` and `backend.rs` hold the `sdl3` window and
  `GLContext`. They cannot move in separate steps.
- Seven Android entry natives exported from `lattice/src/lib.rs` forward into
  alloy: `nativeKeyboardInset`, `nativeHardwareKeyboard`,
  `nativeVideoPlaneLost`, `nativeVideoPlaneVsync`, `nativeDialogResult` (on
  the activity) and `nativeTouch`, `nativeRedrawNeeded` (on the surface).
  `nativeShareResult` already forwards to ore.
- The platform vocabulary (keys and modifiers, pointer types, orientation,
  gamepad state and commands, text input options, cursors, dialog requests
  and answers, `SuspendHold`) is ore's since step 0 (`ore::input`,
  `ore::display`, `ore::gamepad`, `ore::lifecycle`, and under `window`
  `ore::cursor`, `ore::text_input`, `ore::dialog`), and flux and lattice
  name it there. The synthetic gamepad button and axis names are ore's
  since step 2 (`ore::gamepad`).

## Done looks like

- `alloy/Cargo.toml` has no `sdl3` entry, `alloy::sdl3` and
  `alloy/src/sdl_utils.rs` are gone, and alloy names no SDL type.
- `ore` holds every SDL call: init and hints, the pump, the window, the GL
  surface, cursors, text input, keyboard and mouse presence, gamepads,
  dialogs, the audio playback device, the microphone, the camera, and the
  Android vsync and view glue.
- `alloy` keeps the frame loop and what it carries out: `FrameRelease`,
  cadence, refresh counting, the idle tick, liveness, redraw policy, the
  input mute, the resampler, the stepped loop, the raster thread, and the
  mixer (until it moves to forge). The loop has no
  `cfg(target_os = "android")` line left.
- flux and lattice name `ore::` for the platform vocabulary; alloy
  re-exports none of it.
- A `Platform` is buildable without a window, so the windowless host (the
  design's step 4) can use the pump and the lifecycle hook without a
  rewrite. This item does not build that host.
- Behavior is unchanged: frame pacing counters, idle CPU, input, lifecycle,
  audio across a background and resume, and context-loss recovery read the
  same as before, on every platform.

## Decisions

Taken 2026-11-12. The design note's API sketch is updated to match.

1. **ore owns the platform types.** As it already owns `PowerState` and
   `SystemTheme`. `translate_event` splits into SDL -> ore event (in ore) and
   ore event -> `AlloyEvent` (in alloy, mostly a wrap). `AlloyEvent` keeps a
   variant per fact alloy computes (`FrameRendered`, `Tick`, `CadenceHold`,
   ...) and carries ore's input and lifecycle types.
2. **No re-exports; a plugin moves only when all it marshals is ore's.** flux
   and lattice name `ore::` types directly (both already depend on ore).
   alloy does not `pub use` them: that would make it the pass-through the
   design rejects. `flux/src/alloy_plugins/dialog.rs` moves to
   `ore_plugins/`, behind flux's `gui` feature (a dialog needs the pump for
   its answer and the window as its parent). The mixed plugins (`events.rs`,
   `tree.rs`, `video.rs`) stay in `alloy_plugins/` and change imports.
3. **Two paths to the window, split by whether a call is ordered with
   frames.** Title, icon, fullscreen, minimize, raise, cursor, pointer lock
   and text input stay on alloy's command channel, and alloy's loop calls
   the ore window: pointer lock and text input change how events translate,
   so alloy must see them in order. Dialogs go over `ore::main_thread`, as
   the clipboard and the share sheet already do, with ore holding the parent
   window itself; `AlloyCommand::ShowDialog` goes away.
4. **The pump is outside the `window` feature, and the window is its own
   value.** `Platform` (init, `wait`, `waker`, `on_lifecycle`, time,
   gamepads) needs no feature. `platform.open_window(..)` returns an owned
   `ore::window::Window` under `window`; it carries the commands, cursors,
   text input and the GL surface. This replaces the earlier sketch's
   unconditional `platform.window()`. The stepped loop takes the `Window`
   alone. Headless `flux` builds no `Platform`, so the main-thread bridge's
   "no event loop, run at once" rule stays true. Amended at step 2: `wait`
   itself sits under `window` and takes `&Window`, because translating an
   input or resize event reads the window's scale, size and safe area
   live, per event. The waker, the lifecycle hook and the gamepads stay
   feature-free; the windowless host lifts the window-free part of the
   translation out when it is built.
5. **The window and GL surface types.** `Window` owns the `SDL_Window`, is
   not `Send`, and every method is main thread only. The GL side is a
   separate `Send` value, `GlSurface`, for the raster thread (what
   `DisplayContext::Gl` holds today as a raw pointer plus a `GLContext`):
   make current, swap, proc address, extension query. `GlBinding` stays
   alloy's trait, implemented over `GlSurface` and by the EGL pbuffer. No
   trait crosses the boundary for ore to call back into alloy (the CLAUDE.md
   rule): the halves meet on plain values.
6. **Recovery: alloy states the fact, ore carries it out.**
   `RECOVERY_RECREATES_WINDOW` is spelled as a `cfg(target_os)` list, but its
   comment says what it stands for: ANGLE on Windows and macOS, the Adreno
   driver on Android. That is a driver fact, and the design keeps driver
   quirks with the renderer. So the constant stays in alloy, and
   `window.recreate_gl(recreate_window: bool)` in ore does the rebuild
   (destroy, recreate with the same title, size, position, fullscreen and
   maximized state, a fresh context released for the raster thread) and
   returns a `GlSurface`. alloy can key the fact on the real renderer later.
7. **`SuspendHold` is ore's.** ore takes it inside the lifecycle callback
   and hands it over as `Lifecycle::WillBackground(hold)`; alloy wraps it
   into `AlloyEvent::Suspend`. "Taken at the last moment the platform still
   listens" is a platform invariant, and its iOS implementation is an OS
   assertion (okf/backlog/ios-port-readiness.md).
8. **SDL init is ore's, on the host's main thread.** lattice builds
   `ore::Platform` before alloy or forge start anything and passes it (and
   the opened `Window`) to `alloy::setup`, which today calls `sdl3::init`
   itself. The design note's open point goes with it: confirm that the first
   `SDL_Init` fixes SDL's main thread.
9. **The headless chain moves as a unit.** Offscreen, then dummy plus EGL
   pbuffer, then a hidden window: one platform fact, behind `open_window`
   with an interactive or offscreen display, not piecemeal.
   `egl_headless.rs` itself stays in alloy (raw EGL, no SDL).
10. **The `Java_*` natives stay in lattice and forward to ore.** lattice's
    `android_entry!` comment records that a dependency's `#[no_mangle]`
    symbols are exported from the cdylib; do not assume ore can export them
    without checking on the device.
11. **The audio device takes a render callback.** `ore::audio` opens the
    playback device and pulls from a callback the caller gives; alloy's
    mixer (`audio/engine.rs`) is that caller. The Android background and
    foreground restarts (`device_backgrounded`, `device_foregrounded`) run
    inside ore's lifecycle handling. The converter streams
    (`audio_stream_create` and friends) are SDL too and move with the device.

## What moves

**Vocabulary (step 0)**
- From `event.rs`, `keymap.rs`, `gamepad.rs`, `cursor.rs`, `dialog.rs` and
  `lib.rs`: keys, modifiers and `w3c_code_for_key`, pointer types,
  orientation, the gamepad command and the synthetic button and axis names,
  `TextInputOptions` and its enums, `CursorFrame`, `DialogRequest`,
  `DialogFilter`, `Picked`, `SuspendHold`. Plain data, no SDL.

**Init, hints, logging, time (steps 0 and 1)**
- `sdl3::init`, the video-driver log line, and the hints: `SDL_APP_ID`
  (`WINDOW_APP_ID`), the two touch/mouse event hints, the DPI-cursor hint,
  `SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY`, the headless `SDL_AUDIO_DRIVER` /
  `SDL_VIDEO_DRIVER` pair, `SDL_POLL_SENTINEL=0` on Android
  (okf/upstream/sdl-android-wait-event-poll-sentinel-spin.md: the comment
  that explains why it must precede the pump moves with it),
  `SDL_ENABLE_SCREEN_KEYBOARD` (set by the text input command),
  `SDL_OPENGL_ES_DRIVER` (`gl/context.rs`), and the GL library path hints
  lattice sets through `alloy::sdl3` (`gl_libs.rs`).
- `logging.rs`' sink (`sdl3::log::log`): ore exposes a write call; which
  crates log at which level stays alloy's filter.
- `ticks_ns`, `ticks_epoch`, `event_instant`: an ore time module, which also
  removes the `flux` -> `alloy::sdl_utils` edge in `record.rs`.

**The window and the GL surface (step 1)**
- `setup_video`, `setup_headless`, `HeldWindow` and `recover_gl`'s SDL half;
  the window commands in the loop (title, raise, icon, fullscreen,
  minimize/`Background`, pointer lock, text input start and stop);
  `window_safe_area`, `window_display_scale`; `cursor.rs` with the cursor
  wrappers in `sdl_utils.rs`; `start_text_input_with_options`;
  `has_keyboard`, `has_mouse`, `physical_keyboard`,
  `has_screen_keyboard_support`.
- `gl/context.rs` (`configure_opengl`, window GL context, `SdlGlBinding`),
  the SDL parts of `gl/rig.rs` (`SDL_GL_ExtensionSupported`,
  `SDL_GL_GetProcAddress`) and `gl/device_loss.rs` (`SDL_EGL_*`),
  `gl_swap_window_checked`, `WINDOW_SWAP_INTERVAL`, `gl_library_hint`.
- `display_link.rs` (CVDisplayLink, objc2): it is a swap pacer, not a vsync
  source. `SdlGlBinding`'s swap waits on it on macOS, so it moves inside
  `GlSurface`.

**The pump and events (step 2)**
- `wait_event_timeout_ms` plus the `poll_iter` drain, the `FrameReady` custom
  event (becomes the `Waker`), and `event.rs`' `translate_event` with
  `keymap.rs`, `key_from_scancode`, `mod_state`, and the pointer-lock
  coordinate freeze.
- The Android touch side channel (`touch.rs`: the queue, the wake, the
  `MotionEvent` action codes); `nativeTouch` repoints here.
- `PlatformWatch` (`app.rs`): screen keyboard shown and inset, physical
  keyboard presence, the refresh-rate poll, and the 10 s power poll
  (`ore::power` already exists; the poll cadence is alloy's pacing call).
- The lifecycle event watch: the three background / foreground events, run
  synchronously on the pushing thread. Becomes `on_lifecycle`; the closure
  alloy registers calls `set_window_backgrounded` and forwards the events.
  The audio hooks stay in that closure until step 4.
- `gamepad.rs` (524 lines): SDL gamepads; the synthetic pad stays the pad
  owner's API.

**The Android view glue, the vsync source, the dialogs (step 3)**
- `vsync.rs`' Choreographer backend (JNI, the `android` module at the end of
  the file) becomes ore's `display_vsync`, a tick source with a callback. It
  is the only backend today. `VsyncSource`'s request and generation logic,
  `PacingBudget`, `FrameRelease` and `FramePacing` (the pure state machines
  and their tests) stay in alloy. The file is one today, so this is a split.
- `video_plane.rs` (JNI `VideoPlaneView`) and `redraw.rs`.
- `dialog.rs` (SDL file dialogs plus the Android JNI path), reached over
  `ore::main_thread` (decision 3), with its flux plugin (decision 2).
- The remaining lattice natives forward to ore; `alloy::android_result_raw`
  goes away.

**The devices (step 4)**
- `audio/device.rs` and the audio wrappers in `sdl_utils.rs` (decision 11).
  `audio/engine.rs`, `mod.rs`, `ring.rs` and `stream.rs` stay.
- `microphone/mod.rs`' SDL half (open, capture callback, device list); the
  ring stays where its reader is.
- `camera/mod.rs` and `camera/producer.rs`' SDL half (the subsystem init,
  enumeration, open, permission, frame acquire and release, the surface
  conversion and rotation); `camera/tap.rs` only names a time helper.
  okf/backlog/android-microphone-dead-after-background.md wants a reopen
  per recording stream on resume: it lands in ore's lifecycle handling
  after this step, so do it after, not before.

## Stays in alloy

`FrameRelease`, cadence, refresh counting, liveness, redraw policy, input mute
and resampler, the stepped loop (`stepped.rs`, which takes ore's `Window`),
the raster thread, the renderer, the mixer, and two files that keep
`cfg(target_os)` lines as accepted exceptions:

- `frame_timestamps.rs`: the EGL frame-timestamp extension, a driver fact
  that only Android's EGL offers.
- `egl_headless.rs`: raw EGL with no SDL, the second implementation of
  alloy's `GlBinding`; its cfg lines are the EGL library name per OS.

## Order

Each step is a commit that builds and passes the checks below.

0. **Vocabulary and leaves.** The platform types, the time module, the log
   sink, a hint call for `gl_libs.rs`. flux and lattice import `ore::`;
   the two outside edges into `alloy::sdl3` and `alloy::sdl_utils` are gone.
   No behavior changes. Done 2026-11-12. The synthetic button and axis
   names stay in alloy until step 2, because they derive from the SDL
   button and axis tables.
1. **Init, window, GL surface, recovery.** `ore::Platform` and
   `open_window`, the headless chain, `GlSurface` with the swap pacer,
   `recreate_gl`, cursors, text input, keyboard and mouse presence. lattice
   builds the `Platform`. For this one step ore lends alloy's pump the SDL
   event handle it still needs; the next step removes that. Done
   2026-11-12, with these calls: the module is `ore::gl_surface` (GL
   itself stays alloy's; only the SDL side of the surface moved); lattice
   builds the `Platform` and alloy's `setup` opens the window, because the
   headless chain's middle step (the EGL pbuffer) is alloy's; `open_window`
   creates the context so a context failure on the offscreen driver still
   falls through to the hidden window, and `create_gl_surface` only
   releases it; the backbuffer multisample request travels as
   `WindowConfig::backbuffer_samples` and ore applies it on Android alone;
   the hardware-keyboard fact (`set_hardware_keyboard`) is ore's now,
   alloy's function forwarded until step 2 repointed the native; `recreate_gl`
   reapplies the icon, the text input session and the pointer lock itself;
   alloy's logger lets `ore` through at every level (it filtered ore's
   lines out before); `Window::sdl_window` is the second lend, for the
   dialog parent, removed in step 3.
2. **The pump.** `wait`, the `Waker`, `on_lifecycle` with `SuspendHold`,
   `PlatformWatch`, gamepads, the touch side channel. Done 2026-11-12,
   with these calls: `Platform::wait(timeout, &Window)` returns the drained
   events translated (`ore::event::PlatformEvent`, one variant per fact)
   with their instants, under the `window` feature (decision 4, amended);
   `AlloyEvent` keeps its variants and wraps them (`From<PlatformEvent>`),
   so flux and lattice saw no change; the pointer-lock freeze follows
   `window.pointer_locked()` at the start of each `wait`, which the mute's
   forced unlock and a recreated window fall under too; the watch runs at
   the end of `wait` and reports `DisplayRefreshRate` on change only, so
   alloy keeps its own copy (initial from the window with
   `ore::display::DEFAULT_REFRESH_HZ`) and publishes it; the gamepads open
   on the first `platform.gamepads()` call (before the first `wait`, as
   the loop did) and the pump feeds them, the snapshot a plain
   `Vec<Option<GamepadState>>`; `on_lifecycle` is a raw `SDL_AddEventWatch`
   with a leaked box, since the crate's guard removes the watch on drop;
   `Platform::init` takes the event subsystem and the pump in every mode
   (the stepped mode never took them before; a window already initialized
   SDL's events there, so the main-thread bridge is unchanged); the
   keyboard inset lives in `ore::window`, and `nativeKeyboardInset`,
   `nativeHardwareKeyboard` and `nativeTouch` call ore directly, so alloy's
   forwarders are gone; the `link_from_drop` tests are
   `ore/src/tests/platform.rs`. Verified on Linux (Wayland) through the
   dev server with real compositor input: keys with modifiers, mouse
   motion, text input, a resize, the pointer-lock freeze, a synthetic pad,
   idle CPU at rest, the close path; the headless suites pass. On the
   tablet: touch with sample times through `ore::touch`, text input, back,
   background and resume (the suspend handler runs to completion under the
   hold, the queued visibility copy arrives at resume), rotation, and the
   pump thread at 2 to 3 percent CPU at rest (the poll-sentinel guard).
   On the TV: the 32-bit build starts, the remote arrives as keys and as a
   raw joystick, back reaches the app. The screen keyboard's height reads
   0 on the tablet, with the older installed player too, so that is a
   pre-existing gap (okf/backlog/android-keyboard-inset-zero.md), not this
   step's. Still open: the macOS and Windows builds.
3. **Android glue, vsync, dialogs.** The Choreographer source, the video
   plane, the redraw answer, dialogs and their plugin, the natives. Done
   2026-11-12, with these calls: the Choreographer is `ore::display_vsync`
   (`available`, and a `Source` opened on the thread that waits, whose
   `wait_vsync` arms one callback and answers the vsync instant),
   feature-free; alloy keeps `VsyncSource` with the `sol-vsync` thread, the
   request and generation protocol and the delay, so `vsync.rs` has no cfg
   line left and alloy's manifest lost `ndk` and `ndk-sys`;
   `ore::video_plane` and `ore::redraw` compile on every platform (`active`
   and the redraw's `take` and `answer` everywhere, the plane and the JNI
   under Android), and the redraw's wake is the pump's own waker, set in
   `Platform::init`; the dialogs are `ore::dialog::Session` (the `Dialogs`
   session, in the shape of the link and share sessions), whose `begin`
   shows over `ore::main_thread` itself, so `AlloyCommand::ShowDialog` is
   gone; the desktop show calls `sdl3::sys::dialog` with the parent handle
   `ore::window` keeps for the live window (set at open and recreate,
   cleared on drop), because the bridge callback has no window to borrow;
   availability is probed by `open_window` (an interactive display and a
   platform backend); the flux plugin is `ore_plugins/dialog.rs` behind
   `gui`, reached as `flux::dialog`; the four natives forward to ore, and
   `alloy::android_result_raw` and `Window::sdl_window` are gone; alloy's
   manifest lost `jni`. Verified on Linux (Wayland) through the dev server:
   the portal file chooser shows over the bridge, a cancel answers, and a
   second request while one is open is refused; the desktop pick decode is
   reviewed, not exercised (the portal could not be driven to a pick from
   outside). On the tablet, with the new build: a document pick comes back
   as a content URI with its name, a cancel answers, the held redraw is
   answered at launch (no timeout warning in logcat), the video plane plays
   and ends when the activity backgrounds, the pump idles below 2 percent,
   and the pacing counters read as the step 2 build's (6 against 12 missed
   presents at the 28 s mark after a warm launch; a cold first launch after
   the install counts more). On the TV, the 32-bit build starts and
   connects, a warm launch answers its held redraw (the cold dev-server
   launch outlives the 5 s hold, as the step 2 build did), and a dialog
   request resolves to the TV's document-picker stub, which cancels at
   once (pre-existing, see okf/backlog/android-tv-dialog-stub.md). Still
   open: the macOS and Windows builds.
4. **Devices.** Audio playback, microphone, camera; the lifecycle audio
   restarts become ore's. Done 2026-11-12, with these calls: `ore::audio`
   is feature-free and holds three values, `Playback` (the default device
   at its own rate, pulling a render callback that answers a `Pull`: the
   frames asked, the lead the stream still holds, and `put`), `Converter`
   (the unbound stream, in frames) and `Recording` (mono f32 at the opened
   rate, pushing a `Captured` the callback reads from; its drop destroys
   the stream before it drops the callback, so the callback may own a
   ring clone), plus `recording_devices` and `default_playback_format`,
   which alloy reads before it builds the shared mixer state its callback
   captures; alloy implements its `Converter` trait for ore's type and the
   mixer's loop over the pull is unchanged; the Android resume check (the
   open device, the transition counter, the pull window, the reopen) runs
   from ore's lifecycle watch ahead of the host's closure, so the loop's
   last two `cfg!(android)` lines are gone. `ore::camera` is feature-free:
   `start` (the non-blocking subsystem init with its worker and both
   wedge comments), `devices` with `CameraInfo` and `Facing` (flux names
   them from ore), `Camera::open(id, width, height)` choosing the
   uncompressed native spec nearest the size (the format knowledge moved
   with it), `permission`, `frame_period`, `acquire`, and a `Frame` that
   is released on drop and answers its rotation and its RGBA32 rows (the
   surface's own, or converted into a scratch). alloy keeps the session
   state machines, the deadlines, the default size, the producer's poll
   cadence and the rotation. `MicrophoneInfo` and `CameraInfo` are ore's
   types now, built by alloy for the test microphone and the still camera
   too. `camera_dump` was rewritten over `ore::camera` here, since its
   wrappers went. `sdl_utils.rs` keeps `sdl_error` and
   `iostream_from_reader` (no in-tree user) for step 5. ore's tests: the
   converter's counts and a pull on the dummy driver. Verified on Linux
   through the dev server with a probe: the mixer opens the device at its
   rate, a looping tone reaches the sink at exactly the level asked
   (captured from the sink monitor), no underruns; a microphone session
   delivers its sample rate's worth of samples per second and reopens on
   a reload; a UVC webcam lists, opens at its native format, streams at
   30 fps into the app's texture with no skipped upload, and a clip
   recorded from the camera and the webcam's microphone plays back with
   its audio; the rewritten `camera_dump` saves real frames through the
   conversion path. On the tablet, with the new build: the device opens
   at 44.1 kHz, a looping tone plays, and after 30 s in the background
   and a resume AAudio restarts the stream at once (requestStart returns
   0) and the mixer renders on every stats read after; the reopen itself
   stays untriggered, as before the move, because this Android 12 device
   has no freezer to disconnect the stream (the Pixel 7 repro in the
   upstream note). Still open: the macOS and Windows builds.
5. **Cleanup.** Remove `sdl3` from `alloy/Cargo.toml`, delete
   `sdl_utils.rs` and `pub use sdl3`; update the three examples that use
   SDL directly (`event_time_probe`, `present_fence_probe`, `camera_dump`);
   update okf/design/platform-layering.md ("Today", "Migration", "Open"),
   the alloy line in the root CLAUDE.md ("it still holds the SDL window
   ..."), ore's crate doc, and the backlog items that link the moved files
   by path (android-keyboard-capability, desktop-widget-window,
   window-size-prop, android-microphone-dead-after-background,
   ios-port-readiness). Done 2026-11-13, with these calls:
   `iostream_from_reader` and its test went (no in-tree user) rather than
   moving; the camera's Linux-only `PENDING_DEADLINE` lost its cfg and is
   applied where `ore::camera::permission_prompts()` is false, a fact keyed
   on SDL's camera driver name (v4l2 and pipewire never prompt; any other
   or not yet known backend may, so it gets no deadline) rather than on
   the OS; `present_fence_probe` stays in alloy, rewritten over
   `Platform::init`, `open_window`, `create_gl_surface` and a zero-timeout
   `Platform::wait` as its drain, and runs its four phases as before;
   `event_time_probe` moved to `ore/examples/` unchanged, since it measures
   SDL's raw event timestamp against the receipt, which a rewrite over
   `event_instant` would hide; alloy's stale member `Cargo.lock` (from the
   restructure, unused in the workspace) went too; the test that read
   "SDL's" error text is named for the surface's. Alloy's remaining
   `cfg(target_os)` lines are `frame_timestamps.rs`, `egl_headless.rs` and
   the `cfg!` list of `RECOVERY_RECREATES_WINDOW` (decision 6). Verified:
   `cargo tree -p alloy --depth 1` names no `sdl3`; the alloy, ore (with
   and without `window`) and flux suites pass; `camera_dump` opens the
   webcam and saves frames; `make client` builds.
   The macOS and Windows builds and runs, still open after steps 1 to 4,
   are okf/backlog/ore-pump-macos-windows-verification.md.

Tests of moved code move with it into `ore/src/tests/` (`keymap.rs`,
`dialog.rs`, the SDL half of `context_loss.rs`); alloy keeps its
`FrameRelease`, cadence and liveness tests where they are.

## Plans in flight on the same files

Sequence against these before starting a step, so a mechanical move does not
collide with open work:

- okf/done/event-timestamp.md: `sdl_utils::event_instant`, `touch.rs`
  (steps 0 and 2).
- okf/done/renderer-seam.md: `gl/context.rs`, `backend.rs`, the public
  surface of `event.rs` (steps 0 and 1).
- okf/done/video-texture-off-frame-loop.md and okf/done/video-streaming.md:
  the wake closure in `app.rs`, `video_plane.rs` (steps 2 and 3).

## Checks

- `cargo test -p ore --features window` (a bare `cargo test -p ore` skips
  the window part), `cargo test -p alloy`,
  `cargo test -p flux --lib --features gui`, `bun run sol test`, and a
  headless `sol render` (the offscreen path and the EGL pbuffer fallback).
- Linux desktop, on Wayland and on X11, driven through the dev server and
  MCP as the root CLAUDE.md describes: resize, fullscreen, pointer lock,
  text input, cursors (system and image), a file dialog, gamepad (a
  synthetic pad), minimize and raise, sound, the microphone, the camera,
  and the close path.
- macOS and Windows: a CI build on each after every step. After step 1, one
  run on each: the window opens through the bundled GL libraries (the
  `gl_libs.rs` hints), a forced context loss recovers with a recreated
  window, and on macOS the swap pacer still paces.
- Tablet: touch with sample times, background and resume (the suspend hold
  and the audio restart), keyboard inset, a video plane, rotation, a forced
  context loss and its recovery (okf/done/gpu-context-loss.md), and idle CPU
  with the app at rest, which is the regression the poll-sentinel workaround
  guards against. TV: the 32-bit build starts and plays sound.
- Pacing is behavioral: compare the refresh-miss and cadence counters before
  and after, on a release client for any timing figure.
- After each step: no `sdl3::` in the moved alloy files, and each dependency
  that lost its last user leaves alloy's manifest (`objc2-core-video`,
  `objc2-core-foundation`, `jni`, `ndk` and `ndk-sys` are candidates).

## Not in this item

- The mixer to forge, `barcode.rs` to forge, and forge's own OS code
  (content providers, the AMediaCodec decoder's JNI side, the tty) to ore:
  they share no SDL coupling with this.
- The windowless host and the Android windowless entry.
- Desktop widget window, which wants window flags this API will carry but is
  its own feature (okf/backlog/desktop-widget-window.md).
- Any change to pacing policy, the cadence hold, or the stepped loop's
  behavior.
