---
title: Per-mesh environment
description: Light layers mask lights and the hemisphere but every receiver sampled the one scene-wide uEnv cube, so the sky's IBL still lit a sealed interior; Mesh.environment (setEnvironment / the environment prop) gives one mesh its own cube or none, per entry, riding the existing uEnv*/uEnvOn names.
created: 2026-09-26
completed: 2026-09-26
---

# Per-mesh environment

Symptom: the light-layers item (`3d-light-layers.md`) masks the sun
and the hemisphere out of a sealed interior, but a `standard` material
still added `envIrradiance`/`envRadiance` from the scene environment -
the sky cube lit the room through the shell. `phong` only reflects
when `reflectivity` is set, so it could opt out; PBR's image lighting
is intrinsic and the only escape was `sceneSource({ env: false })` in
a tier-3 material.

What shipped: `Mesh.environment` - `setEnvironment(mesh, env)` and the
`environment` prop on every mesh component. `null` removes the
environment term (uEnvOn 0, uEnvIntensity 0 zero the phong weight and
the radiance/irradiance), `{ cube, intensity?, rotation? }` samples
that cube instead of the scene's, absent follows the scene.

Why this shape, against the engines:

- Three's `envMap` is per MATERIAL over a `scene.environment` default;
  here materials are deliberately shared across meshes and interiors
  are mesh-grained (layers already are), so the override sits on the
  mesh. `EnvironmentOptions` moved to `environment.ts` so mesh.ts can
  name it without importing the scene.
- Unity assigns reflection probes by volume overlap with blending and
  a `probeAnchor` override, `reflectionProbeUsage: Off` to opt out;
  Godot's ReflectionProbe affects what its box and `cull_mask` admit,
  with `interior` mode and per-material `disable_ambient_light`.
  Volume-based auto-assignment and blending are deliberately NOT
  built: the primitive is the explicit per-mesh assignment, and a
  volume/blend layer on top stays additive if apps ever need it.

Mechanism: the same `uEnv`/`uEnvIntensity`/`uEnvRotation`/`uEnvOn`
names the scene writes as TARGET params/bindings are written as ENTRY
params/bindings on the overriding mesh's entries, and the engine's
merge rule (entry beats target, "specific beats general" in
alloy target.rs) does the rest - zero per-frame cost, no shader
change. Gated by `Material.env` (regex-detected `\buEnv\b` like the
other declared-uniform flags), so on a non-reflecting material the
override is a no-op, never an error. A change rebuilds the mesh's
entries (the setMaterial detach/attach path) - that is what makes
reverting to "follow the scene" possible at all, since an entry param
cannot be unwritten. A probe view whose `probeCube` is the override's
cube binds the black placeholder for that entry instead (the engine
rejects the same-pass feedback outright; the target-level
`ownsEnvironment` rule at entry grain).

Verified live (light-layers-probe.tsx): two metal `standard` spheres
under a blue sky cube - the `environment={null}` one black but for
the masked suns' glints (irradiance AND radiance dead), the other
fully sky-lit; flipped live to a red cube and back to scene-following
through the debug command; `examples/probe.tsx` (realtime reflection
probe) and `examples/materials.tsx` render unchanged.

Non-goals, on purpose: probe volumes/blending (above), an SH9 diffuse
mode (the environment doc already calls it a later, additive mode),
and per-instance overrides (per mesh like layers).
