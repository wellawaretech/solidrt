# @solidrt/3d - custom shaders

The lighting GLSL and the three tiers of custom looks. Read ../AGENTS.md
first: it carries the model this file builds on.

## Lighting GLSL

Lighting GLSL (`@solidrt/3d/glsl`): exported string constants composed
into shaderMaterial sources with plain template literals - `LIT_VERTEX`
(the standard vertex stage: clip position plus vWorldPos/vNormal/vUv
varyings, normals via mat3(uNormal)), `LIT_VERTEX_COLORED` (the same
plus the colored layout's aColor forwarded raw as vColor - using it makes
the material need that channel) and the pure functions `HEMISPHERE`
(`hemisphere(n, sky, ground)`), `LAMBERT` (`lambert(n, l)`),
`BLINN_SPECULAR` (`blinnSpecular(n, v, l, shininess)`), `FRESNEL`
(`fresnel(n, v, power)`), `PBR` (the GGX metalness/roughness model
`standard` shades with: `ggxSpecular(n, v, l, f0, roughness)` - one
light's lobe, to weight by `lambert(n, l)` and the light color like the
diffuse - `envBrdf(nv, roughness)` for the split sum's scale and bias on
f0, `DIELECTRIC_F0`, and the D/V/F pieces; it defines `PBR_PI`, not
`PI`), and the shadow trio composed IN ORDER:
`SHADOW_SLOTS` (the scene's shadow set: `uShadowAtlas`, per map slot
`uShadowRect[M]`/`uShadowMatrix[M]`, per light index
`uShadowFirst[N]`/`uShadowCount[N]` (its slots; a cascaded light has
several, tightest first), `uShadowBias[N]`, `uShadowNormalBias[N]`),
`SHADOW` (`shadowPoint(coord)` - clip to map point, `shadowInside(p)` -
does the map have it, `shadowSample(map, rect, p, bias)` - one tile's
factor as ONE hardware comparison tap (`uShadowAtlas` is a
sampler2DShadow; the engine binds the comparison sampler for that
declaration, so the 2x2 PCF weighting is the driver's), and
`shadow(map, rect, coord, bias)` composing the
three) and `SHADOW_LOOKUP`
(`lightShadow(i, worldPos, n)` - light i's factor, 1 when it does not
cast; it walks the light's slots and samples the first map that covers
the point, which is the cascade select, blended into the next map over
the outer `SHADOW_BLEND` of the map). A receiving fragment
multiplies light i's term by `lightShadow(i, ...)`, exactly what `phong`
composes; a non-receiving one composes none of the three and declares no
samplers. Lights, colors and exponents are arguments, so
nothing is pinned but the function names; `phong` is composed from these
same constants - customizing never means leaving the system.

## Custom looks: three tiers

A custom look is a citizen of the scene - lit by its lights, shadowed
and fogged like the stock materials, exposed and tone mapped by the same
resolve - at one of three tiers, top first. The same three tiers, the
same words (`unlit`, `prelude`/`surface`, `shaderMaterialClass`,
`instanceBuffers`/`instanceStyle`/`setInstanceStyle`) are
`@solidrt/2d`'s, where the material sits on the LAYER (the layer is the
batch, as the mesh is the draw entry here); the one deliberate
asymmetry is that a 3d `blend` implies `transparent` and the scene's
back-to-front sort, while a 2d layer has no depth and draws in record
order (okf/plans/materials-one-model.md).

1. STANDARD FRAGMENT, CUSTOM VERTEX. Any vertex stage that writes the lit
   varyings (vWorldPos, vNormal, vUv, plus vColor with `vertexColors`,
   vUv2 with `lightMap`) pairs with `phongFragment(options)` /
   `standardFragment(options)` (`/glsl`): the exact fragment `phong` /
   `standard` compile, the same option names and defaults as their
   options with the texture options boolean. An instanced or displaced
   mesh keeps the stock shading whole (`examples/instanced.tsx`: the
   per-instance tint rides vColor). Instance it with the per-entry
   uniforms the source declares (uColor, uSpecular/uShininess or
   uMetalness/uRoughness, the maps opted into) - SEED THEM: a uniform
   no one wrote is zero, and the program shades `base * uColor`, so an
   unseeded uColor renders pure black with no error. `instance({
   params: { uColor: [1, 1, 1, 1], uSpecular: 0.12, uShininess: 24 } })`
   is the white starting point (the stock materials seed exactly this
   from their `color` option).
2. A SURFACE FUNCTION inside the stock material. `phong({ ..., prelude,
   surface })` / `standard({ ..., prelude, surface })`, and the same two
   slots on `unlit` and `sprite` (the unlit program shades nothing, so
   `s.base` is the one field read back, after the cutout and the
   sprite's radial falloff): every other
   option seeds as usual (`color`, the maps, metalness and roughness),
   `prelude` is file scope (uniforms and helpers; a uniform it declares
   is an ordinary per-entry param, `params` on the mesh or
   `setMeshParams`), `surface` declares `void surface(inout Surface s)`,
   called once the program has filled the Surface struct from the
   options (base from the color, the map and the vertex color; the
   normal, bent by the normal map; emissive, ambient, the light model's
   fields) and before it shades. Rewrite any field or `discard`; it
   reads the varyings, the declared uniforms and prelude's names, and
   runs in the shadow twin too (attached whenever a surface is given),
   so what it discards casts no shadow - with the twin seeing a prelude
   uniform at its zero value, since mesh params reach the main entry
   only. One program per distinct source, keyed like every option. The
   struct is the contract, no local of the generated program is; colors
   are linear light, premultiplied throughout, `Surface.base` included.
   The material describes the surface, the package shades it (Godot's
   fragment(), Filament's material()). This is the tier for anything
   whose geometry is not a surface: a point splat discarding outside
   `gl_PointCoord`'s inscribed circle and flipping a fitted normal
   towards the viewer keeps the scene's whole light model in about
   fifteen lines, where hand-rolling the light loop (tier 3) would have
   to match every light's falloff, cone and shadow by hand. The same
   two slots on `phongFragment` / `standardFragment` are the class form
   for a custom vertex stage (tier 1 and 2 together).
3. A FRAGMENT OF YOUR OWN over the scene set. Compose `SCENE` (or
   `sceneSource({ lights, receiveShadow, env, fog })`, each flag leaving a
   declaration out): it declares uCamPos, uOrigin, uHemiSky/uHemiGround,
   the light list, the shadow set, the environment and fog - declare
   none of them yourself. Build a `Surface` with `surfaceOf(base,
   normal)`, set the fields you mean, call `shadeBlinn(s, position)` or
   `shadePbr(s, position)` (premultiplied rgb back: hemisphere, every
   light with its shadow, the environment term, the emissive), add your
   own terms times the alpha, and end with `sceneOutput(rgb, alpha,
   position)` (fog; the pixel is linear light, the resolve exposes, tone
   maps and encodes it). A custom LIGHT MODEL
   loops `sceneLight(i, position, normal)` to `uLightCount` instead of a
   shade function: light i's direction and its color already attenuated,
   cone-faded, layer-masked and shadowed (zero when it cannot reach). The stock
   materials are built from this same set, so the tiers cannot drift.
   The demo `the-third-dimension.tsx` has tier 2 (the ground) and tier 3
   (the knot's rim term).
`standardFragment(options)` is the same for `standard`: phong's options
minus `specularMap`/`env` (the environment is always composed) plus
`metalnessMap`/`roughnessMap`, on the same `litVertex(options)`, with
`uMetalness`/`uRoughness` in place of `uSpecular`/`uShininess`.
`litShadowFragment(options)` is the depth-pass twin (same base, cutout
and surface function, nothing after them), so a discarding material
casts what it draws: build it on `litVertex(options)` with the OPPOSITE cull, instance
it with only the uniform values its source declares (per-entry params
reject unknown names), and pass it as the main instance's `shadow`.
It returns undefined when the options cannot discard - the scene's
default depth override is then already right, carry no `shadow`.
`UNLIT_VERTEX` / `unlitFragment` / `unlitShadowFragment` are the unlit
twins (no lighting flags, no cull; varyings vUv/vWorldPos only). TRAP:
a shadow program that never reads `n` (no triplanar, no surface function
using it) reflects `uNormal` inactive - set `normalMatrix: false` on that
instance or every caster move warns about the skipped write.

## Traps

- A `shaderMaterial` INSTANCE is the pipeline handle: identical sources
  compile twice - no dedupe by source value (deliberate; hidden
  content-keyed caches are the anti-pattern the GPU layer avoids). Create
  one per look at app scope, share across meshes, `dispose()` when done
  for good. Looks that differ only in params/textures are ONE
  `shaderMaterialClass` and many `instance()`s - the app-owned split, not
  a cache. A class instance has no `dispose` of its own; disposing the
  class invalidates every instance.
- A parameterised class whose variants (mapped/unmapped, ...) are SEPARATE
  classes may seed every variant with one param/texture object: a uniform
  a variant declares but does not use compiles out, and the engine then
  accepts the write with a warning and skips it. A name no variant
  DECLARES still throws at add().
- The standard-set contract is checked TEXTUALLY at shaderMaterial()
  creation (uModel and uViewProj must appear in the vertex source) and
  at add() for the per-entry names: a uModel or uNormal that is declared
  but never USED compiles out, and the scene's entry seed is then skipped
  with an engine warning (the engine rejects only names the program never
  declared). The shared names have no such backstop - a declared-but-unused
  uViewProj or uCamPos is skipped silently (shared params tolerate zero
  coverage), so the symptom is an untransformed or unlit render, not an
  error. Use what you declare.
- The layout scan is textual the same way: any `aColor` token in the
  vertex source - a comment counts - selects the "colored" layout, and
  the material then rejects standard geometry at add(). Do not mention
  aColor you do not read.
- A custom fragment that ends in `fragColor = vec4(...)` bypasses the
  scene: no fog (exposure, tone mapping and the encode still happen, in
  the resolve, so it must write LINEAR premultiplied light) - and a
  hand-rolled loop over `uLightDir` renders a spot light as a
  directional one (a lit rectangle on the floor, no cone). What you
  declare is what runs, and the engine injects nothing, so pick a tier
  (the three after the GLSL exports above): a stock fragment
  on your vertex stage,
  a `surface` function, or `SCENE` with `shadeBlinn`/`shadePbr` or
  `sceneLight` and `sceneOutput` at the end. Composing `FOG` and the
  `SHADOW_*` trio by hand still works and is no longer the shape.
