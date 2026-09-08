---
title: Pointer gestures ignore every button but the primary
description: createPointerFeed returns early on any non-primary button, so Three's right-drag pan and middle-drag dolly cannot be bound; a button qualifier wants the chord grammar the feed now has, with the button as the trailing part ("Shift+Middle") the way a key spec ends in its key.
created: 2026-09-08
completed: 2026-09-08
---

# Pointer gestures ignore every button but the primary

## Symptom

`onPointerDown` in packages/core/src/input-pointer.ts returns on
`e.button !== 0`, so a right or middle drag never opens a gesture. The
mouse half of Three's OrbitControls default (left rotate, middle dolly,
right pan) and Blender's middle-drag orbit cannot be bound; the only
desktop pan is the Ctrl-drag chord of
[pointer-gesture-modifiers](../done/pointer-gesture-modifiers.md).

## Done looks like

The chord spec grows a trailing button word, mirroring a key spec's
"modifiers then key": `pointer.drag("Right")`, `pointer.drag("Shift+Middle")`.
A bare spec keeps meaning the primary button, so nothing bound today
changes. A right-drag still has to be told apart from a context-menu
press on platforms that synthesize one, and a touch has no buttons, so
as with modifiers the bare binding stays the finger path.

## What was done

The spec's trailing word may be Left, Middle or Right (bare is Left),
parsed in the feed since a button is the pointer's own concept; buckets
carry the button as a discriminator ahead of the chord rule;
`createTransform` gained `buttons: "any"` (default primary, the feed
opts in and tells its buttons apart itself); the feed tracks the button
per pointer so a second button on a held mouse joins nothing and only
the held button's release closes the gesture; `wheel` and `mouseDelta`
refuse a button word. `orbitBindings` binds `pan` to `drag("Right")`.
Verified on probes/pointer-chord-probe.tsx through the real pipeline: a
right drag moves the target and leaves azimuth at its exact value, and a
right press and release inside a left drag neither ends nor splits it.

Deliberately not done: the middle-drag dolly. `zoom` is an axis and a
drag is a vec2, so it needs a processor projecting a vec2 onto one axis
before a preset can bind it; nothing in the repo asks for one.
