# @solidrt/3d - background and environment

Background, cube maps, environment lighting, reflection probes, baked sky,
panoramas and fog. Read ../AGENTS.md first: it carries the model this file
builds on.

## Background

Background: `scene.setBackground(source | null)`, the `background` option
on createScene, and the reactive `Scene` prop. Drawn as the FIRST entry
of the scene's own pass (attributeless fullscreen triangle, depth off) -
one target instead of a backdrop texture stacked under the scene, with
no separate resize plumbing. Two forms:

- Fragment GLSL. The source gets the shader-target fragment contract
  (vUV 0..1 top-left origin, iResolution, fragColor; no `#version` line
  = the standard preamble), so a `createShaderTexture` backdrop ports
  verbatim, PLUS `in vec3 vRay`: the world-space view ray through the
  pixel, unnormalized (the vertex stage carries its clip position back
  through the shared uInvViewProj at the near and far planes). A
  directional sky - horizon gradient, sun disc, stars - is a few lines
  on `normalize(vRay)`. The background is an ordinary scene entry, so it
  may declare `uniform vec3 uCamPos` (the ray's origin) and any name
  written through `scene.setParams` (an app clock for an animated sky).
  Godot's sky shader and Unity's skybox material are the same idea; the
  radiance bake for environment lighting will consume this same source
  later, so a procedural sky written here lights the scene then. A sky
  writes LINEAR light to fragColor (the skybox form does) and the
  scene's resolve exposes, tone maps and encodes it with everything
  else. Derivatives in uniform control flow only: `fwidth()`/`dFdx()`
  after an early `return` or inside a data-dependent branch are
  undefined in the 2x2 quads that straddle it, and on Mesa Intel a
  hash-grid sky with an "empty cell" early out drew stray 1-px lines
  and L-shapes along every cell edge. Take the derivatives at the top
  of main, before any branch.
- A skybox `{ cube, intensity?, rotation? }` (SkyboxOptions): a cube
  map from createCubeTexture sampled along the same ray - Three's
  `scene.background = cubeTexture` with `backgroundIntensity` and
  `backgroundRotation`. `rotation` is a turn about world y in radians
  (the sky turns as a node with that rotation would); `intensity` a
  multiplier. Replacing a skybox with a skybox rewrites the entry's
  params and cube in place (no recompile), so the reactive prop can
  animate the rotation. Under an orthographic camera every pixel looks
  the same way, so a skybox is one flat color there. A 2D texture id
  throws at the samplerCube binding. `examples/skybox.tsx`.

## Cube-map convention

The cube-map convention: a cube map holds what a GL lookup returns -
each face as seen from OUTSIDE the cube, GL's own (RenderMan) frame -
and every library lookup is a plain `texture(cube, dir)` in world space.
No shader flip, as in Godot and Unity, which convert images at import;
Three instead flips x in the shader for image cubes (`flipEnvMap`) and
not for rendered ones, so a ported Three shader drops its flip, and a
Three-style six-face image set (px, nx, py, ny, pz, nz as seen from
inside) is mirrored per image at load. Every bake here (the tool,
`equirectToCube`, the examples' JS skies) writes GL's table directly,
and a cube the scene renders itself needs nothing. Three's `scene.background = color` is `clearColor` here; a 2D
image form can widen the signature later (a branded TextureId is a
number, so the object form keeps it unambiguous). Translucent grounds
over a background still need blend factors (a separate shader texture
underneath until then).

## Environment

Environment: `scene.setEnvironment({ cube, intensity?, rotation? } |
null)`, the `environment` option on createScene and the reactive `Scene`
prop - the cube map every `standard` material is lit by (always: the
split sum `envRadiance` at its roughness times PBR's `envBrdf` for the
specular, `envIrradiance` - the fully rough sample along the normal, as
Three's getIBLIrradiance and Godot read it - added to the hemisphere
for the diffuse) and every `phong({ reflectivity })` material mirrors,
typically the skybox's own cube turned with it. The cube to use is a
BAKED one: `bun run sol tool 3d/environment sky.hdr -o assets/sky.sol3e`
turns an equirectangular Radiance .hdr (Poly Haven's are CC0) into the
six faces plus the GGX-prefiltered mip chain in linear float, and
`await loadEnvironment("assets/sky.sol3e")` uploads it as an explicit
"rgba16f" chain (createCubeTexture's array-of-levels form: no generated
mipmaps, so no half-float render support needed - it works on every
device; created after an await, so not auto-freed). Unity convolves at
import the same way; Three's PMREMGenerator and Godot's radiance map do
it at runtime. The roughness-to-level rule is `ENVIRONMENT`'s:
roughness r samples level `r * (log2(size) - 2)`, so a roughness of 1
lands on the 4x4 level (ENV_ROUGH_FACE in environment-bake.ts), the
last one the bake convolves; a `mipmap: true` cube from six faces
(equirectToCube, a JS-baked sky) merely box-filters that chain, so its
rough reflections are sharper than they should be and its diffuse term
is a coarse average - fine for a sky gradient, wrong for a photograph.
A 128 environment (2 MiB, the medium row's default) lights any surface; for a
mirror-finish showpiece bake at 256, and for a crisp backdrop pair it
with a separate hi-res LDR skybox (a 2k panorama through equirectToCube)
as `background` while the .sol3e stays the `environment`. Scene-level like Three's
`scene.environment`, Unity's environment reflections and Godot's
sky-lit reflections: ONE `uEnv` samplerCube bound on every target the
scene draws into (a 1x1 black placeholder while unset) and one
shared-params write (`uEnvIntensity`, `uEnvRotation`, `uEnvOn`), however
many meshes reflect; no per-material envMap (Three's Basic/Phong
`envMap` sits on the MATERIAL, but materials are shared here) - a
custom material composes ENVIRONMENT from `@solidrt/3d/glsl`, and a
single object's exception is PER MESH: `Mesh.environment`
(`setEnvironment(mesh, env)`, the `environment` prop on every mesh
component). `null` removes the environment term from that mesh - the
sealed-interior pair to masked lights, since light `layers` cannot mask
image lighting; Unity's reflectionProbeUsage Off, Godot's
disable_ambient_light in spirit - an `{ cube, intensity?, rotation? }`
samples that cube instead of the scene's (an interior probe's cube, a
baked room - Unity assigns probes by volume with an anchor override,
Godot by probe box and cull_mask; here the app assigns explicitly),
and absent follows the scene. The same uEnv* names written per ENTRY,
which beat the target's shared values, so it costs nothing per frame;
the change rebuilds the mesh's entries, a state switch (a doorway
crossing), not a per-frame dial. Applied where the material reflects at
all (`standard`, `phong({ reflectivity })`, a custom ENVIRONMENT
composer - `Material.env`); on any other material it is a no-op, so an
interior can be masked wholesale. A reflection probe drawing such a
mesh never samples its own cube through it (the black placeholder
stands in, the same rule the scene's own environment follows on probe
faces). `reflectivity` 0..1 is the
face-on weight, rising to 1 at grazing angles (Schlick), mixed in as
`rgb = mix(rgb, reflection, weight)`: 1 is chrome, ~0.05 a glossy
dielectric with rim reflections; Three's Phong `reflectivity` under its
MixOperation with a fresnel weight (Three's default MultiplyOperation
tints instead; not offered). The reflection blurs with `shininess`:
roughness `sqrt(2 / (shininess + 2))` picks a mip level of the cube
(`textureLod`), so the environment cube wants `mipmap: true`; a cube
without mipmaps stays sharp. `specularMap`'s red scales it like
`specular`. For `phong` it is not an ambient light source: the hemisphere
light stays its only ambient term (`standard` adds envIrradiance; SH9
is a later, additive mode). A declared `reflectivity` with no
environment set contributes nothing (uEnvOn 0), not a black reflection.
`examples/skybox.tsx` (a JS-baked sky), `examples/environment.tsx` (a
baked HDRI lighting the scene alone).

## Reflection probes

Reflection probes: `scene.createReflectionProbe({ position, size?,
near?, far?, layers?, clearColor?, label? })` renders the scene into a
cube map from a point - Three's CubeCamera, Unity's and Godot's
realtime ReflectionProbe - and returns `{ cube, setPosition, update(),
dispose() }`; `cube` is what `environment={{ cube }}` (a chrome ball
mirroring its surroundings) or `background` takes. `dispose()` destroys
the cube, and a scene whose environment or background names it drops
that first (setEnvironment(null) / setBackground(null)), so a subtree
that owned the environment leaves the scene unlit by it rather than
sampling a destroyed texture; set another when it goes. A view under the
hood: one entry list mirrored from the scene, the light set and scene
params fanned out, its own layer mask (keep the mirroring object out of
its own probe with `layers`), the scene's background drawn first on
every face (the GLSL sky or skybox behind the meshes, through the face
camera, in linear light - what Three, Unity and Godot probes see), a
cube draw target (`createCubeDrawTarget` in core, rendered face by face
with `renderTarget(cube, face)`).
Nothing renders it but `probe.update()`: six scene passes, from the
meshes as the last frame's flush placed them, so call it when the
surroundings moved (every frame for a moving scene, once for a still
one), then the PREFILTER: the faces convolved on the GPU into a second,
`mipmap: true` cube target level by level (`renderTarget(chain, face,
level)`, one small pass each - 48 at 128 - the bake tool's GGX
importance sampling as a fragment, `createPrefilter` in environment.ts,
with the same roughness-to-level rule as a .sol3e chain), so `standard`
blurs a probe by roughness exactly like a baked environment; `prefilter:
false` skips it and hands out the sharp faces (Three's CubeCamera: a
mirror at every roughness). COST: the chain's passes are tiny, but a
pass that samples mip levels of a cube map carries a fixed GPU cost
(about 0.3 ms each on an Intel/Mesa laptop, the same at 8 samples or at
256 - measured 2026-09-05), so a prefiltered probe updated every frame
costs ~14 ms of GPU there against ~1 ms for the six face passes: fine at
60 fps on its own, the largest single item in a frame budget. Realtime
probes are the expensive option in every engine (Unity time-slices
them): update a prefiltered probe when the surroundings changed, or
every few frames, and keep `prefilter: false` for a probe that must
refresh every frame on a tight budget. The faces hold LINEAR light like every buffer (a
probe is a buffer without a resolve), HALF FLOAT where the device
renders it (`limits.halfFloatRenderable`, every GLES 3 device here: a
sun's or an emissive's range survives into the reflection, as in every
engine's HDR probe) and 8-bit clamped elsewhere - the renderer decides
for every target (Godot), not a per-probe knob (`bufferFormat()` in
environment.ts) - and the probe never samples its own cube while
rendering (a black environment stands in: one bounce). The face cameras are plain world-up cameras through an
x-mirrored projection (`Camera.mirror`), because a GL cube face is seen
from outside; the engine inverts the front-face rule on cube target
passes so cull modes keep their meaning. `examples/probe.tsx`.

## Baked sky

Baked sky: `scene.bakeBackground(size?)` is a reflection probe at the
origin that sees no mesh (layer mask 0): the scene's background - the
GLSL sky or the skybox - alone on its six faces, LINEAR (a sky fragment writes light, so the bake stores exactly
what the backdrop draws), prefilters it like a probe and returns the chain's
TextureId for `environment={{ cube }}`: Godot's sky-to-radiance bake, so
a procedural sky lights the scene with no light nodes, and the runtime
way to turn a hi-res LDR skybox into a properly convolved environment
(the `mipmap: true` box chain above is the sharper, cheaper
alternative). A snapshot at the probe format (half float where
renderable; default size the quality row's probeSize, 64 / 128 / 256):
bake again when the sky changes, and
destroy the old cube - it is not auto-freed (an environment normally
lives as long as the app). A buffer is UNCLAMPED (the clamp sits in the
resolve's encode), so a sky's sun disc of 40.0
bakes as 40.0 and shows as the broad bright highlight on rough metal
that HDR is for. A sky that reads
`uCamPos` bakes from the origin; scene params (an app clock) are seen as
of the call. `examples/sky-lit.tsx`.

## Panoramas

Panoramas: `equirectToCube(map, size, opts?)` converts an uploaded
equirectangular 2D texture (createImage, createTexture) into a cube
TextureId on the GPU, synchronously (six face passes straight into a
cube draw target of the panorama's format - rgba8, rgba8-srgb or
rgba16f, no readback; `opts` are createCubeTexture's - `mipmap: true`
for an environment). The center column faces -Z and the top row is +Y,
as in Godot's PanoramaSkyMaterial and Unity's Skybox/Panoramic; Three
centers +X, a quarter turn away. Leave the source texture's wrap at
clamp (`repeat` also wraps vertically and bleeds the poles). An HDR
panorama uploaded as rgba16f converts into a half-float cube (sharp:
a skybox, or `mipmap: true` for the box chain); its PREFILTERED form is
the bake tool above, whose CPU pipeline (`src/environment-bake.ts`:
decodeHdr, panoramaToCube, prefilterCube, the .sol3e encode/decode) is
pure TypeScript and bun-tested - there is no runtime .hdr decoder.

## Fog

Fog: `scene.setFog(fog | null)`, the `fog` option on createScene and
the reactive `Scene` prop, in Three's two shapes: linear `{ color, near,
far }` (`Fog`; fades from near to far, fully fogged past far) or exp2
`{ color, density }` (`FogExp2`, Unity's default; `1 - exp(-(d *
density)^2)`, no start band, never quite opaque - 0.01 is ~63% at 100
units). Either form takes `height` + `heightFalloff` (Godot's fog
height, Unreal's height falloff): full fog at and below `height` (world
y, default 0 - an ABSOLUTE height, which the shader reaches by adding
the target's `uOrigin.y` to its origin-relative position), thinning by
`exp(-(y - height) * heightFalloff)` above -
a valley fills, the hilltops and the sky stay clear; per fragment
height, not integrated along the ray, the cheap tier every engine ships
first. A fragment fades toward `color` by its RADIAL distance from
`uCamPos` (not view depth). It is ONE shared-params write (`uFogColor`,
`uFogNear`, `uFogInv` = 1/(far-near), `uFogDensity`, `uFogHeight`,
`uFogHeightFalloff`; the form not in use is 0, "no fog" is every rate
0, which the scene seeds at creation so there is no enable flag and no
branch - the shader takes the larger of the two distance factors times
the height term), fanned out to every view, so fogging costs nothing
per frame however many meshes. Every standard material (unlit,
lit, sprite) composes it after its alphaTest discard, mixed at the alpha
it writes (premultiplied stays premultiplied); `fog: false` on the
material drops the code from the program (Three's `material.fog`) - a
sky sphere, a far backdrop. A shaderMaterial opts in by composing `FOG`
from `/glsl` (declares the set; `fog(rgb, alpha, worldPos, camPos)`, or
`fogAdditive(rgb, worldPos, camPos)` for a `blend: "add"` look, which
fades toward black instead of the fog color; a stock material with
`blend: "add"` composes that form by itself).
The BACKGROUND is not fogged: it is entry zero with no depth or
distance, so match the fog color to `clearColor` or the background's
horizon, and put `far` at or inside the camera's far plane to hide the
clip. `examples/fog.tsx` cycles the forms over a valley;
`examples/cascades.tsx` fogs its field to the sky.

## Traps

- A reflection probe is a mirrored render (x-flipped projection, winding
  inverted by the engine): anything built from screen-space derivatives
  flips with it, so a normal-mapped surface shows its bumps INVERTED in
  a probe's reflection. Known and shared with every engine's mirrored
  views; a `uMirror` sign on the derivative frame is the fix when it
  matters.
- A generated cube chain (`mipmap: true` from six faces) is a box
  filter, not the GGX convolution the roughness-to-level rule assumes:
  rough reflections read too sharp and the diffuse `envIrradiance` is a
  4x4 average. Bake with `sol tool 3d/environment` for anything
  photographed; the JS sky gradients in the examples get away with it.
- The background covers the whole target with depth off, drawn first: it
  REPLACES the clearColor visually (the clear still runs; you just never
  see it), and a `transparent: true` mesh blends over it in-pass since the
  background is always entry zero.
- The background pipeline/program are SCENE-OWNED (unlike shared
  material pipelines): setBackground(null), replacement, and dispose()
  destroy them. Do not hand the background's pipeline to anything else.
  A skybox is the same slot with the library's fragment; only a
  skybox-to-skybox replace keeps the entry (params and cube rewritten).
- The environment binds through the light rewrite's map set (uEnv
  beside uShadowAtlas on every receiving target, new views included) and
  directly on setEnvironment; the placeholder cube is app-lifetime like
  the shadow placeholder. A `phong` without `reflectivity` declares no
  environment sampler - the flag is part of the class key.
