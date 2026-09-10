---
title: iOS port readiness
description: The app-facing contracts are designed so an iOS port changes no app code; this lists what the port itself still has to supply behind them (the suspend hold, the launch fact's source, no GPU work in the background), so nothing is rediscovered when the port starts.
created: 2026-09-10
---

# iOS port readiness

There is no iOS port and no iOS code. The rule since the lifecycle work
(`okf/done/app-suspend-quit-hooks.md`) is that apps must not change when
the port arrives: every app-facing contract is written for Android, desktop
and iOS at once, and the platform-specific half is a seam the port fills.
This note is the list of those seams, so the port does not have to
rediscover them. It is not a plan for the port.

## Seams the port fills

- **The suspend hold.** `alloy::SuspendHold` (alloy/src/event.rs) is taken
  by the event watch inside the platform callback at `WILL_ENTER_BACKGROUND`
  and dropped when the suspend hook answers or times out. On iOS its
  implementation is a UIKit background task assertion: begin in `take`, end
  in `Drop`, and end again in the expiration handler. The hook's deadline
  (`SUSPEND_HOOK_DEADLINE` in lattice/src/lib.rs, 10 s) must sit inside what
  the assertion buys. No app or lattice change.
- **The quit signal.** iOS gives none on a force quit and rarely fires
  `applicationWillTerminate`; SDL maps that to `TERMINATING`, which is
  already `AlloyEvent::Quit`. The docs already say suspend is the
  persistence moment and a quit signal may never come. Nothing to add.
- **`exit()`.** Never reached on iOS: no app-initiated exit, no back gesture.
  The native branch in `ExitPolicy::exit` is per platform; iOS needs none.
- **The launch fact.** `env.launch` is `"fresh" | "restored"`. Android's
  source is `savedInstanceState`; iOS has no equivalent by default, so the
  port either always reports `"fresh"` or keeps a runtime marker written at
  suspend and cleared at a clean end. Both fit the value set and the doc
  wording; which one is the port's call, made against what iOS actually
  restores by then.
- **No GPU work in the background.** iOS kills an app that touches the GPU
  while backgrounded. `WINDOW_BACKGROUNDED` (alloy/src/lib.rs, from the same
  `WILL_ENTER_BACKGROUND` watch) already stops window frames; the port also
  has to hold the offscreen work that still runs at the top of the raster
  `frame` (`flush_dirty`, shader-target writes) and any texture uploads
  queued behind it.
- **Visibility.** `DID_ENTER_BACKGROUND` / foreground map to `env.visibility`
  as on Android; SDL's UIKit backend sends the same events.

## What the port must not do

Change `onSuspend`, `onQuit`, `exit`, `env.launch` or `env.visibility` in
shape or meaning. If a seam above turns out not to fit, the fix is behind the
seam, and the app contract is reconsidered only with that evidence in hand.
