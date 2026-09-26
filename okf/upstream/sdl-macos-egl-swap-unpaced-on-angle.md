---
title: SDL's macOS EGL path does not pace the swap (ANGLE-Metal swaps never block)
description: With SDL_OPENGL_ES_DRIVER=1 on macOS, SDL swaps through eglSwapBuffers on ANGLE, whose Metal backend honours eglSwapInterval(1) without blocking; SDL's display-link swap wait exists only on the CGL path, so a GLES app runs unbounded (1575 fps measured at 60 Hz).
created: 2026-09-26
status: unfiled
project: SDL (libsdl-org/SDL)
versions: SDL 3.4.10 (sdl3-src 3.4.10), ANGLE 2.1.26628 (Electron v40.9.2 libEGL/libGLESv2), macOS 26.3 on Apple M1
link:
---

# SDL's macOS EGL path does not pace the swap (ANGLE-Metal swaps never block)

## Summary

On macOS an SDL app that asks for a GLES context (`SDL_OPENGL_ES_DRIVER=1`,
`SDL_GL_CONTEXT_PROFILE_ES`) gets ANGLE through SDL's EGL path
(`src/video/cocoa/SDL_cocoaopengles.m`). `SDL_GL_SetSwapInterval(1)`
succeeds, but `SDL_GL_SwapWindow` returns immediately on every call: ANGLE's
Metal backend implements `eglSwapInterval` as `CAMetalLayer.displaySyncEnabled`
(no tearing) and never blocks in `eglSwapBuffers`. A render loop paced by the
swap therefore runs unbounded.

SDL knows this class of problem: its native CGL path
(`src/video/cocoa/SDL_cocoaopengl.m`) runs a `CVDisplayLink` whose callback
counts swap intervals, and `Cocoa_GL_SwapWindow` waits on that count before
`flushBuffer` ("always wait here so we know we just hit a swap interval").
That wait is not applied on the EGL path, which is the generic
`SDL_EGL_SwapBuffers` shared with Android/Wayland/X11, where the EGL driver
itself blocks. On macOS the only EGL there is comes from ANGLE, and ANGLE's
does not.

## Reproduction

Any SDL3 GLES app with vsync on and a per-frame swap, with ANGLE's
`libEGL.dylib`/`libGLESv2.dylib` next to the binary:

```
SDL_SetHint(SDL_HINT_OPENGL_ES_DRIVER, "1");
SDL_GL_SetAttribute(SDL_GL_CONTEXT_PROFILE_MASK, SDL_GL_CONTEXT_PROFILE_ES);
SDL_GL_SetAttribute(SDL_GL_CONTEXT_MAJOR_VERSION, 3);
... create window + context, SDL_GL_SetSwapInterval(1) -> returns true ...
loop { glClear(...); SDL_GL_SwapWindow(w); }   // count iterations per second
```

Measured 2026-09-25 on a Mac mini (Apple M1, macOS 26.3, 1920x1080 external
display at 60 Hz), GL string `ANGLE (Apple, Apple M1, OpenGL 4.1 Metal -
90.5) | OpenGL ES 3.0 (ANGLE 2.1.26628)`: 1575 swaps per second, the swap
call itself 0.0 ms, window in front. Same with the display asleep
(`pmset displaysleepnow`). `SDL_GL_GetSwapInterval` reports 1.

## Expected

`SDL_GL_SwapWindow` with interval 1 blocks until the display takes the
buffer, once per refresh, as it does on the CGL path and on every other
platform's EGL driver.

## Suggested fix

Apply the CGL path's display-link wait on the macOS EGL path too: create the
`CVDisplayLink` for the window's display when the EGL context is created,
count intervals in the callback exactly as `SDL3OpenGLContext` does, and have
`Cocoa_GLES_SwapWindow` wait for the next interval (per the configured swap
interval) before `SDL_EGL_SwapBuffers`. ANGLE can keep its own
`displaySyncEnabled`; the wait only restores the blocking that
`eglSwapInterval` promises.

## Our workaround

SolidRT does exactly that one layer up: `alloy/src/display_link.rs` runs a
`CVDisplayLink` over the active displays, counts ticks into a condvar, and
the window swap (`SdlGlBinding::swap`, `alloy/src/gl/context.rs`) waits for
the next tick before `SDL_GL_SwapWindow`. Verified on the same machine: 61
swaps/s, swap call 15.9 ms, 0 missed presents. The workaround can come out
when SDL paces its EGL swap on macOS. Design context:
`okf/design/frame-timing.md`, "Frame signal, macOS".
