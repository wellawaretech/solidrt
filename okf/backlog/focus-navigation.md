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

- A `createSpatialNav` (components tier, probably) that listens for arrow
  keys at the window level, scores `getFocusables()` boxes directionally
  from the currently focused node, and calls `setFocus` on the winner.
  Select/Enter activates: press for pressables, `startTextInput()` for text
  fields.
- Pressable keyboard activation (Enter/Space -> press) plus `focusable` on
  Pressable, so buttons participate. This is also the moment focus-visible
  needs an origin signal (ring for key-driven focus, none for pointer);
  `policy.focusRing` already exists on the components side.
- Fold the launcher's hand-rolled spatial nav onto this, removing its
  parallel selection state - the original motivation (TV navigation is a
  requirement for the launcher).

Traps for whoever picks this up:

- The registry is candidacy only and non-reactive; recompute candidate
  geometry per keypress, not per registration (boxes move under layout).
- TV text fields have two states: focused (highlight) and editing (session
  started). Enter on a focused field must call `startTextInput()`, not
  submit; Enter while editing submits. TextInput does not distinguish these
  yet.
- With a physical keyboard attached (Android TV + USB) the text session
  starts eagerly at focus and no on-screen keyboard may ever appear -
  do not "fix" typing-without-select on TV by forcing startTextInput.
