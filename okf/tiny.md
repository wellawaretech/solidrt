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


## DX

The `srt` CLI, the dev server, MCP, debug commands, examples and probes.


## Runtime

alloy, forge, flux, lattice.

- video plane audio: `AUDIO_OUTPUT_LATENCY_US` (forge/src/video/audio.rs, 60 ms for the Philips TV's speakers) is a build constant; an external speaker path (HDMI, Bluetooth) needs its own value, so expose it as an `audioDelay` seconds option on `present: "plane"` (default the constant) for an app settings screen, set by the flash-and-beep clip examples/video/assets/avsync.webm.
- video plane start sync: a stream's first start on the TV sometimes logs one audio-sync anchor move just past the 40 ms threshold (-40.1, -40.3 ms), likely because `start_audio` (forge/src/video/plane.rs) anchors on the sink's content time right after its position jumped a whole device buffer; anchor half that first jump earlier and check that TV starts log no move (okf/plans/video-streaming.md, Findings).
