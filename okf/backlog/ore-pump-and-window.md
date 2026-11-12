---
title: Move the SDL pump, the window and the GL surface into ore
description: The biggest piece of okf/design/platform-layering.md still in alloy - SDL init and hints, the event pump and its translation, the Android touch side channel, the lifecycle watch, gamepads, the window and its commands, cursors, text input, the GL window and context with its loss recovery, the Android vsync and view glue - moves behind one ore `Platform` (a plain-function pump, a waker, a lifecycle hook, a window, a vsync source, a GL surface) so alloy keeps only the frame loop, the pacing facts and the stepped loop and names no SDL type for the window; audio, camera and microphone devices stay for their own move.
created: 2026-11-12
---

# Move the SDL pump, the window and the GL surface into ore

okf/design/platform-layering.md decided this and called it "the biggest
single piece, mechanical, not a redesign". okf/done/ore-first-moves.md moved
the window-free pieces and put the first resident into ore's `window`
feature (`ore::theme`), and deliberately left this. Nothing in `backlog/`
or `plans/` covers it yet. This item is the whole move, as one piece of
work with an internal order (below).

Read the design note first: the API sketch, the thread rules and the
main-thread bridge facts are there and are not repeated. What this item adds
is the inventory of what is in alloy today, the seams that need a decision,
and the checks.

## Where it stands

- `alloy/src/app.rs` (1321 lines) is the one function the design describes:
  `setup` does `sdl3::init`, nine SDL hints, the headless driver fallbacks
  (offscreen, then dummy plus an EGL pbuffer, then a hidden window) and
  `setup_video`; `run` holds the `sdl3::Sdl`, the `Window`, the event pump,
  the event watch, `PlatformWatch`, the gamepads, the window commands and
  `recover_gl`, next to the frame signals that really are alloy's.
- Alloy names `sdl3` or `sdl_utils` in 25 files. `lattice` and `flux` reach
  through alloy for SDL (`alloy::sdl3::hint::set` in `lattice/src/gl_libs.rs`,
  `alloy::sdl_utils::ticks_ns` in `flux/src/alloy_plugins/record.rs`), and
  `alloy::sdl3` is a public re-export (`alloy/src/lib.rs`).
- The Android entry natives (`nativeKeyboardInset`, `nativeHardwareKeyboard`,
  `nativeVideoPlaneLost`, `nativeVideoPlaneVsync`, `nativeDialogResult`) are
  exported from `lattice/src/lib.rs` and forward into alloy.

## Done looks like

- `ore` holds the SDL calls for init, the pump, the window, the GL surface,
  cursors, text input, keyboard and mouse presence, gamepads, dialogs, and
  the Android vsync and view glue. One `Platform` (the design's sketch:
  `wait`, `waker`, `on_lifecycle`, `window`, `display_vsync`, `recreate_gl`)
  is what a host builds once, on the main thread, before alloy or forge start
  anything.
- `alloy` keeps the frame loop and what it carries out: `FrameRelease`,
  cadence, refresh counting, the idle tick, liveness, redraw policy, the
  input mute, the resampler, the stepped loop, and the raster thread. It
  names no SDL type for the window, the events or the GL context, and the
  loop has no `cfg(target_os = "android")` line left.
- `alloy::sdl3` and `alloy::sdl_utils`' window, input, time and GL parts are
  gone from alloy's public surface. What stays in `sdl_utils.rs` is the audio,
  camera and microphone device wrappers, until those move.
- `ore`'s pump and lifecycle do not require a window, so the windowless host
  (the design's step 4) can use them without a rewrite. This item does not
  build that host.
- Behavior is unchanged: frame pacing counters, idle CPU, input, lifecycle
  and context-loss recovery read the same as before, on desktop and on the
  tablet.

## What moves

**Init, hints, logging, time**
- `sdl3::init`, the video-driver log line, and the hints: `SDL_APP_ID`
  (`WINDOW_APP_ID`), the two touch/mouse event hints, the DPI-cursor hint,
  `SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY`, the headless `SDL_AUDIO_DRIVER` /
  `SDL_VIDEO_DRIVER` pair, and `SDL_POLL_SENTINEL=0` on Android
  (okf/upstream/sdl-android-wait-event-poll-sentinel-spin.md: the comment
  that explains why it must precede the pump moves with it).
- `logging.rs`' sink (`sdl3::log::log`): ore exposes a write call; which
  crates log at which level stays alloy's filter.
- `ticks_ns`, `ticks_epoch`, `event_instant`: an ore time module, which also
  removes the `flux` -> `alloy::sdl_utils` edge in `record.rs`.

**The pump and events**
- `wait_event_timeout_ms` plus the `poll_iter` drain, the `FrameReady` custom
  event (becomes the `Waker`), and `event.rs`' `translate_event` with
  `keymap.rs`, `key_from_scancode`, `mod_state`.
- The Android touch side channel (`touch.rs`: the queue, the wake, the
  `MotionEvent` action codes) and the `redraw.rs` wake.
- `PlatformWatch` (`app.rs`): screen keyboard shown and inset, physical
  keyboard presence, the refresh-rate poll, and the 10 s power poll
  (`ore::power` already exists; the poll cadence is alloy's pacing call).
- The lifecycle event watch: the three background / foreground events, run
  synchronously on the pushing thread. Becomes `on_lifecycle`; the closure
  alloy registers keeps calling `set_window_backgrounded`, building the
  `SuspendHold` and the audio device hooks until the audio device moves.
- `gamepad.rs` (524 lines): SDL gamepads, the synthetic pad commands stay
  as the pad owner's API.

**The window**
- `setup_video`, `setup_headless`, `HeldWindow` and the window half of
  `recover_gl`; the window commands in the loop (title, raise, icon,
  fullscreen, minimize/`Background`, pointer lock, text input start and
  stop); `window_safe_area`, `window_display_scale`; `cursor.rs` with the
  cursor wrappers in `sdl_utils.rs`; `start_text_input_with_options`;
  `has_keyboard`, `has_mouse`, `physical_keyboard`,
  `has_screen_keyboard_support`; `dialog.rs` (SDL file dialogs plus the
  Android JNI path), which already takes the window as its parent.

**The GL surface**
- `gl/context.rs` (`configure_opengl`, window GL context, `SdlGlBinding`),
  the SDL parts of `gl/rig.rs` (`SDL_GL_ExtensionSupported`,
  `SDL_GL_GetProcAddress`) and `gl/device_loss.rs` (`SDL_EGL_*`),
  `gl_swap_window_checked`, `WINDOW_SWAP_INTERVAL`, and
  `RECOVERY_RECREATES_WINDOW` with the SDL half of `recover_gl`.

**The Android view glue and the vsync sources**
- `vsync.rs`: the Choreographer backend (JNI) and `display_link.rs`
  (CVDisplayLink, objc2) become ore's `display_vsync`, a tick source with a
  callback. `FrameRelease` and `FramePacing` (the pure state machine and its
  tests) stay in alloy. The file is one today, so this is a split.
- `video_plane.rs` (JNI `VideoPlaneView`) and the Android half of `redraw.rs`.
- lattice's exported JNI natives forward to ore instead of alloy
  (`alloy::android_result_raw` goes away).

## Stays in alloy

`FrameRelease`, cadence, refresh counting, liveness, redraw policy, input mute
and resampler, the stepped loop (`stepped.rs`, which takes ore's window handle
instead of an `sdl3` one), the raster thread, the renderer, and
`frame_timestamps.rs` (EGL frame-timestamp extension: a driver fact, not an
OS one). `egl_headless.rs` also stays for now: it is raw EGL with no SDL, and
becomes the second implementation of alloy's `GlBinding`.

## Seams that need a call

1. **Who owns the platform event types.** Recommendation: ore, as it already
   owns `PowerState` and `SystemTheme`. `translate_event` splits into SDL ->
   ore event (in ore) and ore event -> `AlloyEvent` (in alloy, mostly a
   wrap). `AlloyEvent` keeps a variant per fact alloy computes (`FrameRendered`,
   `Tick`, `CadenceHold`, ...) and carries ore's input and lifecycle types, so
   `flux/src/alloy_plugins/events.rs` changes imports only.
2. **The window and the GL surface types.** `ore::window::Window` owns the
   `SDL_Window`, is not `Send`, and every method is main thread only. The GL
   side is a separate `Send` handle for the raster thread (what
   `DisplayContext::Gl` holds today as a raw pointer plus a `GLContext`).
   `GlBinding` stays alloy's trait, implemented by an ore-backed type and by
   the EGL pbuffer. No trait crosses the boundary for ore to call back into
   alloy (the CLAUDE.md rule): the halves meet on plain values.
3. **Where recovery's "recreate the window" lives.** `recover_gl` decides on a
   `cfg(target_os)` list (Windows, macOS, Android). The design keeps GL driver
   quirks with the renderer, but this list is keyed on the OS, and the
   `cfg(target_os)` smell says ore. Recommendation: `recreate_gl` in ore does
   the whole rebuild and returns a binding; alloy sees only "a fresh binding".
   Confirm before moving.
4. **SDL init ownership.** The design leaves it open; the root CLAUDE.md
   already says the host owns the main thread and makes the first `SDL_Init`.
   Recommendation: lattice builds `ore::Platform` (and passes it to
   `alloy::setup`, which today calls `sdl3::init` itself), and ore inits every
   subsystem the window part needs. The audio, camera and microphone wrappers
   keep their own `SDL_InitSubSystem` calls meanwhile (SDL refcounts them).
5. **Headless setup order.** The offscreen -> dummy plus EGL pbuffer -> hidden
   window fallback chain is a platform fact and moves as a unit into ore
   (`open_window` with an interactive or offscreen display), not piecemeal.
6. **Native symbols from a dependency.** lattice's `android_entry!` comment
   records that a dependency's `#[no_mangle]` symbols are exported from the
   cdylib. Keep the `Java_*` natives in lattice and forward to ore; do not
   assume ore can export them without checking on the device.

## Order

Each step is a commit that builds and passes the checks below. Steps 1 and 2
probably have to land together, because `translate_event` takes the window.

1. ore `Platform`, init and hints, time, the log sink, the `Window` and its
   commands, cursors, keyboard and text input; alloy's loop calls them.
2. The pump, `translate_event`, keymap, gamepads, the touch side channel, the
   lifecycle hook, `PlatformWatch`; `Waker` replaces the `FrameReady` event.
3. The GL surface, `recreate_gl` and the SDL parts of `rig.rs` and
   `device_loss.rs`; lattice's `gl_libs.rs` hint goes through ore.
4. The vsync sources, `video_plane.rs`, `redraw.rs`, `dialog.rs`; lattice's
   natives repointed.
5. Delete the dead alloy surface (`alloy::sdl3`'s remaining window users,
   the moved `sdl_utils.rs` parts), update the three examples that use SDL
   directly (`event_time_probe`, `present_fence_probe`, `camera_dump`; the
   last stays on `sdl_utils` until camera moves), and update
   okf/design/platform-layering.md ("Today", "Migration") and the alloy and
   root CLAUDE.md wording ("it still holds the SDL window ...").

Tests of moved code move with it into `ore/src/tests/`; alloy keeps its
`FrameRelease` and cadence tests where they are.

## Checks

- `cargo test -p ore`, `cargo test -p alloy`,
  `cargo test -p flux --lib --features gui`, `bun run sol test`, and a
  headless `sol render` (the offscreen path and the EGL pbuffer fallback).
- Desktop, driven through the dev server and MCP as the root CLAUDE.md
  describes: resize, fullscreen, pointer lock, text input, cursors (system and
  image), a file dialog, gamepad (a synthetic pad), minimize and raise, and
  the close path.
- Tablet: touch with sample times, background and resume (the suspend hold and
  the audio restart), keyboard inset, a video plane, rotation, a forced
  context loss and its recovery (okf/done/gpu-context-loss.md), and idle CPU
  with the app at rest, which is the regression the poll-sentinel workaround
  guards against.
- Pacing is behavioral: compare the refresh-miss and cadence counters before
  and after, on a release client for any timing figure.
- After each step: no `sdl3::` in the moved alloy files, and the unused
  dependency leaves the manifest it left (`objc2-core-video`,
  `objc2-core-foundation` and the Android `ndk` entries are candidates).

## Not in this item

- The audio, camera and microphone devices and the mixer split (their own
  move; they need the mixer in forge first).
- The windowless host and the Android windowless entry.
- Desktop widget window, which wants window flags this API will carry but is
  its own feature (okf/backlog/desktop-widget-window.md).
- Any change to pacing policy, the cadence hold, or the stepped loop's
  behavior.
