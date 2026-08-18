---
title: Geometry GPU buffers accumulate when a Mesh's geometry prop changes
description: Swapping <Mesh geometry> reactively leaves every previous generation's vertex/index buffers resident, because geometry buffers are app-lifetime and only disposeGeometry frees them; the declarative layer has no disposal story to pair with the prop.
created: 2026-08-18
---

# Geometry GPU buffers accumulate when a Mesh's geometry prop changes

Symptom: an app that regenerates geometry reactively (rebuild a map at a new
lane count, regenerate terrain per match) and passes each generation to
`<Mesh geometry={...}>` sees `get_gpu_resources` list every generation's
`-verts`/`-indices` buffers side by side; a few keypresses leave hundreds of
KB of dead vertex data resident. Nothing frees it and nothing warns.

This is the documented model working as designed (`packages/3d/AGENTS.md`:
"Geometry GPU buffers are lazy, shared, and app-lifetime (owner-scoped free
would break sharing); `disposeGeometry` frees them"). The gap is that
`<Mesh geometry>` is the ordinary way to draw, swapping that prop is the
ordinary way to rebuild, and there is no `<Mesh>`-level hook that pairs with
it: the app has to track geometry identity by hand and call
`disposeGeometry(old)` at a moment the component model does not surface.
Three has the same manual `geometry.dispose()`, and the same complaint.

Where it lives: `geometryBuffers` in `packages/3d/src/geometry.ts` caches
`_buffer`/`_index` on the geometry object with `autoFree: false`;
`disposeGeometry` is the only free; the scene's entry add
(`packages/3d/src/scene.ts`, the `geometryBuffers(mesh.geometry)` call)
takes the buffers without recording who uses them.

## Done looks like

An app that only ever swaps `<Mesh geometry>` does not leak, and sharing one
geometry between meshes/scenes stays free. `packages/3d/AGENTS.md` states
the intended pattern for geometry that changes reactively in one sentence.

## Options

1. Refcount by draw entries: the scene increments on entry add, decrements
   on entry remove, frees at zero. Lazy re-creation on next use already
   exists (`disposeGeometry` doc), so a geometry that outlives its last
   entry and is drawn again just re-uploads. Trade: a geometry held for
   later reuse (a cached prefab) re-uploads on every reappearance;
   `disposeGeometry` stays as the explicit override.
2. Opt-in on the component: `<Mesh geometry disposeOnChange>` (or a
   `dispose` callback) that calls `disposeGeometry` on the previous value
   when the prop changes. Cheap, explicit, wrong for shared geometry unless
   the app knows.
3. Status quo plus doc: keep manual disposal, add the reactive pattern to
   AGENTS.md (a `createEffect` on the geometry signal that disposes the
   previous value).

Not decided; check Three's conventions before picking a shape (see
feedback: follow Three, do not copy its mistakes).
