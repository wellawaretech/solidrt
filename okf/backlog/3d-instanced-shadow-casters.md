---
title: Instanced meshes cast no shadow
description: The scene's shadow view draws casters with a position-only depth override that knows uModel and nothing else, so an instanced mesh's per-record transforms are invisible to it and `castShadow` on an InstancedMesh is skipped. The additive fix is a per-class `shadowVertex` the override borrows.
created: 2026-08-27
---

# Instanced meshes cast no shadow

Symptom: `castShadow` on an instanced mesh (or on a mesh whose material
declares instance attributes) does nothing: the mesh draws, its
instances light and receive, but the shadow view never sees them.
Documented as the instanced-caster gap in
[3d-shadow-maps](../done/3d-shadow-maps.md) stage 3 and in
`packages/3d/AGENTS.md`.

Cause: an `overrideMaterial` view (the shadow view is one) replaces
every entry's pipeline with the override's, and the shadow depth
material's vertex stage is `uViewProj * uModel * aPos`. An instanced
material's vertex stage reads its own instance attributes to place each
record; the override cannot know that layout, so the view skips
instanced meshes rather than draw a thousand records at the origin.

## Done looks like

A material class declares an optional `shadowVertex`: its own vertex
stage minus everything but position (the instance placement included),
and the shadow view uses a depth pipeline built from that stage plus
the shared depth fragment for entries of that class - one extra program
per casting instanced class, cached like the others. Materials without
one keep today's behaviour (plain meshes use the shared depth stage,
instanced ones are skipped with the documented warning). Additive: no
existing material changes.
