---
title: Intermittent SIGABRT at headless playback shutdown
description: One changelog-shot run exited 134 with no stderr and has not reproduced in 22 runs since; the suspicion is a shutdown race in the exit()-during-playback path tearing down while the raster thread still holds GL state.
created: 2026-08-06
---

# Intermittent SIGABRT at headless playback shutdown


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

Split out of a two-item "headless render loose ends" file when okf was
restructured; the other half is
[playback-window-size-zero](playback-window-size-zero.md).
