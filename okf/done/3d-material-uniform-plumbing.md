---
title: Scene uniform channel, camera basis, material class/instance split
description: The three ways an app talks to the shared uniform set and to a pipeline all have a gap: a Scene has no app-writable shared params (the workaround goes through scene.texture), the standard set carries no camera basis so billboards reconstruct it from uViewProj rows, and shaderMaterial cannot express one program with many parameterisations.
created: 2026-08-17
completed: 2026-08-17
---

# Scene uniform channel, camera basis, material class/instance split

## Done 2026-08-17

All three landed in `packages/3d`, bare-minimum shape:

1. `scene.setParams(params)` - an immediate `setTargetParams` on the
   scene's target (no batching: the shared channel is one write anyway).
2. The camera write carries `uCamRight`/`uCamUp` beside `uViewProj`/
   `uCamPos`: rows 0 and 1 of the view matrix, world space, no clip flip
   (chosen over `uView` because the app never has to know the flip).
3. `shaderMaterialClass({ vertex, fragment, ...pipeline state })` returning
   `{ instance({ params?, textures? }), dispose() }`; `shaderMaterial` is now
   literally a class with one instance whose `dispose` forwards to the class,
   so nothing changed for existing callers. Instances carry no `dispose`.

The interaction with
[gpu-inactive-uniform-two-tier](gpu-inactive-uniform-two-tier.md)
stands as recorded there; a `Billboard` node stays with the instanced-mesh
sugar. Both are open items, not leftovers of this one.

## Original shaping

Three separate gaps in one item because they are one question: what is in a
scene target's shared uniform set, who may write it, and how a material says
"same program, different values". The shared-vs-per-entry split itself is
sound and is what makes many animated objects cheap; these are the places an
app cannot reach it.

## 1. A Scene has no app-facing shared param channel

Symptom: values every material reads - a clock, a sun direction, fog - have to
be driven per mesh through `setMeshParams`, which is O(meshes) per frame.
That is precisely the cost the shared channel exists to remove. Pushed through
the shared channel instead it is one write per frame regardless of mesh count,
with everything else animating in vertex shaders off a single `uTime`.

The workaround works today and is the reason this is small: `scene.texture`
IS the draw target id, so

```ts
setTargetParams(scene.texture, { uTime })
```

merges an app-owned name into the same shared params the scene graph writes
`uViewProj`/`uCamPos` into. Names merge, a draw target tolerates zero
coverage, and neither side clobbers the other. Until 2026-08-17 it was
discoverable only by reading `scene.ts`; `packages/3d/AGENTS.md` now names it
as the sanctioned scene-wide channel and `setMeshParams` as the per-mesh one.

What remains: the method itself, `scene.setParams({ uTime })`, merging into
the target's shared params, so the sanctioned path does not require knowing
that `scene.texture` doubles as the target id.

## 2. The standard set has no camera basis

Symptom: the shared set is `uViewProj` + `uCamPos`, so a vertex stage has no
view right/up axes. Anything camera-facing - billboards, sprites, screen-
aligned particles, a ground decal - has to rebuild them from the rows of
`uViewProj`:

```glsl
vec3 right = normalize(vec3(uViewProj[0][0], uViewProj[1][0], uViewProj[2][0]));
vec3 up   = -normalize(vec3(uViewProj[0][1], uViewProj[1][1], uViewProj[2][1]));
```

That is correct (the negation is the y-down clip flip baked into
`perspective()`) and it is folklore every app rediscovers. Wanted:
`uCamRight`/`uCamUp`, or `uView` and let the app take its rows. Cheap, and the
scene graph already performs the camera write these would ride on.

A `Billboard` node is the library sugar on top and belongs with the instanced
mesh sugar, roadmap item 12; the uniforms are the part that has to exist
first.

## 3. shaderMaterial cannot express class vs instance

Symptom: `unlit` has the split - one program and one pipeline per material
CLASS, an instance being per-entry uniforms - and `shaderMaterial` has no way
to say it. Identical sources compile twice by design (no dedupe by source
value, deliberately), and the standing advice is one material per look created
at app scope. That holds for a handful of looks; a scene whose materials
differ only in a colour or a texture scale collapses to a few pipelines but
would otherwise pay one program compile per look at startup.

The workaround exploits `Material` being a plain object: compile one base per
source variant, then spread it and swap `params`/`textures`.

```ts
return {
  pipeline: base.pipeline,        // the shared closure - same pipeline
  normalMatrix: base.normalMatrix,
  layout: base.layout,
  params: { uColor, uTexScale },
  textures: { uMap },
}
```

This is exactly what `unlit` does internally, but it depends on `Material`
staying a structurally-typed plain object, which is not a documented
guarantee.

Wanted: make it first class.

```ts
let cls = shaderMaterialClass({ vertex, fragment })   // compiles once
let a = cls.instance({ params: { uColor: [...] }, textures: { uMap } })
let b = cls.instance({ params: { uColor: [...] }, textures: { uMap } })
```

Explicitly NOT a content-keyed cache. The argument against hidden caches
stands; this is an app-owned split.

Interacts with
[gpu-inactive-uniform-two-tier](gpu-inactive-uniform-two-tier.md): a
parameterised class wants one uniform set across several source variants, and
any variant that happens not to reference one of them throws at `add()`. See
that item for the fix.

Demand recorded 2026-08-17.
