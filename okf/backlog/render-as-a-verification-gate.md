---
title: srt render cannot fail, so it is not the gate the docs sell
description: A scene whose build throws is contained, writes an empty frame and exits 0, and --duration is app time so an app that loads asynchronously is captured mid-load; both make a green headless check that proves nothing, and both want one flag (--strict, --settle/--wait-idle).
created: 2026-09-08
---

# srt render cannot fail, so it is not the gate the docs sell

## Symptom

Two independent ways a `render` run reports success on an app that drew
nothing worth seeing.

**Contained errors do not reach the exit code.** Give a material a bogus
attribute format, or throw anywhere in a scene build, and the run logs
one `Contained error`, writes a frame with nothing in it, and exits 0.
The natural shell idiom hides it twice over: `srt render ... | tail -25;
echo $?` reports `tail`'s status.

**`--duration` is app time.** The capture is lockstep on the virtual
frame clock (`alloy/src/playback.rs`), so 40 frames at `--fps 1` render
in about two seconds of real time. An app that fetches, reads files or
builds in an isolate is still LOADING in every frame, however long a
duration is asked for, and every one of those frames is written and
exits 0.

Together: a headless check passes on an app that showed a loading
screen and logged an error.

## Cause

`packages/cli/src/render/main.ts` ends in `process.exit(await
run(runner, playbackArgs))`, and the runner's own status is about frames
written, not about what the app logged. Error containment is right and
documented (`core/AGENTS.md`) - nothing here argues with it. The gap is
that the CLI presents `render` as pass/fail while its exit code cannot
express either failure.

## Done looks like

- `--strict` on `render` (and plausibly `run`): exit non-zero if any
  contained or uncaught error was logged during the run, naming the
  first one. The runtime already formats those lines, so the signal
  exists; it needs a counter and an exit path.
- A way to let real time pass before or between captures: `--settle
  <ms>` (wait this much wall clock before the first written frame) or
  `--wait-idle` (capture once the microtask queue and pending I/O
  drain). `--settle` is the smaller, more predictable one and covers
  the asynchronous-load case that motivated this.
- Docs follow: `cli/AGENTS.md` now says what exit 0 does and does not
  prove; that paragraph shrinks to the flag once it exists.

Adjacent, small enough to ride along: `--fps` is a positive integer
(`lattice/src/main.rs` parses it as u32), so `--fps 0.5` is rejected.
Either accept a rational rate or keep the limit and leave it documented,
but decide it rather than leaving it as a parse accident.

## What it involves

The exit path is CLI plus one runtime counter: the runner would have to
report "errors were logged" back to the CLI (an exit code from the
playback loop, or a line the CLI greps for, and the first is cleaner).
`--settle` is a sleep in `run_playback_loop` before the first written
draw; `--wait-idle` needs a drain signal out of flux, which is why it is
the second choice.
