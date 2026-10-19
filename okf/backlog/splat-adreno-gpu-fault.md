---
title: A heavy splat pass faults the Adreno 610 GPU, and the app goes black without alloy noticing
description: On the Samsung SM-T500 (Adreno 610, OpenGL ES 3.2 V@0502) a splat scene pass of about 1.9 s GPU time trips a KGSL GPU fault (logcat "IOCTL_KGSL_GPU_COMMAND failed - errno 35"); the scene stays black, every later program reflects no uniforms, sometimes the process dies, and contextLosses stays 0. First filed as an SH vertex shader bug and disproven on the device 2026-10-19 (the SH shader draws at every degree and encoding). Wants the fault detected as a context loss, and the pass kept under the driver's limit on weak GPUs.
created: 2026-10-18
---

# A heavy splat pass faults the Adreno 610 GPU

## Symptom

Found 2026-10-18 while pricing the SH palette
(okf/plans/splat-palette-sh.md) on the tablet (SM-T500, Adreno 610,
OpenGL ES 3.2 V@0502.0 dated 10/08/21, 2.7 GB):

- The lizard (786k splats, SH degree 3, u8 or pq) loads with no error,
  49140 instances drawn, the sorts done, and the window is black but for
  the HUD; the frames keep presenting.
- Every program linked after that in the same process reflects no active
  uniforms: addDraw refuses `no active uniform named 'uSplatRecords'
  (active: )` although the link reported success.
- The client was also killed a few times during cloud loads, which was
  read as lmkd at the time; at least some of those deaths are this fault
  (below).
- The Pixel 7 (Mali G710) and the Linux client draw every one of these
  bakes.

It was filed as an SH vertex shader incompatibility, because the train
(SH degree 0) had drawn in an earlier run and the SH bakes never did.

## What it is not: the SH shader (bisected 2026-10-19)

probes/splat-sh-adreno-probe.tsx starts with no splat program, so each
run's program is the first of a fresh process, and draws a synthetic
32 x 32 grid baked in-app from a .ply written in memory (no asset), or a
.sol3s fetched whole. Each row is one fresh process, the lit-pixel count
read off a raw snapshot of the probe's view:

| first program, cloud | result |
|---|---|
| stock SH shader, degree 1, 3 (u8), 3 (pq), grid of 1024 | draws |
| stock SH3 u8, the grid tiled 768 times (786k splats, the lizard's exact textures: sh 4096 x 576 rgba32ui, 49152 instances; the copies culled behind the camera) | draws |
| a second program after any of these (the canary) | draws, uniforms reflected |
| stock SH3 u8, the real lizard, full resolution | black (49140 instances, 2 sorts, 0 lit) |
| the real lizard with its SH bands dropped (degree 0 shader) | black |
| the train (degree 0), full resolution, in this probe | black (64157 instances) |

The SH block, its uniforms and its samplers compile, link, reflect and
draw on this driver, at the lizard's texture sizes too. The program
bisect the item first asked for (removing uCamPos, the transpose, the
second sampler in turn) is built into the probe as variants but was not
needed. No compile or link info log was read: nothing failed to compile.

## What it is: a KGSL GPU fault on a long pass

Every black or killed run logs, 2.6 to 2.7 s after its cloud lands (the
first frames that draw it), and no run that draws logs it:

```
W Adreno-GSL: <gsl_ldd_control:553>: ioctl fd 82 code 0xc040094a (IOCTL_KGSL_GPU_COMMAND) failed: errno 35 Resource deadlock would occur
W Adreno-GSL: <log_gpu_snapshot:462>: panel.gpuSnapshotPath is not set.not generating user snapshot
W Adreno-GSL: ... (IOCTL_KGSL_DEVICE_WAITTIMESTAMP_CTXTID) failed: errno 35 Resource deadlock would occur
```

That is the kernel GPU driver refusing further submissions from a
context it flagged after a GPU fault or hang, and taking a fault
snapshot. Everything the context does afterwards is dead-ended, which
accounts for the whole symptom: the scene pass that faulted is lost
(black), new programs link "successfully" and reflect nothing, and the
frames that were in flight never retire (`submittedFrames -
retiredFrames` stayed at 2 with the last frame 13.8 s old).

The GPU load decides it. The real lizard, SH3 u8, stock shader, one
fresh process per rung, varying only the scene's `renderScale`:

| renderScale | scene pass execMs (/gpu) | result |
|---|---|---|
| 0.25 | 1119 | draws (1.93M lit pixels), no fault |
| 0.5 | 1904 | fault, black |
| 0.75 | - | fault, then the process died 100 ms later |
| 1 | - | fault, then the process died 60 ms later |

The train at full resolution in this probe measured 1901 ms and faulted
too, so SH is not needed: SH only adds vertex cost to a pass that is
already close to the line. Where the line sits (a per-submission time
limit near 2 s is the reading of these numbers) is not established.

alloy does not see it. The context is created with
EGL_EXT_create_context_robustness's lose-context-on-reset strategy (the
tablet grants it: no "GL context without reset notification" line at
startup), raster/frame.rs reads the reset status every frame, and
`/stats` contextLosses stayed 0 through every fault. So an app on this
device sits on a black screen with no error anywhere.

## Done (2026-10-19): the fault is a counted, recovered context loss

Three findings, each a fix, verified on the tablet with the lizard at
renderScale 0.5 (three fresh processes, two faults in one of them):

- The driver does report the fault, as `GL_GUILTY_CONTEXT_RESET`; alloy
  was asking a stub. The tablet advertises `GL_EXT_robustness` only and
  its vendor library exports only `glGetGraphicsResetStatusEXT`, while
  Android's wrapper libraries answer a proc-address lookup with a
  trampoline for every spelling they know (the core ES 3.2 name among
  them), which logs "called unimplemented OpenGL ES API" once per call
  and returns 0. `gl/robustness.rs` took the first non-null pointer; it
  now picks the entry by the extension string (KHR, EXT, ARB, then the
  core name only where the version makes it core) and logs the choice.
  With that, `/stats` contextLosses counts the fault and the recovery
  path runs.
- A new context on the same window is born dead on Adreno: its creation
  already fails a KGSL submission (the main thread's errno 35 line), its
  extension list reads empty (no anisotropy, no timer queries, the
  reset status falling to the core name), and every pass errors. The
  fault poisons the process's GPU device state the way ANGLE's device
  loss marks its display lost, and the cure is the same: Android now
  takes `RECOVERY_RECREATES_WINDOW` (app.rs), so SDL destroys the one
  window, terminates the EGL display with it and recreates both on the
  same native surface. The recovered context is whole (16x anisotropy,
  the EXT reset entry, timer queries), and a second fault in the same
  process recovers again (contextLosses 2).
- The first restored frame logged a text layer framebuffer incomplete
  (0x8cd7) and a GL error 0x502 after a shader pass: one cause. The
  stats overlay (lattice, plugins/draw.rs) kept its rasterized text and
  handed it in as `previous` for the re-render; attaching that dead
  texture fails the framebuffer and latches INVALID_OPERATION for the
  next pass to read. The overlay state now resets on the restore event
  (`gpu_reset`, run from runtime.rs after flux's own restore), and the
  HUD comes back on the recovered context with no warning.

Finishing the detection at engine level (2026-10-19, same day):

- A recovery loop was now possible: an app that keeps drawing the heavy
  pass faults, recovers, re-supplies, draws and faults again every 2.6 s.
  The reset status travels with the loss (`FrameExit::ContextLost {
  detail, status }`), the recovery counts guilty resets (the driver
  blaming this context's own work) and ends the process with the cause
  named at the third within a minute (`GUILTY_RESET_LIMIT`,
  `GUILTY_RESET_WINDOW_SECS` in raster/mod.rs; `note_guilty_reset` in
  recover.rs, unit-tested). The restore event carries `guilty` and
  `losses` to the app (`ContextRestoredEvent` in core), so an app can
  lower its load before that.
- The present fence is the driver-independent watchdog
  (`PRESENT_FENCE_LOSS_SECS`, 10 s, above every kernel's own hang
  timeout): a timed-out fence now stays at the head of the queue so its
  age counts, and a frame whose GPU work has not completed within the
  bound is a context loss with status "no reset", for the drivers that
  stay silent or a context whose reset notification was refused.
- A per-frame `glGetError` poll beside the reset-status poll:
  `GL_CONTEXT_LOST` is a loss (KHR_robustness raises it once), any other
  latched error is logged once a second as a pass's silent no-op. Not
  yet seen firing: the one case that needed it (below) killed the
  process first.
- The Android emulator cannot exercise any of this: its GL is the
  emulator's own GLES translator, not ANGLE, with no forced-loss hook
  and no reset notification (EGL refuses the attribute). It did expose
  a regression: the translator has no multisampled window config, so
  `setup_video` now retries with a single-sample backbuffer (every frame
  through the rig) when the window creation fails with multisampling
  requested, and the attribute stays cleared for a recreated window.
- The tablet, with probes/context-loss-probe.tsx's `hang` (a fragment
  shader that runs past the kernel's timeout; `{ iterations }` makes it
  a long pass that ends by itself) and its `plane` command (a clip on
  the Android video plane), from the kernel log of a bugreport (the one
  non-root view of KGSL: "gpu timeout ctx N", "gpu fault", "gpu failed"
  per fault):
  - A long pass that ends by itself (the lizard, 1.9 s) is one kernel
    fault and a clean recovery, twice in one process; the HUD and the
    probe's content come back. This is the splat case.
  - A shader that never ends (the probe's default) is two kernel
    faults every time: the GPU is not clean after the first reset, and
    the recovered context's first submission times out too, whatever it
    draws (shown with the recreation disabled as well). That second
    fault the driver does not report: the reset status stays "no
    reset", the fences retire (the kernel retires an invalidated
    context's timestamps), the swap succeeds, and only every GL call
    fails from then on. A 16M-iteration pass (about 10 s) behaves the
    same. Nothing in-process recovers this class on this device.
  - The SIGKILL: five faults out of about fifteen were followed within
    60 to 70 ms by a SIGKILL of the process. No lmkd, ActivityManager,
    kernel OOM or KGSL kill line exists in any log buffer, the kernel
    log included; the sender is outside our process and unidentified
    (a vendor GPU-hang policy is the guess). It preempted the on-device
    run of the guilty-reset cap, which stays covered by its unit test.
  - The video plane survives the window recreation (its surface and
    last frame stay up), and the hardware decoder's fate does not depend
    on the recreation: in one run it died 2 s after the fault with an
    OMX timeout (the same with the recreation disabled), in another it
    played on through the recovery. When it dies, forge::video ends the
    stream as a plain end (the plane's surface was not lost, so no
    error reaches the app); that is a follow-up in tiny.md.

Not understood: in one of four faults the process was SIGKILLed 64 ms
after it (no lmkd or ActivityManager record in either log buffer; a
Google app the system trimmed at the same moment in another run was a
coincidence). Three later faults at the same scale all survived. Where
it happens nothing in-process can help, which is the second part's
case.

The 2 s line is the msm KGSL dispatcher's drawobj timeout (2000 ms by
default on this 4.19 kernel); its sysfs knobs are root-only on the
tablet, so the value is the driver's default, not a reading.

## Open questions

- Why a quarter-resolution pass still costs 1.1 s: 136k pixels is not a
  fill cost. The guess, unverified, is the tiled renderer: Adreno bins
  the scene and re-runs the vertex stage for the primitives of each bin,
  so 3.1M heavy vertices (49140 instances x 64) cost more per bin as the
  resolution and the MSAA sample count raise the bin count. A vertex-only
  rung (splat-mesh-probe's "noraster" variant) at a few resolutions
  would split vertex from fill.
- Why the train drew in the 2026-10-18 run (splat-mesh-probe: explicit
  target size from the window and display scale, its own camera) but
  faults at full resolution here (fill mode, the probe's camera).

## Shape

Two parts, one symptom.

- Detect it (DONE above). Route the driver's report into the existing
  context-loss path (okf/done/gpu-context-loss.md), so contextLosses
  counts it and the app recovers, instead of a silent black screen and
  programs that reflect nothing. The frame fence that never retires was
  not needed as a signal: the reset status answers once the right entry
  is asked.
- Keep a splat pass under the limit on weak GPUs. Measure where the time
  goes first (vertex against fill, per the question above). The levers
  are the quality row for low-end devices (renderScale, maxPixels,
  samples; okf/done/3d-quality-tier.md), and splitting a large instanced
  draw into several draws with a flush between them, so no single
  submission runs long; the splat count dial (setRecordCount) is the
  app's own. Done: the lizard at the tablet's default quality draws
  there, slowly if need be, without a fault.

## Reproducing

- The probe: `bun run sol server probes/splat-sh-adreno-probe.tsx
  --file --port <N> --lan` (no local client; a `sol run` would open
  one), then a fresh Player per run: `adb shell am start -S -n
  com.solidrt.player.dev/com.solidrt.app.MainActivity --es
  sol_dev_server <host>:<N>`. Wait for the client to answer the `state`
  command with `probe: "splat-sh-adreno"` and no job, then POST
  `/debug?name=run` with `{ variant, degree, encoding }` and optionally
  `copies`, `url`, `count`, `renderScale` (the header comment lists the
  variants). Snapshot the view labelled `sh-probe-view` raw and count
  the lit pixels; read `/gpu` for the scene pass's execMs and
  instanceCount, `/stats` for orderSorts and contextLosses, and `adb
  logcat -d | grep Adreno-GSL` for the fault.
- A file-mode server serves no assets: the lizard reached the tablet
  through a throwaway `python3 -m http.server` on the LAN address
  serving a directory holding a link to assets/lizard.sol3s, fetched
  with the run's `url`.
- Traps met on the way, each of which reads as "draws nothing" by
  itself: a capture's bounds include its floaters (the lizard's span
  +-200), so a camera framed on them sits past the far plane (100) and
  the mesh is culled with no count ever written; `/settle` returns when
  the fetch ends, before the threaded sort publishes ids, so wait for
  orderSorts >= 1 before the snapshot; and a variant that declares an
  SH sampler but never reads it is refused at addDraw (the mesh binds
  the SH textures by name), so the probe drops the declarations and
  draws such variants on a degree 0 cloud.
