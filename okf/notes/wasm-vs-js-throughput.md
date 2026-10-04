---
title: QuickJS against flux:wasm - throughput on both lanes
description: Measured 2026-10-04 in flux itself on the six wat workloads of the engine note, on x86_64, the arm64 tablet and the armeabi-v7a TV; on compute the native lane runs 18-960x faster than the same loop in QuickJS and the interpreted lane 4-88x, so "a small constant factor" and "portability tool, not a speed tool" were both wrong; the one inversion is a host call out of the module, 3-7x the cost of a plain JavaScript call.
created: 2026-10-04
---

# QuickJS against flux:wasm - throughput on both lanes

The JavaScript half of the throughput question; the engine half, wasmi
against wasmtime's Pulley and Cranelift, is
[wasm-interpreter-throughput](wasm-interpreter-throughput.md). The question
an app developer asks is "should this hot loop move to wasm, and what does
it cost to talk to it", and this note is the answer in numbers. It closes
[wasm-vs-js-throughput](../done/wasm-vs-js-throughput.md), the last open
line of [wasm-native-execution](../done/wasm-native-execution.md).

## Method

`flux/examples/wasm_bench.js`, run on the `flux` binary. It loads the
engine note's wat module through `flux:wasm` (`new Module` on the wat text,
the one import `env.add` bound to the JavaScript function
`(a, b) => (a + b) | 0`) and next to it runs a JavaScript twin of every
export, written operation for operation on JavaScript's own buffers: an
`Int32Array` holding the same LCG fill, a `Uint8Array` for the sieve's
flags, `Math.imul` for the i32 multiplies, the `int_loop` i64 accumulator as
two unsigned halves (a double loses the low bits past 2^53, and a BigInt per
iteration is a different workload), and the `simd_sum` lanes as four scalar
accumulators, since QuickJS has no SIMD. Each case is called five times on
each side and the best wall time (`performance.now`) is reported. The
twin's result is compared with the export's and matched on every case of
every run, so the ratios compare the same work.

QuickJS is the engine flux embeds, quickjs-ng through rquickjs 0.14.0: a
bytecode interpreter, no JIT. The lane is read from `Flux.capabilities`.

Binaries, all release:

- desktop native: `make -C flux flux` (the `wasm-native` feature, wasmtime
  48.0.5 with Cranelift);
- desktop interpreted: the same cargo line with `WASM_NATIVE=0` (wasmi
  2.0.0 with `simd`), copied aside and not staged;
- Android: `cargo ndk -t <abi> --platform 26 build --release -p flux --bin
  flux --features rquickjs/bindgen[,wasm-native] --target-dir target/go`,
  run from `/data/local/tmp`. The flux manifest's rquickjs line has no
  `bindgen` feature (the workspace line lattice uses does), so a `-p flux`
  build for Android must add it on the command line, and armeabi-v7a needs
  the `BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi` target and sysroot
  that `lattice/Makefile.android` sets.

Machines, the engine note's plus the tablet. Desktop is the 13th Gen Intel
i5-1340P (x86_64 Linux), both lanes. Tablet is the Samsung SM-T500
(Snapdragon 662, arm64, Android 12), the native lane it ships with. TV is
the Philips TPM171E (MediaTek mt5891, 4 x ARMv7 with NEON, Android 8.0), the
interpreted lane it ships with. The devices ran the counts divided by ten.

The TV's wasmi column reproduces the engine note's standalone-crate figures
within a few percent (int_loop 1323 ms against 1333, float_loop 1364
against 1364, checksum 580 against 548), so flux's plugin adds nothing
measurable to compute; what it adds sits on the host-call path, below.

## Throughput

Ratio is QuickJS time over wasm time: how many times longer JavaScript takes.

Desktop, x86_64, native lane (Cranelift), full counts:

| case | QuickJS | wasm | ratio |
|---|---|---|---|
| int_loop (50M) | 9464.8 ms | 54.3 ms | 174x |
| float_loop (20M) | 2426.1 ms | 134.7 ms | 18.0x |
| checksum (2000 passes) | 2829.8 ms | 34.7 ms | 81.6x |
| sieve (100 reps) | 863.3 ms | 15.4 ms | 55.9x |
| host_calls (5M) | 353.4 ms | 1165.0 ms | 0.30x |
| simd_sum (4000 passes) | 6333.4 ms | 6.6 ms | 960x |

Desktop, x86_64, interpreted lane (wasmi), full counts:

| case | QuickJS | wasm | ratio |
|---|---|---|---|
| int_loop (50M) | 9491.0 ms | 224.8 ms | 42.2x |
| float_loop (20M) | 2438.2 ms | 185.7 ms | 13.1x |
| checksum (2000 passes) | 2822.9 ms | 105.7 ms | 26.7x |
| sieve (100 reps) | 865.0 ms | 76.8 ms | 11.3x |
| host_calls (5M) | 357.1 ms | 1775.4 ms | 0.20x |
| simd_sum (4000 passes) | 6322.7 ms | 71.9 ms | 87.9x |

Tablet, arm64, native lane (Cranelift), counts divided by ten:

| case | QuickJS | wasm | ratio |
|---|---|---|---|
| int_loop (5M) | 2354.6 ms | 18.2 ms | 129x |
| float_loop (2M) | 1162.2 ms | 30.9 ms | 37.6x |
| checksum (200 passes) | 1151.2 ms | 6.7 ms | 171x |
| sieve (10 reps) | 323.7 ms | 4.1 ms | 79.2x |
| host_calls (500k) | 96.8 ms | 529.4 ms | 0.18x |
| simd_sum (400 passes) | 1824.1 ms | 2.5 ms | 742x |

TV, armeabi-v7a, interpreted lane (wasmi), counts divided by ten:

| case | QuickJS | wasm | ratio |
|---|---|---|---|
| int_loop (5M) | 11160.3 ms | 1323.2 ms | 8.43x |
| float_loop (2M) | 5286.7 ms | 1364.3 ms | 3.88x |
| checksum (200 passes) | 5486.1 ms | 580.0 ms | 9.46x |
| sieve (10 reps) | 1708.6 ms | 299.1 ms | 5.71x |
| host_calls (500k) | 506.7 ms | 3557.5 ms | 0.14x |
| simd_sum (400 passes) | 8452.2 ms | 382.1 ms | 22.1x |

`new Module` on this small module, which on the native lane is a Cranelift
compile in memory (bare `flux` has no cache dir): 2.4 ms on the desktop,
18.9 ms on the tablet. The interpreted lane's translation is 2.4 ms on the
desktop and 2.9 ms on the TV.

## What the numbers say

- **"A small constant factor" was wrong by one to two orders of magnitude,
  and so was "portability tool, not a speed tool".** Even the interpreted
  lane runs compute 4x (float on the TV) to 88x (the SIMD-shaped loop on the
  desktop) faster than QuickJS. The conventional reading, not profiled here:
  QuickJS dispatches a bytecode per operation on tagged values with a type
  check each, while wasmi's register machine runs on typed i32 and f64
  locals and is a far cheaper interpreter per operation. The old wording
  came from comparing wasmi to JITs, never to the interpreter it actually
  sits next to.
- **The native lane is 18x to 960x.** Float is the smallest win on every
  machine (18x desktop, 38x tablet): a QuickJS double is an f64 once its tag
  is checked, and the loop is bound by `sqrt`. Integer and memory loops are
  56x to 174x, since JavaScript has no i32 and every add checks for overflow
  into a double, and the SIMD-shaped loop is 742x to 960x: four scalar
  accumulators against one vector instruction per 16 bytes.
- **Host calls are the inversion, on both lanes and every machine.** A call
  out of the module into a JavaScript import costs 233 ns on the desktop's
  native lane against 71 ns for a plain JavaScript-to-JavaScript call
  (3.3x), 1.06 us against 0.19 us on the tablet (5.5x), and 7.1 us against
  1.0 us on the TV (7x). Against the engine note's Rust-bound import on the
  same desktop (13.7 ms for 5M calls, 2.7 ns each) the difference is the
  plugin's marshalling, the rquickjs call with argument conversion and
  result coercion: about 85x the raw engine path. In the module's own
  currency, one import call on the native lane costs two to three hundred
  iterations of the integer loop (233 ns against 1.1 ns per iteration on the
  desktop, 1.06 us against 3.6 ns on the tablet). This is the "one coarse
  call per frame, never one per entity" rule in numbers.
- **The machines scale as expected.** Per iteration of the integer loop, the
  tablet's native lane is 3.3x the desktop's (3.6 against 1.1 ns) while its
  QuickJS is 2.5x (471 against 189 ns); the TV's interpreted lane is 59x the
  desktop's interpreted lane (265 against 4.5 ns) while its QuickJS is 11.8x
  (2232 ns). The ratio an app sees is smallest on the TV, and still 4x to
  22x.
- **Docs consequence.** The `wasm.d.ts` note states these ranges instead of
  "portability tool rather than a speed tool", and section 4 of the design
  document carries them next to what a module cannot touch.

## Not measured

- A large mixed workload (a whole interpreter compiled with emscripten): it
  needs an emscripten import shim, which the bench does not carry.
- A C-via-emcc variant of the workloads: the wat module is what the engine
  half ran, so the four columns stay on one module.
- The cost of `instance.call` itself (JavaScript into wasm) per call, and of
  memory traffic across the boundary (`readMemory`, `writeMemory`, the
  aliasing `ArrayBuffer`). The cases make one `call` per run, so the tables
  hold compute and the import direction only.
