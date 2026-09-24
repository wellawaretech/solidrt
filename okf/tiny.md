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


## Components

`packages/components`.

- Focus nav: scroll a focused off-screen candidate into view (the nav has the candidate's box; the enclosing ScrollView is the target).
- Focus nav: pressed-state visuals on `select` activation; the focus ring is the only feedback today.
- `focusable` on the other press controls (Switch, Checkbox, Radio, ...); their activation already works through the nav action registry once declared.


## DX

The `srt` CLI, the dev server, MCP, debug commands, examples and probes.

- `make client` regenerates `lattice/resources/player/index.srt.js` (a tracked bundle) and dirties it by a few lines on every build; either build it deterministically or stop tracking it, so a client build leaves no change to commit.
- `examples/video/src/probe/render.tsx`'s header says the assets mount is empty under `srt render`; the render command has copied the project's `assets/` into its staged dir since the build-output-dirs work, so the argv workaround and the comment are stale.
- `alloy/examples/depth_texture.rs` no longer compiles (`DrawSpec` has no field `buffer`, it is `buffers`), so `cargo check -p alloy --examples` fails on it; fix or drop the example.
- `lattice/src/lib.rs:216` (the Android `start` call) warns on an unused `Result` in every Android build.
- Write the multi-pass shader chain example in `packages/core/examples` (a plasma target bound as a cube pipeline's sampler input, only the plasma's uniforms driven): the demonstration that sampler bindings are live dependencies, unblocked since target dependency propagation (okf/done/gpu-example-gaps.md).


## Runtime

alloy, forge, flux, lattice.

- video plane audio: `AUDIO_OUTPUT_LATENCY_US` (forge/src/video/audio.rs, 60 ms for the Philips TV's speakers) is a build constant; an external speaker path (HDMI, Bluetooth) needs its own value, so expose it as an `audioDelay` seconds option on both players (default the constant) for an app settings screen, set by the flash-and-beep clip examples/video/assets/avsync.webm.
- video texture first frame: the first frame after `play` (and after a seek or buffering) is due at once, so it has no lead and is always latched behind the draw gate's peek; `videoLateLatches` therefore reads one per start on every platform, which hides a real late latch among starts. Either anchor a texture player's first frame one lead ahead (the plane must not change), or have the latch not count the first take after `set_playing(true)`.
- video texture nobody shows: a texture player whose output id no live node samples (a preloaded clip, the pacing probe's `openSecond`) is still latched and uploaded every frame it plays; gate the raster take on the id being referenced (the destroy sweep already knows `referenced_texture_ids`), so an unshown player costs decode only.
- video in headless playback: a stepped take that never settles (no frame due after the deadline, the producer not pushing) waits forever with no message; the first render of the latch hung silently for that reason. Log once after a bounded wait (a few seconds of wall time) naming the texture and the deadline, so a wedge is a log line, not a hang to bisect.
- video in headless playback with audio: the PCM sink plays in real time while the capture steps the clock, so a capture with sound plays its audio out of step with the frames; the sink could be silenced in playback (the track is still fed and clocked) until a capture that records sound exists.
- video plane start sync: a stream's first start on the TV sometimes logs one audio-sync anchor move just past the 40 ms threshold (-40.1, -40.3 ms), likely because `start_audio` (forge/src/video/worker.rs) anchors on the sink's content time right after its position jumped a whole device buffer; anchor half that first jump earlier and check that TV starts log no move (okf/plans/video-streaming.md, Findings).
- alloy layout: `set_unrounded_layout` (alloy/src/rendertree/layout/context.rs) calls the plain `invalidate_paint` per changed node, so a resize invalidates O(n * depth); thread one `visited` set through the layout pass and call `invalidate_paint_batched` instead (okf/done/content-damage-perf.md).
