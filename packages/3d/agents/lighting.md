# @solidrt/3d - lights and shadows

Lights and shadows. Read ../AGENTS.md first: it carries the model this
file builds on.

## Lights

Lights and `phong`: lights are graph NODES, like Three. `createDirectionalLight({
direction?, color?, intensity?, angularSize?, layers? })` / `<DirectionalLight>` is parallel light
travelling along `direction` in the node's LOCAL space (default `[0, -1,
0]`, a sun overhead; length ignored), so a parent Group's rotation turns it,
`lookAt(light, target)` points it at a world point, and position/scale do
not matter - deliberately a direction, not Three's
position-minus-target; `angularSize` (degrees, default 0) is its disc,
the soft shadows' source (see Shadows). `createSpotLight({ direction?, color?, intensity?,
distance?, angle?, penumbra?, decay?, sourceRadius? })` / `<SpotLight>` is a cone from
the node's WORLD position along that same LOCAL `direction` (aim by
`direction`, a parent's rotation or `lookAt`; place by setTransform): `angle` is
the cone half-angle in DEGREES ((0, 90], default 60 - degrees like
camera fov and like Unity/Godot; Three's radians convert as
`angle * 180 / PI`), `penumbra` the
0..1 fraction of it fading to the rim (default 0, a hard edge), and the
strength falls off as `1 / d^decay` (default 2) windowed to zero at
`distance` (0 = no cutoff) - Three's SpotLight semantics minus the
target object; `sourceRadius` (world units, default 0) is its disc.
`createPointLight({ color?, intensity?, distance?,
decay?, sourceRadius? })` / `<PointLight>` is the omnidirectional version: position
only, same falloff, no cone. `createHemisphereLight({ sky?, ground?, intensity?
})` / `<HemisphereLight>` is the ambient term, a gradient by the WORLD
normal's tilt (fixed to world up, the node's transform is ignored); one per
scene, the last attached wins. EVERY light (the hemisphere included)
takes `layers?: number`, a filter bitmask defaulting to all bits set:
the light lights, and a casting light shadows, only the meshes whose
`layers` intersect it (Unity's cullingMask, Godot's light_cull_mask;
Three has none) - the sealed-interiors pattern's light half (see Views
and layers), one integer AND per light per fragment on two uniforms,
so the branch is coherent per draw. A mesh a casting light's mask
excludes also draws into none of its shadow maps. The environment term
is not a light and not masked: its per-mesh knob is `Mesh.environment`
(see Environment in environment.md). Placement goes through setTransform, the
light's own fields through `setLight(light, { ... })` (frame-rate-safe,
like setMeshParams). At most `MAX_LIGHTS` (8, exported from the root and `/glsl`)
lights per scene, directional, spot and point together (the hemisphere
is not in the list) - a ninth is reported at the sync that settles the
set, not at add() (a declarative swap's transient overlap is fine),
the way a shadow-budget overflow is (see there), and the lights past
the cap are not lit; it is a shader-source constant, fixed per app. `uLightDir` and `uLightPos` are core-driven:
each light's slots are spatial-core shared-slot sinks following the
node's world rotation (direction, negated so the shader reads the
vector TOWARD the light) and world position, so a MOVING light costs no
JS. The sync rewrites the rest whenever a light attaches, detaches or
changes a field - `uHemiSky`/`uHemiGround` (vec3, intensity folded in),
`uHemiMask` (int, the hemisphere's layers), `uLightCount` (int),
`uLightType[N]` (LIGHT_DIRECTIONAL | LIGHT_SPOT |
LIGHT_POINT), `uLightDir[N]`/`uLightPos[N]`/`uLightColor[N]` (intensity
folded into the color), `uLightParams[N]` (cosInner, cosOuter,
distance, decay) and `uLightMask[N]` (each light's layers; masks cross
as SIGNED 32-bit ints, all bits = -1, the shader only ANDs the bits) -
so a custom fragment composing `LIGHT_SLOTS` +
`LIGHT_LOOKUP` from `/glsl` reads the same list through `lightVector(i,
worldPos, out l)` (returns the attenuation, 0 = skip the light; `phong`
is the shape), and a light change costs one write however many meshes.
The mesh side of the mask is `uniform int uLayers` (declared by
LIGHT_LOOKUP, tested first in lightVector): a per-entry param the scene
seeds from `mesh.layers` and follows through setLayers on every
material whose source mentions it - never per frame, and never
something a fragment declares itself.
A custom fragment that declares only the old directional subset
(`uLightCount`/`uLightDir`/`uLightColor`) still works - it just shades
every light as directional, so keep such materials to directional-only
scenes. Everything starts black: a lit scene with no light shows
nothing, on purpose, like Three. `examples/lamps.tsx` is the spot/point
shape (soft vs hard cone, casting spots, an orbiting bulb).

## Shadows

SHADOWS are a view: `<DirectionalLight castShadow shadow={{ mapSize?,
bias?, normalBias?, radius?, camera? }}>` (`createDirectionalLight({ castShadow,
shadow })`, `setLight`) makes the scene own an internal
`createView({ depth: "texture", overrideMaterial: depth pass })` drawing
the `castShadow` meshes (`<Mesh castShadow>`, `setCastShadow`) from an
orthographic camera at the light's WORLD position along its world
direction, `shadow.camera` (DEFAULT_SHADOW_CAMERA, +-5 and 0.5..500,
for a light that authors none - which the quality row then gives
cascades instead, below) as the frustum,
re-placed whenever the light's world matrix moves - by a JS write or
by native motion (a transition on the light or an ancestor, a clip
player), in the frame the lighting moves.
Any light may cast, bounded by the shadow-slot budget
(`MAX_SHADOW_MAPS` = 8, its own constant, exported from the root and
`/glsl` with `MAX_LIGHTS` and `MAX_CASCADES`: a directional light claims
`shadow.cascades` consecutive slots, a point light six, a spot one).
The budget is checked when the set settles (the scene's sync), never
at attach, so a `<Show>`/`<Switch>` swap whose branches each fit is
fine even though both are attached for an instant. A set over budget
once settled is reported once per change of the set: `<Scene>`
rethrows it inside the tree, so the app's error boundary (or an
`<Errored>` closer in) shows it, and Reset returns to the scene, which
kept rendering - the casters that fit in attach order keep their maps,
the rest light the scene unshadowed. A bare `createScene` throws it
from its sync unless given `onError`. A deliberate overlap must fit
alongside its successor: a casting light with its own exit transition
keeps casting until it frees. `<SpotLight castShadow
shadow={{ mapSize?, bias?, normalBias?, near? }}>` is the same
machinery with a PERSPECTIVE camera: at the light's world position
along its world direction, fov = its cone (2 * angle), near from
`shadow.near` (default 0.5), far from the light's `distance` (or the
directional default 500 when 0) - one map, one slot, the same atlas
and lookup. A perspective map's depth is nonlinear, so `normalBias`
(world units) is the acne knob to reach for; `bias` acts in that
nonlinear depth. `<PointLight castShadow>` casts in every direction
with the same option set: six 90-degree face maps (world-axis
aligned, slot order +X, -X, +Y, -Y, +Z, -Z) as six consecutive tiles
of the same atlas, far from `distance` like a spot - so give a
casting bulb a distance. No cube map: a receiver picks the face by
the dominant axis of the light-to-point vector (SHADOW_LOOKUP), one
projection, one hardware-compare tap - the Three/Godot/Unity-URP
atlas route. ONE kernel softens every edge. `shadow.radius` (every
casting light; default null = the scene tier's width, 1.5 / 2.5 / 3.5
texels for low / medium / high, the reach of Unity's 3x3 / 5x5 / 7x7
soft-shadow tents; 1 = that one tap, a 2x2 bilinear compare) widens it
uniformly: above 1 a disc of hardware taps spread over `radius`
texels, Three's `shadow.radius` / Godot's `shadow_blur` - a Vogel
spiral rotated per pixel by interleaved gradient noise, so it never
bands at any radius; the grain is the trade (Godot's too; there is no
TAA to hide it). A first-person eye over a floor shows the texel stairs
at any map size; the tier's width is what hides them, as in Unity, so
set a radius only to depart from it. A light's SIZE makes the
width follow the geometry instead - contact-hardening soft shadows
(PCSS: Unreal's source angle, Godot's `light_angular_distance` /
`light_size`, HDRP's angular diameter / shape radius): `angularSize` on
a `DirectionalLight` (degrees, the disc's diameter; the sun's is about
0.53) and `sourceRadius` on a `SpotLight`/`PointLight` (world units), on
the LIGHT (its shape, as in all three engines; default 0 = a point
source). With one set, each receiving fragment searches the raw depth
for the occluders in its cone toward the light's disc (none = lit, all
= umbra) and sizes the disc from the mean blocker distance times the
size: razor sharp where a caster meets its receiver, as wide as the
light allows under a far tip, `radius` the floor the penumbra never
narrows below (HDRP's min filter size) and `SHADOW_MAX_TEXELS` (8) the
cap - an edge wider than that stays hard, so a coarse map caps the
softness; the real sun's 0.53 only shows on a map whose texel is under
a centimetre per metre of caster height. The scene's `shadows: {
quality }` ("hard" | "low" | "medium" | "high"; default the quality
row's - hard at low, else the tier's name; `setShadowQuality`, live,
one light-set write) sets how many taps both
regimes spend (8/8, 16/16, 24/32 search/filter) and the width of every
light without a radius (`hard` is the one tap for every light whatever
its radius or size - the low-end profile's switch). Every tap wider than one compares against the RECEIVER PLANE
(the normal `lightShadow` takes, carried into the map), so a wide
kernel on a tilted floor does not shadow itself; `bias`/`normalBias`
apply underneath. Cost: the search and filter taps per receiving
fragment per casting light whose kernel is wider than one tap, twice
inside a cascade blend band - on the SM-T500 (Adreno 610) at its full
2000x1092 with two casters over a ground that fills the frame, a hard
frame is 78 ms of GPU, the low tier 117, medium 140, high 189, so the
tier is the low-end profile's knob and a default sun size stays 0;
`examples/soft-shadows.tsx` is the shape
and `tests/soft-shadows.test.tsx` measures the foot-to-tip widening. Each face map renders a few degrees wider than its face
(URP's fovBias) so a seam fragment's occluder is inside the map it
samples - without the guard band every seam shows a lit slit - and
PCF taps clamp at face-tile edges, so a face seam hardens slightly
instead of bleeding into the neighbour.
`shadow: { cascades: N }` (1..MAX_CASCADES = 4; default the quality
row's 1 / 2 / 2 for a light that authored no `camera` box, and 1 for
one that did - an authored box is the light's frustum, never silently
replaced) replaces the box with
N maps fitted to slices of the SCENE camera's frustum (near ..
`shadow.distance`, default the camera far; the practical split, or
`shadow.splits` - `N - 1` ascending fractions of that range, Godot's
shadow_split_1..3 - to push resolution outward or pull it in; each
slice's bounding sphere as an ortho box along the light, its centre
snapped to the map's texel grid so edges do not swim; re-fitted
whenever the scene camera or the light moves) - a receiver samples the
tightest map that covers the point, fading into the next over the
map's outer 10% (`SHADOW_BLEND`) so the hand-over is a band, not a
seam; contact shadows stay sharp near the camera while the horizon
still has coarse ones, and pulling `distance` in sharpens all of them.
A box is the honest frustum for a bounded scene; cascades are for a
scene that outgrows it, at N times the shadow fill. Every map is a
TILE of the scene's one shadow atlas (a `depth: "texture"` draw target,
a grid of cells the largest `mapSize` (default the quality row's 512 /
1024 / 2048) wide, scaled down uniformly
against `limits.maxTextureSize`), so N maps are ONE pass: the atlas
depth binds as the target-level `uShadowAtlas` of the scene and every
non-shadow view (a white texel when nothing casts), and a second time
as the plain `uShadowDepth` the blocker search reads raw depth through
(the engine picks the sampler by the declared type); maps are MAP slots
dealt in light order, a light's cascades consecutive and tightest
first - `uShadowRect[j]` slot j's tile in atlas UV, `uShadowMatrix[j]`
its view's own view-projection, `uShadowMetric[j]` its camera's near,
far, texel size and depth range (the arrays are one write per
shadow-camera move) - and per light i `uShadowFirst[i]`/`uShadowCount[i]`
name its slots (count 0 = it does not cast) with
`uShadowBias[i]`/`uShadowNormalBias[i]`/`uShadowRadius[i]`/`uShadowSize[i]`
its knobs and `uShadowSearchTaps`/`uShadowFilterTaps` the scene's tier;
`SHADOW_SLOTS` in glsl declares the set. Every `phong` material RECEIVES
by default
(Godot's and Three's default); `phong({ receiveShadow: false })` opts a
material out and drops the map from its program - a material option,
as with vertexColors/triplanar, because the material picks the program
(Godot's `disable_receive_shadows`). The factor is `SHADOW`'s
`shadowSample` on each casting light's own term: the one
hardware-compare tap (sampler2DShadow, LEQUAL, the driver's 2x2 PCF)
at radius 1 without a size, the disc otherwise. `examples/shadows.tsx`
(three casting lights) is the shape; `examples/cascades.tsx` the
cascaded sun; `examples/soft-shadows.tsx` the sized sun and spot.

## Traps

- A CASTING light's position matters (nothing else about a directional
  light's position does): the shadow camera is placed AT the light node's
  world position, Three's rule, so a `castShadow` sun at the origin
  pointing down shadows nothing above it - give it a `position` above the
  scene and a frustum (`shadow.camera`) that covers the casters. Acne
  knobs are Three's: `shadow.bias` (map depth units) and
  `shadow.normalBias` (world units along the receiver normal, the one to
  reach for first, ~0.02); the depth pass culls FRONT faces (Three's
  shadowSide default), so closed casters need little bias but a
  back-culling plane casts only from its back. The shadow side follows
  the material's `cull` (Three's shadowSide rule, Godot's shadow pass):
  a `cull: "none"` foliage card or pane casts from both faces, and a
  UV-mapped `alphaTest` material casts its cutout (leaves, not
  rectangles), through the `Material.shadow` variant the standard
  classes carry (a `shaderMaterial` gets the cull side from its `cull`
  and supplies its own cutout variant as the `shadow` instance option).
  Opting out of receiving is on the
  MATERIAL here (`receiveShadow: false`), not the object (Three's
  `mesh.receiveShadow`) - Godot's split, and URP's. An instanced mesh
  casts when its class declares `shadowVertex` (the vertex stage reduced
  to position, instance placement included; the class builds one depth
  program from it plus the shared depth fragment, culling the shadow
  side and binding only the instance slots the stage reads, and every
  instance shares it) - without one the shadow views
  SKIP instanced meshes, since the plain depth override cannot know
  their records. `examples/instanced.tsx` casts. Every casting light
  is a full extra pass over the casters plus its tiles of the one atlas
  (two sampler units on every receiving program, however many cast),
  so cast from the lights that matter, not all of them.
- SOFT shadows are a light's shape, not a filter knob: `angularSize`
  (directional, degrees) and `sourceRadius` (spot/point, world units)
  make the penumbra follow the caster-to-receiver distance; `shadow.radius`
  is the uniform blur and, under a size, the floor. Both widen through
  the scene's `shadows.quality` tier, which the quality row sets - hard
  at low, so the one write a low-end profile needs is the app tier
  (Quality in large-worlds.md). The penumbra is capped at
  `SHADOW_MAX_TEXELS` map texels, so a coarse map (a wide box camera, a
  far cascade) caps the softness, and a blocker outside that search
  radius leaves its edge hard; raise `mapSize` or pull the frustum in
  before raising the size. The disc is rotated per pixel, so a soft
  edge has grain that a higher tier reduces; a snapshot test measures a
  transition's WIDTH across a row, never a single pixel. The receiver
  plane the wide taps bias against comes from the normal `lightShadow`
  is given (a normal map bends it slightly); a surface seen edge-on by
  the light caps the correction (`SHADOW_SLOPE_MAX`), so acne there is
  `normalBias`'s job as before.
- The kernel is INSTANTIATED ONCE: `lightShadow` scans the light's maps
  in one loop and samples the blend partner through the same call on
  the next iteration. Inlined at three sites the program outgrew an
  Adreno 610 and every pixel paid, the hard path included (115 ms a
  frame against 72 with one copy; the soft tiers two and a half times
  their cost). A counted two-pass loop around the call was unrolled
  into two predicated copies, a dynamically indexed local array went to
  scratch memory, and the kernel's own loops run to the tap-count
  UNIFORM with a masked table index because a bound the compiler can
  cap gets unrolled to the table's full length. Keep every one of
  those when touching SHADOW or SHADOW_LOOKUP, and measure on the
  tablet (`okf/plans/3d-contact-hardening-shadows.md` Findings).
