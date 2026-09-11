---
title: Headless verification traps
description: What bites a probe that verifies GPU state under srt render - deferred texture destroys, sRGB has no readback, production bundling drops Solid's diagnostics, Geometry.vertices typing.
created: 2026-09-11
---

# Headless verification traps

Found while verifying the @solidrt/3d tiny items of 2026-09-11 with
`probes/3d-scope-override-probe.tsx` (`bunx srt render <probe> --file
--duration 1`: no dev server, so no clash with whatever `srt run` the user
has up). Each cost a rerun; none is specific to that probe.

- **A GPU destroy lands at the frame, never synchronously.** `destroyTexture`
  (and everything that calls it, a model's `dispose`) queues the delete
  (done/gpu-deferred-texture-destroy.md), so `readTexture(id)` still
  succeeds right after the dispose. Read liveness a few frames later: the
  probe records the ids at the free and checks them one phase on.
- **`readTexture` refuses `rgba8-srgb`.** A model's base color and emissive
  maps upload as sRGB (glTF stores them encoded), and the runtime has no
  readback path for that format (the error says why: a decode would return
  something other than the stored bytes). The readable observable of a
  model upload is a data map - normal, metalness/roughness - which is plain
  rgba8. Same family as the float formats with no readback
  (notes/gpu-review.md).
- **`srt render` bundles production.** `@solidjs/signals` resolves to its
  prod build there, which carries none of the owner diagnostics
  (RUN_WITH_DISPOSED_OWNER, NO_OWNER_CLEANUP, ...). A check on their
  absence passes vacuously; verify ownership through a side effect the
  scope's cleanup causes (a destroyed texture, a spied dispose), never
  through a console warning.
- **`Geometry.vertices` is typed `ArrayBufferView`.** App code that slices
  it (`subarray`) has to narrow to `Float32Array` first; the package's own
  builders know the concrete type, `srt check` on a probe does not.
