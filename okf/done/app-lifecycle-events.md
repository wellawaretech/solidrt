---
title: Application lifecycle events
description: Fix the Android black screen on resume (EGL surface recreated, demand-driven gate never repaints) and let apps see background and foreground through a sticky visibility event.
created: 2026-07-22
completed: 2026-07-22
---

# Application lifecycle events

Two connected pieces:

1. Bug: on Android, backgrounding the client and bringing it back
   leaves the screen black (observed with the launcher on a real
   device). More prominent since exit-to-launcher landed: `exit()` at
   launcher root on Android backgrounds via moveTaskToBack, so every
   exit/reopen cycle crosses this path.
2. Feature: apps should be able to detect lifecycle transitions - put
   into the background, brought back to the foreground, or about to be
   closed.

Design decided 2026-07-25 (session with stage 1 implementation).

## Decided design

### Two native signals, not one

They are different things and stay separate in `AlloyEvent`:

- `AlloyEvent::Visibility { visible: bool }` - state transition.
  Android `AppDidEnterBackground` -> false, `AppDidEnterForeground` ->
  true (DID, not WILL, on both sides: the transition is only fact once
  complete, and on resume the recreated EGL surface exists by then);
  desktop `Minimized`/`Hidden` -> false, `Restored`/`Shown` -> true.
  Platforms may report one transition through both the app and window
  paths, so consumers tolerate repeated same-value events (the env
  signal dedupes at the core layer in stage 3).
- `AlloyEvent::Exposed` - repaint trigger only, never a state change
  and never surfaced to JS. SDL `WINDOW_EXPOSED` fires for plain
  damage and for a recreated surface; mapping it to visibility would
  be wrong (it fires on fully visible windows).
- `Occluded` is deliberately not mapped: noisy on macOS, and partial
  occlusion is not "hidden".

### Stage 1 - the bug fix, native only (implemented 2026-07-25)

The demand-driven draw gate (`lattice/src/plugins/draw.rs`,
`take_frame_requested`) skips every frame while idle, so the EGL
surface Android recreates on resume (contents undefined = black) is
never presented into. Fix: the lattice events task latches
`platform.request_frame()` on `Exposed` and on
`Visibility { visible: true }`. The resume events themselves wake the
main loop, the next Tick drives a render, and the cached display list
makes it a cheap present-only reuse. Also fixes any desktop
compositor that does not preserve the framebuffer. Over-triggering is
harmless (one atomic store; a spurious frame is a DL reuse).

Nothing is emitted to JS in stage 1.

`SDL_ANDROID_BLOCK_ON_PAUSE` stays at SDL's default (block the event
loop while paused): no ticks, no submissions, battery-friendly, and
it shields the raster thread from presenting into a dead surface
while backgrounded.

### Stage 2 - device verification (ran 2026-07-25: second suspect confirmed)

Device run with stage 1 only: still black, and the log proved both
halves of the diagnosis. The latch worked (a present was attempted
17ms after `nativeResume()`), and it failed exactly as suspected:

    [alloy] present failed: unable to show color buffer in an
    OS-native window (call to eglSwapBuffers failed, reporting an
    error of EGL_BAD_SURFACE)

Android destroyed the EGL surface (`surfaceDestroyed()` at pause),
SDL created a fresh one on resume, but the raster thread's context
binding still pointed at the dead surface. One failure only (count 1
of `PRESENT_FAILURE_EXIT_THRESHOLD` 2), so the client stayed alive
and black - demand-driven means no second attempt.

Fix (implemented 2026-07-25): proactive rebind, not swap-and-retry.
On `Visibility { visible: true }` the lattice events task sends
`Context::rebind_window_surface()` - a fire-and-forget
`RasterCmd::RebindWindowSurface` - BEFORE latching the repaint. The
raster command channel is ordered, so the rebind lands ahead of the
resume repaint's Frame. The raster-side handler re-runs make-current
with the thread's current context (`sdl_utils::gl_remake_current`;
SDL creates the EGL surface on demand), re-asserts the swap interval
(per-surface EGL state), drops the wrapped-FBO-0 surface so the next
frame re-wraps, and resets `present_failures` (a failure recorded
against the dead surface - e.g. a frame in flight at pause - is
stale evidence; this is what keeps the threshold from misfiring
across a background/resume boundary).

Second device run (2026-07-25, proactive rebind in place): STILL
black, identical failure 5ms after nativeResume. SDL source read
(sdl3-src 3.4.10) settled the mechanics:

- The EGL surface is recreated by `onNativeSurfaceChanged` on the
  JNI thread (log: 57.701, before nativeResume at .702), so the new
  surface exists before any of our code runs.
- `Android_GLES_SwapWindow` re-reads `window->internal->egl_surface`
  at swap time; eglSwapBuffers raises EGL_BAD_SURFACE when that
  (new) surface is not current to the calling thread's context -
  exactly a stale raster binding.
- SDL's own backup/restore (`android_egl_context_backup/restore`,
  SDL_androidevents.c) runs on the event-pump thread and backs up
  `SDL_GL_GetCurrentContext()` THERE - NULL for us, since GL lives
  on srt-raster. SDL's restore is a no-op for any app doing GL off
  the main thread; the rebind is fully our job.

Conclusion: the failing frame reached the raster thread BEFORE the
rebind command. The likely latch source is the resume resize
(`set_window_size` -> `request_frame`), racing ahead of (or in place
of) the `Visibility { visible: true }` arm - whether Android
delivers DID_ENTER_FOREGROUND/Restored to us at all is exactly what
the new diagnostic log answers. And with the latch consumed by the
failed frame, nothing re-requests: one failure, idle, black forever.

Fix round 2 (2026-07-25): reactive self-heal in the raster thread,
KEEPING the proactive rebind. In `frame`, a failed present gets one
rebind-and-redraw recovery: rebind (picks up the recreated surface),
re-wrap FBO 0, redraw the SAME display list (the failed draw's
content is lost with the dead binding - a bare swap retry would
present undefined pixels), present again. The retry feeds the
failure threshold honestly: fail -> rebind -> fail again = 2 =
confirmed loss, exit; fail -> rebind -> success = healed, counter
reset by the successful present. `rebind_window_surface()` itself no
longer touches the counter; only the event-driven command resets it
(stale pre-resume evidence). This closes the general race: ANY frame
latched between surface recreation and the rebind command (resize,
expose, timer) self-heals. Diagnostic info logs added: "[srt]
visibility: visible|hidden" (lattice events task) and "[alloy]
window surface rebound" (raster).

Third device run (2026-07-25): crash on resume, and the log nailed
the real bug. Both visibility events arrived (hidden+visible
back-to-back at resume - DID_ENTER_BACKGROUND is only drained when
the pump unblocks), the rebind ran BEFORE the frame, reported
success - and the present still failed; the recovery rebind also
"succeeded", the retry failed, threshold 2, exit(1) (the logged
SIGABRT in Android's RenderThread is teardown noise from exiting
mid-resume). Root cause found in SDL_video.c: SDL_GL_MakeCurrent
SHORT-CIRCUITS when its per-thread bookkeeping says this (window,
context) pair is already current - true on the raster thread - so
the same-pair rebind never reached eglMakeCurrent and the new EGL
surface was never bound. SDL's own android_egl_context_restore
unbinds first for exactly this reason.

Fix round 3 (2026-07-25): `sdl_utils::gl_remake_current` unbinds
(MakeCurrent(window, NULL)) before re-binding (window, context),
forcing the real eglMakeCurrent against the recreated surface. The
unbind step is documented as load-bearing.

Fourth device run (2026-07-25): VERIFIED. Resume log shows
visibility hidden+visible, one "window surface rebound" before any
frame, no present failure, screen restored. Stages 1+2 done.

### Stage 3 - the JS surface (solidrt lens)

- Sticky bus event `visibility` with `{ state: "visible" | "hidden" }`
  emitted from the flux gui `forward()`, exactly like `systemTheme`.
- Core surface: `env.visibility` reactive getter
  (`"visible" | "hidden"`), same pattern as `env.systemTheme`. Web
  vocabulary kept (`visibilityState`'s values), web machinery dropped
  (no `document`, no listener dance) - an effect on `env.visibility`
  is the idiom. String over boolean: matches the web values and
  leaves room for a future state without a type change (shape to
  reconfirm at stage 3 start).
- No close event - the decided simplification. Android `TERMINATING`
  gives effectively no time, desktop window-X quits without entering
  JS (the exit-to-launcher un-interceptable floor), and
  `EngineCmd::Stop` tears the engine down. Promising a close hook we
  cannot honor on any platform would be a lie. The documented
  contract is the mobile-native convention: persist state when
  `env.visibility` goes hidden. That moment is real and reliable on
  every platform we have.
- Android delivery caveat (device-observed 2026-07-25): with
  `BLOCK_ON_PAUSE`, the background events are queued but the pump
  blocks before our loop drains them, so through the normal path
  `visibility: hidden` arrives only AT RESUME - back-to-back with
  visible. Decided + implemented: an SDL event watch (registered in
  alloy's App::run; the sdl3 EventWatch guard binding must outlive
  the loop) forwards `DID_ENTER_BACKGROUND` into the event channel
  the moment it is pushed - the watch runs synchronously on the
  pump thread inside the blocking wait, and the JS thread keeps
  running while blocked, so persist handlers actually execute at
  background time (timers yes, frames no). The queued copy still
  arrives at resume; same-state repeats are legitimate everywhere
  in this pipeline and core's env signal equality-dedupes them.

### Stage 3 implementation notes (2026-07-25)

- `alloy/src/app.rs`: event watch forwarding DID_ENTER_BACKGROUND
  as `Visibility { visible: false }` at push time (Android timely
  hidden).
- `lattice/src/lib.rs`: the Visibility arm now also forwards to the
  engine (`ui_runtime.event`) after its rebind+latch work.
- `flux/src/plugins/gui/events.rs`: `visibility` sticky event with
  `{ state: "visible" | "hidden" }`, systemTheme pattern.
- `packages/core/src/environment.ts`: `Visibility` type +
  `env.visibility` getter (initial "visible", ownedWrite signal,
  sticky subscribe); doc comment carries the contract: no close
  event exists, persist when it goes hidden. Exported from index.ts.
- No flux-types parity needed (bus event names are not typed there;
  same precedent as `back`).
- Known micro-edge, accepted: an engine rebuilt mid-background (dev
  push while hidden) starts with the sticky cache empty and
  env.visibility = "visible" until the resume-time queued events
  correct it; self-converging, and EmitInitEvents can carry
  visibility later if it ever matters.
- Device-verified (2026-07-25, second device, Adreno 610):
  "[srt] visibility: hidden" logged 1ms after nativePause - the
  watch delivers the transition at background time. Also observed:
  one "window surface rebound" at startup (the launch-time
  Shown/foreground event maps to Visibility{true}); redundant but
  harmless, documented over-trigger.
- JS end verified (2026-07-25, examples/grid.tsx, kept as a living
  example of the pattern: split createEffect on env.visibility):
  app logged hidden at background time and exactly one "visible" at
  resume - the raw stream's resume-time duplicate hidden (visible
  in the "[srt]" diagnostic lines) never reached the effect, the
  signal equality-dedupe working as designed. When reading device
  traces: "[srt] visibility:" = native raw stream (repeats
  legitimate), unprefixed = app-side deduped stream.

## Remaining follow-ups

- Guide docs: DONE 2026-07-25 - docs/core.md gained onBack, exit,
  and a new env section with env.visibility (the persistence
  contract spelled out with a createEffect example; env itself had
  no section before, the other properties got a one-line index).
- Joint JS vocabulary review (back/exit verbs in srt:app vs
  visibility state in env) before the public API freeze - shared
  item with okf/plans/exit-to-launcher.md. Still open: a user
  decision, not a code task.

### Out of scope

`LOW_MEMORY`; gating JS timers while backgrounded (SDL's pause-block
already stops ticks/renders; timers keep running); a pre-Stop
"hidden" emit when exiting to launcher; desktop occlusion.

Vocabulary note: back/exit are input/verbs (`srt:app`, see
`okf/plans/exit-to-launcher.md`); lifecycle is state the OS informs
the app about (`env.visibility`). Verbs in `srt:app`, state in `env` -
that split is the design line.

## Stage 1+2 implementation notes (2026-07-25)

- `alloy/src/event.rs`: `Visibility { visible }` + `Exposed` variants
  with doc comments; translation arms next to FocusGained/Lost.
- `alloy/src/raster.rs`: `RasterCmd::RebindWindowSurface` +
  `RasterState::rebind_window_surface()` (make-current, swap
  interval, drop wrapped surface); `present()` returns success;
  `frame()` recovery path (rebind + redraw + re-present on a failed
  swap); `ensure_window_surface()` factored out for the recovery
  re-wrap.
- `alloy/src/sdl_utils.rs`: `gl_remake_current(window)` wrapper
  (SDL_GL_GetCurrentContext + SDL_GL_MakeCurrent).
- `alloy/src/context.rs`: `Context::rebind_window_surface()`
  fire-and-forget, documented next to `submit`.
- `lattice/src/lib.rs` events task: `Exposed` latches
  `request_frame()`; `Visibility { visible: true }` sends the rebind
  then latches. Neither event is forwarded to the engine yet.
- `cargo check -p alloy -p flux -p lattice --features lattice/go`
  clean.

## Raw material (from the 2026-07-22 diagnosis)

- SDL app-level lifecycle events, previously all untranslated:
  `WILL/DID_ENTER_BACKGROUND`, `WILL/DID_ENTER_FOREGROUND`
  (Android/iOS), `SDL_EVENT_TERMINATING`, `SDL_EVENT_LOW_MEMORY`;
  window-level `EXPOSED`/`RESTORED`/`MINIMIZED`/`HIDDEN`/`SHOWN`/
  `OCCLUDED`. Desktop close flows as `AlloyEvent::Quit`.
- The frame-request latch is settable from any thread
  (`platform.request_frame()`; the dev-server connection already
  latches it out-of-loop); sticky events + `env` getters are the
  established shape for state-like signals (`env.systemTheme`).
