---
title: Sequential engine swap, and plugins that own what they open
description: A reload (dev push, app update, app switch) builds the new engine while the old engine's plane player is still releasing its surface, so the new engine's plane open is refused every other time and the app shows no video. Phase 1 makes the swap sequential (old engine torn down, bounded, before the new one is built); phase 2 gives each alloy plugin ownership of what it opens so the engine loop's hand-maintained release list goes.
created: 2026-10-03
---

# Wait for the previous video plane before refusing a new one

The Android video plane is one at a time: `SolidRTActivity.createVideoPlane`
returns null while a `VideoPlaneView` exists, and `VideoPlane::create`
(alloy/src/video_plane.rs) turns that into "video plane not created".
Within one engine the video plugin (flux/src/alloy_plugins/video.rs) keeps
the last closed plane player's worker in `closing` and a new plane open
awaits its exit, so close followed by open works. That state is per engine.

Across an engine swap it is not covered. The runtime builds the new engine
and drops the old one around the same time (lattice/src/lib.rs, the engine
loop); the old plugin's drop sends its player a close and waits for
nothing. The new engine has a fresh plugin state, opens its plane about
150 ms after start, and the old worker is still stopping its codec and
releasing the surface, which takes 250 to 700 ms on the TV. The one-plane
rule refuses the new view; the player lands in the error state and the
app shows no video until it opens again. Every second reload succeeds
because the failed one left no plane behind.

## How it showed

On the TV (Philips TPM171E, armeabi-v7a, 2026-10-03), reloading a probe
that opens avsync.webm on the plane with autoplay:

```
10:00:04.317 [sol] flux engine start
10:00:04.466 Video plane not created: one already exists
10:00:04.708 Video plane removed
```

and the next reload played; the pattern alternated over ten reloads.
Found while verifying okf/done/video-first-frame-audio-anchor.md.

## Where it reaches production

The go runtime applies an app update through the same reload path as a
dev push (lattice/src/go/control.rs), and switching apps rebuilds the
engine. An app with a plane video on screen when an update lands, or a
quick switch into a plane app that autoplays, hits the same window. The
failure is silent for the viewer.

## Done looks like

The engine swap is sequential: the old engine is fully torn down, its
players' workers exited (bounded), before the new engine is built. A
plane open in the new engine then finds no plane and no plane being
released. Ten reloads of the probe above all play on the TV, with no
"Video plane not created" line in logcat. The reload latency grows by
the teardown (0.4 to 1 s with a plane player on the TV), accepted.

## Approach

Decided 2026-10-03: no asynchronous teardown overlapping the next engine.
The overlap saves a few hundred milliseconds of reload latency and buys
the whole class of "the old engine still holds it" bugs (the plane now;
a codec instance, a sink, a texture next), each of which so far got its
own guard.

- In the engine loop (lattice/src/lib.rs), before building the new
  engine: drop the old engine, then wait for its players' workers to
  have exited. Each worker publishes its exit on the player's `Shared`
  (`has_exited`, set after the presenter and its surface are released),
  so the plugin's `Inner::drop` can hand the engine loop the handles to
  wait on, bounded by a timeout a little above the slowest teardown seen.
- A player's `close()` inside a running app stays asynchronous: it must
  not hold the JS thread for a codec stop. The plugin's `closing` handle,
  which a later plane open in the same app awaits, stays with it.
- Not this: a wait inside `VideoPlane::create` (alloy) or one more entry
  in the engine loop's preamble. Both guard one resource against an
  overlap that should not exist.
- Verify on the TV: ten reloads of a plane probe with autoplay, all
  playing; the reload latency with and without a plane player, read from
  the "flux engine start" timestamps.

## Phase 2: plugins own what they open

The engine loop's preamble releases by hand what the finished engine
left in alloy's registries: `close_all_cameras`, `close_all_microphones`,
`close_all_audio`, `reset_spatial`, `release_engine_textures`
(lattice/src/lib.rs, the engine loop). The list exists because the
camera, microphone, audio and spatial plugins own nothing on the Rust
side: they hand JS integer handles over alloy's registries, keep at most
a callback map, and have no drop, so when the engine dies nothing
releases what those handles named. Each entry was added after its own
leak. The video plugin is the model that does not need an entry: its
state holds its players and closes them on drop.

Phase 1 is what makes ownership sufficient: released on drop only means
released before the next engine exists once the swap is sequential.

- Each plugin's state holds what it opened (camera sessions, microphone
  streams, audio tracks and loaded sounds, PCM sinks, spatial nodes)
  and releases it in `Drop`, as the video plugin does.
- The render tree's snapshot textures, which outlive the tree on a
  reload (okf/done/snapshot-texture-leak-reload.md), are released by
  whoever drops the tree, not by the loop after it.
- The preamble collapses to one thing: drop the old engine, wait
  (bounded) for its teardown, build the new one. The textures baseline
  check stays as the assertion that the drop path did its job, which is
  what it is for today; a leftover is a bug in a plugin's drop, not
  something to clean up after.
- Done looks like: no `close_all_*` or `reset_*` call in the engine
  loop; a reload of an app with a camera, a sound playing, spatial nodes
  and a plane video leaves the registries at their baseline, read from
  the textures count and the equivalent counts the other registries
  expose for the check.
