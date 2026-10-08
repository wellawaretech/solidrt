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


## DX

The `sol` CLI, the dev server, MCP, debug commands, examples and probes.

- MCP queries issued right after `load`/`reload` (`get_snapshot`, `get_render_tree`, `call_debug`) time out with "client is connected but did not answer" and succeed on retry: wait for the first frame after a push, or answer "app still loading".
- Slow-frame warnings arrive about once a second during continuous animation and bury the one real warning in `get_logs` at warn: collapse them into a running summary, or file them under their own tag.
- `get_stats` top-level fields describe the latest frame, so an app that just went idle reads "0.09 ms JS" beside a busy window summary: add a `latestFrameAgeMs` field, or put the window summary first.
- `make client` regenerates `lattice/resources/player/index.sol.js` (a tracked bundle) and dirties it by a few lines on every build; either build it deterministically or stop tracking it, so a client build leaves no change to commit.
- `examples/video/src/probe/render.tsx`'s header says the assets mount is empty under `sol render`; the render command has copied the project's `assets/` into its staged dir since the build-output-dirs work, so the argv workaround and the comment are stale.
- `alloy/examples/depth_texture.rs` no longer compiles (`DrawSpec` has no field `buffer`, it is `buffers`), so `cargo check -p alloy --examples` fails on it; fix or drop the example.
- `lattice/src/lib.rs:216` (the Android `start` call) warns on an unused `Result` in every Android build.
- Write the multi-pass shader chain example in `packages/core/examples` (a plasma target bound as a cube pipeline's sampler input, only the plasma's uniforms driven): the demonstration that sampler bindings are live dependencies, unblocked since target dependency propagation (okf/done/gpu-example-gaps.md).
- `sol android`: pass an `--env NAME=value` (repeatable) through to the go client's `sol_env` intent extra (lattice/android MainActivity, 2026-09-27) so a measurement can pin `SOLIDRT_CADENCE_HOLD` without a hand-typed `am start`; the phone bench protocol in okf/done/gaussian-splats.md (stage D findings) is the consumer.
- Splat order gate: ORDER_DIRECTION_EPS_COS (~2 degrees, set by feel) was priced when a re-sort republished 28 MB at 1M; the indexed form republishes 4 MB, so measure a tighter gate (0.5-1 degree) on the Pixel 7 for the popping it removes against the re-sorts it adds (okf/backlog/splat-sort-spike.md measures the sort itself).
- `flux:test`'s `toBeCloseTo` takes a scalar, so the app tests that compare a point, a normal or a motion (`packages/{2d,3d}/tests/collision.test.tsx`, `3d/tests/raycast.test.tsx`) each carry a `rounded()` helper and compare through `toEqual`: let `toBeCloseTo` take an array and print both vectors on failure, then drop the helpers.
- CI compiles the `ktx2` feature nowhere (the release build alone does; `test-js` builds `flux` with `KTX2=0`), so a break in forge's texture encoder or its flux marshalling reaches main unnoticed: add `flux/ktx2` to the `cargo check` step of the `check` job in `.github/workflows/ci.yml`.
- Nothing checks the code blocks of the website docs (`docs/**/*.md`): `docs/extensions/index.md`'s 2d snippet carried a `grid(cols, rows, size)` signature and a `createAtlas(bytes)` call that had been gone for weeks before the atlases change caught it by accident. Extract the fenced `tsx` blocks into a staged entry per page and run them through `sol check` in CI, so a snippet drifts no further than the API it shows.
- `sol test` and `sol pack` in the checkout need `SOLIDRT_HOME=$PWD` to find the staged `solidrt-go`/`fluxc` ("Could not find solidrt-go binary" with dist/linux-x64-gnu staged) while `sol run` finds the client without it: make `requireBinary` resolve the checkout's dist/ the way the run path does.
- `/shutdown` acks before the server's teardown finishes (its `live.json` is still there), so a `sol run` issued right after the ack fails with "A dev server already serves ... on port N"; ack after the record is removed, or have the new server wait out a dying one on its port.
- Control queries on a slow device (the SM-T500) time out with "client is connected but did not answer" right after a `/settle` and on every raw-format `/snapshot` of a 2000x1092 window, while the same call succeeds on retry; give the query timeout room for slow devices, or answer with "busy, retry" instead of the wedge diagnosis.
- No scriptable cold start of a device client: a `/reload` is an engine swap and hides startup bugs (okf/done/android-cold-start-shadow-loss.md was found that way), and the only true cold start is `am start -S` by hand; add a `sol android --restart` (force-stop and relaunch the installed Player at the server) and the matching control endpoint so startup behaviour is testable from a script.
- Derived Players (`solidrt.runtime` projects) are re-signed with sol's development key, so switching between a gradle-built Player.dev and a derived one costs `adb uninstall` and the app's data every time; sign with the Android debug keystore (`~/.android/debug.keystore`) when it exists, so the checkout dev loop stays uninstall-free.
- tower-toppling demo (outside the repo): `publish()` in `src/index.tsx` still draws only the count the previous publish uploaded, the workaround from its FEEDBACK.md issue 1 that the sync-before-paint fix (2ebf397b) made unnecessary; drop it and the `uploaded` bookkeeping.

## Runtime

alloy, forge, flux, lattice.

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
- alloy layout: `set_unrounded_layout` (alloy/src/rendertree/layout/context.rs) calls the plain `invalidate_paint` per changed node, so a resize invalidates O(n * depth); thread one `visited` set through the layout pass and call `invalidate_paint_batched` instead (okf/done/content-damage-perf.md).
- Run the SDL finger-cancel path (`sdl_utils::finger_cancel`, Wayland's `wl_touch.cancel`) on a touch screen: drag in `probes/pointer-cancel-probe.tsx`, pull the notification shade; the log must show `cancel`, never `up`. Verified on Android only (okf/done/touch-cancel.md).
- The record stream is now the same algorithm in `packages/2d/src/records.ts` and the stream half of `packages/3d/src/mesh.ts` (a mirror, a dirty record range, one `writeBuffer` per range or the lease whole under an instance order, growth by replacement buffers): lift it into `@solidrt/core/gpu` as one `createRecordStream` both call - a mechanical move now that the two agree line for line (okf/done/2d-3d-record-layers-one-model.md).
- The 2d node layer's style records (`layer.ts`: `styleData`, a boolean `styleDirty`, a whole-prefix lease publish over the high-water mark) are an instanced mesh's style stream one dimension down; give them `records(spriteLayer)`/`updateRecords` with a dirty range like the records layer has, so a bulk restyle (a palette cycle over thousands of sprites) is one loop and one range write, and the 3d `records(instancedMesh)` pairing is complete.
- Text atlas texel format: mask cells are stored rgba8 like the distance-field kind (`BYTES_PER_TEXEL`, alloy/src/rendertree/text/glyphs/cells.rs), four bytes for one channel; a start screen's warm-ups put the atlas at the 2048 square, 16 MB, one doubling from 64 MB. Give the mask kind an R8 texture (GLES 3.0 has it) and have the glyph pass read `.r` for it, the msdf kind staying rgba8.
