---
title: srt render is never headless on ANGLE
description: On Windows the offscreen video driver fails every time (SDL's offscreen path needs EGL_EXT_device_enumeration, which ANGLE does not implement) and playback silently falls back to a hidden window, so the one command that exists to run without a display requires an interactive window station there; the ANGLE that ships already advertises the extensions a real headless path needs.
created: 2026-08-17
---

# srt render is never headless on ANGLE

Symptom: every `srt render` on Windows prints

```
[alloy] offscreen video driver unavailable (window creation: SDL error:
        eglQueryDevicesEXT is missing (EXT_device_enumeration not supported
        by the drivers?)); falling back to a hidden window
```

and then renders correctly, fully accelerated, into a hidden window. On a
desktop that is fine. The cost is that playback then REQUIRES an interactive
window station: under a service, in Session 0, or over an SSH-only session
`SDL_CreateWindow` fails and there is no third fallback, so the command that
exists for headless capture is the one command that cannot run headless.
For an agent that is the difference between "CI verifies the app renders"
and "CI verifies the app compiles".

[render-headless-determinism](../done/render-headless-determinism.md) landed
the offscreen driver with this fallback (`alloy/src/app.rs`, playback sets
`SDL_VIDEO_DRIVER=offscreen`, clears it and re-enters the interactive path
on failure) and verified it on Wayland. This is the leftover: on the ANGLE
path the fallback is not a fallback, it is the only path.

## Root cause: SDL's offscreen driver and ANGLE cannot meet

SDL's `offscreen` video driver builds its EGL display through the device
platform: enumerate with `eglQueryDevicesEXT`, then
`eglGetPlatformDisplayEXT(EGL_PLATFORM_DEVICE_EXT, device, ...)`. ANGLE
implements device QUERY (`EGL_EXT_device_query`: which device is behind a
display you already have) and device CREATION (`EGL_ANGLE_device_creation`),
but not device ENUMERATION (`EGL_EXT_device_enumeration`: discover devices
before you have a display). Verified against the shipped
`@solidrt/win32-x64-msvc` DLLs: `libEGL.dll` does not export
`eglQueryDevicesEXT`, and `libGLESv2.dll` advertises `EGL_EXT_device_query`
and `EGL_EXT_platform_device` but not `EGL_EXT_device_enumeration`. So this
is not a packaging slip and a different ANGLE build will not fix it: the two
designs disagree about which EGL object exists first.

## Shape

The real fix stays inside alloy (platform facts, no vendored-SDL edits): in
playback, skip SDL's video subsystem for context creation, since SDL provides
neither input nor presentation there anyway. Every extension the path needs
is already advertised by the ANGLE in the box:

- `EGL_KHR_surfaceless_context`: playback renders to an offscreen target and
  reads it back, so it never needs a window surface, only a current context
  (`eglMakeCurrent(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)`). A pbuffer
  surface is the fallback if surfaceless is awkward.
- `EGL_ANGLE_device_creation` + `EGL_ANGLE_device_creation_d3d11`: create an
  `EGLDeviceEXT` directly (or wrap a D3D11 device the runtime made) and hand
  it to `eglGetPlatformDisplayEXT(EGL_PLATFORM_DEVICE_EXT, ...)` - exactly
  the display SDL wanted, obtained without enumeration.

`SDL_VIDEODRIVER=dummy` plus that context is a headless renderer built only
from what already ships. It also has to coexist with the raster thread owning
the one GL context, and with the other platforms where the offscreen driver
already works: keep the SDL offscreen path first and take this route only
when it fails for THIS reason, not as a Windows special case.

Two much cheaper mitigations, DONE 2026-08-17 (stage 1):

1. Say the fallback is expected. `alloy/src/app.rs` matches the failure
   text on `EXT_device_enumeration` / `eglQueryDevicesEXT` and logs a calm
   info one-liner naming ANGLE; any other offscreen failure keeps the warn
   with the SDL error. Matched on the fact, not on the OS.
2. `packages/cli/AGENTS.md` render gotchas document the ANGLE behavior
   (rewritten for stage 2: pbuffer path first, hidden window as the last
   resort needing a desktop session).

Stage 2, IMPLEMENTED 2026-08-17, verified on Windows in a desktop session:

- `alloy/src/egl_headless.rs`: `HeadlessEgl` loads libEGL at runtime
  (khronos-egl `dynamic`; exe-dir copy first, then the loader's search),
  takes `eglGetDisplay(EGL_DEFAULT_DISPLAY)`, an ES 3.0 context and an RGBA8
  / depth 16 / stencil 8 pbuffer at the capture size. A PBUFFER, not
  surfaceless: playback draws to FBO 0 and reads it back, so a pbuffer keeps
  draw_to_window, the MSAA rig, and read_fbo0_pixels untouched. Default
  display, not `EGL_ANGLE_device_creation`: needs no D3D11 device of our
  own; device creation stays the escalation if the default display turns out
  to need a window station.
- `backend::GlBinding` (bind / swap / set_swap_interval / proc_address /
  error) is what the raster thread now holds instead of the raw SDL window
  pointer; `gl::SdlGlBinding` is the interactive impl (same calls as before,
  incl. the unbind-then-bind rebind dance), `HeadlessEglBinding` the
  playback one. `DisplayContext::EglPbuffer` is the second variant.
- `app.rs`: offscreen driver first (unchanged); on the ANGLE-reason failure
  only, `SDL_VIDEO_DRIVER=dummy` (a Window for the playback loop to size
  from, no GL flag) + `DisplayContext::new_egl_pbuffer`; if that fails too,
  the hidden window as before. Wayland never enters it.
- Verified on Linux/Mesa by temporarily forcing the branch: `srt render`
  frames through the pbuffer path are byte-identical to the offscreen
  driver's. Windows (RTX 3070, ANGLE D3D11, shipped libEGL.dll): the branch
  is taken for real, `headless EGL 1.5 pbuffer context`, frames correct.
  Still open: the same run from a non-interactive session (service /
  Session 0 / Windows OpenSSH), which is what decides whether the default
  display suffices there or `EGL_ANGLE_device_creation` is needed. The WSL
  interop control channel runs the exe inside the logged-in desktop session,
  so it cannot test this; a scheduled task set to run whether the user is
  logged on or not can. macOS untested.

Known limitation: `gl/rig.rs` `msrtt()` still asks SDL for extension support and
proc addresses; under the dummy driver that answers "unsupported", so the
headless path uses the explicit MSAA resolve. Desktop does that anyway
(MSRTT is the Android tiled-GPU path).

Confidence: the ANGLE half is direct evidence from the shipped DLLs and can
be re-checked in seconds; the alloy half is read from `app.rs`.
