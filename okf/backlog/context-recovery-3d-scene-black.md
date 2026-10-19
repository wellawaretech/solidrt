---
title: A 3d scene mounted after a context recovery draws black on the SM-T500
description: On the Samsung SM-T500 (Adreno 610) a splat scene created fifteen seconds after a guilty-reset recovery renders black at 2 ms per pass although its data textures carry their content, its id stream is sorted, its program links and no GL error is logged; the recovery was verified on the HUD and the loss probe's content, never on a 3d scene mounted afterwards. Wants the stage that fails found (the rgba16f scene target, its resolve, or the draw itself) and the recovery verified on a scene.
created: 2026-10-19
---

# A 3d scene mounted after a context recovery draws black on the SM-T500

## Symptom

Found 2026-10-19 while measuring the splat ladder for
okf/done/splat-adreno-gpu-fault.md (part B). probes/splat-mesh-probe.tsx
on the tablet: its startup mount (the train at the window size) faulted
the GPU, part A recovered the context (`contextLosses` 1, the log's
"GPU context restored: 9 textures, 3 buffers, 2 programs, 3 targets
rebuilt; the app re-supplies 5 textures, 3 buffers"), and fifteen
seconds later the probe's `scene` command loaded the pq lizard: a new
splat mesh, new data textures, a new scene target, a new program, all
created on the recovered context. Every rung after that read 2 ms per
pass for 4.7M indices at 30 ms per frame, and the scene was black:

- the window snapshot of the scene's texture leaf is black;
- the scene's resolve target (rgba8, read through `/texture`) is rgb 0,
  alpha 1 everywhere;
- the colors data texture (rgba8) reads back with its content (mean
  0.70, alpha mean 0.56);
- the id stream buffer holds a sorted order (varied ids at offset 0 and
  at 1 MB; `orderSorts` ticking);
- the splat-scene draw target renders every frame (`passes` climbing,
  `instanceCount` 49140, 96 indices);
- `/logs` and logcat carry no GL error, no warning after the restore.

A fresh process with the same probe pinned to quarter scale before its
first mount (no fault, `contextLosses` 0) read 276 ms per pass at the
same rung and drew the lizard: the rows in the plan's Findings. So the
2 ms figure is a draw that produces no fragments - every quad culled in
the vertex stage, or a pass whose output never reaches the resolve.

## What was verified before, and what was not

Part A's device verification (the plan's "Done" section) checked the
recovered context's extensions, the HUD coming back, the loss probe's
own content coming back, and a second fault recovering again. The
lizard case verified `contextLosses` 1 then 2 in one process - the app
faulted again, which means its re-supplied textures drew - but a scene
MOUNTED AFTER the recovery was never checked. The difference between
the two (a mesh re-supplied through `ContextRestoredEvent` against one
created from scratch on the recovered context) is where to look.

## Done looks like

- The failing stage named: a snapshot of the scene target itself (it is
  rgba16f: render it through a pass into an rgba8 target, or read the
  resolve with the splat draw replaced by a constant-color draw), the
  draw's uniforms (`/gpu?draw=`: `uViewProj`, `uModel`, `uViewport`),
  the culling branches of the splat vertex stage.
- The cause fixed in the engine or the recovery path, and the recovery
  verified on the tablet with a 3d scene mounted after the fault, drawing
  what the same scene draws in a fresh process.
- A note in okf/done/gpu-context-loss.md's record of what the Android
  recovery was verified against.

## Reproducing

probes/splat-mesh-probe.tsx served with `bun run sol server
probes/splat-mesh-probe.tsx --project --port <N> --lan`, the tablet
client launched fresh (`adb shell am start -S -n
com.solidrt.player.dev/com.solidrt.app.MainActivity --es sol_dev_server
<host>:<N>`): the startup mount of the train at the window size faults
within 3 s of its load (the client's log: "GPU context lost: the swap
succeeded; guilty context reset; recovering"). Then POST `/debug?name=scene`
with `{ "path": "assets/lizard-pq.sol3s" }` and `/debug?name=bench` with
`{ "scale": 0.25, "orbit": true }`: `/stats` reads about 2 ms per
splat-scene pass and the snapshot of the texture leaf (`/tree`, then
`/snapshot?node=<id>`) is black. Note: once part B's submission budget
is in, the startup mount no longer faults on its own; provoke the fault
with probes/context-loss-probe.tsx's `hang { iterations }` instead.
