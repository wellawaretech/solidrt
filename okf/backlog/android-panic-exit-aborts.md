---
title: Make a panic on Android end in the clean exit the panic hook intends
description: On Android a panic on a runtime thread aborts with SIGABRT on the view system's RenderThread (a destroyed mutex) instead of exiting with status 101 as lattice's panic hook intends, leaving a tombstone and a misleading second error.
created: 2026-11-16
---

# Make a panic on Android end in the clean exit the panic hook intends

## Symptom

lattice installs a panic hook (`install_panic_hook` in `lattice/src/lib.rs`).
For a panic on any thread it logs the panic, runs the default hook, stops
the raster thread (`alloy::stop_raster()`) and calls
`std::process::exit(101)`. The intent is a clean exit with status 101.

On Android the process does not end that way. Logcat shows this sequence:

1. The panic line from the hook.
2. `FORTIFY: pthread_mutex_lock called on a destroyed mutex`.
3. `Fatal signal 6 (SIGABRT)` on the thread named `RenderThread`.

So the process dies with a tombstone, and the second error hides the real
cause. A reader of the crash report sees a mutex abort in a platform thread,
not the panic.

## Likely mechanism (inference, not verified on a device)

`exit()` runs the atexit handlers and the static destructors while the other
threads still run. The view system's `RenderThread` is one of them. It then
locks a mutex that a destructor has already destroyed, and FORTIFY aborts.

## Direction

1. Verify on a device. Cause a deliberate panic on a runtime thread and read
   logcat and the tombstone.
2. If the mechanism is confirmed, make the exit immediate on Android: for
   example, an exit that skips the destructors (`_exit`). The choice of exit
   per platform is an OS concern. Put it in ore as a facility, and let
   lattice's hook call it (root `CLAUDE.md` placement rules,
   [platform-layering](../design/platform-layering.md)). Keep the raster
   stop before the exit.
3. Weigh `_exit` against its earlier rejection. For the Linux exit race it
   was turned down
   ([exit-races-raster-thread](../done/exit-races-raster-thread.md)): it
   skips every exit handler and stdio flush, so output can be lost, and it
   hides an ordering bug. On Android the case differs: the hook has already
   logged through the platform logger, and stderr is lost there anyway.
   Confirm that, and keep the immediate exit Android-only.

The same applies to every `process::exit` that can run on Android: alloy's
UI-thread panic hook (`alloy/src/app.rs`) and the raster thread's fatal
exits (`alloy/src/raster/`). One ore facility serves them all. The ordinary
quit on Android already avoids `process::exit`: it finishes the activity
through SDL.

## Known trigger, already fixed

The one known trigger was a `flux:isolate` child that was killed while its
engine started (a dev reload, or `terminate()`). The kill flag interrupted
the engine's own startup scripts, and an init-time `expect` panicked. flux
now arms the interrupt handler after the startup, and the child checks the
kill before it starts its engine. This note is about the panic path in
general: any other panic on Android still ends as described above.

## Related

- [exit-races-raster-thread](../done/exit-races-raster-thread.md): exit
  finalizers that ran while the raster thread was in a GL call, on Linux.
- [playback-shutdown-sigabrt](../done/playback-shutdown-sigabrt.md): the
  same race at headless playback shutdown.
- [go-client-emulator-launch-crash](go-client-emulator-launch-crash.md): the
  same FORTIFY destroyed-mutex abort in the view system's threads at an
  emulator launch, possibly an early native exit.
