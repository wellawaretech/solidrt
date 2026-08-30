---
title: Intermittent SIGABRT at headless playback shutdown
description: Headless playback exits the process while the raster thread is still drawing the frame after the last recorded one; Impeller's encoder then hits a FATAL check (captured 2026-08-30 on animating examples), which is the SIGABRT one changelog-shot run died of.
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

## Captured (2026-08-30)

The stderr the note asked for. `srt render --file packages/core/examples/
frame-animation.tsx --duration 0.2` (and `stagger.tsx`; any example that
keeps requesting frames) prints, after `recording complete (12 of 12
frames)`:

```
[ERROR:flutter/impeller/renderer/backend/gles/buffer_bindings_gles.cc(365)] Break on 'ImpellerValidationBreak' to inspect point of failure: Uniform buffer had no members. This is currently unsupported in the OpenGL ES backend. Use a uniform buffer block.
[FATAL:flutter/impeller/renderer/backend/gles/render_pass_gles.cc(750)] Check failed: result. Must be able to encode GL commands without error.
```

Not every run: the same command at `--duration 0.1` was clean, and a static
`window-root.tsx` never shows it. All frames are on disk either way, and the
exit code was 0 in the runs seen - the abort from the FATAL races
`std::process::exit(0)` and usually loses; when it wins, that is the 134.

Mechanism, from `alloy/src/playback.rs`: after the last frame's readback the
loop still sends `FrameRendered` for it, which makes the UI thread build the
next frame and the raster thread start drawing it; the loop then reaches
"recording complete" and calls `process::exit(0)` with that draw in flight.
Impeller's GL encoder finds its state torn down under it and fails the check.
A static app has no pending frame request, so nothing is drawing at exit.

Done looks like: nothing is drawing when the process exits - do not send the
final `FrameRendered`, or stop the raster thread (drain its queue and join)
before `exit`. The exit-code gate is the reason it matters: a 134 on a
complete capture reads as a failed verification.

