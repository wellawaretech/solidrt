---
title: "flux:ffi: pass a JS buffer as a ptr argument"
description: Signatures are scalar-only and a Uint8Array cannot be handed to a ptr parameter, so every out-parameter or buffer call first loads libc as a second Library for malloc/free plus writeMemory/readMemory; accept Uint8Array/ArrayBuffer for ptr args, pinned for the call.
created: 2026-08-15
completed: 2026-08-15
---

# flux:ffi: pass a JS buffer as a ptr argument

The single biggest ergonomic gap against `bun:ffi`; costs every FFI author
the same hour.

## Problem

Signatures are `i32/i64/f32/f64/ptr` only and there is no way to hand a
`Uint8Array` to a `ptr` parameter. Almost every useful C function takes a
buffer or an out-parameter, so callers must obtain memory from the native
side: load `libc` as a second `Library` purely for `malloc`/`free`, then
`writeMemory` in and `readMemory` out, before the first real call.

## Done looks like

Where a `ptr` arg is declared, a `Uint8Array` or `ArrayBuffer` is accepted
and its backing memory address is passed, pinned for the duration of the
call. This is what `bun:ffi` does and it is safe precisely because calls
are synchronous today. Writes the native side makes into the buffer are
visible in JS afterwards (out-parameters work).

Fallback if pinning is not attainable through rquickjs (e.g. the engine may
move ArrayBuffer storage): `Library.alloc(len): bigint` and
`Library.free(ptr)` as built-ins, so the libc dance is at least not
rediscovered per project. Prefer the pinned form; ship alloc/free only if
pinning is blocked.

## Outcome

Shipped the pinned form: a `ptr` argument accepts an ArrayBuffer or any
typed array (view offset respected, detached throws); the argument list
keeps the value alive for the synchronous call and QuickJS buffer storage
does not move, so no copy and no alloc/free API were needed. Callbacks may
not return a buffer as a ptr (would dangle) and throw if they do. Example:
flux/examples/ffi_buffer.js.

## Involves (as planned)

- flux/src/plugins/modules/ffi.rs: arg marshalling for `ptr` gains a
  typed-array branch. Verify rquickjs gives a stable data pointer for the
  call's duration (QuickJS ArrayBuffers are malloc'd and do not move, but
  detached/resized buffers must be rejected).
- forge/src/ffi.rs stays scalar: the plugin resolves the buffer to an
  address before calling in.
- Off-thread calls cannot rely on the JS-side buffer staying alive; the
  answer is isolates (okf/backlog/isolates-and-ports.md), where the sync
  call plus buffer args keep working, not per-symbol async.
- packages/flux-types + docs mirror the change.
