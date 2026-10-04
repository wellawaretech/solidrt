---
title: wasmi against wasmtime's Pulley - throughput and compile time
description: Measured 2026-10-04 on an x86_64 desktop and the armeabi-v7a TV with the same wat workloads; Pulley on stable Rust runs 2-6x slower than wasmi on x86_64, trades blows on the TV (faster on integer and memory loops, 1.4-2.1x slower on float, host calls and SIMD), and compiles a 287 KB module in 3.3 s on the TV where wasmi translates it in 79 ms. Cranelift native on x86_64 is 1.4-20x faster than wasmi.
created: 2026-10-04
---

# wasmi against wasmtime's Pulley - throughput and compile time

The measurement behind the interpreted-lane decision in
[wasm-native-execution](../done/wasm-native-execution.md): whether Pulley
(wasmtime's interpreter target) can replace wasmi on the targets that cannot
run native code, iOS and armeabi-v7a. The bar there is "not meaningfully
slower". The JavaScript side of
[wasm-vs-js-throughput](../done/wasm-vs-js-throughput.md) is not measured
here.

## Method

One standalone crate linking both engines, pinned as the tree pins them:
wasmi 2.0.0 with `simd`, wasmtime 48.0.5 with `default-features = false` and
`runtime`, `std`, `pulley`, `cranelift`, `wat`. Stable Rust 1.96.0, release
profile with fat LTO and one codegen unit. Both engines load the same wat
module (below), with one host import `env.add` bound through each engine's
plain `func_wrap`. Each case is called five times and the best wall time is
reported; every engine returned the same result on every case. Three lanes:

- wasmi, default config (SIMD and relaxed-SIMD on through the feature).
- wasmtime with `Config::target("pulley64")` on the desktop, `pulley32` on the
  TV. wasmtime requires the Pulley target to match the host's pointer width.
- wasmtime with the default target: Cranelift native on x86_64. On the TV
  this lane is Pulley again, silently: wasmtime has no 32-bit ARM backend and
  `Config` falls back to `pulley_host()` for the compiler target. The two
  columns there agree to the millisecond for that reason.

Pulley's interpreter loop is a `loop` over a `match` on stable Rust. Its
faster tail-call dispatch needs either nightly's guaranteed tail calls or an
unaudited assumption that LLVM makes them, both behind `--cfg` flags the
crate's own documentation calls experimental. The numbers below are the
shipping configuration.

Machines: desktop is a 13th Gen Intel i5-1340P (x86_64 Linux). TV is the
Philips TPM171E, MediaTek mt5891, 4 x ARMv7 with NEON, Android 8.0 (API 26),
the armeabi-v7a device in the device loop. The TV ran the counts divided by
ten. The armeabi-v7a binary was cross-built with `cargo ndk -t armeabi-v7a
--platform 26 build --release` and run from `/data/local/tmp`; wasmtime with
Cranelift and Pulley builds and runs on a 32-bit Android host without
configuration.

## Throughput

Ratio is time over wasmi time; lower is faster.

Desktop, x86_64, full counts:

| case | wasmi | Pulley | ratio | Cranelift native | ratio |
|---|---|---|---|---|---|
| int_loop (50M) | 188.1 ms | 567.5 ms | 3.02x | 45.0 ms | 0.24x |
| float_loop (20M) | 154.9 ms | 969.1 ms | 6.26x | 112.1 ms | 0.72x |
| checksum (2000 passes) | 88.5 ms | 179.4 ms | 2.03x | 28.7 ms | 0.32x |
| sieve (100 reps) | 60.8 ms | 113.3 ms | 1.86x | 12.9 ms | 0.21x |
| host_calls (5M) | 85.9 ms | 215.2 ms | 2.51x | 13.7 ms | 0.16x |
| simd_sum (4000 passes) | 121.0 ms | 253.6 ms | 2.10x | 5.5 ms | 0.05x |

TV, armeabi-v7a, counts divided by ten (the native column is Pulley, see
above, so it is omitted):

| case | wasmi | Pulley | ratio |
|---|---|---|---|
| int_loop (5M) | 1332.9 ms | 944.2 ms | 0.71x |
| float_loop (2M) | 1363.6 ms | 2279.1 ms | 1.67x |
| checksum (200 passes) | 548.2 ms | 483.8 ms | 0.88x |
| sieve (10 reps) | 280.5 ms | 236.4 ms | 0.84x |
| host_calls (500k) | 199.7 ms | 426.9 ms | 2.14x |
| simd_sum (400 passes) | 363.7 ms | 508.9 ms | 1.40x |

Float is Pulley's weakest case on both machines (6.26x on x86_64, 1.67x on
the TV), and host calls its second weakest (2.51x and 2.14x). The host-call
path is the one every `flux:wasm` import takes. The mechanisms behind the two
gaps were not profiled.

## Compile time

The lua64 cartridge, 287,318 bytes of emscripten output, compiled only
(imports are not resolved for this), best of three. wasmi's default is lazy
translation, so its eager mode is the comparable figure.

| engine | desktop | TV | artifact |
|---|---|---|---|
| wasmi, lazy (default) | 1.6 ms | 18.4 ms | none |
| wasmi, eager | 5.9 ms | 79.1 ms | none |
| wasmtime Pulley | 225.7 ms | 3276.8 ms | 720 KB, 2.51x the wasm |
| wasmtime Cranelift native | 219.9 ms | - | 856 KB, 2.98x the wasm |

Single-threaded compilation; wasmtime's `parallel-compilation` feature (rayon)
was not enabled. The desktop figure is the install pre-warm cost on a native
target. The TV figure is what the Pulley lane adds at install for a module of
this size, and a larger module scales roughly linearly.

## What the numbers say

- On x86_64, Pulley is 2 to 6 times slower than wasmi on every workload. This
  is irrelevant to the lane choice (x86_64 runs Cranelift native) but it is
  what the forced-interpreter dev switch would run if it selected Pulley on a
  desktop, so that switch does not reproduce the TV's behaviour either way.
- On the TV, the only device that would actually run the interpreted lane,
  Pulley wins the integer and memory loops by 12 to 29 percent and loses
  float by 1.67x, host calls by 2.14x and SIMD by 1.40x. Against the bar
  ("not meaningfully slower") it is a draw on the compute a module exists for
  and a loss on the two things `flux:wasm` modules do constantly: call the
  host and run f64 math.
- Choosing Pulley on the interpreted targets buys no speed and adds an
  install-time compile of seconds per module on the device and Cranelift in
  the binary (roughly the 7.8 MB desktop figure, against wasmi's 1.39 MB).
  wasmi gives those targets the same throughput with no compile step and no
  compiler.
- Cranelift native on x86_64 is 1.4x faster than wasmi on f64, 3 to 6x on
  integer, memory and host calls, and 22x on SIMD. That is the win the native
  lane is for.

## Workloads

The wat module both engines ran. Every export takes an i32 count and returns
an i64 so results can be compared across engines; `fill` seeds the first
64 KiB with an LCG pattern for `checksum` and `simd_sum`, `sieve` uses the
second 64 KiB.

```wat
(module
  (import "env" "add" (func $add (param i32 i32) (result i32)))
  (memory (export "memory") 2)

  (func (export "fill") (param $unused i32) (result i64)
    (local $i i32) (local $x i32)
    (local.set $x (i32.const 7))
    (loop $l
      (i32.store (local.get $i) (local.get $x))
      (local.set $x (i32.add (i32.mul (local.get $x) (i32.const 1103515245)) (i32.const 12345)))
      (local.set $i (i32.add (local.get $i) (i32.const 4)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 65536))))
    (i64.const 0))

  ;; Tight integer loop: LCG + mix, no memory traffic.
  (func (export "int_loop") (param $n i32) (result i64)
    (local $i i32) (local $x i32) (local $acc i64)
    (local.set $x (i32.const 12345))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $x (i32.add (i32.mul (local.get $x) (i32.const 1664525)) (i32.const 1013904223)))
      (local.set $acc (i64.add (local.get $acc)
        (i64.extend_i32_u (i32.xor (i32.shr_u (local.get $x) (i32.const 16)) (i32.rotl (local.get $x) (i32.const 5))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (local.get $acc))

  ;; Tight f64 loop: bounded quadratic iteration with a sqrt.
  (func (export "float_loop") (param $n i32) (result i64)
    (local $i i32) (local $x f64) (local $y f64) (local $acc f64)
    (local.set $x (f64.const 0.1)) (local.set $y (f64.const 0.2))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $acc (f64.add (local.get $acc)
        (f64.sqrt (f64.add (f64.mul (local.get $x) (local.get $x)) (f64.mul (local.get $y) (local.get $y))))))
      (local.set $x (f64.add (f64.sub (f64.mul (local.get $x) (local.get $x)) (f64.mul (local.get $y) (local.get $y))) (f64.const 0.3)))
      (local.set $y (f64.add (f64.mul (f64.mul (f64.const 2) (local.get $x)) (local.get $y)) (f64.const 0.1)))
      (local.set $x (f64.mul (local.get $x) (f64.const 0.25)))
      (local.set $y (f64.mul (local.get $y) (f64.const 0.25)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i64.reinterpret_f64 (local.get $acc)))

  ;; FNV-1a over the first 64 KiB as i32 words, `passes` times.
  (func (export "checksum") (param $passes i32) (result i64)
    (local $p i32) (local $i i32) (local $h i32)
    (local.set $h (i32.const 0x811c9dc5))
    (block $done (loop $outer
      (br_if $done (i32.ge_u (local.get $p) (local.get $passes)))
      (local.set $i (i32.const 0))
      (loop $inner
        (local.set $h (i32.mul (i32.xor (local.get $h) (i32.load (local.get $i))) (i32.const 16777619)))
        (local.set $i (i32.add (local.get $i) (i32.const 4)))
        (br_if $inner (i32.lt_u (local.get $i) (i32.const 65536))))
      (local.set $p (i32.add (local.get $p) (i32.const 1)))
      (br $outer)))
    (i64.extend_i32_u (local.get $h)))

  ;; Sieve of Eratosthenes over the second 64 KiB (one byte per number), `reps` times.
  (func (export "sieve") (param $reps i32) (result i64)
    (local $r i32) (local $i i32) (local $j i32) (local $count i32)
    (block $done (loop $outer
      (br_if $done (i32.ge_u (local.get $r) (local.get $reps)))
      (memory.fill (i32.const 65536) (i32.const 1) (i32.const 65536))
      (local.set $count (i32.const 0))
      (local.set $i (i32.const 2))
      (block $idone (loop $il
        (br_if $idone (i32.ge_u (local.get $i) (i32.const 65536)))
        (if (i32.load8_u (i32.add (i32.const 65536) (local.get $i)))
          (then
            (local.set $count (i32.add (local.get $count) (i32.const 1)))
            (if (i32.lt_u (local.get $i) (i32.const 256))
              (then
                (local.set $j (i32.mul (local.get $i) (local.get $i)))
                (block $jdone (loop $jl
                  (br_if $jdone (i32.ge_u (local.get $j) (i32.const 65536)))
                  (i32.store8 (i32.add (i32.const 65536) (local.get $j)) (i32.const 0))
                  (local.set $j (i32.add (local.get $j) (local.get $i)))
                  (br $jl)))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $il)))
      (local.set $r (i32.add (local.get $r) (i32.const 1)))
      (br $outer)))
    (i64.extend_i32_u (local.get $count)))

  ;; `n` calls out to the host import, accumulating its result.
  (func (export "host_calls") (param $n i32) (result i64)
    (local $i i32) (local $acc i32)
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $acc (call $add (local.get $acc) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i64.extend_i32_u (local.get $acc)))

  ;; i32x4 multiply-accumulate over the first 64 KiB, `passes` times.
  (func (export "simd_sum") (param $passes i32) (result i64)
    (local $p i32) (local $i i32) (local $acc v128)
    (block $done (loop $outer
      (br_if $done (i32.ge_u (local.get $p) (local.get $passes)))
      (local.set $i (i32.const 0))
      (loop $inner
        (local.set $acc (i32x4.add (local.get $acc)
          (i32x4.mul (v128.load (local.get $i)) (v128.const i32x4 3 5 7 11))))
        (local.set $i (i32.add (local.get $i) (i32.const 16)))
        (br_if $inner (i32.lt_u (local.get $i) (i32.const 65536))))
      (local.set $p (i32.add (local.get $p) (i32.const 1)))
      (br $outer)))
    (i64.extend_i32_u
      (i32.add
        (i32.add (i32x4.extract_lane 0 (local.get $acc)) (i32x4.extract_lane 1 (local.get $acc)))
        (i32.add (i32x4.extract_lane 2 (local.get $acc)) (i32x4.extract_lane 3 (local.get $acc))))))
)
```

## Not measured

- The JavaScript side (QuickJS on the same workloads): measured since, in
  [wasm-vs-js-throughput](wasm-vs-js-throughput.md).
- The cartridge as a running workload: it needs the 29-import emscripten
  shim, which the standalone crate does not have.
- Pulley with tail-call dispatch, which needs nightly or an unverified LLVM
  assumption and is not a shipping configuration.
- wasmtime's parallel compilation, which would divide the compile figures by
  the core count at the price of a rayon dependency.
