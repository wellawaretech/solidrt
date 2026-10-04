---
title: Measure flux:wasm throughput against JavaScript
description: flux:wasm runs on the wasmi interpreter and the docs now say "a small constant factor over JavaScript on tight compute", but nobody has measured wasmi against QuickJS in flux; a small benchmark would back that claim with a number.
created: 2026-08-17
completed: 2026-10-04
---

# Measure flux:wasm throughput against JavaScript

Measured 2026-10-04: numbers and method in
[wasm-vs-js-throughput](../notes/wasm-vs-js-throughput.md). The docs wording
followed (`wasm.d.ts`, sections 4 and 5 of [design/wasm](../design/wasm.md)).

The engine-against-engine half (wasmi against wasmtime, Pulley and
Cranelift) is measured in
[wasm-interpreter-throughput](../notes/wasm-interpreter-throughput.md); this
item is the JavaScript half, wanted by
[wasm-native-execution](../done/wasm-native-execution.md) for the docs
wording.

## Symptom

`flux:wasm` is documented as a portability tool, not a speed tool, and its
docs say tight compute runs "a small constant factor" faster than
JavaScript while call-heavy code can be slower. That framing is inferred
(a guess of 2-5x on numeric loops), not measured: wasmi publishes benchmarks against other wasm
runtimes (Coremark on par with Wasm3 and Stitch, roughly 15-25x below
JIT/native on compute) but never against a JS interpreter, and QuickJS
numbers come from a different benchmark family. So we cannot state a
ratio, and a user asking "should I move this hot loop to wasm?" gets a
shrug.

## Trigger

We want to print a ratio in the docs, or a user asks whether moving a hot
loop to wasm is worth it and we want a real answer.

## Done looks like

A `notes/` entry with numbers from a release client on one desktop machine
and one device, for the same workload written twice: JS, and C compiled
with `emcc -sSTANDALONE_WASM=1 --no-entry`. Workloads:

1. numeric loop (no memory traffic)
2. memory-heavy loop (checksum/hash over a buffer)
3. call-heavy with host imports (marshalling cost of the resumable-import
   protocol)
4. the lua64 cartridge as a real mixed workload

Then adjust the docs wording to whatever the numbers say.

## Involves

A bench under `flux/examples/`, no runtime changes. Half a day.
