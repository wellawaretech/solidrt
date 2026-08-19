---
title: Streaming GPU buffers sampled at frame time
description: A no-publish-call shared-memory buffer - a persistent JS view the raster thread samples during Frame handling - as the follow-on to the begin/end write lease
created: 2026-08-19
---

# Streaming GPU buffers sampled at frame time

The buffer write lease (beginBufferWrite/endBufferWrite, landed with
okf/plans/2d-extension.md) is a per-commit pair: fill a leased block, publish
it. This note records the alternative that was designed, compared, and
deferred - a buffer with NO per-commit call at all - so the reasoning
survives until a consumer justifies building it.

## The shape

`createStreamBuffer(byteLength)` allocates a runtime-owned block, mints a
PERSISTENT ArrayBuffer over it (the wasm.rs
JS_NewArrayBuffer-with-no-free-callback pattern), and registers the block
with the raster thread. JS writes floats whenever it likes. During Frame
handling, the raster thread samples every registered stream: if changed,
upload and mark reader targets dirty (the write_buffer dirty logic). Demand
rides the raf contract: per-frame writers necessarily run inside
requestAnimationFrame, and a pending raf is a standing frame request, so
frames flow exactly while writes happen; writer stops -> frames stop ->
zero cost.

## Why it lost to the lease for v1

1. **The discard trick does not work as sketched.** The pipelining race is
   real (UI builds frame N+1 while N is on the GPU), and "read the seqlock
   generation before/after glBufferSubData, discard if unstable" cannot be
   implemented - the GPU buffer is already overwritten when instability is
   detected. Sound discard requires copying the data region into a
   raster-side scratch first (raw pointer copy - no &[u8] over bytes JS may
   be writing), validating stability, then uploading the scratch. That
   re-adds one raster-thread memcpy per sampled frame, making the "more
   zero-copy" option have MORE copies than the lease.
2. The generation word is a benign-but-outside-the-model data race, and raw
   writers who skip the wrapper get one-frame tearing.
3. One-shot writes outside a frame flow are silently never sampled (the
   lease's end latches request_frame; nothing here does).
4. ~550 lines of the risky kind (unsafe Sync block type, hand-rolled
   seqlock, a new per-frame raster hook) vs the lease's ~400 with in-repo
   precedent for every piece.

## What would justify building it

The one genuine win: the persistent view is CANONICAL storage - update one
sprite record, never rewrite or re-publish the rest. A consumer whose
rewrite/publish cost measurably dominates (a huge mostly-static population
with sparse per-frame updates, or a wasm guest streaming a framebuffer with
zero host calls) is the trigger. It composes with the lease (same raster
dirty logic, same detach pattern; the pool machinery is unrelated), so
nothing landed forecloses it.
