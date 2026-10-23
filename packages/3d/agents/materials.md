# @solidrt/3d - materials and color

The built-in materials (unlit, sprite, phong, standard, shaderMaterial),
instanced materials and color handling. Read ../AGENTS.md first: it
carries the model this file builds on.

## Materials

### unlit

`unlit({ color?, map?, vertexColors?, transparent?, blend?, cull?, alphaTest?, fog?, prelude?, surface? })` -
straight `[r, g, b, a?]` 0..1 sRGB (decoded to linear light, see Color
below), premultiplied internally; `vertexColors: true` multiplies by the
"colored" layout's aColor (the geometry must carry it: withColors, a
gridHelper or axesHelper); `blend` the factors of a transparent draw,
"alpha" when absent, `"add"` for a glow (any mode but "none" implies
`transparent` unless told `transparent: false`, the shaderMaterial
rule); `cull` and
`alphaTest` as on phong (a mapped cutout casts its cutout); `fog: false`
opts out of the scene's fog (all four library materials take it);
`prelude`/`surface` the tier-2 slot every stock material takes (Custom
looks in shaders.md): `void surface(inout Surface s)` called with the base
resolved, `s.base` the only field the unlit program reads back, a
discard casting through the shadow twin like phong's.

### sprite

`sprite({ color?, map?, transparent?, blend?, billboard?, shape?,
falloff?, minScreenPx?, maxScreenPx?, prelude?, surface? })` - unlit on a quad
that turns to face the camera IN THE VERTEX STAGE (off the shared
uCamRight/uCamUp, or uCamPos for `billboard: "fixed-y"`, which yaws
only and stays upright on world y - Godot's BILLBOARD_FIXED_Y, the
tree/character sprite; the default `"full"` is Three's Sprite, flat to
the screen). No per-frame JS however many sprites. `transparent`
defaults to TRUE here (cutouts; Three's SpriteMaterial default), cull is
off. Draw with `createSprite(material)` / `<Sprite>`: a Mesh over a
shared unit plane, no geometry argument, `scale` = world size, rotation
ignored. Picks by a unit box around its center (its reach at any
facing), so hits carry no normal/face/uv. `shape: "radial"` is a
procedural falloff over the quad's inscribed disc, `(1 - 2|uv -
0.5|)^falloff` (default 1; 2 a soft puff) multiplying color and alpha,
so a glow, flare or puff needs no texture; it composes with a `map`
and with `blend: "add"`. `minScreenPx`/`maxScreenPx` clamp the quad's
smaller axis on SCREEN (pixels; 0 = off), scaled uniformly about its
center in the vertex stage by the pixels one world unit covers at the
quad's depth (depth alone, from the shared uniforms, so perspective
and orthographic alike): a floor is the marker that stays findable at
distance, a ceiling the label that must not balloon, equal bounds a
constant screen size (Three's `sizeAttenuation: false`, `scale` then
the aspect alone). The clamp is core's `screenSizeScale`, shared with
@solidrt/2d's sprites; the per-entry `uScreenPx`, so a per-mesh
`params` can override it. `scene.pick`/`view.pick` test the drawn
quad (Picking in interaction.md), and a floored sprite's mesh turns its frustum culling
off (one way; `setCulling` turns it back on). `examples/sprites.tsx`.

### shaderMaterial

`shaderMaterial({ vertex, fragment, params?, textures?, depth?,
depthWrite?, blend?, cull?, label? })` - your own GLSL, the
custom-look escape hatch. The STANDARD UNIFORM SET: the vertex stage
MUST declare and use `uniform mat4 uModel` (the mesh's world matrix,
per entry) and `uniform mat4 uViewProj` (the camera, shared
target-level params) - transform with
`uViewProj * uModel * vec4(aPos, 1.0)`; a source missing either throws
at shaderMaterial() creation. Both are RELATIVE to the target's render
origin (`uniform vec3 uOrigin`, shared like uViewProj; Coordinates and
rotation under Traps): uModel's translation, uCamPos, uLightPos and so
every world position a program meets are small numbers around the
camera however far the scene lies from zero, consistent with each
other; `vWorldPos + uOrigin` is the absolute world for an effect that
keys on it (world-space noise, triplanar mapping - the stock triplanar
and the fog height add it themselves). A scene that stays within 64
near planes of zero never sees a non-zero origin. The rest is opt-in
by declare-and-use:
`uniform vec3 uCamPos` (the camera's world position, shared and written
with uViewProj - the specular/fresnel view vector is
`normalize(uCamPos - worldPos)`), `uniform vec3 uCamRight` / `uCamUp`
(the camera's world-space view axes, shared likewise - a billboard is
`center + uCamRight * x + uCamUp * y`; do NOT rebuild them from
uViewProj rows, that carries the clip flip), `uniform mat4
uInvViewProj` (the camera's inverse view-projection, shared likewise -
a clip position back to world, the world-space ray through a pixel
without knowing the projection), `uniform vec2 uViewport` (the target's
pixel size, shared likewise and per target - each view feeds its own -
so a pixel-space footprint like the splat material's needs no app
write) and `uniform mat4 uNormal` (the world
inverse-transpose, written beside uModel for this material's meshes;
take `mat3(uNormal)` - correct under non-uniform scale, where
mat3(uModel) bends normals off the surface). Attributes come from the
geometry's layout by name; the ones the linked program actually reads
(engine reflection, instance attributes excluded) must all be in the
mesh's geometry layout or add() throws - so a used `in vec4 aColor`
needs `withColors()` geometry and a custom channel needs
`withAttribute()`. One program per class, one pipeline per layout met. Sources without `#version` get the standard
pipeline preamble. App-driven uniforms beyond the standard set: seed
via `params`, then write per mesh with
`setMeshParams(mesh, { name: value })` (validated names; values persist
across entry rebuilds; frame-rate-safe like setTransform) or declaratively
with the `Mesh` `params` prop (same merge semantics - a key that
disappears from the object keeps its old value; for per-frame values
prefer `ref` + setMeshParams from onFrame, the setTransform split).
Scene-wide values (a clock, a sun direction, fog) go through
`scene.setParams({ uTime })` instead - one write for every mesh.
`textures` binds by the declared sampler type, every shape core's GPU
layer has: a `sampler2D` takes a 2D texture, a `samplerCube` a cube map,
a `sampler3D` a 3D texture (`createTexture3D` from `@solidrt/core/gpu`:
sampled by position with hardware trilinear filtering - a brick pool of
paint, a color LUT, a density field) and a `sampler2DArray` an array
texture (`createArrayTexture`: many equal-size maps behind one sampler,
picked by an unnormalized layer index); a shape mismatch throws at the
bind. Volumes re-upload in place with `uploadTexture` and a box. Do not
name an array sampler `uLayers`: that is the scene's layer mask (see
Lights in lighting.md), seeded as an int param on every material that mentions it.

### shaderMaterialClass

`shaderMaterialClass({ vertex, fragment, ...pipeline state })` - the
class/instance split for your own GLSL: compiles once, and
`cls.instance({ params?, textures? })` returns a Material sharing that
pipeline with its own values. `dispose()` lives on the class alone.
Seed only the names the material owns: a per-entry param is applied
AFTER the target's shared params, so a scene-wide name seeded in
`instance({ params })` (uTime, a sun direction) pins that mesh to the
seeded value and `scene.setParams` never reaches it again. Declare the
uniform in the source, leave it out of the instance, and the scene's
write lands.
`shaderMaterial(opts)` is exactly a class with one instance (its
`dispose` forwards to the class).

### Instanced materials

`instanceBuffers: [{ attributes: [{ name, format }] }]` (one layout per
per-instance buffer, instance-step by definition and tightly packed in
list order like a geometry layout; `format` is any of the vertex
vocabulary, so a per-instance tint is a `unorm8x4` at four bytes and a
frame index a `uint16x2`; explicit strides and offsets stay the
engine's, through core's pipeline API) on either shader-material form
makes an INSTANCED material: the vertex stage reads the attributes as
`in` variables beside the layout's own, and each drawn instance gets one
record from each of the mesh's instance buffers. An instance buffer IS a
vertex stream stepped per instance: on the mesh it is an `InstanceStream`
(the layout over the core's record stream - `createRecordStream` in
`@solidrt/core/gpu`: the GPU buffer, the byte mirror, the dirty range
and the range-or-whole publish, the one model @solidrt/2d's layers run
on too), written through
`instanceAttribute(mesh, name)` - the accessor `geometryAttribute`
returns, record index in, values as the shader sees them, the codec
packing the bytes - or in bulk through the mirror itself, `records(mesh,
stream?)` (a Float32Array over an all-float layout, bytes otherwise;
read it at use time, growth replaces it), and published with
`updateRecords(mesh, { stream?, first?, count? })`, which is
`updateVertices` for records: one coalesced buffer write per dirty
stream at the scene's sync, so ten moved records of ten thousand cost
ten - the same `records`/`updateRecords` pair as @solidrt/2d's records
layer. Its meshes come from `createInstancedMesh`
(the first buffer is the core-written matrix, the rest are streams; the
second is the STYLE record, below) or `createRecordMesh` (every buffer a
stream); a `createMesh` mesh is rejected at add(). `instanceStyle: [..]`
is the style record a fresh instance starts with, one value per
component. The stock materials do the same with one flag:
`lit/standard/unlit({ instanced: true })` places by the instance matrix
(the shadow pass too, so the fleet casts), and `{ instanceColors: true
}` adds a per-instance premultiplied `[r, g, b, a]` style record
starting white, stored as `float16x4` (eight bytes, no banding in the
darks) - Unity's per-material instancing switch with Three's setColorAt.

## Color

Color: the scene shades in LINEAR light and outputs sRGB, like Three
(ColorManagement), Godot and Unity's linear space - no gamma mode. Every
`[r, g, b]` color option is sRGB, what a color picker shows: material
`color` and `emissive`, light `color`, the hemisphere's `sky`/`ground`,
fog `color`, the scene's and a view's `clearColor`; the library decodes
it when it writes the uniform or the clear (`srgbToLinear`/`linearColor`
are exported for values you write straight to a uniform yourself). Color
MAPS decode through their format: create a base color, emissive or sky
image with `format: "rgba8-srgb"` (createTexture, createCubeTexture;
createModel does it for glTF's base color and emissive images) - a plain
rgba8 map reads as linear data and renders washed out; data maps
(normal, specular, roughness, light maps) stay rgba8, and an HDR image
is "rgba16f". Vertex colors are linear, as glTF stores them.

Every target is a BUFFER plus a RESOLVE, Godot's and Unity's pipeline:
the meshes draw into a linear buffer (`scene.hdrTexture` /
`view.hdrTexture`: premultiplied linear light, half float where the
device renders it - `limits.halfFloatRenderable`, every GLES 3 device
here - else rgba8 linear and clamped; the renderer decides, no knob),
and one full-screen pass, the resolve, writes the displayed rgba8
`scene.texture`: exposure (`scene.setExposure`, default 1), tone mapping
(`scene.setToneMapping("none" | "aces" | "agx" | "neutral")` - ACES the
filmic curve every engine ships, AgX Blender's (Three, Godot 4.3+),
Neutral the Khronos PBR curve (Three, Unity) that keeps product colors
where a filmic curve shifts them; the reactive `toneMapping`/`exposure`
props), then the sRGB encode with an ordered dither against banding,
premultiplied. So a fragment - stock or custom -
writes LINEAR light and never encodes (`sceneOutput` is fog only; a
material has no output stage), a `transparent` mesh blends in linear
space, the clearColor is tone mapped like a background is, and the
resolve is an auto target, so a static scene still costs zero passes.
The resolve is the post-effect slot: `resolve` on createScene/createView
and the `<Scene>`/`<View3d>` prop - a source, `{ source, textures }`,
or a function of the buffer id returning either, so a chain over
`hdrTexture` is built before the resolve compiles - and
`scene.setResolve` live with the same input. The
source gets the shader-target contract (vUV, iResolution, fragColor),
`uniform sampler2D uScene` (the buffer) and the RESOLVE set from `/glsl`
declared: end with `fragColor = resolveColor(rgb, alpha)`; any
`scene.setParams` name is readable; `DEFAULT_RESOLVE` is the stock one
sample. BLOOM is stock: `bloom: { threshold?, intensity?, radius? }` on
createScene, `scene.setBloom(opts | null)`, the reactive `bloom` prop -
radiance above `threshold` (default 1, what a fully lit white surface
reaches) blurred over `radius` rounds (default 2) of a separable blur
at a quarter of the target's size and added back at `intensity`
(default 0.5): Godot's glow, Unity's Bloom, Three's UnrealBloomPass. A
chain of small auto passes per resolving target, re-rendered when the
buffer is; views follow the scene's bloom unless they carry a `bloom`
of their own (null = off, a clean minimap), the fog model. The glow is
light: over a transparent backdrop (a view with no clearColor over UI)
it still shows, its alpha its own brightness, and an opaque pixel is the
plain sum. The default resolve composes it; a custom resolve does by declaring `uniform
sampler2D uBloom; uniform float uBloomIntensity;` and adding the term
(start from `BLOOM_RESOLVE` in `/glsl`; `examples/bloom.tsx` adds a
vignette that way). `output` composes the DISPLAYED texture after the
resolve, the place for an LDR effect (`examples/scene-post-effect.tsx`). Cost: one full-screen pass and
double the color bandwidth per target, about 0.2 ms per 1080p target on
an Intel iGPU (21 full-HD targets at once still ran at 36 fps); a
device that cannot afford it renders a smaller buffer and the leaf
scales it up. What changed for a scene tuned before linear light:
terminators soften, mid-tones brighten, highlights widen - drop ambient
rather than lights. `emissiveIntensity` scales the emissive in linear
light.

## phong

`phong(opts)` is the Blinn-Phong material (Three's MeshPhongMaterial, Unity
URP's Simple Lit) beside `unlit`: hemisphere ambient plus the light list, Lambert diffuse, Blinn-Phong highlight when
`specular` (0..1 strength) is set with `shininess` (default 30), a
mirror of the scene's environment when `reflectivity` (0..1, the
face-on weight; see Environment in environment.md) is set, blurred by the same
`shininess`, the same `color`/`map`/`transparent`/`blend`/`vertexColors` as unlit,
`triplanar: n` to sample `map` by world position at `n` repeats per
world unit, blended across the three axis planes by the normal, and
`alphaTest: t` for a cutout (a fragment whose final alpha is below `t`
is discarded; Three's alphaTest, glTF MASK): opaque, depth-written, no
sorting, usually with `cull: "none"` for cards, and `instanced: true` /
`instanceColors: true` for an instanced mesh's population (see
Instancing in instancing.md: the placement, the depth pass and a per-instance tint with
no GLSL; the material then draws populated meshes only), and `prelude`
/ `surface` for one procedural term inside the stock shading (tier 2
of Custom looks in shaders.md; also on `standard`). Triplanar
is an OPTION, not the default: generators emit 0..1 UVs per face, so a
map on a plane is a decal (UV) while a map on generated scenery wants one
density across parts of any size (triplanar); the map must be created
with `wrap: "repeat"`. Any `map` on a surface seen at distance also wants
`mipmap: true` at creation, or it aliases as it recedes, and a tiled
surface seen at a grazing angle (a floor, a road) wants `anisotropy: 4`
or more beside it, or trilinear smears the far half into the mip its long
axis picked (`createModel` uploads its images with both; the device clamps
the level, `limits.maxAnisotropy` reports it).

### Surface maps

The surface maps, each an option beside `map` and sampled at its uv:

- `normalMap` (+ `normalScale`, ONE float as in Unity/Godot - Three's
  Vector2 exists to flip DirectX-style green channels, and glTF mandates
  OpenGL-style +Y) bends the lit normal per texel. The tangent frame is
  built per fragment from screen-space derivatives (`NORMAL_MAP` in
  `/glsl`, Three's untangented path), so ANY UV-mapped geometry works
  with no tangent channel; the trade is mild seams on mirrored UVs. Not
  with `triplanar` (throws - triplanar samples by world position).
- `emissive: [r, g, b]` (sRGB like `color`) times `emissiveIntensity`
  (linear, default 1; glTF's emissive strength) and `emissiveMap` add
  light the lights do not provide, after the lighting terms,
  shadow-proof, fogged. `emissive` defaults to WHITE when `emissiveMap`
  is given - the map is the emission - fixing Three's gotcha where an
  emissiveMap alone shows nothing against the black default.
- `specularMap`: its RED channel scales `specular` per fragment (chrome
  and rubber on one mesh); with it `specular` defaults to 1.
- `lightMap` (+ `lightMapIntensity`) adds a baked-light texture by the
  geometry's aUV2 channel (`withAttribute(g, { name: "aUV2", format:
  "vec2" }, fill)`) - ADDED to the light sum like the hemisphere term, so
  a fully baked scene runs with no lights at all. Three's material-slot
  form; Unity and Godot bake at scene level, but here the material picks
  the program.
- `mapTransform: { offset?, repeat? }` samples every uv map of the
  material at `uv * repeat + offset` - ONE transform per MATERIAL
  (Godot's uv1_offset/uv1_scale, Unity's Tiling/Offset; deliberately not
  Three's per-texture transform, since a TextureId is a shared value
  whose sampling is creation-time state). aUV2 is exempt. Scroll it per
  frame with `setMeshParams(mesh, { uMapTransform: [ru, rv, ou, ov] })`.
  Also on `unlit`. Not with `triplanar` (throws - its repeat is the
  triplanar value). A cutout's shadow transforms the same way.

`examples/materials.tsx` shows all five.

## standard

`standard(opts)` is the metalness/roughness material, the look authored
assets expect (Three's MeshStandardMaterial, Godot's StandardMaterial3D,
Unity's Standard): every lit option but the Blinn-Phong knobs
(`specular`, `shininess`, `specularMap`, `reflectivity`), plus
`metalness` (0..1, default 0: a metal has no diffuse and reflects tinted
by `color`, a dielectric reflects 4% face-on), `roughness` (0..1
perceptual, default 1: 0 a mirror, 1 matte; one value widens the
highlight and blurs the environment alike; Unity's smoothness is its
inverse) and the packed data maps `metalnessMap` (its BLUE channel) and
`roughnessMap` (GREEN) - Three's two channel-select options over glTF's
ONE metallicRoughnessTexture, so pass the same texture to both; with a
map the factor defaults to 1 (the map is the value). Shading is GGX
(`PBR` in `/glsl`: distribution, height-correlated Smith visibility,
Schlick fresnel) per light in the same light and shadow loop, the
hemisphere on the diffuse, and the scene's environment ALWAYS - the
split sum, `envRadiance` at the roughness over the cube's mip chain
times the analytic `envBrdf` for the specular, `envIrradiance` on the
diffuse beside the hemisphere - with no `reflectivity` switch: the
environment is intrinsic to the model, and a baked environment with no
lights at all lights a scene (`examples/environment.tsx`). Light intensities read as phong's
(1 lights a white matte surface to 1; Godot's and Unity's convention -
a Three scene's intensities divide by pi). Without an environment a
metal shows only its highlights: no diffuse, nothing to reflect (Three
and Godot do the same), so give the scene one. The same inside a closed
volume: a dark, mostly-metal surface there gets little diffuse light
and has little to reflect, so a hemisphere fill barely registers on it
(a 5x fill and an interior reflection probe both did nothing visible);
keep directional key and fill lights on inside. `examples/standard.tsx`
is the sphere grid. Internally one
`shaderMaterialClass` per option combination (map x vertexColors x
triplanar x transparent x cull x alphaTest x fog x the surface maps),
cached for the app's lifetime, one pipeline per vertex layout - a
thousand lit meshes share one program, and the key's width costs nothing
by itself: classes are created lazily per combination USED, so the
program count is the app's distinct material configurations. The view
vector comes from the shared uCamPos; `uTriplanar` and `uAlphaTest` are
declared only by the classes that use them (the cutoff is a per-entry
value, so every alphaTest material shares one class) so the other
classes do not warn about an inactive uniform.

## Traps

- A `standard` metal in a scene with no environment renders near black:
  its diffuse is zero and the black placeholder cube is all there is to
  reflect (Three and Godot render the same; Unity falls back to an
  ambient probe, this does not). glTF's default metallic factor is 1,
  so an untextured asset is all metal, and createModel's default is
  `standard`: give a model scene an `environment` (loadEnvironment's
  baked .sol3e, or the skybox's cube), or pass `phong` as the material.
- Light intensities are the same numbers for `phong` and `standard`: 1
  lights a white matte surface to 1 face-on. A Three scene's intensities
  are a factor pi larger for the same look; divide when porting.
- Transparency is an EXPLICIT material flag, Three's rule: `unlit({ color:
  [r, g, b, 0.5] })` still draws opaque, and opaque means it: the standard
  classes write alpha 1 when not `transparent` (the scene target is
  composited premultiplied, so a leaked texel or color alpha would punch
  a see-through hole in an opaque draw - the source of "white cutouts"
  on an alpha-mapped model drawn without alphaTest). A `shaderMaterial`
  writes its own fragColor: give an opaque look alpha 1 too.
  `unlit({ ..., transparent: true })`
  (or `shaderMaterial({ transparent: true })`) builds the pipeline with
  `blend: "alpha"` and `depthWrite: false` (depth test stays on, so it hides
  behind opaques without occluding other translucents). The one inference:
  a stock material or `shaderMaterial` with any `blend` but "none" is transparent
  unless told `transparent: false` - every blended draw belongs after the opaques, and
  back-to-front is harmless for add/multiply. The spatial core owns the
  order of every scene and view target (`setDrawSort`; each mesh's bind
  carries its queue and renderOrder): four queues - the background,
  bound to the scene root and pinned first whatever the camera does,
  opaque meshes front-to-back by their distance to the camera, cutout
  meshes (any `alphaTest`, a `shaderMaterialClass({ cutout: true })`)
  after them the same way because their discard defeats early-z, and
  transparent meshes back-to-front by their view-space depth -
  `renderOrder` above depth inside each queue, never across queues,
  all measured at the CENTER of the mesh's world bounds (not the origin:
  off-origin geometry sorts by where it is; not the nearest bounds point:
  a big translucent ground plane would cover the small translucents on
  it), add order breaking ties. The opaque and cutout key is coarse on
  purpose - a logarithmic distance bucket, four per doubling - because
  early-z only needs near layers before far ones, and a coarse key keeps
  the order (and the meshes' add-order material grouping within a bucket)
  stable while the camera moves: a look-around changes no distance, and a
  step shorter than 16% of the nearest bucketed center's distance can
  cross no bucket edge, so neither re-sorts. Dense geometry added
  base-first therefore no longer needs the app to sort it (Godot and
  Unity sort opaques the same way, Three exactly). The core issues one
  `setDrawOrder` per target from the flush whenever a mesh was added,
  removed or re-keyed, a node moved, or the camera moved (with a
  transparent bound, or past that bucketed step), and skips it when the
  resort lands on the permutation already issued. Per-mesh sort only:
  one non-convex translucent
  mesh still overlaps itself in vertex order, and two large interpenetrating
  translucents can sort wrong (center distance, not per-pixel) - that is the
  engine contract, no OIT. A `shaderMaterial({ transparent: true })`
  fragment must write PREMULTIPLIED output (`vec4(rgb * a, a)`).
- A color map created as plain rgba8 renders WASHED OUT: the fragment
  reads its encoded bytes as linear light and encodes them again. Create
  color images (base color, emissive, a sky's faces or panorama) with
  `format: "rgba8-srgb"`; keep data maps rgba8. A rendered rgba8 texture
  (a scene view, a shader target, a UI capture) holds encoded pixels and
  cannot be tagged, so as a `map` it needs `srgbToLinear` from SRGB in a
  custom fragment (no material option yet); a draw target you create
  yourself can be `format: "rgba8-srgb"` and then decodes on sample (it
  is sampler-only: no display, readback or copy).
- Reading `scene.texture` back (readTexture, a snapshot) gives ENCODED
  pixels: an expected linear value v shows as `linearToSrgb(v) * 255` -
  intensity 0.5 reads 188, not 128. The buffer, `scene.hdrTexture`, is
  sampler-only (no readback, copy or display): read radiance through a
  `createShaderTexture` pass over it, scaled into 0..1.
