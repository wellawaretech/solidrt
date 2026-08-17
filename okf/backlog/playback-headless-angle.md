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

Two much cheaper mitigations, worth doing regardless and first:

1. Say the fallback is expected. The line reads like a driver defect on the
   user's machine ("not supported by the drivers?"). On ANGLE it is a fixed
   property, identical on every install, so it should be a calm one-liner
   (or silent below a verbosity flag), not an SDL error surfaced verbatim at
   warn level.
2. Document that `srt render` needs a desktop session on Windows today, in
   `packages/cli/AGENTS.md` next to the other render gotchas
   ([playback-window-size-zero](playback-window-size-zero.md) carries the
   first-frame ones). That is the fact a CI author needs, and it is
   currently discoverable only by the command failing in CI.

Confidence: the ANGLE half is direct evidence from the shipped DLLs and can
be re-checked in seconds; the alloy half is read from `app.rs`.
