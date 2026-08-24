---
title: A <Sprite> outside a <Group> halts the app at mount
description: The unchanged packages/2d/examples/pick.tsx (a <Sprite> directly under <SpriteLayer>) threw "Context must either be created with a default value or a value must be provided before accessing it" from useContext(GroupContext) inside <Sprite>; GroupContext was created without a default, which Solid 2 rc1 no longer tolerates for an absent provider. Fixed with a null sentinel default.
created: 2026-08-24
---

# A <Sprite> outside a <Group> halts the app at mount

## Symptom

`srt run packages/2d/examples/pick.tsx` logged `[REACTIVITY_HALTED] ...
Error: Context must either be created with a default value or a value
must be provided before accessing it` at mount, from `useContext(...)` at
`packages/2d/src/components.tsx` inside `<Sprite>`; the window never
rendered. Any `<Sprite>` not nested in a `<Group>` hit it, so the whole
component face of @solidrt/2d was unusable, while the function face
(addSprite/setSprite) was unaffected.

## Cause

`let GroupContext = createContext<SpriteGroup | undefined>()` (no default)
was read unconditionally by `<Sprite>` and `<Group>` to find an optional
parent. Solid 2 rc1 throws for a default-less context with no provider
above (it used to return undefined). `LayerContext` is fine because
`<SpriteLayer>` always provides it.

## Fix

`createContext<SpriteGroup | null>(null)`: an explicit `undefined` default
is NOT enough, because `getContext` in @solidjs/signals checks the
resolved value with `isUndefined` and throws either way; the "absent"
marker has to be a non-undefined sentinel. `<Group>` passes the read
value straight to `addGroup` (whose `parent` option already accepts
`null`); `<Sprite>` already guarded with a truthiness check. The cheat
sheet's rule of thumb holds: the default-less form is for contexts a
provider always supplies, the default form is for optional fallbacks,
and "optional parent" is the second kind.

## Verified

pick.tsx mounts (the 720x720 layer texture appears under the window, no
halted or error log lines). A probe with a root-level `<Sprite
transition onTransitionEnd>` and a `<Group transition onTransitionEnd>`
holding a plain `<Sprite>` fired `{ component: "position" }` on the root
sprite and on the group after a retarget, and nothing on the nested
sprite (no declaration, own pose unchanged), which closes the
`<Sprite onTransitionEnd>` verification left open by
[spatial-settled-event-routing](spatial-settled-event-routing.md).
