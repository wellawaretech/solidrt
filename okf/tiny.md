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

- `/dialog` and the `get_dialog`/`answer_dialog` tools have no automated test over the HTTP hop (the plugin and the harness are tested; the route was verified by hand on 2026-10-15): add one beside packages/test/tests/debug.test.tsx.

## Components

`packages/components`.

- A pad's east button (B) as a cancel: consoles close menus and leave fields with it, but the only pad back trigger today is the back/select button (alloy's `take_back_edge`), and east is a game action on most pads, so it needs a decision (a `uiBindings` entry the app can drop, or nothing) rather than a default.


## 3d

`packages/3d`.

- Re-run `probes/scene-walk-whole-bench.tsx` on the desktop, the tablet and the TV with release clients after okf/done/spatial-flush-placement-batching.md, reading the raster occupancy from `/stats` beside the flush ms (the removed per-entry scan lived on the raster thread, so flush ms alone understates the change), and record the before/after under that done item's Findings.


## DX

The `sol` CLI, the dev server, MCP, debug commands, examples and probes.

- No control-API way to make a still scene draw N frames for a timing: a bench has to nudge a param through a debug command, and a nudge per debug call keeps a Mali governor at a low clock (the soft-shadows example's `frames({ count })` is the app-side form: an `onFrame` subscriber nudging a param, cancelled after the count). Add `/frames?count=n` (and the MCP tool) that demands n frames and answers when they presented, so `/stats?window=` covers exactly them and no example needs the command.
- `alloy/examples/draw_instanced.rs`, `alloy/examples/draw_ordered.rs` and `alloy/examples/text_layout_bench.rs` (untracked probes) no longer compile: the first two predate the pipeline vocabulary (`PipelineDesc.buffers` layouts for `attributes`/`instance_attributes`, `DrawSpec.buffers` for `buffer`/`instance_buffers`, the seven-argument instance-order call, `BufferUpdate`), the bench shapes through Impeller's `TypographyContext` where `WordCache::get_or_shape` now takes a `FontSet` and a `Fallback`; rewrite against the current API or drop them (`cargo check -p alloy --examples` lists the sites).
- Splat order gate: ORDER_DIRECTION_EPS_COS (~2 degrees, set by feel) was priced when a re-sort republished 28 MB at 1M; the indexed form republishes 4 MB, so measure a tighter gate (0.5-1 degree) on the Pixel 7 for the popping it removes against the re-sorts it adds (okf/done/splat-sort-spike.md: above SYNC_SORT_MAX_KEYS a re-sort now runs on the sort thread, so a tighter gate costs that thread's work and the order's lag, not frames).

## Runtime

alloy, forge, flux, lattice.

- GPU context-loss recovery on macOS: the forced loss (`/gpu?lose=1`, ANGLE-Metal) with `probes/context-loss-probe.tsx` on the Mac builder, the window-recreation path there (okf/done/gpu-context-loss.md; Windows passed 2026-10-15).
- GPU context-loss recovery on the Android emulator: NOT possible as written (checked 2026-10-19): the emulator's GL is its own GLES translator, not ANGLE, with no `GL_CHROMIUM_lose_context` and no reset notification; the Android recreate path was verified on the SM-T500 instead (okf/backlog/splat-adreno-gpu-fault.md). A real Mali device with a forced loss is the open Android check.
- Windows (winbox): rerun probes/context-loss-probe.tsx's `hang` three times within a minute to see the guilty-reset cap end the process with its message, and once to see `guilty: true` on `onContextRestored`; also whether ANGLE raises `GL_CONTEXT_LOST` for the new per-frame error poll. Only where a display driver reset is acceptable on that box.
- text on the TV, two reads owed: the cold-style first paint under the complete-frame rule (okf/done/text-complete-frames.md) with `probes/text-prose-bench.tsx` (`c`, or POST /debug?name=cold; it was 83 ms with three cells a frame made in time and the rest arriving over a few frames, now the frame makes them all, so the number is the whole stall and what `warmText` saves), and the focus-zoom idiom under the raster hysteresis (okf/done/text-layer-motion.md) with `probes/text-motion-probe.tsx`: `window.textLayers` over `/stats?frames=12` per zoom, and the frame time of the one raster at the zoom's start.
- images rasterized from SVG at a size (`parseSvg` to a texture, an icon) under a scale animation: they rasterize at their declared size and are resampled under any scale on the chain, the same problem text layers and snapshots had; the raster density should follow the grid at rest and ahead of a known zoom's end with the same rule (`text::raster_density`, `BuildContext::scale_target`), once the image's raster is keyed on a density.
- packed and OTA apps compile their `.wasm` assets at the first `new Module` instead of at install, with no `INSTALLING` badge: the pre-warm in `lattice/src/go/store.rs` runs from `install_at`, which only a dev push reaches until client-storage stage 4 (okf/plans/client-storage-updates.md) seeds the factory version into the store; call the same pre-warm from that seeding.
- video texture first frame: the first frame after `play` (and after a seek or buffering) is due at once, so it has no lead and is always latched behind the draw gate's peek; `videoLateLatches` therefore reads one per start on every platform, which hides a real late latch among starts. Either anchor a texture player's first frame one lead ahead (the plane must not change), or have the latch not count the first take after `set_playing(true)`.
- video texture nobody shows, the frame demand: an unshown texture player now parks its frames instead of uploading them, but while it plays it still demands a frame every refresh (`YuvFrameSink::push` and `note_due_video`/`streaming_textures` in alloy/src/context/texture.rs, fed into the gate by flux/src/alloy_plugins/frame.rs), because its take, which advances `currentTime` and frees room for a stepped producer, runs only inside a raster `Frame`; drop that demand for an unshown player once its take can run outside the frame path.
- examples/screencast's `Player` reads the player's `finished()` and `buffering()` in JSX; both are plain reads, not signals, so its status text goes stale ("live" after the cast ended). Poll them into signals as examples/call does.
- examples/call's redial after a network drop was never read on a device (okf/done/video-encode.md): the dev client reloads the app when it reconnects to the dev server, which resets the guest before its redial shows; read it with a packed app (`sol pack --apk`) on the guest, Wi-Fi off and on, and the far picture back on a keyframe.
- video on the TV, the reads owed by okf/done/video-texture-off-frame-loop.md and okf/done/video-streaming.md: `videoSkipped` 0 over 60 s of clip720 through a texture, the 20 and 40 ms cadences, the plane's census, lip sync by ear on avsync.webm at 720p through a texture, close during a stall well under the 1 s it held before the join went (unmeasured there since), and memory under `STREAM_MAX_BUFFER_BYTES` at a high bitrate (on the tablet too).
- video plane across a background trip: backgrounding destroys the plane's surface, so the player reports `finished` (`finished_impl`, flux/src/alloy_plugins/video.rs) and an app reopening on resume starts the clip over; keep the position across the trip, e.g. `currentTime` staying readable after the loss so the reopened player can seek to it.
- macOS swap pacer: the display link (alloy/src/display_link.rs) is created over the active displays and follows the main one, so a window on a second display with a different refresh rate is paced at the main display's; set the link's current display from the window's `NSScreen` on window move (`CVDisplayLinkSetCurrentCGDisplay`, needs the objc2-core-graphics feature), and stop the link while no frame is presented (it runs for the binding's life today, as SDL's does).
- runtime size: measure the `release-opt` runtime before and after the flux:ffi removal (libffi-sys and its vendored C build gone from every target, okf/done/ffi-module-removal.md) and record the delta in that done record.
- Custom runtime, the unrun legs: `make android-runtime` in the checkout, then `sol pack --apk` of an app on a custom runtime installed and run on the tablet (the derived runner was built but never run: no staged runner, its base was the published 0.0.67), and a 2d screen over a custom module run on the tablet at all (okf/done/runtime-extension-modules.md step 4 covers a 3d screen on the derived Player only).
- Real-finger rest before a lift below the refresh rate: on the Pixel 7 and the SM-T500, hold `probes/timestamp-probe.tsx` near 15 fps (its `busy` debug command), drag, rest about 80 ms and lift; the lift must read zero velocity (rest measured on the stamps, not under `VELOCITY_REST_MS`), as it does with synthetic input (okf/done/event-timestamp.md).
- Run the SDL finger-cancel path (the `FingerCanceled` arm of `translate_event` in `alloy/src/event.rs`, Wayland's `wl_touch.cancel`) on a touch screen: drag in `probes/pointer-cancel-probe.tsx`, pull the notification shade; the log must show `cancel`, never `up`. Verified on Android only (okf/done/touch-cancel.md).
- alloy/src/barcode.rs (QR decode over a pixel buffer, no SDL) belongs in forge under the alloy/forge placement rule (CLAUDE.md, Projects); move it with its tests/qr_decode.rs and point the camera scanner and the image scan API at forge.
- camera open polling: the open task and `watch_scan` (flux/src/alloy_plugins/camera.rs) sleep `OPEN_POLL_MS` in a loop until `pump_camera` leaves Pending; have alloy's camera wake a waiter instead (the init thread finishing, SDL's camera approved/denied events on the main loop in place of the `camera_permission` poll, the tap's first frame) and block on that wait from `spawn_blocking` in the plugin, keeping tokio out of alloy.
