---
title: Make a test's window size land reliably
description: An app test with { size } sometimes mounts at the run's window (1280x720) instead of the size it set; seen twice in CI on "a size alone keeps scale 1", while the same commit passes on rerun.
created: 2026-11-04
---

# Make a test's window size land reliably

## Symptom

`packages/test/tests/app.test.tsx`, "a size alone keeps scale 1"
(`{ size: [500, 400] }`), fails in CI on Linux some of the time:

```
Expected: an object matching {width: 500, height: 400}
Received: {x: 0, y: 0, width: 1280, height: 720}
Outline:
  window 0,0 1280x720
    text "narrow" 0,0 1280x27.24
```

The mount waited for the window, and the window it got was the run's. The
"narrow" text shows that the app saw a narrow window at some point and
then laid out at 1280x720. The test before it (`{ scale: 2 }`) passed.

Seen in CI runs 39510441760 and 41851288431 (the "Android share
functionality" push). Three runs on the same code right after 41851288431
passed it. Not reproduced locally.

## Where to look first (not verified)

The mechanism is in okf/done/test-window-size-and-scale.md. `setWindow`
marks `WindowReady` as waiting for exactly `(500, 400, 1)`. Alloy's stepped
loop (`alloy/src/stepped.rs`) applies `SetHeadlessWindow` and sends that
resize. `EmitInitEvents` resets the window to the run's size and sends a
resize of its own. Both commands use one channel. The `ReadyState` comment
in `lattice/src/stepped.rs` assumes that `EmitInitEvents` always comes
first, so the matching resize is always the last one.

Lattice sends `EmitInitEvents` after it builds the engine
(`lattice/src/lib.rs`, after `builder.build()`). If the test file can call
`setWindow` before that send, the order flips:

1. The `(500, 400, 1)` resize makes the engine ready.
2. The init events restore 1280x720 and send that resize.
3. `window_ready` ignores the mismatch, but `ready` stays true and the app
   lays out at 1280x720.

This matches the outline above. It is not confirmed: check whether the
engine can evaluate the test file before lattice sends `EmitInitEvents`.

## Done

- The cause is found and fixed, or the hypothesis above is ruled out and
  the real cause is found.
- `--only app` runs the test many times in a loop on Linux without a
  failure. If the cause is ordering, a test pins the order.
