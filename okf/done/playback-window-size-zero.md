---
title: Playback's first frame does not reflect app state
description: The first headless capture predated the app's first frame callback, and playback never answered the init bundle (onFrame's rate stayed 60 at any --fps); the 0x0 windowSize() was the general mount-time first-read trap, not a playback drop. Closed by drawing but not writing the mount frame and a pinned playback init bundle; --step stays an idea.
tags: [render, playback, capture, headless]
created: 2026-08-06
completed: 2026-09-03
---

# Playback's first frame does not reflect app state

Two symptoms with one shape: the first captured frame shows something the
app never meant to show, quietly, and the author debugs the app.

## windowSize() reads 0x0

Playback synthesises one `Resize` event so layout gets its size, but the
JS-side window state never sees it: init events reach JS by an `AlloyCommand`
that only the interactive loop handles. So in any headless render
`windowSize()` is `{ width: 0, height: 0 }` (`displayScale()` reads 1, which
happens to be right now that playback pins the scale).

The failure mode is quiet. An app that sizes anything off `windowSize()` -
a responsive layout, a full-bleed background, a `pct()` fallback computed in
JS - renders as though the window were empty, and the capture looks wrong
rather than erroring. `changelog-shot` sidesteps it by laying out from the
content and capturing the node's own box, which is why it never noticed.

Fix is to deliver playback's synthesised resize to JS the same way the
interactive loop does, so `windowSize()` reports the `--size` the caller
asked for. Recorded in the closed determinism item, moved here so it is
visible in the open list.

Split out of a two-item "headless render loose ends" file when okf was
restructured; the other half is
[playback-shutdown-sigabrt](playback-shutdown-sigabrt.md). Both surfaced while
building `scripts/changelog/` on the now-headless `srt render`, and neither
blocks it.

## Frame 0 is captured before the first frame callback (2026-08-17)

`srt render --duration 0.2 --fps 6` records one frame, and that frame is
the scene as the components MOUNTED it - initial camera props, untouched
node transforms - not the simulated state. The lockstep loop
(`alloy/src/playback.rs`) receives and writes each capture first and only
then sends `FrameRendered`, which is what drives the app's frame callback,
so the very first PNG predates any `onFrame`. The symptom is expensive:
`console.log` inside `onFrame` reports the right positions, a projection
helper confirms the camera is where it should be, and the PNG shows a
different view - which reads as a bug in the camera or transform path
rather than a capture-timing artefact.

Any of these fixes it: do not write frame 0 (or do not record until the
first callback has run), a `--warmup <n>` / `--start <seconds>` flag, or at
minimum a prominent note in `packages/cli/AGENTS.md` next to the existing
render gotchas.

Related, same command: `--fps` sets the app's clock step, so `--duration 14
--fps 1` hands the app fourteen one-second frames. Any app that clamps its
delta (which the scaffold AGENTS.md rightly insists on, for the hot-reload
negative-delta reason - [onframe-tick-reset-on-reload](onframe-tick-reset-on-reload.md))
therefore advances almost no simulated time; getting fourteen real seconds
needs `--fps 30` and 420 PNGs. A `--step <ms>` decoupling the simulation
rate from the capture rate would make headless verification of anything
time-based much cheaper. Both first-frame problems and this one are
properties of the same loop and should be decided together: what the app
has seen by the time frame N is captured, and how app time relates to
frame index.

## Resolution (2026-09-03)

The first symptom was misdiagnosed. Playback's resize did reach JS all
along: `App::run` sends it before the capture loop starts, and core
bootstraps frame 0 on the first resize it sees, so a reactive `windowSize()`
read was right in frame 0 (verified on `window-signals.tsx`). What read 0x0
was a mount-time, non-reactive read, and that is the general first-read
trap in every mode: the engine evaluates the module before it drains any
queued event. What playback did lack was the rest of the init bundle: the
capture loop never reads commands, so `EmitInitEvents` went unanswered and
`onFrame`'s `rate` stayed at the 60 default whatever `--fps` said.

Landed:

- `playback_init_events` (alloy `event.rs`) is playback's stand-in for the
  `EmitInitEvents` answer: the refresh rate pinned to the capture fps, then
  the resize last, because core bootstraps frame 0 on it and everything
  sent before it must be in place for that frame. Theme, input devices,
  orientation and pointer lock stay unsent on purpose: an offscreen capture
  has none, core's defaults cover their absence, and any pin there is a
  product choice.
- The mount frame is drawn but not written (`playback.rs`): PNG k is the
  state after the app's (k+1)th frame callback at time (k+1)/fps, so every
  written frame is one a frame callback has shaped. A static app renders
  identical PNGs; a scripted event at time t lands in the PNG at time t.
- `packages/cli/AGENTS.md` and the `srt render` docs state both.

`probes/playback-first-frame-probe.tsx` shows all of the above per frame:
a mount-time read, a reactive read, and the callback count with its last
tick, on the PNG and in the log.

Not done: decoupling the simulation step from `--fps` (`--step`), recorded
in `ideas.md`.
