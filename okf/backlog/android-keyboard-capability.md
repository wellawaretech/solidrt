---
title: Android reports a keyboard that is not there
description: On a touch-only Android device the capability layer gains "keyboard" as soon as the virtual keyboard opens, so any keyboard-first behavior gated on it would switch on for users who have no keyboard.
created: 2026-08-14
---

# Android reports a keyboard that is not there

Observed on device: the gallery's "Inputs seen" row gains `keyboard` the moment
the on-screen keyboard opens, and keyboard-nav may have appeared in
capabilities alongside it. On a phone with no physical keyboard, both are
wrong.

It does not matter much today because nothing gates on it, which is exactly why
it should be fixed before something does: `keyboardNav` is meant to drive focus
rings and keyboard-first behaviors, and a phone that claims a keyboard would
get focus rings nobody asked for.

Two suspects, independent of each other:

**(a) `env.keyboardSeen` counts soft-keyboard keydowns.** The fallback is
documented in [packages/core/src/environment.ts:132](../../packages/core/src/environment.ts) -
"seen" means a keydown arrived, and a virtual keyboard produces keydowns. It
could be tightened to only count keydowns while the on-screen keyboard is
hidden; `keyboardVisibility`/`keyboardHeight` already exists as a signal, so
the ingredients are there.

**(b) `SDL_HasKeyboard` may return true on touch-only Android.** It feeds the
sticky inputDevices event through `has_keyboard`
([alloy/src/sdl_utils.rs:62](../../alloy/src/sdl_utils.rs)). Read SDL's Android
backend and check whether it calls `SDL_AddKeyboard` unconditionally.

## How to tell them apart

On the phone, read the gallery's Devices row (presence, from (b)) against the
Inputs-seen row (usage, from (a)) with the keyboard closed and then open. If
Devices already claims a keyboard before any typing, (b) is in play.

If presence is the one misreporting, the choice is to fix detection on Android
(a JNI `Configuration.keyboard == KEYBOARD_QWERTY` query) or to stop letting
presence win for keyboard on Android and rely on usage alone. Note
`sdl_utils.rs:80` already ORs `has_keyboard()` with a separate
`hardware_keyboard()` signal, so part of the machinery for the first option may
exist.

Source: root TODO.md, migrated 2026-08-14.
