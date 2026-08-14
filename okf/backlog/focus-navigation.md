---
title: Focus navigation (spatial/D-pad, tab order) on the focusable registry
description: Stage 3 of the focus/key-routing work - move focus across getFocusables() candidates from bubbled arrow keys, activate with select/Enter, and fold the launcher's parallel spatial nav onto real focus.
created: 2026-08-01
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
  ringless). ALL DEVICE-VERIFIED 2026-08-01: desktop (keyboard + SNES pad),
  Android TV (remote nav, select-to-edit raising the IME, typed text), and
  Android tablet (single-tap keyboard, outside-tap blur hides it). Fixes
  found on the way: a frozen derivation memo (short-circuit skipped the
  signal read) and stale same-dispatch signal reads (focus/session are now
  plain-field truth + signal purely for tracking; signal writes flush on
  the microtask). Follow-ups same day: core focus became a reactive accessor
  (`focusedNode()`, replacing getFocusedNodeId; setFocus sole writer), and
  nav now auto-refocuses the nearest successor when the focused control is
  DESTROYED (button swapped by its own action, screen change) - the landing
  waits for the next onLayout because the successor has no box until the
  frame the swap scheduled. Deliberate blurs (outside tap, keyboard
  dismissal) stay blurred: told apart by whether the old node still resolves
  (getNodePath non-empty). createPress/TextInput now DERIVE focused from
  focusedNode() (memoized) instead of tracking onFocus/onBlur; modals still
  enter at their first button.

Traps for whoever picks this up:

- The registry is candidacy only and non-reactive; recompute candidate
  geometry per keypress, not per registration (boxes move under layout).
- DONE 2026-08-01 (surfaced immediately on the TV round: focused address
  field + remote did nothing): focused vs editing. Core exports the
  reactive `textInputActive()`; TextInput's Enter/select (code "Select")
  runs activateField - startTextInput() when no session (raises the TV
  IME), submit + blur while editing - and registers it as its nav action so
  a controller's south button works too.
- With a physical keyboard attached (Android TV + USB) the text session
  starts eagerly at focus and no on-screen keyboard may ever appear -
  do not "fix" typing-without-select on TV by forcing startTextInput.
- Android keyboard detection is externality-based (MainActivity
  isRealKeyboard): TV built-in drivers (Philips TPV_*, MediaTek mtkinp)
  claim ALPHABETIC keyboards, and Configuration.keyboard lies QWERTY, so
  only isExternal()/nonzero vendor-product separates a real keyboard.
  Hotplug via InputDeviceListener (config change does not fire when config
  already claims QWERTY). TV-verified 2026-08-01 via focus-test.tsx (repo
  root; MCP-driven test bed - keep it): session start raises the TV IME,
  typed text flows. IME auto-capitalization: DONE 2026-08-01 - the
  `textInputHints` node prop (type/capitalize/autocorrect/multiline) flows
  core registry -> setTextInputActive(active, hints) -> AlloyCommand ->
  SDL_StartTextInputWithProperties (sdl_utils wrapper; crate lacks it). A
  session hopping between fields restarts on the new node so its hints
  apply. TextInput exposes it as `hints`; the launcher address field is
  capitalize-none/no-autocorrect and the port field type "number". Old
  runtimes ignore the extra argument; effect needs a client rebuild.
  Device-unverified until then.

Deliberately deferred from the components stage (inherited from the
launcher's own stage-1 gaps): scroll-into-view for a focused off-screen
candidate, held-dpad auto-repeat on gamepads (keyboards repeat on their
own), pressed-state visuals on key activation (the ring is the feedback),
and `focusable` on the other press controls (Switch/Checkbox/Radio/...) -
their keyboard activation already works via createPress once declared.
