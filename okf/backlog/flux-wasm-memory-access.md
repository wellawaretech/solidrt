---
type: backlog-item
title: flux:wasm memory views and named call-mismatch errors
description: readMemory copies into a fresh Uint8Array per call - 9 MB/s of garbage lifting a 32bpp framebuffer whose size never changes - while the pure-JS build of the same app reaches a genuinely copy-free path; a readMemoryInto or transient view closes the gap, and "indirect call type mismatch" should name the failing index and signatures.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# flux:wasm memory views and named call-mismatch errors

Source: the wasm game-port demo feedback (2026-08-02).

## readMemory always allocates

Allocation hygiene, not throughput: lifting the framebuffer out of the
guest costs 0.003 ms (8-bit) / 0.016 ms (32bpp) against a ~7 ms tick, but
mints a fresh Uint8Array every frame for a buffer whose size never changes
(2.2 MB/s of garbage at 8-bit, 9 MB/s at 32bpp), and it scales badly in the
direction people go next (bigger framebuffers, video, camera).

The comparison that makes the point: the same game compiled to plain JS
(emcc -sWASM=0) has a real JS ArrayBuffer heap, so
`uploadTexture(id, HEAPU8.subarray(ptr, ptr + len))` is genuinely copy-free
on the way in - the Rust side borrows the bytes directly
(flux/src/plugins/gui/gpu.rs). The wasm build cannot reach that path
because flux:wasm exposes no view onto linear memory, only the copying
readMemory.

Shape: `readMemoryInto(ptr, len, target)` writing into a caller-held
Uint8Array, or a transient view valid until the next guest call. Either
puts the two builds on equal footing. (Even a perfect zero-copy read still
meets the one unavoidable raster-thread copy on the far side - that half is
[[texture-upload-staging]].)

## Call-mismatch errors are opaque

`Error: wasm call failed: indirect call type mismatch` surfaced at an
`instance.call` site with no indication of which function pointer was
wrong - and the actual cause was a torn-down instance during a hot reload,
which the message hid equally well. Name the failing table index and the
expected/actual signatures.
