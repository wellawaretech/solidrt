---
title: ImageFilter::new_matrix asserts on Impeller's by-design null for identity
description: Impeller's ImpellerImageFilterCreateMatrixNew legitimately returns null (marked IMPELLER_NULLABLE) for identity or non-finite matrices; the impellers 0.4.2 crate asserts non-null on that return, so a plain identity matrix aborts the process.
created: 2026-09-02
status: unfiled
project: impellers (Rust bindings; prebuilt Impeller behaves as designed)
versions: impellers 0.4.2 (engine c177d531d694a51d84f7111b8bc4c425442b1df5)
link:
---

# ImageFilter::new_matrix asserts on Impeller's by-design null for identity

`ImageFilter::new_matrix(&Matrix::identity(), ...)` panics inside impellers
0.4.2 (`assertion failed: !result.is_null()`, lib.rs:2090). The fault split,
traced through the engine source at the pinned SHA:

- Impeller behaves as designed. `ImpellerImageFilterCreateMatrixNew` forwards
  to `flutter::DlMatrixImageFilter::Make`, which deliberately returns nullptr
  when the matrix `IsIdentity()` (identity is a no-op, so "no filter") or is
  not finite. The C header declares the return `IMPELLER_NULLABLE`. At most a
  doc nit upstream: the "identity returns null" contract is not spelled out
  in the API comment.
- The impellers crate is the bug. `new_matrix` does
  `assert!(!result.is_null())` on that documented-nullable return, turning a
  legal null into a process abort. It should return `Option<Self>` (or
  special-case identity).

Verified empirically via the raw `sys` binding, bypassing the assert
(`alloy/examples/matrix_filter_probe.rs`, 2026-09-02, desktop Linux, off any
GPU context): identity and NaN matrices return null; translate and scale
matrices construct fine. So the constructor works for any real matrix; only
the identity/non-finite cases hit the assert.

Impact for us: we wanted the identity matrix filter as the backdrop-capture
filter for color-only `backdropFilter` (the backdrop argument is what makes
save_layer capture the pixels beneath). Even a fixed crate would hand us
`None` there - Impeller has no identity matrix filter to give. The
workaround in alloy/src/rendertree/kinds/filter.rs
`to_backdrop_image_filter` - a sub-pixel blur (sigma 0.001) standing in for
the identity - is therefore the right call permanently, not a stopgap:
visually indistinguishable, constructible, and it still triggers the
capture.
