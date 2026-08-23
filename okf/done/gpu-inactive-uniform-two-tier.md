---
title: Two-tier handling for declared-but-inactive uniforms
description: Uniform validation threw on any name absent from the reflected table, but GL reflection only sees active uniforms - a declared uniform the compiler optimized out counted as a typo. A compile-time scan of the source for declared uniform names now lets that sub-case warn instead of throw.
created: 2026-07-30
completed: 2026-08-23
---

Done 2026-08-23: the two-tier scan. `declared_uniform_names` (alloy
gpu/program.rs) strips comments and collects every `uniform` declarator
name from the full source (preamble included, per stage on the raw path:
`CompiledStage` carries the names since `linkProgram` never sees sources).
Names declared but not reflected land in the `UniformTable` as
`UniformKind::Inactive`, so the table that crosses to the UI-thread mirror
is still the single validation currency. `validate_params` and
`validate_texture_bindings` warn and skip an Inactive name; never-declared
still throws, with the active list in the message. The raster-side rebind
guards accept declared names too (`ShaderProgram::accepts_uniform`); the
apply in pass.rs already skipped names with no location. Tests in
alloy/src/tests/gpu_validate.rs cover both tiers and the scanner (comments,
multi-declarator, arrays, interface blocks skipped).

Not done, on purpose: "validate at creation" in `@solidrt/3d`. The uniform
table never crosses to JS, so the library cannot check a material before
`add()` without a new reflection API; with declared-but-inactive no longer
throwing, the remaining failure is a genuine typo, which the existing
message already names. The `optional: true` marker stays rejected.

The 3d material variants that multiply an unused alpha in purely to keep the
shared param object valid can drop that now.

The record as filed follows.


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

## It bit, 2026-08-17

The predicted shape, but from a second direction that is harder to design
around: a parameterised material. The natural implementation is one uniform
set plus several shader variants (mapped/unmapped, blended/opaque, culled or
not), and any variant that happens not to reference one of the shared
uniforms compiles it out and then throws at `add()` - at attach time, far
from the material definition, for a param object that is correct. The
workaround was to make every variant reference every uniform
unconditionally (multiplying an unused alpha in) purely to keep the param
object uniform across variants, which is a real cost paid in shader source to
satisfy a validation rule.

Two things would each remove the class, and they are alternatives, not
stages:

- The two-tier scan above: declared-but-inactive warns, never-declared
  throws. Correct and general.
- **Validate at creation, not at attach.** The failure is knowable when the
  material is built; surfacing it there names the material instead of naming
  a draw entry. Cheaper than the scan and fixes the confusing part (where the
  error appears) without fixing the wrong part (that it errors at all), so it
  composes with the scan rather than replacing it.

A third form was asked for and is worth recording as rejected-unless-argued:
an `optional: true` marker per param. It puts the knowledge in the call site
that already has it, but it is a second source of truth beside the shader
source, and the scan derives the same fact without being told.

First consumer:
[3d-material-uniform-plumbing](../done/3d-material-uniform-plumbing.md) section 3.
