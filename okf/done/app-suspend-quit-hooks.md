---
title: App suspend and quit hooks
description: "The env.visibility persist contract is racy today and structurally broken on iOS; replace it with a suspend hook and a quit hook whose async work the runtime waits for, plus a launch fact saying whether the system killed the previous session. Includes making Android exit() finish the activity instead of backgrounding it."
created: 2026-09-09
completed: 2026-09-10
---

# App suspend and quit hooks

`okf/done/app-lifecycle-events.md` closed with a documented contract: there is
no close event, so persist state when `env.visibility` goes hidden. That is not
a contract the runtime keeps.

## The contract does not hold

Nothing in the path waits for anything. The SDL event watch sends on a channel,
lattice forwards to the engine, and `flux/src/alloy_plugins/events.rs` posts an
`exec.exec` job to the JS thread. Fire and forget, end to end. The app's handler
starts an async write and the runtime has already moved on.

It works on Android for one reason, which that plan states as the enabling fact:
the JS thread keeps running while the pump blocks, so a promise-based write has
time to land. That is an Android property, not a guarantee.

iOS does not have it. Once the background handler returns the process is
suspended and every thread freezes within seconds, so a write or an HTTP request
started at that moment is frozen mid flight and arrives next launch truncated or
not at all - intermittently, in the field. iOS also splits the two events we
currently collapse: the persist moment wants `WILL_ENTER_BACKGROUND`, while the
visibility transition wants `DID_ENTER_BACKGROUND` for the same reason Android
does (a transition is only fact once it completed). So any contract phrased as
"persist when visibility goes hidden" is already wrong on a platform we have
committed to.

## Shape

The runtime reports the platform facts and controls the lifetime around them.
Every policy on top is the app's, so this is two registrations and one startup
fact, not a storage mechanism.

**`onSuspend(handler)`** - the app is being suspended and may be killed without
further notice. The handler may do anything, and may be async: write a file,
write a database, send an HTTP request. The runtime holds the platform open,
awaits the returned promise, and stops waiting at a documented deadline.

**`onQuit(handler)`** - the user asked to close the app. The same
wait-for-async machinery, a different fact. What happens there is the app's
business and may well be nothing: an end user quitting an application usually
does not want state saved, which is exactly why this is a hook and not a runtime
save.

**The launch fact** - whether this launch is fresh or a relaunch after the
system killed a suspended app. Reported, never interpreted; the app decides
whether restoring is appropriate. Without it, loading saved state at startup is
guessing, and an app would resume a session the user deliberately ended.

| | `onSuspend` | `onQuit` | launch fact |
| --- | --- | --- | --- |
| Android | `WILL_ENTER_BACKGROUND`, delivered at push time by the event watch in `alloy/src/app.rs` | `AlloyEvent::Quit` at `onDestroy` | `savedInstanceState` non-null in `SolidRTActivity.onCreate` |
| iOS | `WILL_ENTER_BACKGROUND` (sdl3 0.18.4 has `AppWillEnterBackground`; unmapped today) | force quit gives no signal | UIKit equivalent, to be determined |
| Desktop | never fires | `AlloyEvent::Quit`; `exit()` | always fresh |

One mapping covers both mobile platforms. `SDL_OnApplicationWillEnterBackground`
fires on Android too, immediately before the DID event in `Android_OnPause`, so
`WILL_ENTER_BACKGROUND` is the persist signal on both and no per-platform special
case is needed. `env.visibility` keeps using DID, which is the transition and is
only fact once it completed.

`onQuit` means "this app instance is ending". On Android the suspend hook has
always run first - `onPause` precedes `onDestroy` - so the quit hook there is not
a last chance, it is only for work that specifically needs a real close. `exit()`
(the sole `srt:app` verb, `lattice/src/plugins/app.rs`, and the default action of
an unprevented `back`) resolves through `ExitPolicy::exit` in
`lattice/src/lib.rs`, per host: an app running under the player ends with
`EngineCmd::Stop` while the process lives on, and the player root or standalone
runtime ends the process.

`onSuspend` never firing on desktop is correct rather than a gap: a desktop app
is not killed while minimized, so there is nothing for the signal to report. A
desktop crash is a failure, not a lifecycle event.

`env.visibility` is unchanged and keeps mapping desktop minimize alongside
Android background. It stays what its name says, a rendering and UI fact
(nothing is on screen, stop animating, pause the audio), and stops being
documented as the persistence moment.

## The hard part

Holding the platform open while JS handlers run. This is a new mechanism: `back`
is fire and forget too, with core deciding the default action on the JS side, so
there is no precedent to copy.

- Android gets roughly one second once the activity is finishing, and cannot be
  extended: `SDLActivity.onDestroy` sends quit, does `mSDLThread.join(1000)`, and
  then calls `nativeQuit()` and `super.onDestroy()` regardless. A plain
  background with the task still alive is looser, but the budget is the one that
  has to hold.
- iOS needs a background task assertion, taken before the handlers run and ended
  after, which buys tens of seconds instead of the few the delegate gets.
- Desktop needs teardown to wait for `onQuit` before `EngineCmd::Stop`.

On deadline expiry: let the platform proceed and record that the handler did not
finish. Fighting iOS there gets the app killed outright.

This is why the barrier is not an iOS-only concern. Once Android exits by
finishing the activity (below), both mobile platforms have a hard bounded window
and the same mechanism serves both.

## Decided: Android exits for real, by respecting the platform

Today `exit()` on Android does not exit. `ExitPolicy::exit` sends
`AlloyCommand::Background`, which is `window.minimize()` and routes to
`Activity.moveTaskToBack`. The process, the QuickJS heap and every signal survive,
so the app resumes exactly where it was. That was chosen as the back-at-root
convention, but stock Android back-at-root is `finish()`, and `moveTaskToBack` is
the deviation.

Decided: drop the pretence. The player loses its "Exit SolidRT?" confirmation
entirely - it promises something the platform does not do, and a modal asking
permission to leave is not an Android idiom - and `exit()` finishes the activity
instead of backgrounding it.

It needs no JNI entry point. When SDL's `main()` returns, SDLActivity finishes
the activity itself (`SDLActivity.java`, the SDLMain runnable) and sets
`mSDLMainFinished`. So `exit()` on Android becomes "let the run loop return",
the graceful sibling of the desktop `process::exit(0)` branch. The `onCreate`
guard then calls `System.exit(0)` if the process is reused on the next launch
unless `nativeAllowRecreateActivity()`, so a relaunch genuinely starts fresh
rather than inheriting native statics.

Work: remove the confirmation modal and its signal from the player, let its root
`onBack` stop calling `preventDefault()` so core's default action runs, and
replace the `AlloyCommand::Background` branch in `ExitPolicy::exit`. Whether
`AlloyCommand::Background` still has a caller afterwards is worth checking before
keeping it.

A symptom worth keeping as the illustration: pressing Exit left `confirmExit`
set, the process survived `moveTaskToBack`, and the next launch resumed straight
into the stale confirmation dialog, with the app still listed as running. Fixed
in the player separately by dismissing before leaving, but the dialog only had
state to leak because the process never died.

## What the SDL source settles

Read from `sdl3-src-3.4.10` while deciding the above, all of it load-bearing:

- **The event watch is the sanctioned mechanism, not a workaround.**
  `Android_OnPause` states it: "as soon as the enter background event has been
  queued, the app will block. The application should do any life cycle handling
  in an event filter while the event was being queued." That is exactly the watch
  in `alloy/src/app.rs`, which was arrived at from device observation.
- **Destroy discards the event queue.** `Android_OnDestroy` calls
  `SDL_FlushEvents(SDL_EVENT_FIRST, SDL_EVENT_LAST)` before `SDL_SendQuit` and
  `SDL_EVENT_TERMINATING`, commenting that state storage should already have
  happened in `SDL_EVENT_WILL_ENTER_BACKGROUND`. Anything that waits to drain the
  queue never sees the background event on a finishing activity. Delivery at
  watch time is what survives.
- **`finish()` still produces the suspend signal.** It runs the ordinary
  lifecycle, so `onPause` fires `pauseNativeThread` -> `nativePause` -> the queued
  lifecycle pause -> `Android_OnPause`, which sends WILL and then DID. Closing the
  app and backgrounding it are indistinguishable at that moment; only `onDestroy`
  a moment later tells them apart.

## This reopens the no-close-event decision, on a changed premise

That decision reads: promising a close hook we cannot honor on any platform
would be a lie. The premise was that we cannot honor it. A wait barrier makes
desktop `Quit` honorable, and the mobile side is not a close at all but a
suspend, a different fact with a different meaning. So this is not a reversal for
its own sake - the thing that made it a lie is what is being built.

## Non-goals

- **No runtime-owned state slot.** Considered and dropped: a producer returning
  bytes for the runtime to write is atomic and hard to misuse, but it forecloses
  the database and the HTTP request. The app owns its storage.
- **No persistence cadence.** Considered and dropped: it came from treating a
  desktop crash as equivalent to a mobile kill, which then needed periodic writes
  to compensate for desktop having no suspend signal. They are not the same
  category.
- **No saved-game concept.** Session continuity and a user-managed save are
  different things, and only the app can tell them apart.

## Related gap

`flux:fs` has no `rename`, so an app doing its own atomic write (temp file, then
replace) cannot. Under this design the app owns the write, which makes the
absence load-bearing rather than cosmetic.

## Decided

- Names and homes: `onSuspend` and `onQuit` live in core next to `onBack`,
  and core's `exit()` wraps the bare `srt:app` verb so the quit hooks run
  first. The vocabulary split holds: the verb stays in `srt:app`, the launch
  fact is state and lives in `env`.
- The launch fact is `env.launch`, `"fresh" | "restored"`, a plain value.
- The iOS deadline is the port's to pick, inside what the background task
  assertion buys; Android's is SDL's one-second join, which the runtime fits
  inside with a 700 ms quit deadline. Everything iOS still needs is filed in
  `okf/backlog/ios-port-readiness.md`.

## Proving consumer

A demo app under development loses its run when the app is backgrounded on
Android for more than a few seconds, while returning immediately keeps
everything.

That symptom is probably not an OS kill, and the stale-exit-dialog bug is the
evidence: the client process demonstrably survived `moveTaskToBack` across long
backgrounds, and in dev the app runs inside that same process. The likelier
cause is the dev socket dropping, the client reconnecting on resume, and the
fresh connection getting a `reload` push that rebuilds the engine. The dev
client's behavior is out of scope here and unchanged by this work.

So the demo is the shape to build against, not the proof. The case this item
actually serves is the genuine one: a suspended app the OS reclaims, which no
amount of dev-loop tidying removes.

## Findings

- The two ends need different waits. An app-initiated `exit()` is a JS call,
  so the wait is entirely on the JS side: core's `exit()` dispatches `onQuit`,
  awaits the promises (its own deadline, `EXIT_HOOK_DEADLINE_MS`), then calls
  the native verb; no native barrier exists for it. Only the platform-initiated
  ends (`AlloyEvent::Quit`, `AlloyEvent::Suspend`) go through the native gate,
  `run_hook` in `lattice/src/lib.rs`: a bus event carrying a native `done()`,
  awaited with `tokio::time::timeout`. Both platform ends and the JS exit
  share one dispatch on the JS side (`dispatchQuit`, which first awaits a
  suspend still in flight).
- The hold is a guard, not a blocked thread. `alloy::SuspendHold` is taken by
  the event watch inside the platform callback and dropped by the gate after
  the hook answers or times out; iOS's background task assertion becomes its
  implementation. Blocking the SDL/UIKit main thread instead would trip iOS's
  main-thread watchdog, and buys nothing on Android.
- The quit gate can `.await` inside the lattice events task because the task
  and the engine share one LocalSet: the JS side keeps progressing while the
  Quit arm waits. The suspend gate runs as its own `spawn_local` so later
  events (the Quit that follows a finishing Android activity) still flow.
- A hook nobody listens to must answer at once, or every window close waits
  out the deadline: `emit_hook` calls `done` immediately when the event has
  no listener, and core subscribes at module load.
- Android `exit()` = `EngineCmd::Quit`: the engine loop breaks, `ui_thread`
  returns, the LocalSet drops the event receiver, the forwarder thread stops,
  alloy's next send fails and `run` returns; `SDL_main` returns 0 and
  SDLActivity finishes the activity. `AlloyCommand::Background` had no other
  caller and is gone. Desktop still ends by `process::exit(0)`.
- JS timers are frame-driven: `setTimeout` deadlines live on the virtual
  timeline and fire only when the frame verb calls `advance_virtual_time`,
  which the lattice events task runs per Tick/FrameRendered. A gate that
  awaited inside the events task therefore froze the very timer the handler
  was waiting on (first SIGTERM run: 2 s timeout, no write), and a blocked
  Android pump produces no ticks at all during suspend. So the gate runs as
  its own local task and pumps time itself every `HOOK_TIMER_PUMP` (16 ms)
  through `flux::advance_virtual_time_to_now`, which advances to the timer
  wheel's own now-source. Promise-based IO never needed this; timers did.
- Verified on desktop (2026-09-10, `probes/lifecycle-hooks-probe.tsx`, an
  `onQuit` handler that waits 300 ms then appends a line to a file): the
  app-initiated path (`exit()` via a debug command, dev client returning to
  the player) and the platform path (SIGTERM to the client, SDL Quit, the
  gate, `process::exit`) both left the line on disk; the SIGTERM'd client
  lived about half a second, the handler's delay plus the write. Suspend and
  the Android finish need a device run.
- The launch fact (stage 2, 2026-09-10): `env.launch`, `"fresh" |
  `"restored"`, a plain value (fixed for the process, no signal) behind the
  sticky `launch` event the runner emits into every engine. Android's source
  is `savedInstanceState != null` in `SolidRTActivity.onCreate`, handed to
  native as `--restored` through `getArguments()` (the go flavor appends its
  `--dev-server` to that base list) and parsed by both `SDL_main`s into
  `lattice::Launch`; desktop passes `Launch::Fresh`.
- iOS readiness, checked rather than built (no iOS port exists; the app API
  must not change when one does): `onSuspend` rides the same
  `WILL_ENTER_BACKGROUND` from the same delegate callback, with the
  assertion slotting into `SuspendHold`; `onQuit` already documents that a
  quit signal may never come and suspend is the persistence moment, which
  is iOS's truth (`TERMINATING` maps to `Quit` for the rare case it fires);
  `exit()` is simply never reached on a platform without app-initiated exit
  or a back gesture; `env.launch` is the one open implementation: iOS has no
  saved-instance-state fact, so a port either always reports `"fresh"` or
  keeps a runtime marker at suspend. Both fit the value set and the doc
  wording, so the API holds; the choice is the port's.
- Verified on the tablet (SM-T500, Android 12, 2026-09-10, the same probe
  served over `--lan`): home press -> `[srt] suspend` in logcat, the
  handler's 300 ms wait and write landed (`suspend` line read back after
  resume, no deadline warning); `exit()` from the app under the player wrote
  its `quit` line and returned to the player; back at the player root ->
  `engine loop quit`, `Finished main function`, `onPause`, `onDestroy`, the
  activity gone; a relaunch after that finish logs `launch fresh` (the first
  attempt reuses the old process, hits SDLActivity's re-create guard and
  `System.exit(0)`s, and Android starts a new process at once, also fresh);
  home press, `am kill`, relaunch -> `launch restored` and the JS side read
  `env.launch === "restored"` with the pre-kill `suspend` line on disk.
  `SolidRTActivity.onCreate` logs the fact (`SolidRT: launch ...`) for exactly
  these traces.
- Not exercised: a finishing activity while an app (not the player) is on
  screen, which is the one case where the suspend and quit gates run back to
  back inside SDL's 1 s join. In the dev client an app's exit returns to the
  player, so that path only exists in a packed APK.
- The proving-consumer hypothesis held on the Pixel 7 (2026-09-10): the dev
  socket drops ~5 s into background, the reconnect on resume gets the
  server's push-on-connect (`currentReload` in
  `packages/cli/src/server/main.ts` `onOpen`) and the client reloads the
  engine unconditionally (`connection.rs` `"reload"`), losing the app's
  state. That is the dev loop by design (a reconnecting client takes the
  server's current app) and stays as is. The hooks are unaffected: the
  suspend line was on disk before every drop.
- The "GPU context lost" exit the demo hit on background was real for a
  raster-bound app and is fixed in alloy (a backgrounded flag set by the
  same event watch; see okf/backlog/gpu-context-loss.md findings).

