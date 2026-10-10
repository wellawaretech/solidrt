# Tiny

One line each, grouped by area. No frontmatter, no ceremony. Work small enough
that nothing has to be decided: the fix is obvious and it just needs someone
who is already in that file. Delete the line when it is done - a tiny item
carries no reasoning worth keeping, so it leaves no `done/` record.

If you need more than one line, or need to explain why, it is not tiny.
Something that needs a decision, a design or an investigation is an idea
([ideas.md](ideas.md)) or a `backlog/` file.

The headings are areas because these get picked up opportunistically, when you
are in that code anyway. File an item where the work happens, not where the
symptom shows. A heading that outgrows this file splits into its own.

## Core

`packages/core` - the renderer and the reactivity surface.

- Layout slide per-axis motion (Reanimated's curved and sequenced presets): `x`/`y` sub-motions on the `layout` entry, additive on okf/done/transition-layout-animations.md.
- Shared-element layout transitions: a `layoutId` key on the `layout` entry; a node mounting with the id of a node exiting this tick inherits its last box, which the exit ghost already knows (Framer `layoutId`, SwiftUI `matchedGeometryEffect`). On the box lane (okf/done/transition-layout-size.md).

## Flux

`flux/` - the JavaScript runtime and its plugins.

- video volume and mute: neither `flux:video` player exposes any, while their PCM sink is a plain SDL3 audio stream (`PcmStream`, alloy/src/audio.rs) that has a gain; add `volume` and `muted` with the web names over `SDL_SetAudioStreamGain` (flux/src/alloy_plugins/video.rs, flux-types gui/video.d.ts).


## Components

`packages/components`.

- Focus nav: scroll a focused off-screen candidate into view (the nav has the candidate's box; the enclosing ScrollView is the target).
- Focus nav: pressed-state visuals on `select` activation; the focus ring is the only feedback today.
- `focusable` on the other press controls (Switch, Checkbox, Radio, ...); their activation already works through the nav action registry once declared.


## 3d

`packages/3d`.

- Mixer per-clip speed after `play()` (Three's `action.timeScale`): the core takes `setPlayer(player, { speed })` already; the mixer only reads `speed` at play. A `mixer.setSpeed(name, speed)` over the action's player. Pause is covered by the model node's `timeScale` (okf/done/native-motion-time-scale.md).


## DX

The `sol` CLI, the dev server, MCP, debug commands, examples and probes.

- `alloy/examples/draw_instanced.rs`, `alloy/examples/draw_ordered.rs` and `alloy/examples/text_layout_bench.rs` (untracked probes) no longer compile: the first two predate the pipeline vocabulary (`PipelineDesc.buffers` layouts for `attributes`/`instance_attributes`, `DrawSpec.buffers` for `buffer`/`instance_buffers`, the seven-argument instance-order call, `BufferUpdate`), the bench shapes through Impeller's `TypographyContext` where `WordCache::get_or_shape` now takes a `FontSet` and a `Fallback`; rewrite against the current API or drop them (`cargo check -p alloy --examples` lists the sites).
- `lattice/src/lib.rs:216` (the Android `start` call) warns on an unused `Result` in every Android build.
- `sol android`: pass an `--env NAME=value` (repeatable) through to the go client's `sol_env` intent extra (lattice/android MainActivity, 2026-09-27) so a measurement can pin `SOLIDRT_CADENCE_HOLD` without a hand-typed `am start`; the phone bench protocol in okf/done/gaussian-splats.md (stage D findings) is the consumer.
- Splat order gate: ORDER_DIRECTION_EPS_COS (~2 degrees, set by feel) was priced when a re-sort republished 28 MB at 1M; the indexed form republishes 4 MB, so measure a tighter gate (0.5-1 degree) on the Pixel 7 for the popping it removes against the re-sorts it adds (okf/backlog/splat-sort-spike.md measures the sort itself).
- Nothing checks the code blocks of the website docs (`docs/**/*.md`): `docs/extensions/index.md`'s 2d snippet carried a `grid(cols, rows, size)` signature and a `createAtlas(bytes)` call that had been gone for weeks before the atlases change caught it by accident. Extract the fenced `tsx` blocks into a staged entry per page and run them through `sol check` in CI, so a snippet drifts no further than the API it shows.
- No scriptable cold start of a device client: a `/reload` is an engine swap and hides startup bugs (okf/done/android-cold-start-shadow-loss.md was found that way), and the only true cold start is `am start -S` by hand; add a `sol android --restart` (force-stop and relaunch the installed Player at the server) and the matching control endpoint so startup behaviour is testable from a script.
- Derived Players (`solidrt.runtime` projects) are re-signed with sol's development key, so switching between a gradle-built Player.dev and a derived one costs `adb uninstall` and the app's data every time; sign with the Android debug keystore (`~/.android/debug.keystore`) when it exists, so the checkout dev loop stays uninstall-free.
- tower-toppling demo (outside the repo): `publish()` in `src/index.tsx` still draws only the count the previous publish uploaded, the workaround from its FEEDBACK.md issue 1 that the sync-before-paint fix (2ebf397b) made unnecessary; drop it and the `uploaded` bookkeeping.

## Runtime

alloy, forge, flux, lattice.

- text on the TV, two reads owed: the cold-style first paint under the complete-frame rule (okf/done/text-complete-frames.md) with `probes/text-prose-bench.tsx` (`c`, or POST /debug?name=cold; it was 83 ms with three cells a frame made in time and the rest arriving over a few frames, now the frame makes them all, so the number is the whole stall and what `warmText` saves), and the focus-zoom idiom under the raster hysteresis (okf/done/text-layer-motion.md) with `probes/text-motion-probe.tsx`: `window.textLayers` over `/stats?frames=12` per zoom, and the frame time of the one raster at the zoom's start.
- images rasterized from SVG at a size (`parseSvg` to a texture, an icon) under a scale animation: they rasterize at their declared size and are resampled under any scale on the chain, the same problem text layers and snapshots had; the raster density should follow the grid at rest and ahead of a known zoom's end with the same rule (`text::raster_density`, `BuildContext::scale_target`), once the image's raster is keyed on a density.
- packed and OTA apps compile their `.wasm` assets at the first `new Module` instead of at install, with no `INSTALLING` badge: the pre-warm in `lattice/src/go/store.rs` runs from `install_at`, which only a dev push reaches until client-storage stage 4 (okf/plans/client-storage-updates.md) seeds the factory version into the store; call the same pre-warm from that seeding.
- Android touch device list: fingers no longer pass through SDL's `Android_OnTouch`, which registered a touch device on its first touch, so a touchscreen attached after start is missing from `num_touch_devices` and the `touch` capability stays false; have `alloy/src/touch.rs` mark touch present on the first sample and re-emit `InputDevices`.
- video plane audio: `AUDIO_OUTPUT_LATENCY_US` (forge/src/video/audio.rs, 60 ms for the Philips TV's speakers) is a build constant; an external speaker path (HDMI, Bluetooth) needs its own value, so expose it as an `audioDelay` seconds option on both players (default the constant) for an app settings screen, set by the flash-and-beep clip examples/video/assets/avsync.webm.
- video texture first frame: the first frame after `play` (and after a seek or buffering) is due at once, so it has no lead and is always latched behind the draw gate's peek; `videoLateLatches` therefore reads one per start on every platform, which hides a real late latch among starts. Either anchor a texture player's first frame one lead ahead (the plane must not change), or have the latch not count the first take after `set_playing(true)`.
- video texture nobody shows: a texture player whose output id no live node samples (a preloaded clip, the pacing probe's `openSecond`) is still latched and uploaded every frame it plays; gate the raster take on the id being referenced (the destroy sweep already knows `referenced_texture_ids`), so an unshown player costs decode only.
- video in headless playback: a stepped take that never settles (no frame due after the deadline, the producer not pushing) waits forever with no message; the first render of the latch hung silently for that reason. Log once after a bounded wait (a few seconds of wall time) naming the texture and the deadline, so a wedge is a log line, not a hang to bisect.
- video in headless playback with audio: the PCM sink plays in real time while the capture steps the clock, so a capture with sound plays its audio out of step with the frames; the sink could be silenced in playback (the track is still fed and clocked) until a capture that records sound exists.
- video plane across a background trip: backgrounding destroys the plane's surface, so the player reports `finished` (`finished_impl`, flux/src/alloy_plugins/video.rs) and an app reopening on resume starts the clip over; keep the position across the trip, e.g. `currentTime` staying readable after the loss so the reopened player can seek to it.
- macOS swap pacer: the display link (alloy/src/display_link.rs) is created over the active displays and follows the main one, so a window on a second display with a different refresh rate is paced at the main display's; set the link's current display from the window's `NSScreen` on window move (`CVDisplayLinkSetCurrentCGDisplay`, needs the objc2-core-graphics feature), and stop the link while no frame is presented (it runs for the binding's life today, as SDL's does).
- packed Android apps: the camera capability (on by default) makes `sol pack` add `android.permission.CAMERA`, and with no camera `<uses-feature>` in the runner Play infers the camera as required (`aapt2 dump badging` prints `uses-feature: name='android.hardware.camera'`), filtering the app off every device without one, TVs included; move the go overlay's `required="false"` camera declaration into `lattice/android/app/src/main/AndroidManifest.xml`, with `android.hardware.camera.autofocus` beside it (Play's docs list it as implied too).
- forge/build.rs `link_android_builtins`: existed for libffi's `__clear_cache`; with libffi gone, build the Android client without it and keep it only if the vendored opus or Basis Universal code leaves a compiler-rt builtin undefined (check with `nm -u` on the cdylib).
- runtime size: measure the `release-opt` runtime before and after the flux:ffi removal (libffi-sys and its vendored C build gone from every target, okf/done/ffi-module-removal.md) and record the delta in that done record.
- Custom runtime, the unrun legs: `make android-runtime` in the checkout, then `sol pack --apk` of the tower-toppling demo installed and run on the tablet (the derived runner was built but never run: no staged runner, its base was the published 0.0.67), and the 2d screen (`src/tower2d.tsx`, `flux:physics2d`) run on the tablet at all (okf/done/runtime-extension-modules.md step 4 covers the 3d screen on the derived Player only).
- Run the SDL finger-cancel path (`sdl_utils::finger_cancel`, Wayland's `wl_touch.cancel`) on a touch screen: drag in `probes/pointer-cancel-probe.tsx`, pull the notification shade; the log must show `cancel`, never `up`. Verified on Android only (okf/done/touch-cancel.md).
