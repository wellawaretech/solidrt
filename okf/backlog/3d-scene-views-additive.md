---
title: A View3d is fixed-size and has no overlay projection of its own
description: A <View3d> takes width and height only, so a second view cannot fill a box the way <Scene> does, and a view handle has no project, unproject or screenRay, so an overlay or a drag plane over a minimap has to redo the view camera's math by hand; the 2d views have the same additive list.
created: 2026-09-07
---

# A View3d is fixed-size and has no overlay projection of its own

## Symptom

`<View3d width height>` is the only shape: a split-screen or a picture-
in-picture view cannot say "fill this box" and follow it at device
density, which `<Scene>` does (fill mode, `getBoundingBoxViewport` in
onLayout). A view handle exposes `setCamera`/`camera`, `pick`, the
pointer handlers and `listen`, but not `project`, `unproject` or
`screenRay`: a marker over a minimap, or a drag plane under a view's
pointer events, recomputes the view camera's projection in the app.
The 2d views carry the same open list in
[2d-layer-views-additive](2d-layer-views-additive.md).

## Done looks like

All additive on `ViewHandle` and `<View3d>`:

- Fill mode: width and height omitted, the leaf lays out at 100% of its
  parent and the target follows its on-screen box, the `<Scene>` rule
  (both or neither, mount-fixed; `output` needs explicit sizes).
- `view.project(point)`, `view.unproject(x, y, w)`, `view.screenRay(x, y)`
  over the view's camera and size - the scene's, parameterized as
  `pixelRayOf` already is for `view.pick`.

Involves: scene.ts's view handle (the projection helpers take a camera and
a size), view3d.tsx (the fill branch of scene.tsx, shared), the docs table.
