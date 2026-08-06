---
type: backlog-item
title: Headless render loose ends - a shutdown abort and a blind windowSize()
description: Two things left over from the headless-render work: playback aborted once with SIGABRT at shutdown and has not reproduced, and windowSize() reads 0x0 in playback so an app that lays out from it renders wrong instead of failing.
status: open
tags: [render, playback, capture, headless, lifecycle]
timestamp: 2026-08-06T00:00:00Z
---

# Headless render loose ends

Both surfaced while building `scripts/changelog/` on top of the now-headless
`srt render` (see [render-headless-determinism](render-headless-determinism.md)
and [app-process-argv](app-process-argv.md), both closed). Neither blocks that
tool, which works and is byte-reproducible; both are the kind of thing that
costs an hour to someone who hits them cold.

## 1. Intermittent SIGABRT at playback shutdown

One run of `changelog-shot` exited 134 (SIGABRT). It has not reproduced in 22
consecutive runs since, and no stderr was captured from the failing run, so
there is no message and no stack - only the exit code.

What the app does at that point: `captureSnapshot` -> `readTexture` ->
`destroyTexture` -> `encodeImage` -> await a `flux:fs` write -> `exit()`. The
write is awaited before `exit()`, so this is not a half-written file.

The suspicion is a shutdown race in the new `exit()`-during-playback path -
tearing the process down while the raster thread is still holding GL state -
because that path is the newest code in the neighbourhood and it now runs
before the frame budget is spent rather than after. That is a hypothesis, not
a finding: the abort could equally predate it.

Next time it happens, the run's stderr is the thing to keep. Failing that, a
loop of a few hundred renders under `catchsegv`/coredumpctl would either
produce the message or set an upper bound on the rate.

## 2. windowSize() reads 0x0 in playback

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
