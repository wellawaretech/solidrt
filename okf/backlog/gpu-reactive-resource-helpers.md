---
type: backlog-item
title: Reactive GPU resource lifetimes
description: Core's gpu helpers only freed on owner disposal, wrong for resources rebuilt on signal changes; shipped as a manual option plus createShaderMemo at a stable id.
status: done
timestamp: 2026-07-23T00:00:00Z
---

# Reactive GPU resource lifetimes

Shipped 2026-07-23. @solidrt/core's gpu helpers supported exactly one
lifetime: free when the creating owner is disposed - wrong for resources
rebuilt on a signal change inside a long-lived component (each rebuild
stacked another onCleanup on the component owner: a leak until unmount, then
a double-free against manual destroys). The linux-terminal resize feature hit
exactly this and had to drop to raw flux:gpu imports.

Both additions from the write-up, in packages/core/src/gpu.ts:

- **`{ manual: true }`** on createTexture / createMutableTexture /
  createShader / createPipeline (in its opts) / createBuffer: skips the
  owner-scoped onCleanup registration, removing the dual-import-path wart for
  apps that manage disposal themselves (`CreateOptions` type).
- **`createShaderMemo(() => spec)`** with
  `spec: { fragmentSrc, width, height, params?, textures? }`: returns an
  accessor for the current texture id and keeps the resource in step with the
  reactive spec. Changes that keep the compiled program valid mutate in place
  at a stable id (size -> setShaderSize, params -> setShaderParams, per
  [[gpu-in-place-resize]]); a changed fragment source or changed sampler
  bindings rebuilds at a fresh id, updates the accessor, and destroys the old
  id. Because [[gpu-deferred-texture-destroy]] shipped first, that destroy is
  simply immediate in code - the runtime reclaims it only after the tree lets
  go, so no flush-sequencing lives in the helper at all (the original design
  called for dispose-after-flush precisely because that runtime guarantee did
  not exist yet). Current id is freed on owner disposal.

No texture analog was added on purpose: data textures change id-stably via
uploadTexture/resizeTexture, so there is no rebuild-and-swap to manage.

Not live-verified yet: typechecked (srt check against the linux project) but
no app currently calls createShaderMemo or { manual: true }; the linux
terminal no longer needs either since resize became id-stable.
