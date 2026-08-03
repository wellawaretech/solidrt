---
type: upstream-issue
title: rquickjs external ArrayBuffer callbacks double-invoked on detach
description: QuickJS invokes an external ArrayBuffer's free callback on detach AND again at finalization (with data == NULL); rquickjs's shims ignore the data pointer and consume their opaque unconditionally, so safe from_source + detach() - or pure JS transfer(0) on any Rust-minted buffer - is a double free.
project: rquickjs (github.com/DelSkayn/rquickjs)
versions: rquickjs 0.12.1 (rquickjs-core 0.12.1, rquickjs-sys 0.12.1 vendoring quickjs-ng 0.15.1)
status: unfiled
link:
timestamp: 2026-08-03T00:00:00Z
---

# rquickjs: external ArrayBuffer callbacks double-invoked on detach

Found 2026-08-03 while giving flux:wasm a linear-memory ArrayBuffer view
(SIGSEGV in the detach-on-grow tests). Sibling engine-side issue:
[[quickjs-ng-transfer-external-buffers]].

## Draft report

`ArrayBuffer::from_source` / `from_source_shared` / `from_source_immutable`
plus `ArrayBuffer::detach()` - all safe API - is a double free.

QuickJS's contract for external buffers is malloc/free-style:
`JS_DetachArrayBuffer` invokes `free_func(rt, opaque, data)` and does NOT
clear it, and `js_array_buffer_finalizer` invokes it AGAIN at collection
time, then with `data == NULL`. A C-idiom callback that does `free(ptr)` is
fine with that (free(NULL) is a no-op). rquickjs's callbacks are not:

- the `from_external` shim (value/array_buffer.rs) ignores the data pointer
  and does `Box::from_raw(opaque)` unconditionally - called twice, that is a
  double free of the boxed drop closure (and a double drop of whatever the
  source held);
- `ArrayBuffer::new`'s `drop_raw` reconstructs the backing
  `Vec::from_raw_parts(ptr, capacity, capacity)` from the data pointer - on
  the second call `ptr` is NULL with a nonzero capacity, immediate UB.

Reachable without any unsafe:

1. Safe Rust: `ArrayBuffer::from_source(ctx, vec![0u8; 16])?` then
   `detach()`; crash at runtime teardown when the finalizer re-runs the shim.
2. Pure JS, no Rust-side detach at all: quickjs-ng exposes
   `ArrayBuffer.prototype.transfer`, and `transfer(0)` calls
   `JS_DetachArrayBuffer` internally. Any buffer created by
   `ArrayBuffer::new` / `TypedArray::new` / `from_source` that a script
   calls `.transfer(0)` on double-frees at teardown.

Suggested fix: the callbacks must tolerate the second invocation - return
early when the data pointer is NULL. The legitimate first call never passes
NULL (Vec/Box/Arc sources hand out dangling-but-non-null pointers even when
empty), so a NULL check is exact. Alternatively track detachment and skip.

## Local impact and workaround

flux:wasm's `instance.memory` needed a detachable external buffer (detach on
guest memory growth). Workaround: `array_buffer_over` in
flux/src/plugins/modules/wasm.rs creates the buffer via raw
`qjs::JS_NewArrayBuffer` with `free_func = NULL` (both invocation sites
become no-ops) and pins the backing wasm instance from the plugin's handler
registry instead of from a drop closure.

The rest of flux remains exposed through path 2 (every `TypedArray::new`
buffer we return: readMemory copies, sqlite blobs, subprocess output, file
reads), but only if a script calls `.transfer(0)` on one - nothing does.

On `resolved`: `array_buffer_over` can revert to `ArrayBuffer::from_source`
with a source holding the instance Rc, and the registry pin
(`MemoryView._instance`) comes out.
