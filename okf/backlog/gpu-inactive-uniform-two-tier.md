---
title: Two-tier handling for declared-but-inactive uniforms
description: Uniform validation throws on any name absent from the reflected table, but GL reflection only sees active uniforms - a declared uniform the compiler optimized out counts as a typo. A compile-time scan of the source for declared uniform names would let that sub-case warn instead of throw.
created: 2026-07-30
---

# Two-tier handling for declared-but-inactive uniforms

Split out of [gpu-callsite-validation](../done/gpu-callsite-validation.md), which
landed the strict variant: every param/texture name is checked against the
program's reflected (active) uniform table and an absent name throws. The
wrinkle is GL's: a uniform that is declared but optimized out reflects as
absent, so commenting out a uniform's use in GLSL turns the JS write that
drives it into a throw - the historical reason WebGL made the whole class
silent.

The two-tier fix: at compile time, text-scan the full source (preamble
included, comments stripped) for `uniform` declarations and keep the declared
name set alongside the active table (per stage for the raw compile path,
since `linkProgram` no longer sees sources). Validation then distinguishes
never-declared (throw, as today) from declared-but-inactive (warn, forward).
Known approximations to accept: names produced by macro expansion or
`#if`-excluded branches, struct/block members, arrays reflecting as
`name[0]` - all only matter when the uniform is ALSO inactive, so a
warn-level miss is tolerable.

Worth doing when the strict throw bites in practice (shader iteration where
uniform uses are toggled while the JS driver keeps writing them). Until then
strict keeps the code smaller and the contract simpler.
