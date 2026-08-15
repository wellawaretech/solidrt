---
title: "flux:ffi polish: typed memory reads, optional symbols"
description: readMemory returns raw bytes so every numeric result goes through hand-rolled DataView reinterpretation, and every declared symbol must resolve so one absent symbol takes down a whole binding; add typed read helpers and a per-symbol optional flag.
created: 2026-08-15
completed: 2026-08-15
---

# flux:ffi polish: typed memory reads, optional symbols

Small surface, real bug classes removed. Bundled because each is a few
lines and they ship together.

## Typed memory reads

`readMemory(ptr, len)` returns a raw `Uint8Array`, so numeric results need
manual reinterpretation through a `DataView`. C libraries hand back `double` arrays and typed
datasets: exactly where hand-rolled stride and
endianness handling goes quietly wrong.

Done: typed reads for the scalar set, e.g. `readF64Array(ptr, count)`,
`readF32Array`, `readI32Array`, `readI64Array` (BigInt64Array), or a typed
overload of `readMemory`. Native byte order, copied out (same contract as
`readMemory`). Optionally a matching typed `writeMemory` accepting any
typed array, which it nearly does already since it takes the bytes.

## Optional symbols

Every declared symbol must resolve or the constructor throws. For a large C
library that drifts across distro versions, bindings become all-or-nothing:
one renamed or absent symbol takes down a binding that would otherwise
work.

Done: `{ optional: true }` on a `SymbolDecl` yields `undefined` in
`symbols` when absent instead of throwing. The `symbols` record type
already permits `undefined` in its value signature, so this is mostly the
resolve loop in forge/src/ffi.rs plus the type.

## ptr/i64 return as BigInt

Returns always come back as BigInt, so pointer arithmetic drags BigInt
literals through the call site. Purely sugar and arguably correct as-is
given precision. Not changing; recorded here so the ask is not re-raised.
If anything, a documented `Number(ptr)` guidance for the common
address-fits-in-53-bits case is enough.

## Outcome

- Typed reads shipped as `readMemory(ptr, count, type?)` reusing the ffi
  type names (u8 default, i32/i64/f32/f64/ptr -> matching typed array,
  native order, copied out); no new method names. `writeMemory` accepts any
  typed array or ArrayBuffer. Reinterpretation lives in the plugin; forge
  `memory_read` stays bytes.
- Optional symbols shipped as `{ optional: true }` on the declaration;
  forge carries `SymbolDecl { name, sig, optional }` and an unresolved
  code pointer, the plugin exposes `undefined` in `symbols`.
- BigInt returns unchanged, as decided above.
- Verified in flux/examples/ffi_buffer.js.

## Involves (as planned)

- forge/src/ffi.rs: optional resolve.
- flux/src/plugins/modules/ffi.rs: typed read methods, `optional` in decl
  parsing.
- packages/flux-types + docs mirror the change.
