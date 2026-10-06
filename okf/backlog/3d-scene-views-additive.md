---
title: A View3d is fixed-size
description: A <View3d> takes width and height only, so a second view cannot fill a box the way <Scene> does; the 2d <View2d> has the same gap, and whichever side moves first settles the shape for both.
created: 2026-09-07
---

# A View3d is fixed-size

## Symptom

`<View3d width height>` is the only shape: a split-screen or a picture-
in-picture view cannot say "fill this box" and follow it at device
density, which `<Scene>` does (fill mode, `getBoundingBoxViewport` in
onLayout). `<View2d>` in `@solidrt/2d` is in the same place
([2d-layer-views-additive](2d-layer-views-additive.md)), so each
package's second view is fixed-size while its first view fills.

The projection half this item once carried is done: a 3d `ViewHandle`
has `project`, `unproject`, `screenRay` and `raycast` over its own
camera and size, and a 2d `ViewHandle` has `project`, `unproject` and
`pick` through its camera. Only fill mode remains.

## Done looks like

Additive on `<View3d>` and `<View2d>` alike: width and height omitted,
the leaf lays out at 100% of its parent and the target follows its
on-screen box, the `<Scene>`/`<SpriteLayer>` rule (both or neither,
mount-fixed; `output` needs explicit sizes).

Involves: view3d.tsx and view2d.tsx (the fill branch of scene.tsx and
sprite-layer.tsx, shared), the docs tables.
