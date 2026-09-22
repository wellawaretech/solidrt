---
title: Material hooks: surface on the stock materials, local varyings, sprite falloff
description: Adding one procedural term to phong or standard means rebuilding the material as a shaderMaterialClass and seeding its uniforms by hand, world-keyed detail swims on a moving mesh because the lit vertex stage carries no local position or normal, and a sprite without a map is a hard square; three small additions to material.ts and glsl.ts.
created: 2026-09-22
---

# Material hooks: surface on the stock materials, local varyings, sprite falloff

Context: `packages/3d/src/material.ts` (`phong`, `standard`, `sprite`,
the class caches keyed by option combination), `glsl.ts`
(`LitSourceOptions` with `prelude`/`surface`, `litVertex`,
`unlitFragment`). Tier 2 of "Custom looks" in AGENTS.md exists today
only through `shaderMaterialClass({ vertex: litVertex(o), fragment:
standardFragment({ ...o, surface }) })`. Each item is independent; the
first two together make tier 2 a one-liner.

## 1. `surface` and `prelude` on phong and standard

Symptom: one procedural detail term (a stripe, a wear mask, a
displacement-driven tint) on a stock material means the full class
rebuild above plus seeding `uColor`/`uMetalness`/`uRoughness` by hand,
with the documented unseeded-`uColor`-renders-black trap waiting.

Done looks like:

- `PhongOptions` and `StandardOptions` take `prelude?: string` and
  `surface?: string`, forwarded into the fragment (and the shadow twin,
  which already takes `surface`, so a discard casts its hole). The
  stock seeds stay: `color`, the maps, metalness and roughness seed
  exactly as without a surface.
- The class cache keys on the two strings as well (the key is a joined
  list of option values; a source string is a fine Map key). One
  material per distinct surface source, as with every other option.
- A uniform `prelude` declares is a per-entry param like any other:
  `params` on the mesh or `setMeshParams`, validated by name.
- AGENTS.md: tier 2 rewritten as `standard({ ..., prelude, surface })`,
  the class form kept for tier 3. `examples/` gets the ground of the
  third-dimension demo as `standard({ surface })`, replacing its class.

## 2. Mesh-local position and normal varyings

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
- The same option on phong and standard (with item 1 it is set for a
  `surface` that names them; without it, an explicit flag), part of the
  class key.
- Unlit gets the position half (`vLocalPos`) for the same reason.
- AGENTS.md "Custom looks": the varying list, and the rule "key
  procedural detail on vLocalPos for anything that moves".

## 3. A procedural falloff on sprites

Symptom: `sprite()` without a `map` draws a hard square, so every
glow, flare or puff needs a generated texture or a forty-line billboard
class.

Done looks like:

- `SpriteOptions.shape?: "radial"` with `falloff?: number` (exponent,
  default 1): alpha multiplied by `pow(1 - 2 * length(vUv - 0.5), falloff)`
  clamped, over the quad's inscribed disc; composes with a `map` (the
  map's alpha times the disc). A fixed-y billboard takes it the same.
- Part of the sprite class key; the fragment is the unlit one with the
  term spliced before the alpha test and fog, so `alphaTest` cuts the
  soft edge where asked and `blend: "add"` glows work unchanged.
- `examples/sprites.tsx` gains one radial glow; AGENTS.md's sprite
  paragraph names the option.

Not this: soft particles against the depth buffer under MSAA (a
resolved depth texture beside multisampling), which is its own item.
