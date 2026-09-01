---
title: Gaussian splat rendering
description: Captured 3DGS scenes (phone scans, photogrammetry successors) are a growing content class nothing here can display; the render path is instanced camera-facing quads with alpha blending, which the stack already has, plus a strict back-to-front instance order per camera move, which is the one real gap and lands as the projected-key mode of gpu-instance-order. Staged so a demo-tier viewer is library-only work today.
created: 2026-08-24
---

# Gaussian splat rendering

## Symptom

3D Gaussian splatting is becoming the default format for captured scenes -
phone scans, drone captures, product spins - and none of it can be shown.
The format is simple (a point list: position, covariance as scale +
rotation quaternion, opacity, spherical-harmonic color), the renderer is
not a mesh pipeline, and every platform that matters to capture content
already has viewers. This is a content-class gap, not a feature-parity
one.

## What the render path needs, against what exists

The standard non-compute approach (every WebGL viewer): each splat is an
instanced camera-facing quad; the vertex stage projects the 3D covariance
to a 2D conic, the fragment evaluates the gaussian falloff times opacity,
and everything blends back-to-front with depth-write off.

Already landed: instanced materials with per-record attributes
(`shaderMaterialClass({ instanceAttributes })` + `createInstancedMesh`),
the camera basis in the shared params (`uCamRight`/`uCamUp`), custom GLSL,
`blend: "alpha"` with premultiplied settled, depth-test-without-write, the
scene transparent sort placing the cloud against other transparent
meshes, and bounds-based picking for instanced meshes (the cloud is one
mesh with explicit bounds).

The one hard gap is the within-cloud order: a strict back-to-front sort
of every record whenever the camera moves meaningfully. That is
[gpu-instance-order](../done/gpu-instance-order.md)'s projected-key mode, filed
separately because the sprite layer is its second consumer. With the
existing reorder-on-camera-settle philosophy a parked camera costs
nothing - a profile no browser viewer has.

## Stages

Stage 1 - library-only viewer. CLI conversion at pack time (.ply and one
compressed format, .spz or .ksplat, decoded under Bun) into the exact
instance-record layout, so runtime load is one buffer upload - the item-7
direction applied to splats. A splat material in `@solidrt/3d` (quad
geometry, covariance projection in the vertex stage, gaussian fragment),
SH degree 0 (plain RGB) in the record. Order via a throttled JS sort on
camera settle: viable to tens of thousands of splats, and it proves the
material and the converter before any engine work.

Stage 2 - production scale. Swap the JS sort for
[gpu-instance-order](../done/gpu-instance-order.md) (projected key,
descending, `retain: true`). The delivery vehicle is a GENERIC order
option on `createInstancedMesh`, not splat-private plumbing: any
transparent instanced population (future particles included) wants the
same knob, and the splat viewer is just its first consumer - big enough
to be its own item, folded here because the splat work forces it anyway.
What it takes beyond passing `instanceOrder` through to the entry:
`setInstances` publishes via `writeBuffer`, which THROWS on an ordered
buffer, so an ordered mesh's publishes (the load upload included) switch
to the lease (`beginBufferWrite`/`endBufferWrite`); and the camera feeds
`orderDirection` on settle - with retain, a direction update alone
re-orders core-side, no republish. Done looks like: a few hundred
thousand splats orbiting smoothly on desktop, order updates with zero
per-frame JS, parked camera renders nothing new.

Stage 3 - memory and fidelity, each on demand: packed instance attribute
formats (u8-normalized color/opacity, half-float position and covariance)
if record memory shows up - an engine attribute-format item; SH degree 1+
(9-48 extra coefficients per splat) via half-float data textures, riding
[gpu-float-texture-formats](../done/gpu-float-texture-formats.md), which skinning
also wants.

## Not in this item

Training or editing splats, compute-tile rasterization (no compute on the
GLES 3.0 floor; a 3.1 probe-up could revisit, but the core CPU sort makes
it a nice-to-have), streaming/LOD for city-scale captures, runtime-fetched
user splats (the runtime-content problem is general, not splat-specific).
