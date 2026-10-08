---
title: A fill-mode scene or sprite layer adds no intrinsic size to the layout
description: The fill leaf of <Scene> and <SpriteLayer> is a <texture> at 100%, and a texture measures its pixel size, so flexbox's automatic minimum pins its ancestors to the target's size (a 1280 px tall screen in a 720 px window) and an indefinite height comes out square; the target follows the box, so the leaf holds whatever size it first fed itself.
created: 2026-10-08
completed: 2026-10-08
---

# Fill-mode leaves add no intrinsic size

Fill is the default for `<Scene>` (3d) and `<SpriteLayer>` (2d): with no
`width`/`height` the built-in leaf is a `<texture>` at 100% by 100%, and the
target follows the leaf's box
([scene.tsx](../../packages/3d/src/components/scene.tsx),
[sprite-layer.tsx](../../packages/2d/src/components/sprite-layer.tsx)). The
leaf is still a replaced element, though: `Texture::measure` reports the
texture's pixel size as its intrinsic size
([texture.rs](../../alloy/src/rendertree/kinds/texture.rs), `replaced_size`
in [rendertree/mod.rs](../../alloy/src/rendertree/mod.rs)). That leaks into
layout two ways, and nothing warns about either:

- Every flex item that holds the scene gets an automatic minimum
  (`min-height: auto`) of the leaf's intrinsic size, so the ancestors cannot
  shrink below the target. A column of header, scene pane (`flexGrow: 1`)
  and footer in a 720 px window laid out 1280 px tall, with the footer at
  y = 1234, off screen. The workaround is `minHeight={0}` on every ancestor
  up the chain.
- Where the leaf's height is indefinite, `replaced_size` derives it from the
  width by the texture's aspect. The target starts at `FILL_INITIAL_SIZE`
  (1 x 1), so the leaf comes out square, the target follows to 1280 x 1280,
  and the scene renders into a square that the window crops ("the camera
  looks too close").

The target follows the box and the box follows the target, so whatever size
the first layout picks is held. At a display scale above 1 the loop should
grow rather than hold, since the target is the box times the scale in device
pixels and the leaf measures those pixels as logical units. That follows from
the code and has not been observed.

## Done when

- A fill leaf contributes nothing to its ancestors' sizes. A fill scene in a
  flex column with no `minHeight` anywhere lays out exactly as an empty
  `<view>` at 100% would, at any display scale, and the target follows that
  box. The same holds for the sprite layer's fill leaf.
- A fill leaf whose box comes out empty (a parent with no size) warns once
  in dev and names the fix, giving the parent a size, the way `createScroll`
  warns about its own unsized case.
- Fixed-size scenes and layers (`width`/`height` given) keep their measured
  size.
- An alloy layout test (header, fill leaf, footer in a fixed window) and a
  component test in each of 3d and 2d.

## What it involved

The leaf stays a texture: events, the `output` contract and `fit` all hang
off it. So the change is in how it measures. Two shapes were on the table:

- A prop that drops the intrinsic size, the analog of CSS `contain: size`.
- A `<view>` at 100% around the leaf, with the texture absolutely positioned
  inside it (inset 0). No engine change, but a node per scene and a moved
  event target.

## Decision

`contain="size"` as a layout prop on every element, not a texture flag
(decided 2026-10-08, widening the note's first shape). CSS size containment
says exactly what the leaf needs: a replaced element under it has a natural
width and height of 0 and no natural aspect ratio, and a container sizes as
if it had no content. The engine already had both halves: the measured-leaf
wrapper in `LayoutContext::perform` is one place to zero the intrinsic for
every leaf kind, and `design_size_layout` was already "size as a replaced
element outside, lay the children out inside in ContentSize mode". It became
`boxed_layout`, the design-size view and a contained container being its
two callers. One `contain_size` flag on `LayoutData`, decoded in `apply_jsx`
beside `position`, read back for the inspector. Only `"size"` is a value;
the other CSS containment values are not modelled.

The fill leaves of `<Scene>` and `<SpriteLayer>` set it. Nothing in the repo
carried the `minHeight={0}` workaround, so no app code changed. The
once-only warning with the creation stack that `createScroll` had became
`warnOnce` in core, used by all three sites.

The scroll viewport needs no containment: taffy already gives an item with
`overflow` other than visible an automatic minimum of 0, as CSS does, so the
`overflow="hidden"` viewport was never pinned by its content. The remaining
`minHeight={0}` in the repo (editor-field's root) is on a box that must grow
with its content when unconstrained, which containment would forbid.

## Non-goals

The automatic minimum itself. Yoga, which React Native layouts are written
against, has none: no React Native layout carries `minHeight: 0`. Dropping
it is defensible through the solidrt lens but changes how text and
overflowing content behave in every existing layout, so it is its own item:
[flex-automatic-minimum-default.md](../backlog/flex-automatic-minimum-default.md).
