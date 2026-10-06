---
title: Custom runtime findings
description: What building the tower-toppling physics runtime taught about cdylib exports, the frame protocol's demand gate, publishes as demand, and the module's cost on desktop and on the SM-T500 tablet.
created: 2026-10-06
---

# Custom runtime findings

From [runtime-extension-modules](../done/runtime-extension-modules.md).

- A `#[no_mangle]` symbol in a dependency is exported from a downstream
  cdylib: `nm -D` on the stock lattice cdylib lists
  `blake3_compress_in_place_portable`. The same rule is what carries the
  JNI functions in lattice to the Java shell from a custom cdylib.
- The frame protocol's demand list is the right place for a module's
  "keep going": a physics world is "a physics world" in a settle's report,
  and an app with awake bodies is not at rest.
- A request made in `advance` is consumed by the same frame's draw gate.
  The first physics run proved it: 288 awake bodies, two frames in eight
  seconds, `settle` reporting rest. A module's demand is standing demand
  and has to be re-requested past the gate like a transition, onFrame, rAF
  and a playing video are; `advance` now notes which ticks demanded and
  `draw` re-requests for them. The example's tick counted frames without
  demanding, so the whoami check could not have caught it.
- A publish is demand too: `setRecords` and `updateRecords` upload, and an
  upload asks for the next frame, so an app that publishes every frame
  never rests (50 presented frames a second with every body asleep). The
  demo publishes while bodies move and in the frame they come to rest;
  at rest the stats window counts 0 frames.
- Measured on the final player (desktop, performance profile, nothing else
  running): the bench (the tower, five crate rains and three shots, up to
  791 bodies) holds 60 fps with no late frames, the world's steps
  (2 per frame at 1/120 s) 2.8 ms mean, 2.7 median, 4.5 p95, 9.0 max per
  frame, the app's publish 0.11 ms. The wasm lane measured 0.4 to 0.8 ms
  per step for 289 bodies, so per body the native module is at least its
  equal. With a build running beside it the same bench read 8.5 ms and 25
  late frames: measure on a quiet machine only.
- The 2d module over a record layer: 156 bodies at 0.12 ms per frame. The
  layer's sprite slots and the group's indices stay one order as long as
  nothing is removed between resets (a record layer's remove shifts, a
  group's swaps).
- On the SM-T500 tablet (Adreno 610, derived Player, release) the same
  bench reads 120 frames in 8 s with 119 late: the world's steps take
  38.8 ms mean, 37.2 median, 60.5 p95, 64.3 max per frame at up to 791
  bodies, the app's publish 0.45 ms. The tablet is CPU-bound in Rapier at
  that body count, about fourteen times the desktop's step cost; the
  module plumbing is not the limit. During the rain alone (389 bodies)
  the scene presents at 20 fps.
