---
title: Crate alternatives for hand-rolled spatial code
description: Survey of crates that could replace or extend alloy/src/spatial, with measured dependency costs; conclusion is no action, demand-gated pointers only.
created: 2026-08-31
---

# Crate alternatives for hand-rolled spatial code

Survey of what the ecosystem offers against the hand-rolled code in
`alloy/src/spatial/` (~2.1k lines: transform arena + dirty queue + sinks,
BVH with SAH rotations, raycast/pick, transitions, math). Decision:
nothing is adopted now; the pointers below are demand-gated.

## The shape of the answer

Crates exist for the leaf algorithms (math, BVH, raycast), not for the
part that carries the value: the transform arena, dirty propagation, and
the SinkWriter flush into DrawParams/shared slots/instance records. That
part is solidrt-specific and stays hand-rolled by necessity. The
hand-rolled BVH already does SAH rotations and hits 0.004 ms on the
scene-walk bench, so no crate solves a performance problem there.

## Candidates, by verdict

If picking grows into real collision queries (capsules, convex shapes,
closest-point, sweeps): **parry3d** (dimforge) is the fit; swap bvh.rs +
pick.rs rather than grow them into a collision library. Since 0.30 parry
is built on **glam** (via their glamx wrapper), NOT nalgebra; adopting
parry and adopting glam merge into one decision. Narrower alternative:
the **bvh** crate (svenstaro).

If a path/polygon-to-mesh feature lands (3d path sweep end caps, filled
paths): triangulation is the algorithm not worth hand-rolling; degenerate
cases (collinear, self-touching contours) drown naive versions.
Options: **earcutr** (ear clipping, tiny, no exactness guarantees),
**spade** + **robust** (constrained Delaunay with exact predicates),
**lyon** (full path tessellation: bezier flattening, strokes, fills; the
standard for Rust 2d GPU renderers). Caps = spade/earcutr; strokes/fills
scope = lyon.

If a CPU hot loop shows in a profile (resample.rs, yuv.rs conversions,
vectorized spatial matrix flush): **wide** (+ safe_arch) is portable SIMD
on stable Rust and is already in the workspace lock transitively. For
plain matrix math: **glam** (SIMD mat4/quat), only if math itself shows
in a profile; today spatial math is 132 lines feeding a
sub-10-microsecond BVH.

If per-frame allocation churn shows in a profile: **smallvec** (inline
child lists / dirty queues) and **foldhash** (fast hasher for hot maps;
std default is SipHash, deliberately slow). Both already in the lock
transitively; adding a direct dep costs nothing new.

Not interesting: **rstar** (R*-tree; redundant with the owned BVH for
any 2d index need), **ena** (union-find, no use case), **heapless**
(no_std), the num-traits/approx layer (our math is concrete f32),
proc-macro plumbing (build-time only).

Non-recommendation: the spatial arena is exactly what **thunderdome** /
generational-arena provide (generational ids + free list), but it is
~40 working, tested lines; swapping is churn for symmetry.

## Measured costs (2026-08-31, this machine, cold release builds)

Scratch crates actually instantiating the code paths, default release
profile, stripped sizes, CARGO_BUILD_JOBS=8:

| crate                            | cold build | binary delta | dep graph |
|----------------------------------|-----------:|-------------:|----------:|
| baseline hello-world             |       ~0 s | (344 KB base)|   1 crate |
| nalgebra 0.35 (mat4/quat/inverse)|       12 s |       +30 KB | 23 crates |
| parry3d 0.30 (TriMesh + raycast) |       17 s |      +114 KB | 47 crates |

Binary size is NOT a real argument against these in optimized Rust:
monomorphization keeps only instantiated code, so even nalgebra costs
tens of KB for concrete f32 usage. The "nalgebra bloats binaries"
reputation is debug builds and API-surface horror, not linker output.
Compile time is paid once, cold; warm/incremental builds never touch it.

## Anatomy of parry3d's 47 transitive crates

- ~15 micro-crates (smallvec, arrayvec, slab, either, bitflags, log,
  foldhash, ordered-float, ...): header-file-sized, ecosystem plumbing.
- ~5 numerics (num-traits, num-complex, libm, approx): std ships no
  numeric trait hierarchy, everyone converges here.
- ~7 proc-macro stack (syn x2, quote, proc-macro2, ...): compile-time
  only, zero bytes in the binary.
- ~4 portable SIMD (simba, wide, safe_arch, bytemuck): std::simd is
  unstable, this is glamx's vectorization layer.
- ~5 real algorithmic freight (rstar, spade, robust, ena): serve parry's
  mesh-transformation corner (convex decomposition, mesh intersection),
  unreachable from a raycast user, and parry has no feature flag to shed
  them. The one legitimately unearned group.

Most of the plumbing is already in our workspace lock via existing deps,
so the marginal addition would be well under 47. The maintained-by-whom
surface is dimforge, rust-lang nursery, and top-tier infrastructure
crates throughout; spade/rstar/ena for unused features is the only part
held against it.

## The residual arguments that survive scrutiny

Not size, not compile time. What remains: dependency count as audit
surface for 500 lines of working BVH, vocabulary friction (glam Vec3/Mat4
next to euclid and the [f32; 3] arrays the flux:spatial FFI uses,
conversions at every boundary), and the fact that the owned BVH already
outperforms the need. Arguments of taste and scope; they flip the moment
a roadmap item (collision queries, path-to-mesh) needs the algorithm.
