---
title: Mesh-local position and normal varyings in the lit vertex stage
description: litVertex writes vWorldPos and vNormal only, so world-keyed procedural detail swims across a moving mesh and an app that wants object-space detail copies the whole lit vertex stage to add vLocalPos/vLocalNormal, which drifts from every later litVertex change; a localVaryings option on the lit sources and the stock materials.
created: 2026-09-22
---

# Mesh-local position and normal varyings

Context: `packages/3d/src/glsl.ts` (`LitSourceOptions`, `litVertex`,
`unlitVertex`), `material.ts` (`phong`, `standard`, `unlit`, the class
caches keyed by option combination). The two sibling asks filed with
this one landed 2026-09-22: `prelude`/`surface` on `phong` and
`standard` (tier 2 of "Custom looks" is now a stock-material option, in
the class key, with the shadow twin attached whenever a surface is
given), and `sprite({ shape: "radial", falloff })`. This is what is
left, and it is what makes a `surface` keyed on object space possible
without a custom vertex stage.

Symptom: `litVertex` writes `vWorldPos` and `vNormal` only, so
world-keyed procedural detail on a moving mesh swims across the
surface. The app copied the lit vertex stage to add `vLocalPos` and
`vLocalNormal`, which drifts silently from future `litVertex` changes.

Done looks like:

- `LitSourceOptions.localVaryings?: boolean`: the vertex stage writes
  `vLocalPos` (object-space position, before uModel, after skinning and
  morphs) and `vLocalNormal` (object-space normal), and the fragment
  declares them in. Off by default: two more varyings cost every lit
  fragment otherwise.
- The same option on phong and standard (set for a `surface` that
  names them; without one, an explicit flag), part of the class key.
- Unlit gets the position half (`vLocalPos`) for the same reason.
- AGENTS.md "Custom looks": the varying list, and the rule "key
  procedural detail on vLocalPos for anything that moves".
