---
title: The Adreno 610 breaks the first program that compares against a cleared depth texture
description: On the SM-T500 (Adreno 610, OpenGL ES 3.2 V@0502.0) the first program object in a process to draw through a sampler2DShadow against a depth texture that was cleared and never drawn into returns 1.0 from every comparison for the rest of its life; later programs are fine; the raster spends that draw on a throwaway program at start.
created: 2026-10-06
---

# The Adreno 610 breaks the first program that compares against a cleared depth texture

Device: Samsung SM-T500, Adreno 610, driver "OpenGL ES 3.2 V@0502.0
(GIT@5eaa426211, I07ee46fc66, 1633700387) (Date:10/08/21)", Android 12.
Not seen on Mesa (Intel), not documented by the vendor. Found through
`okf/done/android-cold-start-shadow-loss.md`.

## The behaviour

The first program object in the process that draws through a
`sampler2DShadow` (our `SamplerCache::compare`: COMPARE_REF_TO_TEXTURE,
LEQUAL, LINEAR) against a depth texture that has been cleared but never
drawn into keeps returning 1.0 from every comparison tap for the rest of
its life, whatever the texture holds later. Everything about the draw is
otherwise correct: the depth texture has the right contents (read back as
color through a plain `sampler2D` pass), the uniforms, bindings and
entries match a working state byte for byte, and a second entry on the
same program is just as blind. Every program that draws through the
comparison sampler after that first one compares correctly, including a
program compiled before it but first drawn after it, and a program with
the same source compiled later.

Setting `TEXTURE_COMPARE_MODE` and `TEXTURE_COMPARE_FUNC` on the depth
texture object itself (in addition to the sampler object) changes
nothing, so the driver specializes the program on the texture's contents
state at that first draw (a cleared, never drawn attachment), not on its
parameters. A depth texture that already holds a draw at the first
comparison does not trigger it: the same scene with a static caster whose
records are present from creation casts from a cold start.

## How it showed

A scene whose receivers are the first comparison users in the process,
with every shadow caster at record count 0 during the first frames (the
tower-toppling demo publishes its first bricks a few frames after
mount): the shadow atlas holds only its clear when the scene's `phong()`
program first samples it, that program is poisoned, and no shadow lands
on the ground until an engine swap compiles the material again. A reload
or any earlier 3d scene in the process makes another program take the
hit. The shadow system's placeholder (a one-texel cleared depth target,
bound while nothing casts) is the same shape, so a scene that gains its
first caster later would be hit the same way.

## What the engine does

`gl::warm_compare_sampler`, at raster start on every platform: a 1x1
depth texture cleared through its own FBO, one comparison tap into a 1x1
color target through a throwaway program, everything deleted again. The
process's first comparison draw is then one whose result nobody reads.

## How it was found

Through the control API against the installed derived Player, cold
starting (`am start -S`) a probe variant per step and reading the raster
inventory, the atlas depth as color, a fresh comparison pass over the
same depth id, a fresh material running the scene's own `lightShadow`
inside the scene pass, a second entry on the broken program, the phong
source compiled as an app class at mount and drawn late, the library's
phong compiled late, and finally an app-level warm-up pass before the
scene (fixes) against the same through a plain `sampler2D` (does not).
