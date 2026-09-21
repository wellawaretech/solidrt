---
title: The raster thread's CPU per frame on Android is the driver's swap, not our code
description: On the SM-T500 (Adreno 610, Android 12) the raster thread spends about 8 ms of CPU per 60 fps frame on a one-rectangle fullscreen frame; a DWARF simpleperf profile puts 63% of it inside eglSwapBuffers (the Adreno driver's pre-queue work plus the BLAST binder transaction) and 25% in the draw, with no busy-wait anywhere; HWUI pays a similar price on the same device, and the recipe for profiling the client's threads is here.
created: 2026-09-21
---

# The raster thread's CPU per frame on Android is the driver's swap, not our code

Measured 2026-09-21 on the Samsung SM-T500 (Adreno 610, driver V@0502,
Android 12, 1200x2000 at 60 Hz) with `probes/vsync-cadence-probe.tsx`: one
120x120 rectangle sliding in a fullscreen window, 0.3 ms build, 60 fps, 0
missed presents. Taken right after the main-loop spin was fixed
([[android-main-loop-rarely-sleeps]]), because the process `cpuPct` still
read 55 to 70 on the probe and the raster thread was most of it.

## The numbers

Per-thread CPU over 5 s, from `/proc/<pid>/task/<tid>/schedstat`:

| thread | CPU per 5 s | voluntary switches | nonvoluntary |
|---|---|---|---|
| srt-raster | 2.7 s (54%) | 758 | 674 |
| SDLThread | 0.26 s (5%) | 754 | 56 |
| srt-vsync | 0.28 s (6%) | 1103 | 40 |
| srt-ui (x2) | 0.36 s + 0.07 s | 379 + 396 | 89 + 8 |

At 300 frames that is about 8 to 9 ms of raster-thread CPU per frame.

Where it goes, from a simpleperf DWARF profile of the raster thread (2427
samples at 1 kHz over 5 s, 98.6% of call chains resolved; the
frame-pointer variant lost half of them inside the vendor driver):

| where | share | per frame |
|---|---|---|
| eglSwapBuffers, the Adreno driver's own work before the buffer is queued | 43% | 3.5 ms |
| eglSwapBuffers, libgui `Surface::queueBuffer`: the BLAST transaction to SurfaceFlinger over binder, synchronous per frame | 17% | 1.3 ms |
| draw: Impeller encoding plus the driver's command building and submission (`ioctl_kgsl_gpu_command`) | 25% | 2 ms |
| present fence wait (`gsl_syncobj_wait`), sync object create and destroy, allocator, rest | 15% | 1 ms |

The GPU timer reads 11.9 ms of GPU time per frame for the same one
rectangle: the fullscreen clear and the in-tile 4x MSAA resolve of a
1200x2000 surface.

## What it is and is not

- Not a spin. The present fence waits sleep in the kernel driver
  (`glClientWaitSync` is about 1% of samples), the swap's throttle on the
  previous buffer is a futex wait, and the driver's swap share is spread
  over dozens of small unnamed functions, which is command generation, not
  a poll loop.
- Not our code. Impeller's own functions sum to a few percent; the rest is
  libGLESv2_adreno (53% of samples), the kernel (15%), libc allocator
  (8%), libgui and binder (7%).
- Normal for this device. Scrolling the system Settings app, its HWUI
  RenderThread (Skia GL, no MSAA) spent 428 ms of CPU for 62 rendered
  frames, about 6.9 ms per frame, with heavier content than the probe.

So on this tablet the process `cpuPct` for a 60 fps fullscreen app has a
floor around 55%, set by the driver and the compositor path, and "cpuPct in
the tens" is not a reachable target here.

## Levers, none taken

- The 4x multisampled window multiplies the driver's tile count and the
  GPU resolve. A single-sample window would cut both but takes the rig path
  with a full-frame copy, which [[android-vsync-release-chain]] measured as
  slower on the GPU. An A/B needs a rebuild and both CPU and GPU read
  together; not run.
- The BLAST transaction per frame is Android 12's design.
- The per-frame sync object create and destroy (about 2 to 3%) has no
  reuse path in GL.

## Recipe and traps

- Per-thread CPU and sleep behaviour without root: sample
  `/proc/<pid>/task/<tid>/schedstat` (first field, ns of CPU) and the
  `voluntary_ctxt_switches` line of `status` twice, 5 s apart. Zero
  voluntary switches on a thread that should block is a spin; the process
  `cpuPct` cannot tell which thread.
- `simpleperf record -p <pid>` is denied to the shell user on this device;
  `simpleperf record --app com.solidrt.go -t <tid> --duration 5 -f 1000
  --call-graph dwarf -o /data/local/tmp/perf.data` works against the
  debuggable dev APK. Use `--call-graph dwarf`: with `fp` half the chains
  break inside the driver. Pull the file and report with the NDK's host
  binary, `/opt/android-ndk/simpleperf/bin/linux/x86_64/simpleperf report
  -g caller --sort symbol -n`.
- The release profile has `strip = true`, so our own frames appear as
  `libmain.so[+offset]`; only Impeller, SDL, libgui and libc names resolve.
  A `--symdir` pointing at the unstripped build output does not help
  because the build id differs. Kernel addresses stay unnamed
  (`/proc/kallsyms` is unreadable without root).
- Adreno's user-mode driver has no symbols; the named entry points that do
  resolve are in `libgsl.so` (`gsl_command_issueib_sync`,
  `gsl_syncobj_wait`, `gsl_command_readtimestamp`) and the kgsl ioctls.
