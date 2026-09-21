---
title: SDL_WaitEventTimeout busy-waits on Android because the poll sentinel wakes the lifecycle semaphore
description: On Android every SDL_PumpEvents pushes a poll sentinel, every pushed event sends the lifecycle WAKE, and the event wait blocks on that same semaphore, so SDL_WaitEvent/SDL_WaitEventTimeout spin through their whole timeout at 100% CPU; SDL_POLL_SENTINEL=0 is the workaround.
created: 2026-09-21
status: unfiled
project: SDL (libsdl-org/SDL)
versions: SDL 3.4.10 (sdl3-src 3.4.10); same code in release-3.4.16 and main as of 2026-09-21
link:
---

# SDL_WaitEventTimeout busy-waits on Android because the poll sentinel wakes the lifecycle semaphore

## Summary

On Android, `SDL_WaitEvent` and `SDL_WaitEventTimeout` never block. The
main thread spins at 100% of one core for the entire wait, even when the
app is idle. The cause is a self-wakeup: the event pump pushes the poll
sentinel event, pushing any event sends the Android lifecycle WAKE, and the
Android wait loop blocks on the very semaphore that WAKE signals.

Desktop platforms are not affected, because there the wakeup is gated on
`wakeup_window`, which is only set while the backend is inside its own
`WaitEventTimeout`.

## The chain (src/events/SDL_events.c, src/core/android/SDL_android.c)

`SDL_WaitEventTimeoutNS` has a dedicated Android path:

```c
#ifdef SDL_PLATFORM_ANDROID
    for (;;) {
        SDL_PumpEventsInternal(true);

        if (SDL_PeepEvents(event, 1, SDL_GETEVENT, SDL_EVENT_FIRST, SDL_EVENT_LAST) > 0) {
            return true;
        }
        ...
        Android_PumpEvents(delay);
    }
```

`Android_PumpEvents(delay)` calls `Android_WaitLifecycleEvent`, which blocks
in `SDL_WaitSemaphoreTimeoutNS(Android_LifecycleEventSem, delay)`. That
semaphore is signaled by `Android_SendLifecycleEvent`.

`SDL_PumpEventsInternal(true)` ends by pushing the poll sentinel:

```c
    if (push_sentinel && SDL_EventEnabled(SDL_EVENT_POLL_SENTINEL)) {
        ...
        sentinel.type = SDL_EVENT_POLL_SENTINEL;
        SDL_PushEvent(&sentinel);
    }
```

`SDL_PushEvent` adds through `SDL_PeepEventsInternal`, whose tail sends the
wakeup for every successful add, the sentinel included:

```c
    if (used > 0 && action == SDL_ADDEVENT) {
        SDL_SendWakeupEvent();
    }
```

and on Android the wakeup is unconditional:

```c
static void SDL_SendWakeupEvent(void)
{
#ifdef SDL_PLATFORM_ANDROID
    Android_SendLifecycleEvent(SDL_ANDROID_LIFECYCLE_WAKE);
#else
    ... gated on _this->wakeup_window ...
#endif
}
```

So one pass of the wait loop is:

1. `SDL_PumpEventsInternal(true)`: `Android_PumpEvents(0)` drains the
   previous WAKE with a try-wait, then the sentinel push queues a new WAKE
   and signals the semaphore.
2. `SDL_PeepEvents` finds no real event.
3. `Android_PumpEvents(delay)`: the semaphore already has a count, the
   wait returns immediately with the WAKE, sets its timeout to zero, and
   returns.
4. Back to 1.

The loop only leaves when a real event arrives or the timeout expires. The
WAKE dedup in `Android_SendLifecycleEvent` (at most one WAKE queued) does
not help, because each pass consumes the queued WAKE before the next
sentinel push re-queues it.

The semaphore itself is a real `sem_timedwait` (`HAVE_SEM_TIMEDWAIT` is
set by the CMake Android build), so this is not the polling fallback in
`SDL_syssem.c`.

## Reproduction

Any Android app whose main loop blocks in `SDL_WaitEvent` or
`SDL_WaitEventTimeout(ms > 0)`. Observe the SDL main thread at 100% CPU with
no events flowing, for example with `top -H -p <pid>`, or read
`voluntary_ctxt_switches` from `/proc/<pid>/task/<tid>/status` for the SDL
thread over a few seconds: it stays at zero.

Measured with an SDL 3.4.10 app on a Samsung SM-T500 (Android 12), main
loop in `SDL_WaitEventTimeout` with a 16 ms timeout, app idle:

| | SDL main thread |
|---|---|
| CPU over 5 s | 5.3 s (99%) |
| voluntary context switches over 5 s | 0 |

A Pixel 7 (Android 17) shows the same. The same app on Linux (Wayland)
reads 7% process CPU with the same loop.

With `SDL_SetHint(SDL_HINT_POLL_SENTINEL, "0")` before `SDL_Init`, same
device, same idle app:

| | SDL main thread |
|---|---|
| CPU over 5 s | 104 ms (2%) |
| voluntary context switches over 5 s | 358 |

Animating at 60 fps with the hint: 270 ms CPU over 5 s, 785 voluntary
switches, no dropped frames. The disabled sentinel is the only difference.

## Suggested fix

Do not send the wakeup for the sentinel push. The sentinel is only ever
pushed by the pumping thread itself, so nothing waits to be woken by it.
One way:

```c
    if (used > 0 && action == SDL_ADDEVENT && events[0].type != SDL_EVENT_POLL_SENTINEL) {
        SDL_SendWakeupEvent();
    }
```

An alternative that keeps `SDL_PeepEventsInternal` untouched is to have the
Android `SDL_SendWakeupEvent` only queue the WAKE while the main thread is
inside `Android_PumpEvents` with a non-zero timeout, mirroring the
`wakeup_window` gate the desktop path already has.

## Our workaround

`alloy/src/app.rs` sets `SDL_POLL_SENTINEL` to `0` on Android before
`SDL_Init`. The cost is that every `SDL_PollEvent` then pumps and a poll
drain is no longer bounded against an event flood; acceptable for our
loop, which drains one queue per iteration. Remove the hint once a fix
lands and the pinned SDL carries it.
