---
title: Nested pressables
description: A press on a Pressable nested inside another fires both onPress callbacks; the contract is resolved by component-gestures' innermost-wins arena and the usage survey stands.
created: 2026-07-22
completed: 2026-07-22
---

# Nested pressables

A press on a pressable nested inside another pressable misbehaves.
Observed 2026-07-22 in the launcher: the trash icon (a `Pressable`
inside the app-card `Pressable`) does not delete. Suspected in other
places too (user report); needs a survey. Design to be worked out in
its own session.

## Mechanics as they are today (facts, code-read)

- Pointer events dispatch leaf-to-root through ancestors
  (`stopPropagation` exists and works), so a down/up over the trash
  icon ALSO runs the card's down/up handlers. This matches the DOM
  model, where nested click handlers both fire unless the inner one
  stops propagation.
- `Pressable` (`packages/components/src/pressable.tsx`) implements
  press as: primary-button down sets `pressed`, up over the node fires
  `onPress` if still pressed, `onPointerLeave` cancels. No pointer
  capture involved, and it NEVER calls `stopPropagation` - so every
  press on a nested pressable also completes a full press on every
  ancestor pressable, and both `onPress` callbacks fire.
- `Button` wraps the same machinery, so Button-inside-Pressable has
  the same behavior.
- The launcher's old inline-confirm flow stacked guards on top
  (`confirming()` signal checks in the card's onPress) - both handlers
  still ran, with signal-read ordering deciding the outcome.
- The native remove path is NOT the problem: `srt:apps` `remove()`
  verified working via probe during launcher stage 2.

## What is not yet established

- The exact failure chain on the trash icon (which of the two
  handlers won, and whether it differs between mouse and touch).
  Touch arrives as Finger events; whether synthesized enter/leave on
  touch cancels the press differently from mouse is unverified.
- Where else the pattern exists: survey components, examples, and app
  code for pressables/buttons inside pressables.
- Note: the launcher redesign (app info screen) removes its nesting by
  making the card body and the icon button siblings; that sidesteps
  the bug there but does not fix the pattern.

## Design questions (next session)

RESOLVED 2026-07-23 in okf/plans/component-gestures.md: Flutter-style
innermost-wins arena (its stage 3); raw pointer events keep bubbling,
the arena governs recognizers only. Ancestor press-state contract:
ancestors keep hover, never pressed, and never fire. IMPLEMENTED and
verified same day via examples/_press_probe.tsx (nested cards: only the
inner counter fires, outer shows no pressed state) - the trash-icon bug
class is dead on desktop. Touch-path device verification still pending;
the usage survey item stands.

- What should the contract be? Options on the table:
  - DOM-faithful: both fire; inner handlers must call
    `stopPropagation` themselves (today's contract, just undocumented
    - then components like Button should probably stop propagation by
    default, which is the common DOM idiom done for you).
  - Flutter-style gesture arena: innermost pressable wins the press
    exclusively; ancestors never fire. Simpler mental model for a
    component library; diverges from raw pointer events (which should
    keep bubbling regardless).
- Whichever contract: press state (`pressed`/`hovered` styling) on the
  ancestor while a child pressable is being pressed also needs a
  decision (the card visually pressing while you press its trash icon).
- Verify the touch path (Finger events) behaves identically to mouse
  for whatever contract is chosen; test on the Android device.
