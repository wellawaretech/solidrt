---
title: Async shader compile and readback
description: Compile/link and readTexture are the two GPU calls whose cost class differs from everything else on the surface - both block the JS thread and the raster thread, i.e. the frame loop - and both are calls the standards eventually grew an async-only form for; invisible while compiles happen at startup, real for live-coding and any per-frame readback consumer.
created: 2026-07-31
---

# Async shader compile and readback

From [gpu-review](../notes/gpu-review.md) (lesson 11), filed 2026-07-31
when the review's shortlist closed and this was one of two ranked lessons
with no home. Recorded together because they are the same observation twice:
these are the two calls on the GPU surface whose cost class is different from
everything around them, and both standards refused a synchronous form for
exactly that reason.

Not urgent on either half. The point of filing is that the cost is invisible
today for a reason that expires.

## Compile and link

WebGPU added `createRenderPipelineAsync` for one reason: driver shader
compilation takes tens to hundreds of milliseconds, and blocking on it drops
frames.

solidrt's `compileShader`/`linkProgram`/the fused creates go through a
blocking RPC that stalls both the JS thread and the raster thread - the frame
loop. That is invisible today because compiles happen at startup, before
anything is animating. It stops being invisible for the shader-editor /
live-coding workload that `createShaderMemo`'s error reporting was built for:
exactly the case where a compile happens while the app is rendering, once per
keystroke.

Note the blocking RPC is also what buys the review's item 2 in
"where solidrt beats both" - a bad shader throws on the line that wrote it,
where WebGPU can only report asynchronously. Any async form must not give
that up: the shape would be an additional async entry point, not a change to
the existing one.

## Readback

Both standards refused a synchronous readback too: WebGPU has *only*
`mapAsync`, and WebGL2 grew the PBO + fence pattern, because `glReadPixels`
drains the whole GPU pipeline before returning.

`readTexture` is that stall, deliberately: it is documented as the bake path,
not a rendering path, and for one-shot bakes a stall is fine. What matters is
that the async shape already exists in the surface - `captureSnapshot`
returns a Promise - so if a live readback consumer ever appears (pixel
picking, GPU histograms, per-frame analysis), the precedent is set and the
sync form does not need to grow into it.

Adjacent, already filed: [[capture-pixels-round-trip]] (both capture
consumers only ever wanted pixels, so the texture round-trip doubles the sync
points) and stage 2 of [[angle-cross-context-impeller-textures]]
(non-blocking creates and readbacks). If any of these three is picked up, it
is worth looking at all of them at once - they share the blocking-RPC
boundary.
