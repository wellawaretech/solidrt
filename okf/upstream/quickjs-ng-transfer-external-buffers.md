---
type: upstream-issue
title: quickjs-ng ArrayBuffer.prototype.transfer mishandles external buffers
description: transfer() on a JS_NewArrayBuffer-backed (external) buffer calls js_realloc on a pointer the JS allocator does not own when the length changes (heap corruption), and re-homes the pointer with a NULL opaque when it does not (breaks the free-callback contract, escapes embedder invalidation). resize() guards external buffers; transfer() lacks the same guard.
project: quickjs-ng (github.com/quickjs-ng/quickjs)
versions: quickjs-ng 0.15.1 (as vendored by rquickjs-sys 0.12.1)
status: unfiled
link:
timestamp: 2026-08-03T00:00:00Z
---

# quickjs-ng: ArrayBuffer.prototype.transfer mishandles external buffers

Found 2026-08-03 while chasing the rquickjs detach double free
([[rquickjs-detach-double-free]]); this is the engine-side sibling. Nearest
existing upstream issue is quickjs-ng#471 (external SharedArrayBuffer
support), which does not cover the transfer path.

## Draft report

`js_array_buffer_transfer` (quickjs.c, all three magic variants: `transfer`,
`transferToImmutable`, `transferToFixedLength`) assumes the backing store
came from the JS allocator. For an external buffer - created with
`JS_NewArrayBuffer(ctx, buf, len, free_func, opaque, false)` over memory the
embedder owns - each of its paths misbehaves:

1. `new_len != old_len`: `js_realloc(ctx, bs, new_len)` on a pointer
   js_malloc never allocated. Heap corruption, and the embedder's original
   pointer is freed behind its back on success.
2. `new_len == old_len`: the pointer is re-homed into a fresh buffer via
   `js_array_buffer_constructor3(..., bs, abuf->free_func, NULL /* opaque */,
   false)`. Two breakages: the callback later runs with a NULL opaque it was
   never designed for (embedder state lost - leak or crash depending on the
   callback), and the embedder's handle to the allocation is now a detached
   husk, so `JS_DetachArrayBuffer`-based invalidation no longer reaches the
   live alias. An embedder that detaches when its backing store moves (the
   `WebAssembly.Memory.buffer` pattern) is left with a JS-reachable
   use-after-free.
3. `new_len == 0`: `JS_DetachArrayBuffer` runs `free_func` here and the
   finalizer runs it again later with `data == NULL`. Consistent with the
   free(ptr) idiom, but combined with (2) the same callback can also fire
   with a NULL opaque, so the full contract is hard to satisfy; at minimum
   it deserves documentation.

`js_array_buffer_resize` already has the exact guard this needs:

    // TODO(bnoordhuis) support externally managed RABs
    if (abuf->free_func != js_array_buffer_free)
        return JS_ThrowTypeError(ctx, "external array buffer is not resizable");

Suggested fix: the same rejection in `js_array_buffer_transfer` for
`abuf->free_func != js_array_buffer_free` (all three variants). A richer
alternative - copy into js_malloc'd memory and detach the original through
its normal path - preserves transfer semantics, but the guard alone restores
soundness and matches resize.

Minimal repro (C): `JS_NewArrayBuffer` over a static or malloc'd byte array
with any free_func, eval `buf.transfer(1024)`, observe realloc on the
foreign pointer (crash under ASan); eval `buf.transfer()` (same length) and
observe the new buffer's free_func firing with opaque == NULL at teardown.

## Local impact

Every buffer rquickjs creates from Rust data is external (`ArrayBuffer::new`
registers a Vec-reconstructing free_func), so all byte arrays flux returns
(readMemory copies, sqlite blobs, subprocess output, file reads) hit (1) on
a length-changing `transfer`, and flux:wasm's `instance.memory` view
additionally hits (2): a same-length `transfer` escapes the plugin's
detach-on-grow, leaving a stale alias into moved wasmi memory. No plugin
level defense exists short of deleting `transfer` from the prototype; not
worth it while nothing calls transfer. Tracked here until upstream guards
the path.
