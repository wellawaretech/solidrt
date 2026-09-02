---
title: Pipeline target resize from a 1x1 creation leaves the framebuffer incomplete
description: A createPipelineTexture target created at 1x1 (the size a 0x0 startup window clamps to) fails its first real setTargetSize with "shader framebuffer incomplete after resize: target framebuffer incomplete: 0x8cd6" (GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT), killing the app; hit twice, reproducibly, on the Linux desktop client while bringing up the 2d starlings demo.
created: 2026-09-02
---

# Pipeline target resize from a 1x1 creation leaves the framebuffer incomplete

## Symptom

Bringing up `packages/2d/demos/src/starlings.tsx` (a `createRecordLayer`,
which renders through `createPipelineTexture`) on the Linux desktop client
(release, 2026-09-02):

1. `windowSize()` reports 0x0 until the window's first real frame
   (`[alloy] frame size 0x0 -> 1706x960` comes later in the log), so a
   layer created at mount from `untrack(windowSize)` asks for a 0x0
   target. That creation fails outright with
   `createPipelineTexture: target framebuffer incomplete: 0x8cd6` -
   arguably fine (0x0 is not a target), though the error could say so.
2. Clamping creation to 1x1 and resizing to the real size from a
   `createEffect` on `windowSize` then fails in the resize instead:
   `setTargetSize: shader framebuffer incomplete after resize: target
   framebuffer incomplete: 0x8cd6` (records.ts setSize ->
   `setTargetSize(texture, w * oversample, h * oversample)` with
   oversample 1, w x h = 1706x960). Reproduced twice across engine
   restarts. 0x8cd6 is GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT.

The resize path (`alloy/src/gpu/target.rs` `Target::resize` ->
`attach_storage`) creates a fresh RGBA8 texture at the new size and
attaches it to the kept FBO; nothing about 1706x960 should be incomplete,
so the suspicion is state carried from the 1x1 creation (or a first
resize racing target initialization before the first render), not the new
attachment itself. Not yet isolated to a minimal probe.

## Why it matters beyond the demo

`<SpriteLayer>` in fill mode follows its leaf's box with `setSize`, so
any sprite-layer app cold-started before the window reports a real size
walks the same 1x1-then-resize path. The demo works around it by gating
mount on `windowSize().width > 0` (see the comment in starlings.tsx), so
the layer is created at its real size and the startup resize never
happens; window resizes after that would still exercise the same code.

## Done looks like

- A minimal repro (create pipeline texture at 1x1, resize, render) in an
  alloy example or flux test, then the actual fix in the resize path.
- `createPipelineTexture`/`createRecordLayer` given a 0-size target
  either work once resized or fail with an error that names the real
  problem.
- The mount gate in starlings.tsx becomes unnecessary (it can stay as
  belt and braces).
