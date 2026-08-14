---
title: Exit to launcher
description: "Leave the running app without a client restart: one native ExitRequest dispatching a preventable window-level back event, with a watchdog that exits anyway if the engine wedges."
created: 2026-07-22
completed: 2026-07-22
---

# Exit to launcher

> Superseded in part 2026-07-28: the desktop BrowserBack carve-out
> below is reversed - AC_BACK is now a back trigger on every platform,
> and the gamepad "back" (select) button is a third trigger. See
> `okf/plans/launcher-remote-nav.md`.

Let the user exit the running application and return to the launcher
without restarting the client. Deferred out of scope by
`okf/plans/go-client-launcher.md` ("needs a client-owned gesture").
Designed, implemented and verified 2026-07-24 (desktop chord at the
launcher; back on an Android device). Apps must be bundled against
the new core to get the default exit-on-back (core's `back` listener
ships in the bundle); pre-change bundles need a re-push.

## Decided design

### One normalized back intent

All back triggers funnel into a single native event before anything
decides what it means. Named `AlloyEvent::Back` (renamed from the
draft's "ExitRequest" during implementation, 2026-07-24: the event
carries the user's intent - go back - and exiting is only the default
action when no handler claims it; the name also mirrors the JS `back`
event, like WindowFocus/windowFocus):

- Android: back button and back swipe, both arriving as
  `SDLK_AC_BACK` (see mechanics below). `SDL_ANDROID_TRAP_BACK_BUTTON`
  is set to `1` permanently; no per-screen hint flipping. One code
  path decides instead.
- Desktop: Ctrl/Cmd+Shift+Backspace (decided 2026-07-24; Backspace's
  old browser-back mnemonic, no known OS/app collisions; deliberately
  not Esc or Ctrl/Cmd+W). Dev-flow affordance; production desktop
  needs nothing new. Caveat found in review: desktop keyboards have a
  real BrowserBack media key that also arrives as AC_BACK, so the
  AC_BACK -> ExitRequest translation is Android-only; on desktop that
  key stays a normal `keydown "BrowserBack"`.
- Desktop window close stays `AlloyEvent::Quit` and quits the process
  directly. It is the un-interceptable floor and never enters JS.

AC_BACK and the chord are swallowed at event translation: they must
not additionally ride the generic key path as `keydown`, or apps see
both a `back` event and a raw key and handle them inconsistently.

### JS surface: window-level `back` event with a preventable default

Back does not act natively (except the watchdog below). It
dispatches a `back` event into JS:

- Window-level, not a tree event. Back has no hit position and no
  natural target node (Android's own model is activity-level). No
  bubbling story in stage 1; if focus traversal lands later, tree
  dispatch can be added without changing the API shape.
- Consumption signal is `preventDefault()`, never propagation.
  Core's leave is the event's default action, exactly the standard
  web shape. (stopPropagation-as-consumption was considered and
  rejected: an app that pops a screen but forgets to stop propagation
  would pop AND exit - a double-action bug - and unrelated
  middleware stopping propagation would silently eat back.)
- Core installs the default action: if no handler prevented it,
  core calls native `exit()`. Apps implement nothing to get correct
  exit behavior; they only add a handler to keep the user in
  (in-app navigation: pop a modal, previous screen).
- `exit()` is also exposed to apps. Required for the
  unsaved-changes flow: preventDefault, show a dialog, then exit
  programmatically on "discard". Without it, intercepting is a
  one-way trap. Naming decided 2026-07-24: `exit` (standard term;
  "leave" was considered and dropped). It reads correctly from the
  app's side - the app exits itself; what that means (launcher /
  process death / backgrounding) is the host's decision. The hosting
  module is still open, to confirm alongside the lifecycle vocabulary
  (`okf/plans/app-lifecycle-events.md`).

### Native watchdog (the client-owned guarantee)

The launcher plan's constraint stands: exiting must not depend on app
cooperation. Routing exit through JS would break that for a wedged
engine (infinite loop, blocked event loop): the event never
dispatches, core's default never runs, and with the trap hint at 1
the activity no longer finishes itself - the user would be trapped.

So: when native dispatches a Back, it arms a timeout (~2s). If JS
neither consumed the event nor called `exit()` by then, native acts
itself. Well-behaved apps never notice it; a hung app cannot hold
the user. This is in scope for stage 1 - it IS the constraint, not
hardening.

Implementation shape (decided 2026-07-24): a liveness probe, no JS
ack protocol. Bus dispatch is synchronous on the JS executor, so
lattice queues the `back` emit, then queues a no-op closure that
sets a flag, and starts the timer. Flag set = JS processed the
dispatch (prevented, or `exit()` already ran); flag unset at the
deadline = wedged engine. An app that responsively prevents every
back is legal and is NOT overridden by the watchdog; the user's
escape is the desktop window-X and Android's un-trappable
home/recents (see hardening note below).

Known edge, accepted 2026-07-24 (nothing shipped, so no compat
concern): core registers its `back` listener in attachWindow, so an
app that keeps the engine alive without ever mounting a Window has
zero listeners and back stays silent (engine responsive, watchdog
rightly quiet). Escape = window-X / home+recents. A native
zero-listener fallback (flux::has_listeners in the probe) was
implemented and deliberately reverted as not needed; if this ever
bites, the cheaper fix is registering core's listener at module
init instead of attachWindow.

Deviation found during implementation (2026-07-24): on timeout the
watchdog QUITS THE PROCESS (exit code 1) rather than Stop-to-
launcher. JS runs on the same thread/LocalSet as the EngineCmd
select, so a wedged engine also blocks Stop processing - there is no
graceful path without interrupt-driven JS abort (rquickjs interrupt
handler), which is the noted future refinement. The watchdog task
itself runs on the multi-thread tokio runtime for the same reason.
A generation counter (bumped per engine build) keeps a watchdog
armed against a dying app from firing into its successor (the
double-back-during-transition race).

### What leaving means (context branch, decided in lattice/client)

| Context                                            | Leave does                    |
|-----------------------------------------------------|-------------------------------|
| Standalone runtime (one app, no launcher)           | quit                          |
| Client, app running (dev push or installed launch)  | `EngineCmd::Stop` -> launcher |
| Client at launcher root                             | quit                          |

Context detection is native-known: artifact type distinguishes
standalone from client; "current source is LAUNCHER_SOURCE"
distinguishes launcher root from a running app.

Quit per platform:
- Desktop: process exit.
- Android: finish/background the activity (moveTaskToBack), not
  process death. Since Android 12 the system's own back-at-root
  behavior backgrounds the task rather than finishing it; trapping
  back and then exit(0) would be harsher than the platform and lose
  warm start. Applies equally to the standalone runtime on Android.

### The launcher uses the same API

The launcher is JS, so it composes for free: its scan/manual/settings
and narrow-layout detail screens handle `back` + `preventDefault()`
to pop to home; at home it leaves the event alone, the default runs,
and `exit()` at launcher root = quit (backgrounding the activity on
Android, stock feel). This removes the per-screen trap-hint flipping
sketched earlier.

### Stage 1 scope

1. Native ExitRequest normalization (AC_BACK + desktop chord), trap
   hint to 1, swallow both from the generic key path.
2. Window-level `back` event into JS; `preventDefault()` suppresses
   the default.
3. Core default action calling native `exit()`; `exit()` exposed to
   apps.
4. `exit()` context branch (table above). The Stop re-anchor fix
   turned out to already exist upstream (the Stop arm re-anchors to
   the default sandbox and resets fonts; landed with the tear-down
   work of 2026-07-23) - nothing to do.
5. Native watchdog timeout backing every Back.
6. Desktop window-X unchanged as the un-interceptable floor.

Out of scope: lifecycle/visibility events (see
`app-lifecycle-events.md`; the JS vocabulary of `back`/`exit` and
close/visibility should be reviewed together before either ships),
predictive-back animation, focus-based tree dispatch of `back`,
further Android hard-floor hardening beyond the watchdog
(double-back-to-exit etc.).

All previously open details are decided 2026-07-24: the desktop
chord is Ctrl/Cmd+Shift+Backspace, and `exit()` lives in a new
`srt:app` lattice module ("the running app's own surface", the
future home for lifecycle verbs; verified that `srt:apps` is
imported only by the launcher, so the adjacent name is acceptable).
Apps consume both through `@solidrt/core`: `onBack(fn)` and `exit`;
the `back` event itself rides the srt:events bus internally.

## What already exists

- `EngineCmd::Stop` (`lattice/src/lib.rs`) is exactly "return to the
  launcher": it restarts the engine into `LAUNCHER_SOURCE`, the same
  restart machinery dev pushes and `srt:apps` `launch()` use (that
  mid-session path is verified working). Today only the dev session
  sends it (`go/connection.rs`, on a server stop).
- The draft's "Stop does not re-anchor" wrinkle no longer exists:
  the Stop arm re-anchors to the startup default sandbox and resets
  the font set (landed with the tear-down/re-anchor work,
  2026-07-23).

## Android back mechanics (established 2026-07-22)

- Stock Android delivers both the back button and the back swipe
  gesture as `KeyEvent` `KEYCODE_BACK`. Our vendored SDLActivity
  forwards it into native SDL as a normal key event: scancode
  `SDL_SCANCODE_AC_BACK`, keycode `SDLK_AC_BACK`.
- The hint `SDL_ANDROID_TRAP_BACK_BUTTON` decides what happens beyond
  that key event. Our manifest sets it to `0`, so `onBackPressed()`
  runs the system default and the activity finishes (leave the app).
  Set to `1`, the activity stays alive and the AC_BACK key event is the
  only signal. (Decided design sets it to `1` permanently.)
- The hint is read dynamically at each press (`nativeGetHintBoolean`
  in `onBackPressed`), so it can be flipped at runtime with
  `SDL_SetHint`. The decided design does not use runtime flipping.
- Predictive back (Android 13+, `android:enableOnBackInvokedCallback`)
  would replace the KeyEvent delivery with an `OnBackInvokedCallback`.
  SDL does not opt in, so the KeyEvent path keeps working; the cost is
  no predictive-back animation.
- Nothing in alloy or flux special-cases AC_BACK today: it rides the
  generic key path (`AlloyEvent::KeyDown` -> `emit_key` -> JS
  `keydown`) with `key` set to SDL's name for it ("AC Back"). With
  the hint at `0` the activity is already finishing, so in practice
  alloy never sees it today. The decided design intercepts it at
  translation instead.

## Constraints

- The trigger is client-owned (launcher plan decision): exiting must
  not depend on app cooperation. Honored by the native watchdog plus
  the desktop window-X floor; app handling of `back` is a courtesy
  path to keep the user in, never required to let them out.
- Round-trip caveat carried over from the launcher plan: fonts
  register only at client startup, so a launched store app's custom
  fonts still wait for a client restart. Exit-to-launcher does not
  change that.
