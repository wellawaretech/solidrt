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
- `/dialog` and the `get_dialog`/`answer_dialog` tools have no automated test over the HTTP hop (the plugin and the harness are tested; the route was verified by hand on 2026-10-15): add one beside packages/test/tests/debug.test.tsx.


## Components

`packages/components`.

- Focus nav: pressed-state visuals on `select` activation; the focus ring is the only feedback today.
- `focusable` on the other press controls (Switch, Checkbox, Radio, ...); their activation already works through the nav action registry once declared.
- A pad's east button (B) as a cancel: consoles close menus and leave fields with it, but the only pad back trigger today is the back/select button (alloy's `take_back_edge`), and east is a game action on most pads, so it needs a decision (a `uiBindings` entry the app can drop, or nothing) rather than a default.


## 3d

`packages/3d`.

- A casting light whose `layers` share no bit with the scene's own `layers` casts nothing, silently: its shadow view draws the scene's meshes filtered by the light's mask (scene-shadows.ts makeView). Warn once per light at the sync that finds it, naming the light's and the scene's masks.
- `shaderMaterialClass` should throw when a source declares `uLayers` as anything but `int`: a `sampler2DArray uLayers` today fails at `add()` with "param 'uLayers' is a sampler2DArray; bind it via textures", although it was bound via textures (the scene seeds its layer mask under that name). Name it in the error at the class.
- Re-run `probes/scene-walk-whole-bench.tsx` on the desktop, the tablet and the TV with release clients after okf/done/spatial-flush-placement-batching.md, reading the raster occupancy from `/stats` beside the flush ms (the removed per-entry scan lived on the raster thread, so flush ms alone understates the change), and record the before/after under that done item's Findings.


## DX

- the dev server binds loopback only: on 2026-10-15 a tablet on the same Wi-Fi could not reach `sol run --port 34899` and `live.json` reported `address: 127.0.0.1` (earlier sessions recorded a LAN address there), so the device had to go through `adb reverse`; check whether LAN binding regressed or needs a flag, and make the Android dev loop notes say which.

The `sol` CLI, the dev server, MCP, debug commands, examples and probes.

- No control-API way to make a still scene draw N frames for a timing: a bench has to nudge a param through a debug command, and a nudge per debug call keeps a Mali governor at a low clock (the soft-shadows example's `frames({ count })` is the app-side form: an `onFrame` subscriber nudging a param, cancelled after the count). Add `/frames?count=n` (and the MCP tool) that demands n frames and answers when they presented, so `/stats?window=` covers exactly them and no example needs the command.
- `packages/cli/agents/debugging.md` has no protocol for GPU timings on an Android device: on the SM-T500 every kgsl clock node is permission-denied, the `gpu-usr` thermal zone is readable over adb, and the same build read 240 and 335 ms a frame minutes apart as it warmed from 33 to 40 C. Add the rule: cool below a threshold (or idle a fixed 100 s) before each row, and say which.
- `alloy/examples/draw_instanced.rs`, `alloy/examples/draw_ordered.rs` and `alloy/examples/text_layout_bench.rs` (untracked probes) no longer compile: the first two predate the pipeline vocabulary (`PipelineDesc.buffers` layouts for `attributes`/`instance_attributes`, `DrawSpec.buffers` for `buffer`/`instance_buffers`, the seven-argument instance-order call, `BufferUpdate`), the bench shapes through Impeller's `TypographyContext` where `WordCache::get_or_shape` now takes a `FontSet` and a `Fallback`; rewrite against the current API or drop them (`cargo check -p alloy --examples` lists the sites).
- The scaffold AGENTS.md "FIRST check whether a dev server ... is already running" line (`packages/cli/src/init/scaffold/AGENTS.md`) does not say how, so an agent greps `ps` and misses the server; name the check in the line: `list_clients` (its `key` is this project) or `~/.solidrt/servers/*/live.json`.
- `lattice/src/lib.rs:216` (the Android `start` call) warns on an unused `Result` in every Android build.
- Splat order gate: ORDER_DIRECTION_EPS_COS (~2 degrees, set by feel) was priced when a re-sort republished 28 MB at 1M; the indexed form republishes 4 MB, so measure a tighter gate (0.5-1 degree) on the Pixel 7 for the popping it removes against the re-sorts it adds (okf/done/splat-sort-spike.md: above SYNC_SORT_MAX_KEYS a re-sort now runs on the sort thread, so a tighter gate costs that thread's work and the order's lag, not frames).
- The test harness cannot assert a mount-time throw: an error at mount replaces the app with the error window and `app.mount` resolves to it, so the dev-validation policy (a contradicting prop throws) is unpinnable from an app test - `<Scene width height renderScale>` is one such case. Add an `app.mountFails(ui)` (or a rejecting `mount` option) that resolves to the error the window shows.
- The control API cannot resize the window, so the resize and display-scale paths (a fill target following its leaf, a post chain resizing before the paint) are only exercised by hand; add `/window?width=&height=` (and the MCP tool) so a probe can drive them.
- Nothing checks the code blocks of the website docs (`docs/**/*.md`): `docs/extensions/index.md`'s 2d snippet carried a `grid(cols, rows, size)` signature and a `createAtlas(bytes)` call that had been gone for weeks before the atlases change caught it by accident. Extract the fenced `tsx` blocks into a staged entry per page and run them through `sol check` in CI, so a snippet drifts no further than the API it shows.
- No scriptable cold start of a device client: a `/reload` is an engine swap and hides startup bugs (okf/done/android-cold-start-shadow-loss.md was found that way), and the only true cold start is `am start -S` by hand; add a `sol android --restart` (force-stop and relaunch the installed Player at the server) and the matching control endpoint so startup behaviour is testable from a script.
- Derived Players (`solidrt.runtime` projects) are re-signed with sol's development key, so switching between a gradle-built Player.dev and a derived one costs `adb uninstall` and the app's data every time; sign with the Android debug keystore (`~/.android/debug.keystore`) when it exists, so the checkout dev loop stays uninstall-free.
- tower-toppling demo (outside the repo): `publish()` in `src/index.tsx` still draws only the count the previous publish uploaded, the workaround from its FEEDBACK.md issue 1 that the sync-before-paint fix (2ebf397b) made unnecessary; drop it and the `uploaded` bookkeeping.

## Runtime

alloy, forge, flux, lattice.

- GPU context-loss recovery on macOS: the forced loss (`/gpu?lose=1`, ANGLE-Metal) with `probes/context-loss-probe.tsx` on the Mac builder, the window-recreation path there (okf/done/gpu-context-loss.md; Windows passed 2026-10-15).
- GPU context-loss recovery on the Android emulator: NOT possible as written (checked 2026-10-19): the emulator's GL is its own GLES translator, not ANGLE, with no `GL_CHROMIUM_lose_context` and no reset notification; the Android recreate path was verified on the SM-T500 instead (okf/backlog/splat-adreno-gpu-fault.md). A real Mali device with a forced loss is the open Android check.
- forge::video: a hardware decoder error mid-stream that is not a lost plane surface (seen on the SM-T500 2 s after a GPU reset: OMX error -62, then the codec unusable) ends the stream as a plain end with no error to the app (plane.rs `next`: "the owner learns the cause from the plane itself", which here was not lost). Report it as a decode error, and consider reopening the codec on the same surface and resuming at the current time.
- Windows (winbox): rerun probes/context-loss-probe.tsx's `hang` three times within a minute to see the guilty-reset cap end the process with its message, and once to see `guilty: true` on `onContextRestored`; also whether ANGLE raises `GL_CONTEXT_LOST` for the new per-frame error poll. Only where a display driver reset is acceptable on that box.
- text on the TV, two reads owed: the cold-style first paint under the complete-frame rule (okf/done/text-complete-frames.md) with `probes/text-prose-bench.tsx` (`c`, or POST /debug?name=cold; it was 83 ms with three cells a frame made in time and the rest arriving over a few frames, now the frame makes them all, so the number is the whole stall and what `warmText` saves), and the focus-zoom idiom under the raster hysteresis (okf/done/text-layer-motion.md) with `probes/text-motion-probe.tsx`: `window.textLayers` over `/stats?frames=12` per zoom, and the frame time of the one raster at the zoom's start.
- images rasterized from SVG at a size (`parseSvg` to a texture, an icon) under a scale animation: they rasterize at their declared size and are resampled under any scale on the chain, the same problem text layers and snapshots had; the raster density should follow the grid at rest and ahead of a known zoom's end with the same rule (`text::raster_density`, `BuildContext::scale_target`), once the image's raster is keyed on a density.
- packed and OTA apps compile their `.wasm` assets at the first `new Module` instead of at install, with no `INSTALLING` badge: the pre-warm in `lattice/src/go/store.rs` runs from `install_at`, which only a dev push reaches until client-storage stage 4 (okf/plans/client-storage-updates.md) seeds the factory version into the store; call the same pre-warm from that seeding.
- Android touch device list: fingers no longer pass through SDL's `Android_OnTouch`, which registered a touch device on its first touch, so a touchscreen attached after start is missing from `num_touch_devices` and the `touch` capability stays false; have `alloy/src/touch.rs` mark touch present on the first sample and re-emit `InputDevices`.
- video plane audio: `AUDIO_OUTPUT_LATENCY_US` (forge/src/video/audio.rs, 60 ms for the Philips TV's speakers) is a build constant; an external speaker path (HDMI, Bluetooth) needs its own value, so expose it as an `audioDelay` seconds option on both players (default the constant) for an app settings screen, set by the flash-and-beep clip examples/video/assets/avsync.webm.
- video texture first frame: the first frame after `play` (and after a seek or buffering) is due at once, so it has no lead and is always latched behind the draw gate's peek; `videoLateLatches` therefore reads one per start on every platform, which hides a real late latch among starts. Either anchor a texture player's first frame one lead ahead (the plane must not change), or have the latch not count the first take after `set_playing(true)`.
- video texture nobody shows: a texture player whose output id no live node samples (a preloaded clip, the pacing probe's `openSecond`) is still latched and uploaded every frame it plays; gate the raster take on the id being referenced (the destroy sweep already knows `referenced_texture_ids`), so an unshown player costs decode only.
- video in headless playback with audio: the PCM sink plays in real time while the capture steps the clock, so a capture with sound plays its audio out of step with the frames; the sink could be silenced in playback (the track is still fed and clocked) until a capture that records sound exists.
- examples/screencast's `Player` reads the player's `finished()` and `buffering()` in JSX; both are plain reads, not signals, so its status text goes stale ("live" after the cast ended). Poll them into signals as examples/call does.
- video plane across a background trip: backgrounding destroys the plane's surface, so the player reports `finished` (`finished_impl`, flux/src/alloy_plugins/video.rs) and an app reopening on resume starts the clip over; keep the position across the trip, e.g. `currentTime` staying readable after the loss so the reopened player can seek to it.
- macOS swap pacer: the display link (alloy/src/display_link.rs) is created over the active displays and follows the main one, so a window on a second display with a different refresh rate is paced at the main display's; set the link's current display from the window's `NSScreen` on window move (`CVDisplayLinkSetCurrentCGDisplay`, needs the objc2-core-graphics feature), and stop the link while no frame is presented (it runs for the binding's life today, as SDL's does).
- packed Android apps: the camera capability (on by default) makes `sol pack` add `android.permission.CAMERA`, and with no camera `<uses-feature>` in the runner Play infers the camera as required (`aapt2 dump badging` prints `uses-feature: name='android.hardware.camera'`), filtering the app off every device without one, TVs included; move the go overlay's `required="false"` camera declaration into `lattice/android/app/src/main/AndroidManifest.xml`, with `android.hardware.camera.autofocus` beside it (Play's docs list it as implied too).
- forge/build.rs `link_android_builtins`: existed for libffi's `__clear_cache`; with libffi gone, build the Android client without it and keep it only if the vendored opus or Basis Universal code leaves a compiler-rt builtin undefined (check with `nm -u` on the cdylib).
- runtime size: measure the `release-opt` runtime before and after the flux:ffi removal (libffi-sys and its vendored C build gone from every target, okf/done/ffi-module-removal.md) and record the delta in that done record.
- Scaffolded runtime in CI: a job on the native test matrix that runs `sol init --runtime` into a temp app, points the lattice git dependency at the checkout under test as written (a `file://` URL with `rev`, not a path dependency: only a git dependency makes cargo parse every Cargo.toml in the repository, which is how the `{{crate}}` template broke v0.0.73), builds `make -C runtime client` and runs its `tests/runtime.test.tsx` with `sol test`; the one proof that the scaffold builds as shipped (a full lattice build per platform, hence a job of its own).
- Custom runtime, the unrun legs: `make android-runtime` in the checkout, then `sol pack --apk` of the tower-toppling demo installed and run on the tablet (the derived runner was built but never run: no staged runner, its base was the published 0.0.67), and the 2d screen (`src/tower2d.tsx`, `flux:physics2d`) run on the tablet at all (okf/done/runtime-extension-modules.md step 4 covers the 3d screen on the derived Player only).
- Run the SDL finger-cancel path (the `FingerCanceled` arm of `translate_event` in `alloy/src/event.rs`, Wayland's `wl_touch.cancel`) on a touch screen: drag in `probes/pointer-cancel-probe.tsx`, pull the notification shade; the log must show `cancel`, never `up`. Verified on Android only (okf/done/touch-cancel.md).
- alloy/src/barcode.rs (QR decode over a pixel buffer, no SDL) belongs in forge under the alloy/forge placement rule (CLAUDE.md, Projects); move it with its tests/qr_decode.rs and point the camera scanner and the image scan API at forge.
- camera open polling: the open task and `watch_scan` (flux/src/alloy_plugins/camera.rs) sleep `OPEN_POLL_MS` in a loop until `pump_camera` leaves Pending; have alloy's camera wake a waiter instead (the init thread finishing, SDL's camera approved/denied events on the main loop in place of the `camera_permission` poll, the tap's first frame) and block on that wait from `spawn_blocking` in the plugin, keeping tokio out of alloy.
