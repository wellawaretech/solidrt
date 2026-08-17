---
title: Playback's first frame does not reflect app state
description: Two ways the first headless capture shows something the app never intended - windowSize() reads 0x0 because playback's synthesised Resize never reaches JS, and frame 0 is captured before the app's first frame callback runs, so it shows the mount state, not the simulated one; plus --fps doubling as the simulation step, so a clamped-dt app barely advances at low capture rates.
created: 2026-08-06
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
