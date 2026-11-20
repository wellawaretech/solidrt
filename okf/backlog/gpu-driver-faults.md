---
title: Know the GPU drivers' faults, and see when one bites
description: A register of known GPU driver faults on any GPU with OpenGL ES 3.0, read before GPU work and when a device misbehaves, and device checks that catch a wrong picture, which today only someone looking at the device does.
created: 2026-11-20
---

# Know the GPU drivers' faults, and see when one bites

## The problem

Impeller absorbed driver faults for us. With anvil
([own-renderer](../plans/own-renderer.md)) we own them, and we found the
first one by luck. On the tablet (Adreno 610, driver V@0502.0) the stage 6
layers made the window present the drawn frame once in three, with a black
buffer and the player's start screen between, and a layer could composite
black. A `glFlush` after each layer's passes fixed it (99e1f2a1). Two gaps
showed.

**Nothing told us what to expect.** Chromium, Skia and ANGLE keep public
lists of the driver faults they work around. We read them only after the
fault. None names this exact one, but four entries sit next to it (below),
and one of them bears on the next change the plan makes. Read before the
step, they would have said where to look on the device.

**Nothing caught it.** The maintainer saw the flicker by looking at the
tablet. The bench reads that ran on the faulty client gave plausible
numbers: frame counters count presents, not correct pictures. The control
API's `/snapshot` paints the current state afresh into an offscreen target
and `/texture` reads a texture the renderer owns, so both pass on a client
whose window presents garbage. Screen recordings on the device
(`adb shell screenrecord`) caught the period of three on the tablet at 15
to 22 fps, but only when someone thought to analyse them; on the TV they
run at about 9 fps against a 50 Hz panel and say nothing about flicker.
The stage's device step was close to closing on those numbers.

## Done looks like

- **A register** of the driver faults on GL paths, the ones we use and
  the ones we may use, on any GPU
  with OpenGL ES 3.0 (the context ore asks for, native or through ANGLE).
  The target is every such GPU, not a set of devices: the devices we test
  on (a growing set) are where we can see a fault, not where we support
  one. Faults cluster by vendor (Adreno, Mali, PowerVR, Mesa, NVIDIA,
  Apple, ANGLE on Windows), but a vendor list is no boundary. Per
  entry: the symptom, the GPUs and driver versions, the source (a Chromium
  bug list id, a Skia `GrGLCaps` condition, an ANGLE feature, or our own
  find with its commit), the path of ours it touches, and our workaround
  if we have one. The paths that matter so far: multisampled render to
  texture, framebuffer switches within a frame, framebuffer invalidation,
  blits and reads from a multisampled target, render work reordered across
  targets, program binaries, instanced draws. Every workaround in code
  names its entry; every entry with a workaround names its code site.
- **Read at both ends.** A step brief that touches GPU state (a new pass,
  a new framebuffer pattern, a new extension) lists the entries that apply
  and the device check that would show each. A fault seen on a device is
  looked up in the register before it is debugged from scratch. One line
  in the root CLAUDE.md (or alloy's) says so.
- **Detection that does not need someone watching:**
  - A presented-frame check behind a debug switch: the client compares the
    buffer it presents with what the frame drew (a few sampled pixels or a
    hash, read back before the swap) and counts the frames that differ, in
    the stats beside the present counters. Most GPUs we run on never
    reach our hands, so the check is also how a fault on one of them
    shows: it must be cheap enough to sample on an app's device, not only
    on a bench.
  - The device bench scripts fail when that count is not zero, instead of
    reporting numbers. Where the device can record its screen, they keep
    the recording and check every frame for foreign frames (a brightness
    outlier check found the tablet's period of three after the fact).
  - The bench summaries check their own arithmetic: frames plus missed
    presents over a window must equal the refresh rate times the window.
    The stage 6 device tables failed it for a day unseen (samples 16 s
    apart, divided by 10 s, the TV's 25 fps reported as 41), and one row
    read 759 frames per 10 s on a 50 Hz panel. A summary that fails the
    check prints no numbers.
  - A renderer step that changes GPU state does not close on device
    counters alone: its device run includes a look by the maintainer,
    asked for explicitly, with what to look for.

## What it involves

- Fetching the lists: `curl "<googlesource url>?format=TEXT" | base64 -d`
  on `chromium/src` `gpu/config/gpu_driver_bug_list.json` and
  `gpu_workaround_list.txt`, `skia` `src/gpu/ganesh/gl/GrGLCaps.cpp`,
  `angle/angle` `src/libANGLE/renderer/gl/renderergl_utils.cpp` and
  `include/platform/autogen/FeaturesGL_autogen.h`. Sort them by path,
  not by GPU, and keep the paths we do not use yet: the renderer grows,
  and a new path must find its faults already listed. Each path says
  whether we use it and where. Drop only the entries no OpenGL ES 3.0
  GPU can meet (GPUs that cannot run it, faults of contexts below it).
  Do not copy the lists whole: one line per entry, with its source id.
  The Vulkan lists wait for a Vulkan backend.
- The register is one file, `okf/notes/gpu-driver-faults.md`: it spans ore's window and EGL
  setup, alloy's rig and anvil, so it has no single code home.
- The workarounds alloy already carries (the rig's MSAA latch, the
  multisampled window fast path, the readback paths) go in as the first
  entries of our own.
- The presented-frame check is the open design question. A read of the
  default framebuffer before the swap stalls the frame and, on a
  multisampled window, needs a resolve; it must stay off the frames a
  bench measures, so how it samples is part of the work.

## State (2026-11-20)

The register is written: [gpu-driver-faults](../notes/gpu-driver-faults.md),
with the path inventory, our 20 finds with their code sites, 338 upstream
entries sorted by path, and its "Open checks": the upstream entries that
touch a path we use on a GPU we run on, each waiting for a look at our
code or a device. Open here: the checks, the CLAUDE.md line, and the
detection.
