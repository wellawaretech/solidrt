---
title: Platform layering
description: Where OS integration lives - our own OS layer (`ore`) that wraps SDL where SDL reaches and our native code where it stops (SDL is the OS abstraction, decided 2026-11-04), with alloy reduced to the window's scene, rendering and frame timing (facts-based; policy stays lattice's) and forge to portable capabilities; the main loop's split along the existing SDL pump, the facts that constrain it (SDL's thread rules, its activity-bound Android helpers, the headless link cost), the test set of platform features an app platform needs (push, alarms, background sync, secure storage, printing and more), the alternatives rejected, and the landing zone that makes new work land right before any existing code moves. Read before placing any platform or OS feature.
created: 2026-11-04
---

# Platform layering

Every OS feature reopened the same question: alloy or forge? The rule
asked two things - "could the headless `flux` CLI do it" (forge) and "does
it go through SDL" (alloy) - and they disagree whenever SDL wraps something
that needs no window. Each time we either put the feature in the wrong
crate or rewrote tested code: forge's own WAV decoder exists because
`SDL_LoadWAV` "would put a decoder in alloy" (okf/done/audio-unification.md),
and opening a link stalled on the same fork (now okf/done/ore-landing-zone.md).

The cause: both crates were defined by SDL. alloy was "what needs SDL",
forge "what does not". Talking to the OS on the app's behalf had no home,
so each feature landed by its implementation, and swapping the
implementation would move it across crates. Meanwhile a growing set of
platform work (push, alarms, background sync, secure storage, printing)
was postponed, and every item of it would have hit the same question.

## Decision

SDL is the OS abstraction library (2026-11-04), not the window library
that happens to have extras. We wrap it in our own OS layer: one crate
that is the platform boundary. It uses SDL where SDL reaches and our own
native code (JNI, Win32, Objective-C, D-Bus) where SDL stops, behind one
uniform contract per facility. SDL is an implementation detail of that
crate; no other crate names it.

## The layers

- **OS layer** (crate `ore`: the raw material everything else is refined
  from): every OS facility, one
  contract each. The window and its events, input devices, the GL surface,
  the display's vsync source, the audio device, the camera, sensors,
  gamepads, clipboard, dialogs, opening a link, locale, power, user
  folders, notifications, alarms, secure storage, content providers, the
  main-thread bridge, the Android entry points and JNI glue. The window
  part sits behind a `window` feature, so headless `flux` leaves it out.
  Depends on SDL, libc, ndk, jni, windows-sys, objc2; on nothing of ours.
- **alloy**: the window's scene and its rendering. The renderer and raster
  thread, the rendertree (layout, hit testing, pointer routing, damage,
  compositing), the spatial core, and frame timing (`FrameRelease`,
  cadence, refresh counting, liveness), facts-based: it carries out what
  lattice chooses. Uses the OS layer's window, GL
  surface and vsync source; names no SDL type.
- **forge**: portable capabilities. Codecs, sqlite, http, p2p, compression,
  crypto, image, svg, wasm, the audio mixer. Calls the OS layer where a
  capability touches the OS (a `content://` document, a hardware decoder's
  JNI side).
- **lattice**: the hosts (GUI, test, render, and the windowless host to
  come) and their policy.
- **flux**: JS marshalling, one plugin layer per crate it marshals.

```
                     runtime
                        |
                     lattice ----------------+
                    /   |   \                |
                   /    |    \               |
                  |   flux    |              |
                  |  /  :  \  |              |
                  | /   :   \ |              |
                 alloy  :   forge            |
                    \   :   /                |
                     \  :  /                 |
                       ore  <----------------+
                        |
             SDL, libc, ndk, jni, windows-sys, objc2

  edge = "is used by", read upward
  ----  lattice reaches ore directly (hosts, windowless entry)
  :     flux -> ore: ore_plugins (openLink, locale, ...)
        flux -> alloy only with the gui feature
```

- ore: SDL and the OS libraries only. Its `window` feature is enabled
  through alloy.
- alloy: ore with the window part.
- forge: ore without the window part (content providers, a hardware
  decoder's JNI side, process facts).
- flux: forge and ore; alloy only with `gui`. Plugin layers
  `forge_plugins/`, `alloy_plugins/` and `ore_plugins/`.
- lattice: all four directly. runtime: lattice.

lattice reaches ore for host concerns only, never for features:
- the windowless host: ore's pump without the window (lifecycle, the
  main-thread bridge) and, on Android, the windowless entry that loads the
  native side and publishes ndk-context; no alloy runs there;
- init order: the host owns the main thread and makes the first
  `SDL_Init` on it, before alloy or forge start anything;
- app identity: the single-instance hand-off (`ore::link`'s `hand_off`
  and `listen`), called with the app's id, and the app's own scheme handed
  to the engine as a plain value (`OwnScheme`), which the app-facing
  registration (`registerProtocolHandler` in `flux:link`) reads;
- storage roots: the per-app user folders (`storage.rs` and `gl_libs.rs`
  call `ore::folders::pref_dir`).

App-facing facilities (opening a link, locale, notifications, ...) never
pass through lattice: they reach JS through `ore_plugins/` in flux. Routing
host concerns through alloy would send window-free work through the window
crate; routing them through flux would make the marshalling layer a
pass-through for work it has no part in.

Headless `flux` links ore without `window`: the SDL cost measured below.
ore never depends on forge, though the audio device is ore's and the
mixer forge's: the device takes a render callback, the host plugs the
mixer into it, and they meet on rendered frames.

Placement, in order:

1. Does it reach the OS or SDL - anything per platform, any
   `cfg(target_os)`? OS layer.
2. Is it the scene, rendering, or frame timing? alloy: the facts and
   their execution, never the policy choice.
3. Is it the same code on every platform? forge.
4. Is it how an app is hosted, or a policy choice over alloy's facts?
   lattice.

That makes rule 2 of the platform-layering memory enforceable: platform
differences are normalized at the platform boundary, and a
`cfg(target_os)` outside the OS layer is a smell anyone can grep for. GL
driver quirks stay with the renderer: they are keyed on the driver, not
the OS.

## Facts

### SDL thread rules (SDL 3.4.18 headers)

| Area | Functions | Thread rule |
|---|---|---|
| filesystem (pref path, user folders, glob) | 11 | any thread |
| cpuinfo | 19 | any thread |
| audio | 58 | any thread (one call: the stream's own thread) |
| camera | 15 | any thread |
| log, hints, time, asyncio | 46 | any thread |
| process | 9 | 5 any thread, 4 not thread safe |
| locale (`SDL_GetPreferredLocales`) | 1 | not thread safe |
| power (`SDL_GetPowerInfo`) | 1 | not thread safe |
| misc (`SDL_OpenURL`) | 1 | main thread only |
| clipboard, dialog, messagebox, tray | 40 | main thread only |

"Not thread safe" means one caller at a time, not a thread affinity: a
lock in the OS layer covers it. Main thread only is real affinity.
`SDL_RunOnMainThread(callback, userdata, wait)` (SDL 3.2+) runs a callback
on the main thread during event processing, or at once when already on it.

Where JS runs:
- GUI host: JS on `sol-ui`; the main thread pumps SDL events (alloy's
  three-thread model).
- Headless `flux`: `#[tokio::main(flavor = "current_thread")]`, so JS runs
  on the main thread and a main-thread call runs directly.

### SDL on Android is activity-bound

SDL's Android OS helpers go through the activity singleton:
`SDLActivity.openURL` calls `mSingleton.startActivity`, and the library
loading and JNI setup run from `SDLActivity.onCreate`. A process the OS
starts for a receiver, a service or a worker (push, alarms, background
sync) has no activity, so these helpers are not available there. Our own
JNI through ndk-context with the application context is, and
`forge/src/fs/content.rs` already works that way. In a windowless start,
nothing has published ndk-context yet; the windowless entry must do it.

### The headless link cost

Measured with a throwaway crate linking `sdl3 =0.19.0`
(`build-from-source-static`), release, stripped, Linux x86_64:

| Binary | Size |
|---|---|
| baseline (no SDL) | 0.34 MB |
| pref path, power, locale, user folder, `SDL_OpenURL`, no `SDL_Init` | 4.35 MB |
| `SDL_Init` of events, audio, camera, sensor, plus locale | 4.34 MB |

- About 4 MB, whatever is called: the static archive pulls in most of
  SDL either way. Against the 59.6 MB release `flux` binary, about 7%.
- Building SDL from source took about 2 minutes on this machine (16
  cores, balanced power profile, 8 jobs; a rough figure). The `flux`
  target builds it once per target directory and feature set, as the
  client already does.
- On CI the first run with ore linked into headless `flux` added about a
  minute to the `test-flux` jobs (Linux 137 to 180 s, Linux arm64 109 to
  165 s, macOS 117 to 193 s, Windows 273 to 282 s; 2026-11-04, warm
  caches for everything but SDL).
- `SDL_Init` of events, audio, camera and sensor succeeds with no display
  (`DISPLAY` and `WAYLAND_DISPLAY` unset): the window-free subsystems
  work headless.

## The main loop

The interactive loop (`App::run` in `alloy/src/app.rs`) is one function
with five jobs:

| Job | Today | Layer |
|---|---|---|
| Wait and drain: `wait_event_timeout_ms`, `poll_iter`, `translate_event`, the Android touch side channel, the background event watch, the `PlatformWatch` polls (keyboard, power, refresh rate), gamepads | raw SDL in the loop | OS |
| Wake: `FrameReady` pushed by the raster and vsync threads | SDL custom event | OS (mechanism) |
| Frame signals: `FrameRelease`, vsync arming, refresh counting, cadence, idle tick, `FrameRendered`, liveness, redraw | alloy | alloy |
| Raster results: presents, context loss (`recover_gl` rebuilds the window and GL) | mixed | alloy, calling the OS layer for the rebuild |
| Commands: window operations (title, icon, fullscreen, cursor, pointer lock, text input, minimize), dialog | SDL calls in the loop | OS; the pacing commands stay alloy |

The split stays on the existing SDL pump. The OS layer is the pump, as a
plain function; alloy keeps its loop and carries out the pacing lattice
chooses. No
inverted callback framework:

```rust
impl Platform {
  // The pump: SDL_WaitEventTimeout plus the drain, translated and
  // timestamped; runs queued main-thread callbacks as SDL does.
  fn wait(&mut self, timeout: Duration) -> impl Iterator<Item = (PlatformEvent, Instant)>;
  // Any thread: wake the wait (FrameReady today).
  fn waker(&self) -> Waker;
  // Synchronous lifecycle (the Android background watch), on the pushing
  // thread. WillBackground carries the suspend hold, taken by the OS layer.
  fn on_lifecycle(&self, f: impl Fn(Lifecycle) + Send + Sync);
  fn display_vsync(&self) -> Option<VsyncSource>; // Choreographer
  // The `window` feature. The window is its own value: a windowless host
  // builds a Platform and never opens one.
  fn open_window(&mut self, ...) -> Result<Window, String>;
}

impl Window {
  // title, icon, cursor, lock, text input, ...
  fn gl_surface(&mut self) -> Result<GlSurface, String>; // Send, for the raster thread
  // After a context loss. Whether the window must go too is a driver
  // fact, so the renderer says it.
  fn recreate_gl(&mut self, recreate_window: bool) -> Result<GlSurface, String>;
}
```

alloy's loop is today's loop with `platform.wait(remaining)` in place of
`event_pump.wait_event_timeout_ms(ms)` and the `poll_iter` drain, and
`window.set_title(..)` in place of the SDL calls. Blocking on
the queue, the pushed wake and the send-then-wake ordering stay as they
are.

- Into the OS layer, all platform normalization: `translate_event`
  (`event.rs`), the pointer-lock coordinate freeze (SDL does not freeze
  x/y in relative mode), the touch side channel (`touch.rs`),
  `PlatformWatch`, gamepads, cursors, the Choreographer source (`vsync.rs`'s
  backend), window and GL setup (`setup_video`, `recover_gl`'s SDL half)
  and, inside the GL surface, the CVDisplayLink swap pacer
  (`display_link.rs`: the macOS swap waits on it; it is not a vsync
  source).
- Stays in alloy: `FrameRelease`, cadence, refresh counting, the idle
  tick, liveness, redraw, the input mute and the resampler, the stepped
  loop.
- Gone from alloy: every `sdl3::` type, and the `cfg(target_os =
  "android")` lines in the loop. The audio device hooks on background and
  foreground become the OS layer's own business, since the audio device
  lives there.

This is rule 5 of the platform-layering memory: alloy is facts-based,
policy is lattice's. The vsync source and `FrameRelease` are alloy's; the
pacing mode and the cadence-hold policy `FrameRelease` carries out are
lattice's choice.

The main-thread bridge comes with the pump, in the GUI host and in
headless `flux` alike (`ore::main_thread`, confirmed 2026-11-04 in the SDL
3.4.18 source and with a probe on Wayland, X11 and offscreen):

- Queuing a callback from another thread wakes a blocked wait: SDL sends
  the video driver's wakeup with a shown window, a lifecycle wake on
  Android, and with no window the wait polls every 1 ms.
- The callback runs inside the wait, 0.1 to 0.8 ms after it is queued,
  and the wait does not return for it: no event comes out. A bridge call
  answers through what its callback captures, never through a turn of the
  loop, so the loop needs no change.
- On the main thread, or while `SDL_INIT_EVENTS` was never initialized,
  the callback runs at once on the calling thread. Headless `flux` never
  initializes events and runs JS on its main thread, so a call from its
  JS runs directly; a call from an isolate thread there runs on that
  thread (fine for `SDL_OpenURL` on every platform).

## Positional audio

The worked example of a feature across the layers. Spatial audio already
splits along them:

- Scene half, alloy: `spatial/emitters.rs`. Listeners and emitters sit on
  spatial nodes; a pass at the end of `flush` reads their transforms and
  derives distance, lateral angle, doppler from the motion and occlusion
  against the culling occluders.
- Mixer, forge: today in `alloy/src/audio/` (voices, buses, the spatial
  layers, the low-pass, the resampler). It applies gain, pan, rate and the
  cutoff, ramped, and never sees a node or a transform. Portable code.
- Audio device, OS layer: opening the device, its callback that pulls
  from the mixer, the Android background and foreground restarts.
- Seams: `SpatialParams` per voice plus a ramp, written by
  `context/spatial.rs` through `set_audio_spatial`; and the device
  callback pulling rendered frames from the mixer. Moving them changes
  the crate each call reaches, not its shape.

Headless `flux` then plays sounds, buses and fades with no window, but has
no emitters or listeners: those are positions in a scene. A script that
wants panning sets it directly with `setPan`.

## Today

OS integration is spread over three crates:

- **alloy**: the SDL pump and window (above), the audio device and the
  mixer, the microphone, the camera, the power poll and the theme event
  (the queries are ore's), the activity calls of the dialogs, the video
  plane and the redraw answer (through `ore::android::with_activity`),
  `barcode.rs` (rxing, portable: forge), gamepads, dialogs. The clipboard
  moved to ore (okf/done/clipboard-image.md); power, the theme, thread
  placement and the Android JNI glue did too (okf/done/ore-first-moves.md).
- **forge**: content providers, process facts, subprocess, tty, the
  AMediaCodec decoder's JNI side, the WAV decoder in place of
  `SDL_LoadWAV`. `process::home_dir` delegates to `ore::folders::home_dir`
  (still a slice of `SDL_GetUserFolder` rebuilt from
  `HOME`/`USERPROFILE`).
- **lattice**: nothing of the storage roots: the pref path is
  `ore::folders::pref_dir`. One SDL hint for the GL library load on macOS
  (`gl_libs.rs`) stays until the window part moves.

A headless `flux` script has no per-app data directory, no locale, no
power info and cannot open a link.

## Test set

The platform features an app platform needs; every row is an OS layer
facility. Columns: whether it needs the window part; whether it needs
code running with no window (a cold start by the OS); whether `sol pack`
must add something (manifest, entitlement, Gradle dependency); what SDL
covers; the web name to borrow, if any.

| Feature | Window | Windowless | Packaging | SDL | Web shape |
|---|---|---|---|---|---|
| Open a link | no | no | no | `SDL_OpenURL` (main thread; Android via activity) | none |
| Deep links, macOS | no | no | Info.plist scheme | none | `registerProtocolHandler` |
| Clipboard image | yes | no | no | `SDL_SetClipboardData` | `navigator.clipboard` |
| Share sheet | no | no | FileProvider | none | `navigator.share` |
| Location | no | later (background location) | permission | none | `navigator.geolocation` |
| Gamepad haptics | no | no | no | `SDL_RumbleGamepad` | `GamepadHapticActuator` |
| Desktop widget window | yes | no | no | window flags | none |
| Locale | no | no | no | `SDL_GetPreferredLocales` | `navigator.languages` |
| Power and battery | no | no | no | `SDL_GetPowerInfo` | Battery Status |
| Vibration | no | no | Android permission | none (haptic is for devices) | `navigator.vibrate` |
| Wake lock | yes | no | Android permission | `SDL_DisableScreenSaver` | Screen Wake Lock |
| Local notifications | no | delivery yes | permission, receiver | none | Notifications API |
| Alarms (scheduled) | no | yes | exact-alarm permission, receiver | none | none (Notification Triggers dropped) |
| Push notifications | no | yes | Firebase Gradle setup, APNs entitlement | none | Push API |
| Background sync | no | yes | WorkManager / BGTaskScheduler | none | Background Sync |
| Secure storage | no | no | none | none | none |
| Printing | yes (dialog) | no | none | none | `window.print()` |
| Biometrics | yes (prompt) | no | permission | none | WebAuthn (partly) |
| In-app purchases | no | no (verification server side) | Play Billing dependency | none | Payment Request (partly) |
| Review prompt | yes | no | Play Core dependency | none | none |
| Media session | no | yes (background audio) | foreground service | none | Media Session API |
| Badge count | no | no | none | none | Badging API |
| App shortcuts | no | no | shortcuts XML | none | manifest `shortcuts` |

What the set says:

1. **SDL covers little of it.** The OS layer is SDL where it reaches and a
   large body of our own native code beside it, behind the same kind of
   contract. That is the layer's purpose, not a gap in the decision.
2. **Windowless execution is a run mode of the app, not a separate
   binary.** Push, alarms, background sync and media session start or keep
   the app's process without a window, on Android into a receiver, service
   or worker, not the activity. The app's JS must run there. OS facilities
   cannot be tied to the window, and the windowless host is a lattice
   concern beside the GUI, test and render hosts.
3. **Every feature with a packaging column touches `sol pack` and the
   Android project template** beside the Rust. The existing pattern (a
   `Flux.capabilities` entry, the camera capability adding the permission)
   extends to it.
4. **Desktop has no equivalent for several** (push, alarms without a
   running process). Each needs its desktop contract stated plainly, one
   behavior.

## Pieces the OS layer carries

- **Main-thread bridge**: one way to run a main-thread-only call from any
  thread: `SDL_RunOnMainThread`, processed by the pump (above).
- **Windowless run mode**: our own Java entry points (receiver, service,
  worker) beside `SolidRTActivity` that start the native side without the
  activity and publish ndk-context with the application context; a
  lattice host that runs the app's JS with no window. The vendored SDL
  files stay untouched.
- **Packaging half**: capabilities to manifest entries, entitlements and
  Gradle dependencies. A library goes into the stock runner, as everything
  does (the stock runtime carries every capability; a custom runtime is
  how a developer gets less). `sol pack` patches manifest entries into a
  copy of that runner and runs no Gradle, and a custom runtime swaps only
  `.so` files into it, so the Java side cannot change per app or per
  runtime (okf/backlog/android-per-app-libraries.md).
- **Permissions**: one model across camera, microphone, location,
  notifications and exact alarms; today `request_android_permission` in
  alloy serves the camera and microphone only.
- **Web shapes**: per feature, the standard name and the simplified
  contract (the solidrt lens), recorded in its backlog item.

## Alternatives rejected

- **A. forge holds all OS services and links SDL; alloy is the window.**
  One placement question, but SDL stays spread over two crates (alloy for
  the window, forge for the rest), and the OS boundary stays mixed with
  the portable libraries. The platform boundary is still not one place.
- **B. A crate for the window-free OS services only, beside alloy and
  forge.** Drawn this way, its line runs through forge's modules (fs except
  content providers, video except AMediaCodec) and leaves SDL in alloy for
  the window. D is B done right: the boundary includes the window and all
  of SDL.
- **C. Merge alloy and forge behind a `window` feature.** One placement
  question, enforced by the compiler, but one very large crate (iroh,
  sqlite and the codecs with Impeller and ANGLE), worse incremental builds,
  `cargo test -p` granularity lost, and still no single platform boundary.

## Migration

New work lands right first; existing code moves when a feature touches
it, not as a big-bang refactor.

1. **Landing zone.** Create the OS crate with the main-thread bridge and
   its first facilities, opening a link and locale. SDL init stays in
   alloy for now: the OS crate's first facilities are window-free SDL
   calls that work without alloy's context, and headless `flux` initializes
   what it needs on its main thread, which is also its JS thread.
2. **Rule.** Rewrite the placement rule (all six places are listed in
   okf/done/ore-landing-zone.md, work step 6): new
   work that reaches SDL or the OS goes into the OS crate; nothing new of
   that kind lands in alloy, forge or lattice.
3. **Moves, as features touch them.** The pump and window interface (the
   biggest single piece, mechanical, not a redesign), the audio device and
   camera (with the mixer to forge), the Android glue, the pref-path
   calls, forge's OS modules, `barcode.rs` to forge. The scheme
   registration and the single-instance hand-off (`links.rs`) moved with
   the link facility (okf/done/ore-landing-zone.md).
4. **Windowless run mode**, its own backlog item, before push, alarms or
   background sync.

## Open

- The Java libraries the platform features need (Firebase for push, Play
  Billing, WorkManager) ship in the stock runner; a custom runtime can
  neither drop nor add one, and per-app configuration (a Firebase
  project) has to reach stock code at run time:
  okf/backlog/android-per-app-libraries.md.
- The SDL main thread: confirm that the first `SDL_Init` fixes it (the
  bridge's wake and run are confirmed, see "The main loop").
- SDL init ownership once the window moves: the OS layer inits every
  subsystem, on the host's main thread.
- The windowless Android start: whether SDL's native library loads and
  initializes without `SDLActivity`, or the windowless host uses only our
  own JNI.
- iOS (a future target): APNs, BGTaskScheduler and entitlements fit the
  same pieces; not checked further.
- None of the test-set features has a backlog item except those already
  in `backlog/` (deep-links, share-sheet, location-module,
  gamepad-haptics, desktop-widget-window), opening a link, now part of
  okf/done/ore-landing-zone.md, and the clipboard image, now
  okf/done/clipboard-image.md (its Windows bitmap rest is
  okf/backlog/clipboard-image-windows-bitmap.md).
