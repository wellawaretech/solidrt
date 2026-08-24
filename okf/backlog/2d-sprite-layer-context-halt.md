---
title: A <Sprite> outside a <Group> halts the app at mount
description: The unchanged packages/2d/examples/pick.tsx (a <Sprite> directly under <SpriteLayer>) throws "Context must either be created with a default value or a value must be provided before accessing it" from useContext(GroupContext) inside <Sprite>, halting the reactive system; GroupContext is created without a default, which Solid 2 rc1 no longer tolerates for an absent provider.
created: 2026-08-24
---

# A <Sprite> outside a <Group> halts the app at mount

## Symptom

`srt run packages/2d/examples/pick.tsx` logs `[REACTIVITY_HALTED] ...
Error: Context must either be created with a default value or a value
must be provided before accessing it` at mount, from `useContext(...)` at
`packages/2d/src/components.tsx` inside `<Sprite>`; the window never
renders. Any `<Sprite>` that is not nested in a `<Group>` hits it, so the
whole component face of @solidrt/2d is unusable, while the function face
(addSprite/setSprite) is unaffected.

## Cause

`let GroupContext = createContext<SpriteGroup | undefined>()` (no default)
is read unconditionally by `<Sprite>` and `<Group>` to find an optional
parent. Solid 2 rc1 throws for a default-less context with no provider
above (it used to return undefined). `createContext<SpriteGroup |
undefined>(undefined)` - an explicit default - is the likely one-line fix;
`LayerContext` is fine because `<SpriteLayer>` always provides it.
Surfaced while verifying
[spatial-settled-event-routing](../done/spatial-settled-event-routing.md),
whose `<Sprite onTransitionEnd>` prop is unverified until this is fixed.
