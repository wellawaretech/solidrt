---
title: Stop the raster thread before the window drops and before exit
description: On Linux nearly every client exit dumped core (a closed dev window, every `sol test` app test file) - the window drop or `process::exit` unloaded the EGL driver while sol-raster was still in a GL call; the stepped loop's raster fence ran after the window drop, and the interactive exit paths had no fence at all. Closed by stopping the raster thread before every exit: it finishes its queue and parks with its context current, and the window is held in a guard that stops it first.
created: 2026-11-15
completed: 2026-11-15
---

# Stop the raster thread before the window drops and before exit

## Symptom

Seen on Linux with the NVIDIA proprietary driver (Wayland). `solidrt-go`
ends in SIGSEGV during shutdown almost every time. systemd-coredump records
every closed dev window and every `sol test` app test file (one
`solidrt-go --test` process per file). The work completes first: results
are reported and nothing visible is lost. The exit code and the core dump
are the damage: a crash on a passing test run reads as a failure, and the
coredump noise hides real crashes.

Every dump has the same two threads:

- `sol-raster` crashes in `libnvidia-eglcore.so`, called from the GL draw
  path (a frame in flight, or a GL object create). `SEGV_MAPERR`: it reads
  freed driver memory.
- The main thread or another thread frees the driver under it, on one of
  two paths:
  - `dlclose` of `libEGL_nvidia.so.0` (its `__cxa_finalize`), from the
    window drop after the run loop returns. The test client takes this path.
  - `exit()` running the same finalizers through libc's exit handlers. The
    dev client takes this path when its window closes.

This is the race [playback-shutdown-sigabrt](../done/playback-shutdown-sigabrt.md)
closed for `sol render` on Mesa. It is not NVIDIA-specific; NVIDIA only
loses the race more often.

## Cause (confirmed in the code)

The raster thread is detached: `alloy/src/threads.rs` drops its
`JoinHandle` at spawn, so nothing can join it. The only shutdown ordering is
the `RasterCmd::Drain` fence from the playback fix, and two paths miss it:

- **Stepped mode (tests and renders): the fence now runs too late.**
  `alloy/src/app.rs` calls `run_stepped_loop(window, ...)` and then
  `raster.drain()`, under a comment that says nothing may be drawing when
  the window drops. Since `Move sdl event pump to ore (1)` the stepped loop
  takes the `Window` by value, so the window (and with it the EGL library)
  drops when `run_stepped_loop` returns, before the drain. The fence is in
  the code but no longer orders anything. `start_tests` runs in
  `Mode::Stepped`, which is why every test file crashes.
- **Interactive mode: no fence at all.** `AlloyEvent::Quit` in
  `lattice/src/lib.rs` runs the quit hook and calls
  `std::process::exit(0)` from the UI thread. `ExitPolicy::exit` outside a
  render calls `std::process::exit(0)` too. Neither fences the raster
  thread first.

The regression of the stepped fence is the lesson: the order lived in a
comment beside a call, and a refactor moved the drop past it without any
signal.

## Done looks like

- Every exit path of the stepped and interactive loops (a test run, a
  render, a closed window, a JS `exit()`) has nothing in a GL call when the
  window drops or the process exits.
- A loop of `sol test` runs and of dev windows closed through the
  compositor exits 0 with no new entry in `coredumpctl list`, on NVIDIA and
  on Mesa.
- The order is structural, not a comment: a future change cannot drop the
  window ahead of the raster stop without a compile error or a failing test.

## What it involves

1. Keep the raster thread's `JoinHandle` and own it next to the window it
   draws for. A stop is "send a stop command (or close the channel), let
   the frame in flight finish, release the context on that thread, join".
   It runs before the `Window` drops. The stepped loop borrows the window
   (or returns it) so the drop happens after the stop, in `app.rs`.
2. Route the interactive exits through the same stop: lattice asks alloy
   to stop the raster thread before `process::exit`, the way the render
   host already fences before its exit. `Context::drain` is the fence that
   exists today and is proven by the playback fix; a full join is the
   stronger form when the UI thread is not the caller that blocks it.
3. The raster thread's own fatal exits (`process::exit(1)` on an
   unrecoverable context loss, in `raster/mod.rs` and `raster/frame.rs`)
   are outside this race: the thread that exits is the only GL user.

Rejected as the fix:

- `_exit` instead of `exit`: it skips the driver's finalizers, but also
  every other exit handler and stdio flush, so log and result output can be
  lost. It hides the ordering bug instead of removing it.
- Pinning `libEGL` with `RTLD_NODELETE`: it removes only the `dlclose`
  path, not the `exit()` one, and reaches beneath SDL's library handling.

## Resolution (2026-11-15)

Landed:

- `alloy::stop_raster()`: sends `RasterCmd::Stop`, answered in order once
  everything queued ahead has executed, and waits for the answer (bounded by
  `RASTER_STOP_DEADLINE`, 250 ms, so a hung driver cannot hold the exit
  open). The thread then parks for the rest of the process with its GL
  context still current; it never releases the context and never ends. The
  channel closes first, so a blocking command sent after the stop fails
  instead of waiting on a parked thread. A stop from the raster thread
  itself (its panic hook) returns at once; a second stop finds nothing to
  do. `RasterCmd::Drain`, `RasterSender::drain` and `Context::drain` are
  gone; the stop replaces the fence.
- The order is structural: `App::run` holds the window in `RasterWindow`,
  whose `Drop` stops the raster thread before the `Window` field drops. The
  stepped loop borrows the window. Every return of the run loop (the stepped
  loop's end, the interactive loop's end, the Android quit) goes through
  that drop; the window cannot be moved out of a type with a `Drop`.
- Every exit on the UI side stops the thread first: the `Quit` hook task,
  `ExitPolicy::exit` on desktop, the "did not respond to back" exit, and
  both panic hooks (alloy's for the UI thread, lattice's process-wide one).

Why the thread parks with its context current instead of releasing it and
ending, which was the plan: releasing is what the driver cannot survive.
With the stop in place the dumps moved from sol-raster to the thread that
destroyed the context. The main thread faulted in `libnvidia-eglcore` from
`SDL_EGL_DestroyContext` in the window drop (headless), and the UI thread
faulted in the same eglcore function from the driver's own `exit()` handler
(interactive), with sol-raster parked and out of GL in both dumps. Before
this change neither destruction had ever actually run here: headless
destroyed a context still current on the raster thread, which EGL defers,
and the interactive exit never dropped the window. Three variants of the
stop, one test file, two runs each, on the NVIDIA proprietary driver
(615.78.08, Wayland):

- release the context, park: 2 dumps
- tear down the renderer and every GL object, release, park: 2 dumps
- keep the context current, park: 0 dumps

So it is the release (`eglMakeCurrent` with no context, on the thread the
context was current on) after which NVIDIA's Wayland EGL layer faults in
the context's destruction, by `eglDestroyContext` or by its exit handler
alike; leaked GL state makes no difference. A thread that ends releases its
context too, through the driver's thread-exit destructors (the mechanism of
the Mesa crash in [playback-shutdown-sigabrt](playback-shutdown-sigabrt.md)),
so the thread does not end either. A context current on a parked thread is
destroyed by nobody: SDL's destroy defers it, the exit handlers pass it by,
the process end reclaims it. Mesa (llvmpipe, through glvnd's vendor
override) exits clean in every variant.

Rejected on the way:

- Release, destroy the context on the main thread (`Window::destroy_gl`,
  the first half of `recreate_gl`), then let the thread end and join it:
  the release is the fault, so the handshake would crash where the plain
  stop did.
- Destroy the context on the raster thread while it is current: ore's
  contract has SDL create and destroy contexts on the main thread, and it
  would still release first inside SDL's destroy.

Measured and split out, not addressed here: `recover_gl` (context loss
recovery) releases the dead context on the raster thread and destroys it
on the main thread, the pattern that faults. A simulated loss on this
driver dies in SIGSEGV in `recreate_gl`'s `SDL_EGL_DestroyContext`, in the
same eglcore function, instead of recovering:
[context-recovery-nvidia-wayland](../backlog/context-recovery-nvidia-wayland.md).
On the way: NVIDIA's `eglGetProcAddress` answers every name with a stub, so
`glLoseContextCHROMIUM` resolved and `/gpu?lose=1` reported `forced: true`
while nothing happened; `Robustness::load` now resolves the hook by the
extension string, like the reset-status entry.

Verified, release client, against `coredumpctl list` (no new entry
anywhere):

- NVIDIA (615.78.08, Wayland): three full `sol test packages/core/tests`
  runs (35 files each, 186 tests, every file one `solidrt-go --test`
  process), all exit 0; three dev windows closed through the compositor,
  each logged as "Local client exited". Before the change every one of
  these processes dumped core; the suite also takes 17 s instead of 48 s,
  the difference being the cores written.
- Mesa (llvmpipe 26.2.5, through glvnd's vendor override): one full suite
  run and one closed dev window, both clean.

The flux gui tests, alloy's unit tests and clippy pass.
