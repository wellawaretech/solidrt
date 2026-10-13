---
title: Render to a VR headset through OpenXR
description: solidrt has no XR path; an OpenXR display backend in alloy, a pose-driven stereo camera in @solidrt/3d, the 2d tree as a compositor quad layer and controllers as an input source would run apps on standalone Android headsets (Meta Quest), with a desktop OpenXR runtime for development.
created: 2026-10-13
---

# Render to a VR headset through OpenXR

## Problem

solidrt has no XR path. The XR row of
[3d-feature-parity](../notes/3d-feature-parity.md) was untracked; Three
(WebXR), Unity and Godot all have one. A standalone headset such as the
Meta Quest is an Android arm64 device with native GLES 3.2 on an Adreno
GPU, so the Android client already runs on it, but only as a flat panel:
nothing renders per eye, follows the head, or reads the controllers.

## What already fits

- `alloy/src/egl_headless.rs` creates an EGL display, config and context
  without a window. That is what an OpenXR GLES session is created from.
- The raster thread presents through the `GlBinding` trait (bind, swap,
  swap interval) over a `DisplayContext` (`alloy/src/backend.rs`). An XR
  display is a third variant beside the SDL window and the pbuffer: it
  submits swapchain images instead of swapping a window.
- `alloy/src/vsync.rs` has a request/answer vsync contract with an
  Android Choreographer backend. `xrWaitFrame` fits the same contract.
- `@solidrt/3d` renders one scene from several cameras (views, and views
  into tiles of one atlas target). Two eyes are two views.
- The 2d tree already renders to textures (repaint boundaries,
  snapshots), which is what a world-space UI panel needs.

## Done looks like

Staged; each stage is useful on its own.

1. **A head-tracked scene.** An `xr` feature in alloy: an OpenXR instance
   and session over the raster thread's EGL context, one swapchain per
   eye, and the frame loop (wait, begin, locate views, render, end with a
   projection layer). The session states (ready, synchronized, visible,
   focused, stopping) map onto the existing lifecycle events. The frame
   clock's reference becomes the runtime's predicted display time, so the
   refresh estimator in `alloy/src/present.rs` is only the fallback there
   (the same seam as [presentation-feedback](presentation-feedback.md)).
   In `@solidrt/3d`, a camera form that takes a pose (position plus
   quaternion) and four asymmetric field-of-view angles: `perspective()`
   in `packages/3d/src/math.ts` is symmetric today. Each eye is a view
   whose camera the runtime writes every frame, under an app-placed rig.
2. **UI as a quad layer.** The 2d tree rendered to a texture and handed
   to the compositor as an OpenXR quad layer, which the runtime
   reprojects itself, so text stays crisp. A ray-to-quad intersection
   turns a controller ray into synthetic pointer events in the existing
   pointer model.
3. **Controllers.** OpenXR actions (grip and aim poses, buttons,
   thumbsticks, haptics) delivered through the event bus like gamepads.
   Scene picking takes a pixel today and needs a ray form.
4. **No third render.** Two eye views should not pay for an unused scene
   target: [3d-scene-without-own-target](3d-scene-without-own-target.md).
5. **Performance.** Two eyes at 72 to 120 Hz at above panel resolution.
   Multiview rendering and fixed foveation are the standard wins, measured
   on the device. Headset GPUs differ widely (the Quest 2 is an Adreno 650
   class part, the Quest 3 an Adreno 740), so the
   [low-end baseline](3d-low-end-gpu-performance.md) applies.

The JS surface takes WebXR's names and shapes through the SolidRT lens: a
session, frames carrying views (a transform and a projection matrix),
input sources with grip and aim poses, reference spaces. Open at design
time: whether an app declares itself XR in its packaging config (it runs
as a VR app from launch) or enters a session at runtime, and what a
non-XR app shows in a headset.

## Decisions before stage 1

1. **Desktop development runtime.** Monado, the open-source OpenXR
   runtime, has a simulated headset driven by keyboard and mouse; its
   compositor needs Vulkan. That would let stage 1's core (session,
   swapchains, frame loop, pacing, the pose camera) run on a Linux
   machine without a headset. The catch: alloy's Linux client is GLES
   over EGL, and the `openxr` crate (Ralith/openxrs) offers a GLES
   session only on Android (`graphics/opengles.rs`, `SessionCreateInfo`
   has one `Android` variant); on desktop it offers GL through GLX,
   Wayland or Windows only. Monado accepts GLES over EGL through
   `XR_MNDX_egl_enable`, which the crate does not wrap. Adding it is a
   session type of our own through the crate's public `Graphics` trait,
   filling raw `openxr-sys` structs in a small unsafe module: close to the
   CLAUDE.md line about reaching beneath a trusted crate's safe API.
   Without it, the headset is in the loop from the first frame.
2. **Installing Monado.** It is not in the Arch repositories; it would be
   an AUR build from source on the development machine.
3. **The Android loader.** Which OpenXR loader the APK bundles and how
   the build obtains it, under the dependency policy.

## Only provable on the device

The Android glue builds without a headset but is first tested on one:
the manifest entries that mark a VR app (the VR intent category, the
head-tracking feature), the instance created from the Java VM and the
activity (SDL3 hands both out; wrappers in `alloy/src/sdl_utils.rs`),
whether SDL's own surface coexists with the session, what happens when
the headset comes off or the system menu opens, refresh-rate selection,
and performance.

## Device setup

- Developer mode needs a developer team on the Meta Horizon Developer
  Dashboard and a verified account (SMS two-factor, or a payment method;
  nothing is charged). Then the Meta Horizon phone app, the headset's
  settings, Developer Mode on, and a reboot.
- Connect with a data-capable USB-C cable and accept the USB debugging
  prompt in the headset. On Linux, systemd's `70-uaccess.rules` grants
  adb devices by interface class, so no udev rule is needed.
- After the first USB pairing, `adb tcpip 5555` and
  `adb connect <ip>:5555` work as on any Android device.
- The display sleeps when the proximity sensor sees no one wearing the
  headset. `adb shell am broadcast -a
  com.oculus.vrpowermanager.prox_close` keeps it awake;
  `... automation_disable` restores it.
- Sideloaded APKs appear under the "Unknown sources" filter in the
  headset's Library.

## Involves

alloy (an `xr` module, a `DisplayContext` variant, the raster thread's
submit path, a pacing source), `sdl_utils.rs` (Android activity and JNI
wrappers), the lattice Android build (a VR manifest variant and the
loader), thin marshalling in `flux/src/alloy_plugins/`, `@solidrt/3d`
(the pose camera and an XR rig), core (the quad-layer surface), docs.
