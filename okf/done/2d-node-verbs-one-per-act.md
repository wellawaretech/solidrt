---
title: One 2d verb per act over sprites and groups - setTransition, setTimeScale, setParent, destroy
description: @solidrt/2d names its node verbs per handle kind (setSpriteTransition/setGroupTransition, setSpriteTimeScale/setGroupTimeScale, destroySprite/destroyGroup/destroyText, setSpriteParent against setGroup's parent), where its own worldPosition and timeRate already take Sprite | SpriteGroup and @solidrt/3d has one verb per act; the split costs the package its own code.
created: 2026-11-05
completed: 2026-11-05
---

# One 2d verb per act over sprites and groups

## Symptom

The 2d package spells one act two or three ways, by handle kind:

| act | sprite | group | text run | @solidrt/3d |
| --- | --- | --- | --- | --- |
| declare native motion | `setSpriteTransition` | `setGroupTransition` | - | `setTransition` |
| declare a time scale | `setSpriteTimeScale` | `setGroupTimeScale` | - | `setTimeScale` |
| let go | `destroySprite` | `destroyGroup` | `destroyText` | `destroy` |
| re-parent | `setSpriteParent(s, p, opts)` | `setGroup(g, { parent, keepWorld })` | - | `add(parent, child)` |

Three things say this is drift, not a decision:

- The package is mixed inside itself: `worldPosition` and `timeRate`
  already take `Sprite | SpriteGroup`. The sprite and group bodies of
  each pair above are near copies over the same `spatial.*` call.
- The package's own code pays for the split. `region.ts` branches on
  `"_children" in x` to pick `destroyGroup` or `destroySprite` (the
  proxy teardown and the cell-children loop), the test the generic verb
  would carry once.
- Re-parenting is two shapes for one act: a group re-parents through its
  per-frame bag, a sprite through a separate verb, although `addSprite`
  takes `parent` in its bag.

It is not one of the settled asymmetries
([2d-3d-vocabulary-drift](../done/2d-3d-vocabulary-drift.md)): those are
the creation grammar and the destroy-only removal, both of which this
item keeps.

## Shape

One verb per act, the handle kind a union, 3d's word where 3d has the
act:

- `setTransition(target: Sprite | SpriteGroup, transition)` - the group
  form keeps `stagger` and its uniform `scale`; on a record sprite it
  throws as `setSpriteTransition` does today.
- `setTimeScale(target: Sprite | SpriteGroup, scale)` - the same throw
  on a record sprite.
- `destroy(target: Sprite | SpriteGroup | TextRun, opts?: DestroyOptions)`
  - one rule with the creation grammar: what you add into a layer
  (`addSprite`, `addGroup`, `addText`), you let go with `destroy`. Record
  sprites included, as `destroySprite` takes them today.
- `setParent(target: Sprite | SpriteGroup, parent, opts?: ReparentOptions)`
  - `{ keepWorld }` as today; `parent` leaves `setGroup`'s bag, so both
  per-frame bags (`setSprite`, `setGroup`) carry pose and style only.
  `parent` stays in the `addSprite`/`addGroup`/`addText` bags: that is
  where a handle is born, not a move.

The old names go; no compatibility layer
(the components, demos and examples move with them).

A run is let go of as a whole: `destroy(run)` removes it from its font's
listeners and destroys its group. Its pose and its parent are its group's,
so a run moves with `setParent(run.group, ...)`, as it poses with
`setGroup(run.group, ...)` today; only `destroy` takes the run itself,
because `addText` made it.

`setParent` has no 3d counterpart on purpose: in 3d, `add(parent,
child)` re-parents because a 3d node can exist detached; in 2d `add`
means "create into the layer", so the move needs its own word.

Names shared with 3d (`setTransition`, `setTimeScale`, `destroy`) are
intended, like the 64 root names the packages share today: same concept,
same name, an app importing both aliases one.

### Engine comparison

- Unity: `Object.Destroy(obj)` for any object, `transform.SetParent(p,
  worldPositionStays)` for any transform.
- Godot: `queue_free()` on any Node, `reparent(new_parent,
  keep_global_transform)` on any Node.
- Three: `parent.remove(child)` / `parent.attach(child)` (world kept)
  on any Object3D.

All three have one verb per act whatever the node kind, and none has a
per-kind remove or re-parent. The kind union is the shape they share;
what they call the act, we take from 3d where 3d has it.

## Done when

- The four verbs above are the only spelling; `setSpriteTransition`,
  `setGroupTransition`, `setSpriteTimeScale`, `setGroupTimeScale`,
  `destroySprite`, `destroyGroup`, `destroyText` and `setSpriteParent`
  are gone, and `setGroup` takes no `parent`/`keepWorld`.
- `region.ts` has no kind branch on teardown.
- `bun run sol test packages/2d` passes; `sol check packages/2d` passes.

## Involves

`packages/2d/src/layer.ts` (the verbs), `text.ts` (the run's release),
`region.ts`, `records.ts`, `animation.ts`, `index.ts`, the components
(`sprite.tsx`, `group.tsx`, `text2d.tsx`), `tests/` (time-scale,
native-motion, records, materials, text, region, screen-floor), the
examples (`springs`, `parity`, `markers`) and demos (`starlings`,
`relay`, `slipstream`), `AGENTS.md` and `agents/{motion,views,records,
text}.md`. On the 3d side only doc mentions: `packages/3d/AGENTS.md` and
a comment in `node.ts`.

## Outcome

Built as shaped, with these details:

- `addGroup` takes its own bag, `AddGroupOptions` (`GroupOptions` plus
  `parent`), as `addSprite` takes `AddSpriteOptions`; `GroupOptions` is
  the per-frame bag only.
- `destroy(run)` reaches the text half through the run's internal
  `_release` (leave the font, drop the glyph handles), so `layer.ts`
  imports only the `TextRun` type and knows nothing of fonts.
- The text-run listener leak this item first carried was fixed before
  it: a run whose group dies another way leaves its font at the next
  landing, and a dead sprite's outline write is a no-op
  (`tests/text.test.tsx`).
- `mixer.ts` and `spatial.d.ts` were not involved: their `destroyGroup`
  is flux:spatial's animation-group verb, not 2d's.
- 3d's `destroy` takes no `{ snap }`; the shared name keeps 2d's option
  as a 2d extension.
