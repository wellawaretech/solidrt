---
type: backlog-item
title: Focus navigation (spatial/D-pad, tab order) on the focusable registry
description: Stage 3 of the focus/key-routing work - move focus across getFocusables() candidates from bubbled arrow keys, activate with select/Enter, and fold the launcher's parallel spatial nav onto real focus.
status: open
timestamp: 2026-08-01T00:00:00Z
---

# Focus navigation on the focusable registry

Core now has the primitives (landed 2026-08-01): key events bubble from the
focused node to the window root, `focusable` declares candidacy into a
registry enumerable via `getFocusables()`, geometry comes from
`getBoundingBoxViewport`, and `startTextInput()` is the explicit non-pointer
trigger for text entry (select on a focused field). What is deliberately NOT
in core is the navigation policy itself.

The work:

- DONE 2026-08-01: `createFocusNav` in components (focus-nav.ts) -
  window-level arrow keys + gamepad dpad edges score `getFocusables()` boxes
  (launcher's ahead + 2*across metric), Tab/Shift+Tab walk visual reading
  order (rows top-to-bottom then x, wrapping), both moving real focus;
  `scope` option traps navigation in a subtree (modals), pulling stale
  outside focus in. createPress gained `focused` state,
  Enter/Space/"Select" activation (consumed), and a package-internal
  nav-action registry that gamepad south activates through. Button:
  `focusable` by default, focus ring under `policy.focusRing`. Pressable:
  `focusable` opt-in.
- DONE 2026-08-01: launcher folded in. parts/nav.tsx DELETED; NavButton ->
  Button, navTarget pressables -> `<Pressable focusable>` with a local
  focusRing helper (parts/types.ts), the settings mode row a focusable
  Pressable wrapping the SegmentedControl. Modal trapping moved into
  components: Modal pushes its container onto a nav scope stack
  (pushNavScope in focus-nav.ts) that is every nav's default scope, so the
  `modal` flag disappeared entirely. policy.focusRing now also counts
  gamepads (TV remotes register as gamepads; keyboard-free TVs were
  ringless). Desktop-verified 2026-08-01 (keyboard + SNES pad); TV round
  still open. Follow-up fix same day: navigation resumes at the nearest
  candidate to where focus last sat when the focused control vanishes (a
  button swapped by its own action, e.g. Disconnect -> Connect), instead of
  restarting at the top-left; modals still enter at their first button.

Traps for whoever picks this up:

- The registry is candidacy only and non-reactive; recompute candidate
  geometry per keypress, not per registration (boxes move under layout).
- TV text fields have two states: focused (highlight) and editing (session
  started). Enter on a focused field must call `startTextInput()`, not
  submit; Enter while editing submits. TextInput does not distinguish these
  yet (it is also not `focusable` yet, deliberately).
- With a physical keyboard attached (Android TV + USB) the text session
  starts eagerly at focus and no on-screen keyboard may ever appear -
  do not "fix" typing-without-select on TV by forcing startTextInput.

Deliberately deferred from the components stage (inherited from the
launcher's own stage-1 gaps): scroll-into-view for a focused off-screen
candidate, held-dpad auto-repeat on gamepads (keyboards repeat on their
own), pressed-state visuals on key activation (the ring is the feedback),
and `focusable` on the other press controls (Switch/Checkbox/Radio/...) -
their keyboard activation already works via createPress once declared.
