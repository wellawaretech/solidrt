---
title: Per-mesh environment
description: Light layers mask lights and the hemisphere but every receiver samples the one scene-wide uEnv cube, so the sky's IBL still lights a sealed interior; a per-mesh environment assignment (a probe or cube per mesh, Unity's anchor override / Godot's per-instance probe blending in spirit).
created: 2026-09-26
---

# Per-mesh environment

Symptom: the light-layers item (`3d-light-layers.md`) masks the sun
and the hemisphere out of a sealed interior, but a `standard` material
still adds `envIrradiance`/`envRadiance` from the scene environment -
the sky cube lights the room through the shell. `phong` only reflects
when `reflectivity` is set, so it can opt out; PBR's image lighting is
intrinsic (see AGENTS.md "standard") and today the only escape is
`sceneSource({ env: false })` in a tier-3 material.

Context: `uEnv` is bound once per receiving target by `writeLights`
(with the probe-own-face placeholder exception); reflection probes
already have `layers` for what they RENDER, but not for who SAMPLES
them. Meshes have no environment knob.

Shaped enough when it says: how a mesh names its environment (the
scene's, a probe's cube, an explicit cube, or none), where the sampler
moves from target params to entry textures, and what Unity
(ReflectionProbe anchors, blend distances) and Godot (probe interior
mode, per-instance blending) do that we deliberately simplify.

Done looks like: an interior mesh keeps a masked scene (light layers)
AND shows no sky reflection or irradiance, either via `env: null` on
the mesh/material or via an interior probe assigned to it; AGENTS.md
"Environment" and the sealed-interiors note updated.
