---
title: HDR scene buffer - tone map in a resolve pass
description: A 3d view renders straight into its 8-bit target with exposure, tone mapping and the sRGB encode in every fragment, so transparent meshes blend in encoded space, the clearColor is never tone mapped and no post effect (bloom) can see radiance; Godot and Unity render the scene into a half-float buffer and tone map once, in a final pass.
created: 2026-09-06
---

# HDR scene buffer - tone map in a resolve pass

Symptom: three consequences documented in packages/3d/AGENTS.md (Color)
since the linear color pipeline landed (3d-environment 3b) - a
`transparent: true` mesh blends over already-encoded pixels, the
clearColor bypasses exposure and tone mapping, and anything that would
read the scene's radiance (bloom, auto exposure, a depth-of-field over
HDR) has nothing to read: the fragment already produced display bytes.

Cause: the scene target is rgba8 and displayed raw by the runtime, so the
output stage (`OUTPUT`'s `outputColor`) runs per fragment. That is Three's
default path (no EffectComposer) and was the right first step. Godot and
Unity render into a half-float color buffer and tone map in one
full-screen pass at the end; Three does the same once a composer is in
play (its `OutputPass`).

What 3d-environment 4c put in place: `format: "rgba16f"` on 2D draw
targets, sampler-only. A view can now render linear, unclamped radiance
(the probes already do: `LINEAR_OUTPUT`) into a half-float target and a
resolve pass - one attributeless triangle sampling it, applying
`outputColor` with the scene's exposure, tone mapping and encode - writes
the displayed rgba8 target. The fragment-side encode stays as the option
for a tight budget (a TV, the Pi): the resolve costs a full-screen pass
plus double the color bandwidth per view, which is exactly what needs
measuring before it becomes a default anywhere. MSAA on the half-float
target works (the resolve renderbuffer is at the format).

Shape to decide when picked up: per view (`createView({ hdr: true })`, a
Unity camera's "Allow HDR") or per scene; whether the resolve pass is the
place post effects plug in (Godot's glow lives there); and whether the
clearColor becomes linear light under exposure (it would, in an HDR
buffer - a behavior change to document).
