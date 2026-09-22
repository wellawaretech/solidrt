# @solidrt/3d - agent notes

A retained 3D scene graph above `@solidrt/core/gpu`. Meshes, materials and
a camera compile to ONE depth-buffered draw target (`createDrawTarget` +
one `addDraw` entry per mesh); the scene's output is an ordinary texture
id composited as a `<texture>` leaf, so it takes layout, transforms,
blendMode and pointer events like any element.

The layer below is worth knowing by name: `@solidrt/core`'s
`examples/gpu-*.tsx` are the raw pipeline vocabulary this package
composes - `gpu-particles.tsx` (points topology, `gl_PointSize` /
`gl_PointCoord` splats, additive blend), `gpu-instancing.tsx`
(`instanceCount` and `gl_InstanceID`), `gpu-pipeline.tsx` (a custom
vertex + fragment pair over an interleaved buffer), `gpu-draw-list.tsx`
(a mutable list of draws sharing one depth buffer), `gpu-raw-program.tsx`
(compile/link/pipeline by hand). What the GPU layer can do is answered
there, not here.

Contents:

- [The model](#the-model)
  - [Two layers: core and components](#two-layers-core-and-components)
  - [Node lifecycle](#node-lifecycle)
  - [Rendering is the runtime's](#rendering-is-the-runtimes)
  - [Views and layers](#views-and-layers)
  - [Culling](#culling)
  - [Level of detail](#level-of-detail)
  - [Shadows](#shadows)
  - [Retargeted motion](#retargeted-motion)
  - [Geometry layout, indices and topology](#geometry-layout-indices-and-topology)
  - [Material classes](#material-classes)
  - [Pure pieces and checks](#pure-pieces-and-checks)
- [Components](#components)
  - [Component props](#component-props)
  - [Output composition](#output-composition)
  - [Fill and fixed sizes](#fill-and-fixed-sizes)
  - [Input](#input)
  - [Orbit camera](#orbit-camera)
  - [First-person camera](#first-person-camera)
  - [Overlay projection](#overlay-projection)
  - [Picking](#picking)
  - [Collision](#collision)
  - [Pointer events](#pointer-events)
  - [Geometry generators](#geometry-generators)
  - [Geometry as data](#geometry-as-data)
  - [Profile kit](#profile-kit)
  - [Materials](#materials)
  - [Instancing](#instancing)
  - [Background](#background)
  - [Cube-map convention](#cube-map-convention)
  - [Environment](#environment)
  - [Reflection probes](#reflection-probes)
  - [Baked sky](#baked-sky)
  - [Panoramas](#panoramas)
  - [Fog](#fog)
  - [Color](#color)
  - [Lighting GLSL](#lighting-glsl)
  - [Custom looks: three tiers](#custom-looks-three-tiers)
  - [Lights](#lights)
  - [phong](#phong)
  - [standard](#standard)
- [Models](#models)
  - [Three layers](#three-layers)
  - [Async loading](#async-loading)
  - [Placement and sockets](#placement-and-sockets)
  - [Wardrobe pieces](#wardrobe-pieces)
  - [Applied glTF material fields](#applied-gltf-material-fields)
  - [Animation](#animation)
  - [Root motion](#root-motion)
  - [Skins](#skins)
  - [Not in the subset](#not-in-the-subset)
- [Traps](#traps)
  - [Models and skinning](#models-and-skinning)
  - [Materials and color](#materials-and-color)
  - [Environment and background](#environment-and-background)
  - [Coordinates and rotation](#coordinates-and-rotation)
  - [Visibility and instancing](#visibility-and-instancing)
  - [Shadows](#shadows)
  - [Views and scene params](#views-and-scene-params)
  - [Shader materials](#shader-materials)
  - [Picking, pointer and collision](#picking-pointer-and-collision)
  - [Geometry and components](#geometry-and-components)

## The model

### Two layers: core and components

Two layers. The imperative core is Solid-free: `createScene`,
`createMesh(geometry, material)`, `add`/`remove`, `setTransform`,
`lookAt`, `getRotation`, `setVisible`, `setRenderOrder` - plain objects
over the spatial core (`flux:spatial`, `alloy/src/spatial/`): every node
in a scene has a core node, JS keeps the LOCAL position/quaternion/scale
as the readable truth and forwards each write, and the core's flush
(one call per microtask) recomputes only the moved subtrees and writes
each entry's uModel (plus uNormal for materials declaring it) and its
visibility switch itself - a move costs its subtree, never the scene.
ONE `setTargetParams` (the shared uViewProj + uCamPos) per camera
change, however many meshes. World matrices live in the core only:
`worldPosition`/`lookAt`/picking read them back (`worldMatrix`, pending
writes included). See okf/backlog/spatial-core.md for what still runs
in JS and why. The component face (`Scene`/`View3d`/`Group`/`Mesh`/
`PerspectiveCamera`) syncs props into that core over context and renders
nothing itself.

### Node lifecycle

Two removal verbs, one meaning each, the same words with the same
meanings in @solidrt/2d. `remove(child)` DETACHES an intact subtree -
Three's `parent.remove`, Godot's remove_child: children stay under the
removed node, core nodes free on leave and recreate on re-enter, so a
removed subtree re-adds cleanly (`add(parent, child)` re-parents through
it); it SNAPS, since a detached node is coming back. `destroy(node)` is
Unity's Destroy, Godot's queue_free: the subtree is gone for good, every
handle in it inert (writes are no-ops, add throws, remove skips), and it
is the removal an `exit` rides on (see Retargeted motion). The components
destroy on unmount. Nothing else is destroyed: GPU resources stay on
their own disposers (`disposeGeometry`, `disposeInstances`,
`model.dispose`, the scene's `dispose`), each of which frees a node still
animating out on the spot. An instance node (addInstance on an instanced
mesh) is slot-bound to its mesh: `destroy` frees it (the slot recycles at
the free) and the generic add/remove throw on it. (@solidrt/2d has no
detach - a sprite cannot exist outside its layer - so `destroySprite` /
`destroyGroup` are its only removal, and a sprite that should come back
is hidden.)

### Rendering is the runtime's

Rendering is the runtime's. The target is `render: "auto"`: it
re-renders when entries change, so a STATIC scene costs zero passes and
the library registers no frame loop. Continuous animation is the app's
own `onFrame` writing a signal (declarative) or `setTransform` on a
`ref`-grabbed node (the frame-rate escape hatch - signals carry
structure, per-frame motion goes straight to the scene).

### Views and layers

VIEWS: `scene.createView({ width, height, overrideMaterial?, depth?,
clearColor?, ... })` renders the same scene into a second target from
its own camera (`view.setCamera`, the scene's CameraUpdate shape; in a
component tree `<View3d>` is the same as a Scene child, see Components).
Each mesh gets one entry in the view's target bound as one more draw sink of
its CORE node, so the one flush writes every target - the app writes
nothing per view. Geometry buffers and (without an override) materials
are shared; the light set and `scene.setParams` names fan out to every
view, `view.setParams` is the view's own channel - and names a view
sets itself (or its `fog` option, below) become VIEW-OWNED: the
scene's setParams/setFog fan-out skips them from then on, so a view
override survives scene-wide writes. A view's backdrop is its
clearColor (the scene background draws on PROBES, not views); a view
has no picking. A view is a buffer plus a resolve like the scene
(`view.hdrTexture`, `view.texture`, the `resolve` option and
`view.setResolve` - see Color); a tiled view (`into`) renders LINEAR
light into the atlas and the app resolves the atlas once: a
`createShaderTexture` over `resolveFragment()` from `/glsl` (the stock
source with `uScene` and the RESOLVE set declared) with the atlas bound
as `uScene` and `resolveParams({ toneMapping, exposure })` as its params
- `examples/scene-atlas.tsx`. LAYERS select what a target
draws, Three's model exactly: `layers` on a mesh is its membership
bitmask (default 1, `setLayers`/the `layers` prop, NOT inherited from
Groups), and each target carries a mask (default 1) - `layers` on
createScene/createView, live via `setLayers` on the scene handle and
each view. A mesh draws where mask & layers is non-zero, so a minimap's
marker meshes live on bit 2: invisible in the main render, drawn by
the map view whose mask admits them. Shadow views follow the SCENE's
mask (what the scene cannot see must not darken it), and
pick()/raycast()/overlap()/sweep() skip scene-masked-out meshes like
invisible ones - unless the query passes its own `{ layers }`, which is
how a low-poly collision mesh lives undrawn in the scene yet answers
ground and collision queries (the physics-collider pattern).
SEALED INTERIORS are the other manual use: from outside, a closed shell
hides its whole interior, but frustum culling cannot know that and
there is no occlusion or portal culling. Put the interior, the shell
and an opening "plug" on their own layers and switch the scene mask by
the camera's zone (inside, outside, in the doorway) - the reactive
`<Scene layers>` makes it one signal, and a light's reach is masked
the same way once lights carry layers.
Per-view fog: `fog: FogOptions | null` on createView overrides the
scene's fog for that view (null = unfogged - the clear minimap over a
fogged scene); absent follows the scene. `overrideMaterial` (Three's
`scene.overrideMaterial`, scoped to the view) draws every mesh with one
material - a depth pass, a normal/id visualizer - skips instanced
meshes (unless the override itself declares their exact
`instanceBuffers` record layouts) and draws in add order. `depth: "texture"` exposes `view.depthTexture`, the shadow-map
input; the same option on createScene exposes `scene.depthTexture`,
the input for a depth-reading post effect in `output` (not combinable
with `samples` - no multisampled sampleable depth). `ortho: { left, right, top, bottom }` on any camera swaps
perspective for `orthographic()` (`fov` ignored; `ortho: null` returns);
the scene's own camera takes it too, and pick() follows.
`examples/scene-views.tsx` is the shape.

### Culling

CULLING is the core's, per target, on by default: every camera write
(the scene's, a view's, a shadow tile's) also sets that target's
frustum in the spatial core, and the flush switches an entry whose
world box falls wholly outside it to instance count 0 - the same
switch as `visible`, so a culled mesh costs nothing per frame and a
still camera re-tests nothing (a camera move re-tests every sink on
that target, in Rust; a node move re-tests its own). The box is the
picking box (the local bounds through the world matrix, the
Godot/Unity AABB test; Three uses spheres); an instanced mesh without
explicit `bounds` is culled by the union of its live instances' boxes
(the joint group below over the instance nodes, so the box follows
them: Godot's MultiMesh AABB, with `bounds` as its custom_aabb; Three
computes an InstancedMesh sphere once and leaves it stale, Unity wants
worldBounds), and a record mesh without `bounds` is never culled. Per
mesh: `frustumCulled: false` (Three's name; `setCulling`) for geometry
a vertex stage moves beyond its box - a fullscreen quad, a custom
displacement - and `cullMargin` (world units, Godot's
`extra_cull_margin`) for bounded displacement such as wind. Sprites
cull by their quad's reach at any facing. A SKINNED part is culled by
the union of its joints' boxes (the bake computes each joint's
influence box in joint space, `ModelSkin.jointBounds`, .srtm VERSION
5; the joint nodes carry them as culling-only bounds, outside the
picking index), so the box follows the pose with no per-frame JS -
Unity's bone bounds, Godot's per-bone AABBs; no `updateWhenOffscreen`
knob is needed. Probe faces set no frustum (six cameras, one target).
Shadow tiles cull casters against their light frustum, which is what
makes `shadow.distance` and cascades cheaper.

### Level of detail

LOD is the core's too, beside culling: a group's level is picked per
target after each flush by PROJECTED SIZE - the sphere around the
group's boxes (radius = half the box diagonal, conservative like the
frustum box), its diameter as a fraction of the viewport height,
`radius / (distance * tan(fov / 2))` under a perspective camera and
`radius * 2 / (top - bottom)` under an orthographic one (Unity's
screen-relative model; Three's distance is wrong for an ortho minimap,
a zoomed camera or a scaled-up tree). `createLod([{ node, size }, ...],
{ fade? })` / `<Lod fade?>` with `lodSize` on its direct children: the
levels nearest first, `size` the projected size BELOW which a level
hands over to the next, the last one's the cull threshold (0 = never
culled); a level is a direct child (a parentless node is adopted by
`setLod`, one parented elsewhere is refused, never moved). Every camera
write is also its target's view for the core, which derives the eye,
focal and ortho flag it measures with (under `lodBias`): the scene and
each view pick by their own camera (a minimap sees the far level of the
same tree), shadow tiles by the SCENE camera (a caster draws the level
the camera sees, so its shadow matches - Unity's and Godot's rule),
probe faces by each face's camera. The gate composes with the frustum and `visible` into the same
instance-count switch, so a still camera re-tests nothing and a thousand
groups cost no JS per frame; a child of the group that carries no
`lodSize` is drawn always. Without a `fade` the switch is hard with a
one-sided hysteresis band (a level is left at its threshold and
re-entered only once the size climbs a tenth above it), so a boundary
never flickers; with one, each threshold `s` widens into `[s, s * (1 +
fade))` where both levels draw with COMPLEMENTARY screen-hash dithers
(the near one keeps the pixels below the band position, the far one
the rest - a partition, never a double draw) through the `uLodFade`
vec2 every stock fragment composes, its band position in 64 steps so an
entry is rewritten when its step changes, not every frame, and written
solid again when the entry switches off; a custom class opts in by composing
`LOD_FADE` from `@solidrt/3d/glsl` and calling `lodFade()` first in
main. Shadow tiles never fade: their depth pass switches hard at the
band's midpoint, invisible for two variants of one shape.
`createInstancedLod([{ geometry, material, size }, ...], opts)` /
`<InstancedLod levels capacity?>` is the population form: one instance
set, one matrix buffer PER LEVEL, and the core stages each instance's
record into the level ITS OWN size picks - a spread forest as three
entries, near trees full and far ones cards. Returned as the first
level's mesh (the other levels are its children at identity, sharing
its instance slots; `setLayers`/`setCastShadow` on it reach them,
`setInstanceStyle` writes every level's style stream; a pick or pointer
event on an instance names that first mesh whatever level the instance
is drawn at). Records are
target-agnostic, so instances pick by the SCENE camera and every view
draws that choice; instanced levels switch hard (`fade` is refused).
`scene.setLodBias` (Unity's lodBias) multiplies every measured size:
below 1 hands over sooner, the one-line quality knob for a low-end
device. Picking and collision see the level the queried target draws
(`scene.pick`/overlap/sweep/moveAndSlide the scene's, `view.pick` the
view's); a collider that must not follow the camera is its own undrawn
mesh on a collision layer. Nested groups chain. Mesh simplification is
a bake job, not a runtime one, and a far level for EMISSIVE DETAIL is a
design job: geometry simplifies, point lights do not. Fine lights
aggregated into bigger dots read as a uniform point cloud from afar;
what works is folding them into a dim emissive floor under the coarse
blocks, brightness proportional to the local light density squared,
visible only through the gaps - and tiny lights kept in the MIDDLE
level bring the speckle back one level closer. `examples/lod.tsx` is
the shape; `probes/3d-lod-bench.tsx` the cost.

### Shadows

SHADOWS are a view: `<DirectionalLight castShadow shadow={{ mapSize?,
bias?, normalBias?, radius?, camera? }}>` (`createDirectionalLight({ castShadow,
shadow })`, `setLight`) makes the scene own an internal
`createView({ depth: "texture", overrideMaterial: depth pass })` drawing
the `castShadow` meshes (`<Mesh castShadow>`, `setCastShadow`) from an
orthographic camera at the light's WORLD position along its world
direction, `shadow.camera` (+-5, 0.5..500 by default) as the frustum.
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
atlas route. `shadow.radius` (every casting light, default 1 = that
one tap, a 2x2 bilinear compare) softens the edge: above 1 a 3x3 grid
of hardware taps `radius` texels apart, Three's `shadow.radius` /
Godot's `shadow_blur`, nine taps per receiver fragment. A first-person
eye over a floor shows the texel stairs at any map size; 2 is the
usual figure, past ~3 the taps separate into bands. Each face map renders a few degrees wider than its face
(URP's fovBias) so a seam fragment's occluder is inside the map it
samples - without the guard band every seam shows a lit slit - and
PCF taps clamp at face-tile edges, so a face seam hardens slightly
instead of bleeding into the neighbour.
`shadow: { cascades: N }` (1..MAX_CASCADES = 4) replaces the box with
N maps fitted to slices of the SCENE camera's frustum (near ..
`shadow.distance`, default the camera far; the practical split; each
slice's bounding sphere as an ortho box along the light, its centre
snapped to the map's texel grid so edges do not swim; re-fitted
whenever the scene camera or the light moves) - a receiver samples the
tightest map that covers the point, fading into the next over the
map's outer 10% (`SHADOW_BLEND`) so the hand-over is a band, not a
seam; contact shadows stay sharp near the camera while the horizon
still has coarse ones, and pulling `distance` in sharpens all of them.
The box is the honest tier for a bounded scene; cascades are for a
scene that outgrows it, at N times the shadow fill. Every map is a
TILE of the scene's one shadow atlas (a `depth: "texture"` draw target,
a grid of cells the largest `mapSize` wide, scaled down uniformly
against `limits.maxTextureSize`), so N maps are ONE pass: the atlas
depth binds as the target-level `uShadowAtlas` of the scene and every
non-shadow view (a white texel when nothing casts); maps are MAP slots
dealt in light order, a light's cascades consecutive and tightest
first - `uShadowRect[j]` slot j's tile in atlas UV, `uShadowMatrix[j]`
its view's own view-projection (the whole array is one write per
shadow-camera move) - and per light i `uShadowFirst[i]`/`uShadowCount[i]`
name its slots (count 0 = it does not cast) with
`uShadowBias[i]`/`uShadowNormalBias[i]` its knobs; `SHADOW_SLOTS` in
glsl declares the set. Every `phong` material RECEIVES by default
(Godot's and Three's default); `phong({ receiveShadow: false })` opts a
material out and drops the map from its program - a material option,
as with vertexColors/triplanar, because the material picks the program
(Godot's `disable_receive_shadows`). The factor is `SHADOW`'s one
hardware-compare tap (sampler2DShadow, LEQUAL, the driver's 2x2 PCF)
on each casting light's own term. `examples/shadows.tsx` (three
casting lights) is the shape; `examples/cascades.tsx` the cascaded sun.

### Retargeted motion

RETARGETED motion is native: `setTransition(node, { position:
{ duration: 400 }, ... })` makes setTransform writes TARGETS the core
animates toward every frame (position/scale per lane, rotation along
the quaternion geodesic - a spring keeps its velocity through
retargets), so a mesh gliding to a slot or a camera rig easing costs
one JS write per target change, zero per frame. The declaration lives
on the SceneNode and re-applies on every scene enter; the pose a node
enters with snaps, unless a component's `from` (`position: { duration,
from: [x, y, z] }`, a quaternion for `rotation`) animates it in from
there - at every scene enter, since each enter creates the core node
anew. A component's `exit` is where it animates to when `destroy` lets
go of the node: it stays drawn while it leaves and is a GHOST meanwhile -
no pick, raycast, overlap, sweep or pointer event sees it - then frees
when the last exit settles; children go first and the parent stays their
frame until the last of them has settled, so a dying character's parts
leave in its frame (a mixer driving its joints keeps driving them). Either
endpoint takes `{ value, duration?, curve?, bounce?, delay? }` to own its
direction's motion (an ease-out enter, an ease-in exit), and `delay` on
an entry holds its writes on the animation clock (a late frame catches
up). `stagger` (ms) goes on an ANCESTOR's declaration (a `<Group
transition={{ stagger: 40 }}>`), never on the animating nodes: it spaces
the enters and exits of its descendants that begin in one frame by
`index * stagger` (add order in, tree order out whatever order the
unmount lets them go in; nearest declaring ancestor wins) and cascades
nothing unless they declare `from`/`exit` -
a squad of instances spawning in is one `stagger` on their group, and
for the nodes straight under the scene root it is `<Scene stagger>` /
`createScene({ stagger })` (a declaration on `scene.root`). A populated
mesh unmounts as `destroy(mesh)` then `disposeInstances(mesh)`, and the
dispose waits for a mesh still animating out, so an `<Instance>`'s exit
plays through its `<InstancedMesh>`'s unmount (`model.dispose` waits the
same way); the scene's and a layer's `dispose` cut a corpse short - the
whole target is going. `examples/exits.tsx` is the live example: crates
that pop in and shrink away on tap, the shelf cascading out and back
through `<Show>`. Each natural settle calls the node's `onTransitionEnd`
(plain field like the pointer handlers) with `{ component }` - never on
a cancel, snap, leave or exit; the raw "spatialTransitionEnd" engine
event (srt:events, carrying the CORE node id `_node`) stays for
flux:spatial consumers.

### Geometry layout, indices and topology

One interleaved vertex buffer per geometry, described by an open layout
(`Geometry.layout`, absent = "base"): an ordered attribute list that
starts with `aPos` float32x3 and may carry any named channels after it,
each in a vertex format. Formats are WebGPU's spelling of (component
type, count, normalized): the float32 family (`float32` .. `float32x4`),
`float16x2/x4`, the normalized integers `unorm8x4`, `snorm8x4`,
`unorm16x2/x4`, `snorm16x2/x4`, the unsigned integers `uint8x4`,
`uint16x2/x4`, `uint32` .. `uint32x4` and the signed integers `sint8x4`,
`sint16x2/x4`, `sint32` .. `sint32x4`; `VERTEX_FORMATS` is the table.
Every format is a multiple of 4 bytes, so offsets and strides are
4-aligned by construction. A format feeds the shader `in` of its KIND
and component count, WebGPU's rule: float and normalized formats feed
`in vec4` (the fetch converts: `in vec4 aColor` reads a `unorm8x4` as
0..1), `uint*` formats feed `in uvec4` and `sint*` formats `in ivec4`,
exact at every width - so a per-vertex id or an index into a wide
table rides as `uint32` into `in uint aId`, and the "skinned" layout's
`aJoints` is `uint8x4` into `in uvec4 aJoints`. A layout that crosses
kinds (`uint8x4` into `in vec4`) is the missing-attribute error at
add() (`formatFeeds` is the rule). The base prefix `aPos` float32x3 + `aNormal`
float32x3 + `aUV` float32x2 is what every generator emits and what the
stock materials read, not a rule: a hand-built geometry declares what
it has, so a point cloud may carry `[aPos float32x3, aData float32]` at
16 bytes a point instead of 36, or a color as `unorm8x4` at 4 bytes
instead of 16. Pair a prefixless layout with a material that reads only
what it carries: `unlit` (aPos only) or a shaderMaterial; a stock lit
material over it throws the ordinary missing-attribute error at add().
A layout handed to a generator must start with the prefix and stay
float32-family (the generator writes floats); packing is a pass over
the result. `withAttribute(geometry, { name, format }, fill)` appends
one channel in any format (Three's `setAttribute` for an interleave;
the fill is given in float and encoded on write); "colored" names the
common case, the prefix plus `aColor` float32x4 - the per-vertex data
channel (a tint, baked AO, any four scalars; standard name, your
contents) - and `withColors(geometry, fill)` is its spelling. Fill is a
flat size-per-vertex array or a per-vertex callback receiving `(index,
pos, normal, uv)` (zeros for a channel the layout lacks). The fill is
raw floats: a tint the stock materials read under `vertexColors` is
premultiplied linear like every shader color, so encode an sRGB pick
with `premultipliedColor(color)`. `Geometry.vertices` is an
`ArrayBufferView` (a Float32Array for an all-float layout, a Uint8Array
when a channel is packed; never the contract): read and write a channel
through `geometryAttribute(geometry, name)`, an accessor whose
`get(i, k)`/`set(i, k, v)` speak floats whatever the format, and count
vertices with `vertexCount`. Materials read attributes BY NAME: a
material's vertex stage may declare any subset of its geometry's
channels, and a channel the program reads that the geometry lacks (by
name, or with a different component count) throws at add(). What a program reads is the ENGINE's word (`material.attributes()`
= `programAttributes` reflection of the linked program, instance
attributes excluded), not a parse of the GLSL: an `in` the compiler
dropped does not count, and the engine also rejects a pipeline whose
attribute lists leave a read attribute uncovered. The material
keeps one program and builds one pipeline per layout its meshes bring,
so a geometry may carry more than a material reads. The whole layout
ships whether a material reads every attribute or not (inactive
attributes only keep the stride), so extra channels cost their bytes on
every draw of that geometry - keep data-light passes (a wireframe
reading only aPos) on base-layout geometry. `layoutStride` (bytes)/
`layoutSlot` (byte offset, format, components)/`layoutKey`/
`layoutAttributes` are the layout arithmetic; two layouts with equal
keys interleave identically (merge requires that).
STREAMS: a geometry may carry more than one vertex buffer.
`Geometry.vertices` under `layout` is stream 0 (where aPos lives) and
`Geometry.streams` holds extra `{ layout, vertices }` buffers, each with
its own attribute list and the same vertex count (Unity's vertex
streams; Three's one buffer per channel when a stream carries one
attribute). `withAttribute(geometry, attr, fill, { stream })` appends a
channel to stream 1..n or opens stream n + 1; `geometryAttribute`,
`geometrySlot`, `geometryKey`, `geometryLayouts`, `geometryStreams`
and `geometryVertexCount` read across streams, and a material's
pipeline is one per `geometryKey` (every stream's layout, `|` between
them). Streams exist for data on its own schedule: `updateVertices(
geometry, { stream?, first?, count? })` re-uploads that vertex range of
that stream from the geometry's own array in place (write the array
through the accessor first; Three's `needsUpdate` with an update
range), reaching every mesh, view and wireframe over the geometry
while the other streams never move; a stream-0 update drops the cached
bounds and rewrites the same range of the core's copy of the positions,
so the mesh picks and culls where it is now drawn (the box follows at
the next flush, the triangle index is rebuilt by the next query - a
deforming mesh needs no re-attach). transformGeometry and
the edge builders share extra streams with their source; merge
concatenates every stream; the `.srtm` container refuses streams (run
time data). `setDrawRange(mesh, first, count?)` draws indices `[first,
first + count)` of the mesh's geometry (Three's `setDrawRange`, on the
mesh because the scene owns the entries), applied to every entry of the
mesh and reset by setGeometry. The worked example is
`examples/streams.tsx`.
Indices are uint16 or uint32 - the `Geometry.indices` array type picks
the draw's index format, so hand-built geometry past 64k vertices just
uses a Uint32Array (generators emit uint16). What the indices LIST is
`Geometry.topology` (`"triangles"` when absent; `"lines"`,
`"line-strip"`, `"points"`, `"triangle-strip"`): the index buffer and
its primitive travel together (Godot's surface primitive, Unity's
SetIndices topology), a material builds one pipeline per (layout,
topology) pair its meshes bring, and validateGeometry enforces the
count rule at add() (a whole number of triangles or lines, one
primitive's worth for a strip). Materials have no topology of their
own. Only a triangle list gets a triangle narrowphase; lines and points
pick and collide by their bounds box (which follows `updateVertices` all
the same), and cast no shadow. `geometryTopology`
reads the field with the default applied. POINTS: a point cloud is
ordinary indexed geometry - one vertex per point, the index buffer
listing them, `topology: "points"`, no constraint on the index count.
Under `"points"` the vertex stage writes `gl_PointSize` (honored to the
pixel; `size / gl_Position.w` attenuates it with distance) and the
fragment reads `gl_PointCoord` (0..1 across the sprite) to shape the
splat - `discard` outside the inscribed circle for round ones. The
worked example is `@solidrt/core`'s `examples/gpu-particles.tsx`. Do
NOT reach for instancing to draw a cloud (Instancing, below, has the
cost model). Geometry GPU buffers are lazy, shared, and
reference-counted by draw entries: removing the last
entry frees them at the end of the microtask (a same-tick rebuild keeps
the upload), so swapping `<Mesh geometry>` reactively never accumulates
old generations; the vertex upload is keyed on the vertices view
itself, so geometries sharing a vertex array (a wireframe over its
source) share one buffer. `disposeGeometry` is the immediate explicit
free.

### Material classes

Materials dedupe hard: one program + one pipeline per material CLASS
(a `shaderMaterialClass` per option combination for unlit, lit and
sprite alike: map x transparent x cull x alphaTest, phong's extras on
top), `depth: true` + `cull: "back"` unless the material says otherwise
(`cull: "none"` for double-sided geometry; phong flips the normal on back
faces); an instance is just per-entry uniforms (`uColor`) and bindings
(`uMap`).

### Pure pieces and checks

The pure pieces (`math.ts`, `geometry.ts`,
`profile.ts`, `sweep.ts`, `gltf.ts`, `model-file.ts`) are Solid-free and
GPU-free BY DESIGN so they can be checked headless (and, for the two
model modules, run under bun in `tools/model.ts`); keep them that way.
The rigs under `checks/`
(`geometry-check`, `sweep-check`, `pick-check`, `dispatch-check`,
`gltf-check`) run on
flux from the repo root: `bunx srt bundle -f --stdout
packages/3d/checks/<name>.ts | target/release/flux -`. Run the ones
touching what you changed. `raycast-check.tsx` and
`collision-check.tsx` are the exceptions: they assert the documented
picking contract (triangle accuracy, the box tier, pick/raycast
parity, layer masks, the `{ meshes }` filter) and collision contract
(exact sweep times, the surface rule, the slide filter, layers and
meshes on overlap/sweep, moveAndSlide's landing) against a real scene,
so they run on the playback client instead:
`bunx srt render packages/3d/checks/<name>.tsx --project --duration 3
--size 128x128`. Run them whenever a doc edit touches picking or
collision claims - two copies of this contract have drifted before.

## Components

### Component props

| Component | Props |
| --- | --- |
| `Scene` | `width?`, `height?` (target pixels - both, or neither = FILL, below), `clearColor?`, `camera?` (partial CameraUpdate, `ortho` included - the declarative scene.setCamera; same state as `PerspectiveCamera`, use one form; the camera CONTROLS are not a third form - `OrbitCamera`/`FirstPersonCamera` drive position and target only, so `fov`/`near`/`far` come from here even while a control moves the camera, and the default `far` of 100 is what clips a scene in metres), `background?` (fragment GLSL, or a skybox `{ cube, intensity?, rotation? }`), `environment?` (`{ cube, intensity?, rotation? }`, the cube reflective materials mirror), `fog?` (FogOptions: linear `{ color, near, far }` or exp2 `{ color, density }`, either plus `height`/`heightFalloff` - see Fog), `toneMapping?` (`"none"` default, `"aces"`, `"agx"` or `"neutral"`), `exposure?` (default 1), `bloom?` (`{ threshold?, intensity?, radius? }`, the stock bloom on the resolve, reactive), `layers?` (target mask, default 1), `depth?` (`"texture"` exposes scene.depthTexture; not with samples), `samples?` (1/2/4/8 MSAA), `label?`, `ref?(scene)`, `output?(texture)`, `resolve?` (the source, `{ source, textures }`, or a function of the buffer id returning either; fixed at creation - see Color), `events?` (pointer events, default on), `pointer?` (the leaf's pointer feed, fed from the scene's root), `onPointerDown/Move/Up?`, `onWheel?`, `onTap?` (the scene's own handlers, the last stop of the walk - `event.mesh` null over empty space) |
| `View3d` | a Scene child rendering the scene again from a camera of its own (scene.createView as a component): `width`, `height` (target pixels, live; fixed-size only for now), `x?`, `y?` (the tile's top-left in `into`, live), `into?` (tile an app-owned draw target - one pass for every view into it; fixed at creation), `camera?` (partial CameraUpdate on the view's camera, live; same state as a `PerspectiveCamera` child), `layers?` (the view's mask, live), `clearColor?`, `label?`, `overrideMaterial?`, `fog?` (FogOptions, or null for none), `depth?`, `samples?`, `filter?`, `wrap?` (createView's, fixed), `ref?(view)`, `output?(texture)` (else a built-in `<texture>` leaf at the target size, a tile shown through srcX/srcY), `resolve?` (as Scene's; not with `into`), `bloom?` (BloomOptions overrides the scene's, null turns it off in this view, absent follows the scene; not with `into`), `events?`, `pointer?` (the view leaf's feed, fed from the view's root), `onPointerDown/Move/Up?`, `onWheel?`, `onTap?` (the view's own handlers); camera-control children drive the VIEW (inside, `useScene()` reports the view as `viewport` and the view's feed as `pointer`); node children mount to the scene as outside, and under the view's leaf get their ordinary pointer handlers, picked through the view's camera (`view.pick`), the view as the root of that walk |
| `Group` | `position?`, `rotation?` (Euler radians, XYZ order), `quaternion?` (either, not both), `scale?` (number = uniform), `visible?`, the bubbling pointer events (below: down/move/up/wheel/tap from a hit descendant; a group is never the struck node, so it takes no `onPointerEnter`/`onPointerLeave` - `Lod` likewise), `ref?(node)` |
| `Lod` | a Group whose direct children carrying `lodSize` (a prop every node component takes: the projected size below which that child hands over, see Level of detail) are its levels, sorted by size, never by JSX position; plus `fade?` (the cross-fade band fraction, default 0); a child without `lodSize` is drawn always |
| `InstancedLod` | as InstancedMesh minus `material`, plus `levels` (`[{ geometry, material, size }]` nearest first, fixed at creation; instanced materials as InstancedMesh's), `castShadow?` (every level); `<Instance>` children populate it as under `InstancedMesh`, each drawing the level its own projected size picks |
| `Mesh` | `geometry`, `material`, transforms as Group, `params?` (per-mesh uniforms, merge semantics - no unset), `renderOrder?`, `castShadow?`, `layers?` (membership bitmask, default 1), `frustumCulled?` (default true; false for geometry a vertex stage moves beyond the node's box - a billboard, a fullscreen quad; see Culling), `cullMargin?` (world units of slack around the box for bounded displacement), pointer events (below), `ref?(mesh)` |
| `Sprite` | as Mesh minus `geometry`: a camera-facing unit quad, `scale` is its world size, rotation is ignored; pair with a `sprite()` material |
| `InstancedMesh` | as Mesh (an instanced `material`: a stock one with `instanced`/`instanceColors`, or a class declaring INSTANCE_MATRIX_ATTRIBUTES), plus `capacity?` (instance slots, default 64; the buffers double past it), `bounds?` (optional: instances pick by themselves and the mesh culls by their union), `label?`; a PARENT: `<Instance>` children populate it, `<Group>` children are squads; the record buffers are component-owned and freed on unmount |
| `Instance` | one instance of the enclosing `InstancedMesh`: transforms, `transition`, pointer events as Group, plus `style?` (the material's style record, one value per component - `[r, g, b, a]` under `instanceColors`), `ref?(instance)`; a parent too (a `<Mesh>` under an instance rides with it) |
| `RecordMesh` | as Mesh, plus `records` (the per-instance records in the material's first instance layout, an ArrayBufferView; buffer capacity starts at the first value and grows on larger rewrites), `count?` (records drawn, default all), `bounds?` (local [minX..maxZ] over the population - without it the mesh never picks); the record buffers are component-owned and freed on unmount |
| `PerspectiveCamera` | `fov?` (vertical DEGREES, default 60), `near?`, `far?`, `position?`, `lookAt?`, `up?` - or the Scene `camera` prop, the same state (last write wins) |
| `SpotLight` | transforms as Group, `direction?` (local aim, default [0, -1, 0]), `color?`, `intensity?`, `distance?` (falloff cutoff, 0 = none), `angle?` (cone half-angle DEGREES, default 60), `penumbra?` (0..1 rim fade, default 0), `decay?` (falloff exponent, default 2), `castShadow?`, `shadow?` (mapSize, bias, normalBias, near), `ref?(light)` |
| `PointLight` | transforms as Group (position is what matters), `color?`, `intensity?`, `distance?`, `decay?`, `castShadow?` (six face maps, six shadow slots), `shadow?` (mapSize, bias, normalBias, near), `ref?(light)` |

### Output composition

Output composition: without `output`, `Scene` emits a minimal
`<texture width height>` leaf and nothing else is forwarded - anything
more goes through `output(texture)`, which renders in place of that leaf:
a `<d-texture>`, a leaf with blendMode/fit/pointer/layout props, or a
post-effect chain (`createShaderTarget` sampling the id with a
covering-triangle pass; created in the callback it disposes with the
Scene). Return null for no leaf at all and compose `scene.texture`
elsewhere. Called once, untracked, inside the scene context. `output`
composes the DISPLAYED texture, the resolve's output: an LDR effect
lives here; one that needs the scene's radiance is a `resolve` (Color). Scene
`width`/`height` are target pixels and the leaf's own width/height are
layout, so render and display size separate - render at 2x and display
smaller for supersampling.

### Fill and fixed sizes

Fill (the default): omit `width`/`height` and the built-in leaf is laid
out at 100% of its parent's box (give it a sized parent, as on the web)
while the target follows the leaf's on-screen size in DEVICE pixels -
display scale, `designSize` fits and ancestor transforms included
(getBoundingBoxViewport x displayScale, applied from onLayout, so no
frame draws at a stale size). A bare `<Scene>` in a pane renders at
native density on any display, mesh events are wired automatically
(event scaling reads the leaf's untransformed box back with
`getLayoutBox`), the `pointer` feed you hand in is spread on the leaf,
and the camera aspect follows the box. Fill
or fixed is decided at mount; `output` requires explicit sizes (the
target cannot follow a leaf it does not own), and giving exactly one of
width/height throws.

Fixed sizes still matter where the target is a measured quantity: probes
and checks that snapshot exact pixels, supersampling via `output`, or a
scene composited at a size unrelated to its layout. `width`/`height` are
DEVICE pixels and the leaf's layout is LOGICAL - a fixed 720-pixel scene
under a HiDPI `designSize` fit is stretched across ~1100 device pixels
and looks soft, and nothing warns; that trap is what fill removes (the
examples fill; scene-views and scene-post-effect keep fixed sizes to
show multi-view composition and supersampling). A custom `output` leaf
whose layout differs from the target
takes `handlersFor` (below), not `handlers`; `useScene()` works inside
`output` because it runs in the scene context. One spread carries
everything: the `pointer` feed listens at the scene's root, so
`{...useScene().scene.handlers}` feeds the nodes, the scene's own
handlers and the feed alike (a detached leaf gives its feed a `layout`,
it has no layout box to normalize by).

### Input

Input, the rule (ARCHITECTURE.md): a camera control consumes a
device-free abstraction and never handles events or reads a device
itself. The controls expose `axes` (core's createAxes contract - rates
sampled per frame, immediate deltas in device-free units) and pose verbs;
an input map (`createInputMap`, core AGENTS.md) drives the axes by action
name, and the APP binds devices to the map: the scene leaf's pointer feed
(`createPointerFeed()`, handed to `<Scene pointer>` / `<View3d pointer>`,
where it listens at the root of the pointer walk, so a mesh that claims
its press keeps the control out of that drag; an imperative scene or
view feeds it with `feedPointer(scene, feed)`), a pad (`gamepad(0)`, or `gamepad.next()`
for the next pad to press a button), the keyboard (`keyboard`, its key
events through `input.handlers` on the window).
Nothing binds by default: `<OrbitCamera />` without `input` moves only
through its handle. The standard wiring is a preset, plain bindings the
app applies and edits:

```tsx
let pointer = createPointerFeed()
let input = createInputMap(orbitActions)
input.bind(orbitBindings({ pointer, gamepad: gamepad() }))
<Scene pointer={pointer}>
  <OrbitCamera input={input} distance={7} />
```

The vocabulary is shared with @solidrt/2d, one kind and unit per word:
`rotate` vec2 (orbit turns), `zoom` axis (octaves, positive in), `pan`
vec2 (element heights), `look` vec2 (turns), `move` vec2 ([right,
forward], forward = -y in the screen convention), `rise` axis. Keys and
sticks move the CAMERA where a drag moves the content, so the presets
bind them to `rotate`/`pan` through `invert()`; `look` is the exception,
a drag and a stick both turn the eye.

### Orbit camera

Camera control: `createOrbitCamera(scene, { target?, azimuth?, elevation?,
distance?, min/maxDistance?, min/maxElevation?, orbitSpeed?, rotateSpeed?,
zoomSpeed?, panSpeed?, damping?, clampPose?, zoomAnchor?, rotateAnchor? })`
- azimuth/elevation/distance around a target with optional auto-orbit. The
first argument is anything with the scene's `setCamera`, `camera()` (the
fov maps pan travel to world) and `size()` (the aspect `fit` frames
against): a Scene, or a View to drive one view's camera independently.
Its `axes` are `rotate` (a delta of one element height sweeps one full
turn, Three's OrbitControls convention, so a drag feels the same on a
phone and a 4k window; a rate turns at 0.5 turn/s at full deflection),
`zoom` (octaves: a delta of 1 halves the distance, a rate of 1 halves it
per second) and `pan` (the target slides so the scene tracks the fingers
1:1 at the target's depth, three.js DOLLY_PAN, weighted by `panSpeed`). A
delta bracketed by a gesture (a drag, a pinch, two fingers) applies at
once, the content staying under the fingers; an unbracketed one (a wheel
notch, a key step) is an impulse and glides in - `damping` scales that
settle time, 0 applies it at once - notches compounding on the pending
value so a fast scroll is one push, the 2d camera's rule and the same
knob there. The verbs: `rotateBy(azimuth, elevation)` radians,
`zoomBy(factor, anchor?)` (factor > 1 in, as the 2d camera's zoomAt),
`panBy(right, up)` world units, `setPivot(point)`, `set(pose)`; every one
pushes the pose at once. Two commanded moves ease instead, inside
update(dt): `glideTo({ azimuth?, elevation?, distance?, target? })`, and
`fit(bounds, { glide? })`, which frames a `[minX, minY, minZ, maxX, maxY,
maxZ]` box (geometryBounds, a model's `bounds`) - target to its centre,
distance to where the bounding sphere fills the tighter of the vertical
and horizontal fov at the target's size, azimuth and elevation kept,
clamps applied; a snap unless `glide`, so a park-then-snapshot repeats
(under ortho only the target moves). Any input drops a glide (a finger
landing holds the view), and a `set()` that writes a pose field snaps
and drops any motion; `set({})` and `set({ orbiting })` leave it running.
Every write - input, verb, glide frame, set() - goes through the range
clamps and then `clampPose(pose)`, which sees the whole pose and returns
the fields to change: bound where a pan may put the target, or hold the
eye above a floor (an elevation floor that depends on the distance, which
a fixed `minElevation` cannot say); a glide's goal is clamped when set
and its frames as they land, so it never shows an illegal pose.
Zoom aims at the target unless `zoomAnchor(focal, {eye, target})`
maps the gesture's focal point - a FRACTION of the element, [0..1, 0..1] -
to a world point (ground hit, target-depth plane, ...): then that point
stays pinned under the pointer and the target slides toward it; only the
app can build that mapping, since it needs the projection
(scene.unproject over the scene's size). A pinch holds one anchor for its
whole gesture (the fingers' interleaved events make the span oscillate,
and re-anchoring per delta turns that into a crawl); the wheel, arriving
unbracketed, anchors per notch. Pair it with `rotateAnchor({eye, target})`:
called when a rotate gesture begins, its point is projected onto the
view axis and re-seats the pivot without moving the picture, so a drag
after an anchored zoom orbits what the camera looks at, not wherever the
zoom left the target. Call `orbit.update(dt)` from your onFrame to
integrate the rates, the auto-orbit and any glide (no frame loop of its
own), and use its return - true when the pose changed since the previous
update, nudges included - to gate per-frame dependents like reprojecting
HUD overlays. `orbiting()` (the auto-orbit switch) and `active()` (the
frame-loop gate: orbiting with a non-zero rate, a glide or damped notch
in flight, or any rate driving - the predicate every camera control
shares) are reactive (HUD-safe); the pose is plain state via
`pose()`/`set()` (also the debug-command shape). It
drives position and target only; fov/near/far stay on scene.setCamera
(or the Scene `camera` prop). The auto-orbit pauses while a gesture is
open.

In a component tree, skip the wiring: `<OrbitCamera input={input}
azimuth={1.2} distance={7} />` as a Scene child reaches the scene through
context (as a `<View3d>` child, that view: the context's `viewport` is
the nearest owner's), drives from the map in `input` (its `rotate`,
`zoom`, `pan` actions, or the names in `actions`; live - a new map
reconnects) and nothing else, and runs a frame loop only while
`active()`, so a camera moved by drags alone keeps the app demand-driven
idle. The pose props are initial values: runtime pose changes (and the
debug-command hookup) go through `ref`'s handle, whose set() and verbs
push the pose and whose glideTo/fit ease it. Every other prop is live -
forwarded to the control as a getter and read where it applies, never
snapshotted - so clamps, rates, `damping` and anchors follow their props
without a remount, and a clamp change (`clampPose` included) re-clamps
the pose at once. A `fit` from `ref` at mount reads the scene's size of
that moment: a fill-mode `<Scene>` has its creation size until its first
layout, so fit after the first frame (or hand a fixed-size scene).

### First-person camera

First-person control: `createFirstPersonCamera(scene, { position?, yaw?,
pitch?, min/maxPitch?, moveSpeed?, lookSpeed?, fly?, clampPosition? })` -
a position plus yaw/pitch (yaw 0 faces -z, positive turns left; pitch
positive looks up), Unity's FirstPersonController shape (look AND move in
one control) where Three splits PointerLockControls from a hand-written
key loop. Its `axes`: `look` (a delta of one element height sweeps half a
turn - a drag across the screen turns the walker around - and the
pointer feed's `mouseDelta` delivers mouse motion under pointer lock in
the same unit; a rate turns at 0.4 turn/s), `move` ([right, forward],
forward = -y: a stick pushed up or W reads [0, -1]; a rate walks at
`moveSpeed`, a delta is a step in world units, diagonals clamped to unit
length so they walk no faster) and `rise` (world up at `moveSpeed`, fly
mode only). Walking (the default) flattens the heading onto the ground
plane at fixed height; `fly` moves along the view. The verbs:
`lookBy(yaw, pitch)` radians, `moveBy(right, forward, up?)` in the
walker's frame, `set(pose)`, and `glideTo({ position?, yaw?, pitch? })`,
which eases there inside update(dt) (yaw to the number given, not the
shortest turn), dropped by any input and by a set() of a pose field -
the orbit camera's glide, without its damping: this control's unbracketed
delta is mouse motion under pointer lock, and that is never eased. Every
option but the initial pose is read
where it applies (`fly` per step, `clampPosition` per move, the rates and
pitch clamps per input), so a field changed on the options object takes
effect on the next move: walk and fly are one control. The control NEVER
calls `lockPointer` - click-to-lock and Escape-to-release are the app's
window-level decisions (see `examples/first-person.tsx`) - and has no
collision of its own: `clampPosition(next, current)` is the whole hook -
bounds, a floor height, or `moveAndSlide` over `next - current` against
the collision layer (see `examples/collision.tsx`, whose `jump` is one
more action on the same map: Space or the pad's south button, read by
name); a glide consults it every frame as a walk does every step. Call
`update(dt)` from onFrame; `active()` is reactive - a rate driving (a
held key, a deflected stick), or a glide in flight - and gates the loop.
The map
tracks held keys from the down/up pair through `input.handlers`; spread
its `onBlur` too, since the up never arrives once focus has left.

`<FirstPersonCamera input={input}>` as a Scene child wires the control
to the scene and the map, nothing else: the app spreads
`input.handlers` on the window (keys bubble there from wherever focus
is, so a click on the scene needs no focus dance) and hands the scene
its `pointer` feed. The frame loop runs only while `active()`. Every prop
but the initial pose is live: `fly={flying()}` toggles walk/fly on the
running control (pose carries over, no remount, and `clampPosition` may
swap with it), `moveSpeed`/`lookSpeed` follow their props, and a pitch
clamp change re-clamps at once. Like the orbit control it drives
position and yaw/pitch only: fov/near/far stay on scene.setCamera (or
the Scene `camera` prop), so a walker in a scene bigger than the default
`far` of 100 sets one there. checks/orbit-check.ts and
checks/first-person-check.ts pin both controls headless (they import
`@solidrt/core/input` only).

### Overlay projection

Overlay projection: `scene.project(point)` maps a world point to scene
pixels (top-left origin, y down - the output texture's own space; `w` is
the camera-forward distance in world units under either projection) and
returns null for a point at or behind a PERSPECTIVE camera's plane; an
ortho camera places every point (`w` may be <= 0 there - negative near
is legal ortho). It reflects a pending `setCamera` immediately, so
set-then-project in one tick is exact. `scene.unproject(x, y, w, out?)`
is its exact inverse: the world point at that pixel and camera-forward
distance `w` - project()'s `w` round-trips in both modes (the
drag-at-depth recipe: project the grabbed point once, keep its `w`,
unproject each move). `scene.viewProj(out?)` copies the view-projection
matrix for batch work. Never rebuild the camera matrices by hand for a
HUD.

### Picking

Picking: `scene.pick(x, y)` is project()'s inverse - the camera ray
through a scene pixel, returning `Hit[]` (`{ mesh, distance, point }`,
world units, nearest first; every hit along the ray, not just the front
one). `scene.raycast(origin, direction, opts?)` is the world-space
primitive under it, and `scene.screenRay(x, y)` the ray itself
(`{ origin, direction }`; direction's camera-forward component is 1, so
`origin + w * direction` = unproject) for intersection work pick cannot
do - drag planes, ground grids, filtered raycasts. All three cast
exactly the same ray. raycast's `opts` filters the query:
`{ layers }` (Unity's layerMask) replaces the scene's mask for this ray,
and `{ meshes }` (Three's intersectObjects) is an include-list; a
per-frame ground query passes one or the other instead of skipping
skyboxes and actors by hand.
The index and the narrowphase live in the spatial core: every attached
mesh's local box is a leaf in a dynamic AABB tree the flush refits from
the fresh world matrices (O(moved) per frame, a query O(log meshes)), and
an ordinary mesh is then tested per triangle against its geometry's
shape (one CPU copy of the positions per distinct geometry, created with
its GPU buffers and rewritten by `updateVertices`; the mesh's box comes
from the same copy, so a deforming mesh is found where it is drawn),
so hits carry `face`, `uv` and a world-space `normal` facing the ray, and
a ray through a knot's hole misses. A large geometry's triangles are
BVH-indexed too - built by the first ray that reaches the shape, log-cost
after - so raycasting a merged static scene stays cheap (see the batching
advice). An instanced mesh is box-only (its
explicit population bounds; records are opaque), and so is a mesh whose
geometry is not a triangle list (lines, points, strips have no triangle
narrowphase): it is tested by the
box's twelve triangles, so its hits carry the struck face's `normal` and
no `face`/`uv` - and a ray from inside it meets the far side, the
surface contract overlap/sweep share. Both methods
flush pending writes first (the lookAt/project immediacy contract), and
both skip invisible meshes.

### Collision

Collision: `scene.overlap(volume, opts?)` and `scene.sweep(volume,
motion, opts?)` are the same index's other two questions - what a volume
touches, and where a moving one first touches: Unity's
OverlapSphere/Box/Capsule and SphereCast/CapsuleCast/BoxCast, Godot's
intersect_shape and cast_motion (Three has the raycaster alone; every
Three game adds a physics library or three-mesh-bvh for this). A
`Volume` is a sphere (`{ center, radius }`), a capsule (`{ a, b,
radius }`, the radius swept along the segment - a character) or an
oriented box (`{ center, halfExtents, rotation? }`). overlap returns
`Overlap[]` - `{ mesh, point, normal, depth }`, the deepest contact per
mesh: the point on the mesh, the unit direction out of it and the depth
along it that clears the contact (Godot's get_rest_info, Unity's
ComputePenetration, per hit; unordered) - and sweep `Impact[]` -
`{ mesh, time, point, normal }`, per mesh its first touch with `time`
the fraction of the motion, earliest first. Both take raycast's `opts`
(`{ layers, meshes }`) and test per triangle in WORLD space against the
same shapes, so any transform holds, an instanced mesh or sprite counts
by its box, and the physics-collider pattern is one `{ layers }` away.
Two contracts to know: the tests are SURFACES (a volume wholly inside a
closed mesh with no triangle in reach touches nothing - the trimesh rule
in every engine), and a sweep from a volume already in contact reports
time 0 only while the motion closes in; leaving or sliding along the
contact is no hit, which is what lets a slide along a wall proceed. A
zero motion touches nothing. The filters run in the core: every query
hands `flux:spatial` a `{ root, layers, nodes }` filter (the scene root,
the mask, the `meshes` include-list as node ids - an instanced mesh
contributing every instance node), and a mesh's `layers` is written to
its core node (`setLayers`), so a query never sees another scene's or a
2d layer's nodes and JS only maps hits back to meshes.
`scene.moveAndSlide(volume, motion, opts?)` (and the free
`moveAndSlide(scene, ...)` spelling of it) is the controller over them:
Godot's CharacterBody3D.move_and_slide and Unity's
CharacterController.Move as one PURE call - no node, no velocity; the
first-person camera composes it in `clampPosition`, a node-driven body
applies `result.motion` with setTransform. The loop runs in the spatial
core (`alloy/src/spatial/mover.rs`, one FFI call per body per frame; the
2d layer's mover is the same loop), the JS side only packs the volume
and maps hits back to meshes. It pushes the body out
of anything it starts inside, sweeps, stops a skin short, slides the
rest along the contact plane up to `maxSlides` times, then snaps down
onto a floor within `floorSnap` unless the motion rises - and the
`floor` it reports is the one it ENDS on (within `floorSnap` below), not
one it touched on the way. `MoveOptions`
adds `up`, `floorMaxAngle` (45 degrees: flatter is floor, steeper a wall
the body slides down), `maxSlides` (6), `skin` (0.01) and `floorSnap`
(0.1, 0 off) to the query filters; the result is `{ motion, floor, wall,
ceiling, hits }`. Gravity is the caller's - fold it into `motion` and
zero it while `floor` is set (the snap keeps reporting the floor while
the body stands still) - and a walkable floor absorbs the vertical part,
so a body never creeps down a slope it can stand on.
`examples/collision.tsx` is the whole pattern: a capsule walker through
`clampPosition`, gravity as a frame loop that runs only while airborne,
the level on a collider layer bit beside its drawn one, pickups lit by
one overlap per move. Deliberately absent and additive when asked: a
step offset (Unity's stepOffset; Godot has none either) and a cylinder
volume (Godot only).

### Pointer events

Pointer events (scene-pointer.ts): the element event model one tree
deeper, with the SCENE as the root of the walk (under a `<View3d>` leaf:
the view). `onPointerDown/Move/Up/Enter/Leave/Wheel/Tap` are plain
fields on any node (and Mesh/Group/Instance props); the nearest hit is
the target - the struck instance of an instanced mesh, else the mesh -
and down/move/up/wheel dispatch there, bubble through the ancestors and
END AT THE SCENE's listeners (`scene.listen({...})`, the `<Scene>`
`onPointer*`/`onWheel`/`onTap` props); over empty space the walk is the
scene alone, with `event.mesh` null (a tap there is the deselect idiom).
`event.mesh` stays the hit (`NodePointerEvent` in the chain,
`ScenePointerEvent` at the root with `mesh` nullable), `currentTarget`
the node whose handler runs (the scene at the end), `stopPropagation()`
stops the walk, and a stopped DOWN claims the whole press: that
pointer's move, up and tap never reach the scene either (the chain still
bubbles). That one rule is how a mesh drags itself under an
`<OrbitCamera>` without the view turning: stop the down, own the
captured moves - the control's pointer feed listens at the root
(`<Scene pointer>` does it, `feedPointer(scene, feed)` by hand) and sees
only what the nodes let through. Capture is per pointerId to the press
target, the scene included (a drag from empty space keeps delivering to
the root as it crosses meshes; a drag from a mesh keeps naming it, with
`point`/`distance` null while the ray misses it). Enter/leave fire on
the struck node alone - a group never receives them - while the root
sees every move, so "hovering empty space" is a root move with `mesh`
null. Taps are synthesized by the dispatch (DOM click, Unity's click
handler): `onTap` fires after the up when the press released on the
target it pressed within the slop (8 window px, core's recognizer slop,
so a press is never both a tap and a drag), was the only pointer down
for its whole press (a pinch never taps), with `tapCount` counting
repeats within 300 ms and 20 px on the same target (a double tap is
`tapCount === 2`); a release on another instance of the same mesh is no
tap. Wheel walks like a move with `deltaX/deltaY` (a node stopping it
keeps the zoom out). Every event carries `native`, the leaf's element
event, for core's recognizers, and the element fields (pointerId,
pointerType, button, modifiers) plus `x`/`y` in scene pixels
(screenRay's input - a drag plane is one intersection away). Root
listeners all run, in registration order (the root is the last stop,
nothing is left to claim). Wiring: the built-in `<Scene>` leaf carries
`scene.handlers` automatically (opt out: `events={false}`, which throws
at mount together with a `pointer` feed - the feed would listen at a root
no event reaches; the same on `<View3d>`); an `output`
leaf or imperative composition spreads `{...scene.handlers}` onto the
element showing the texture. `scene.handlers` assumes that leaf is LAID
OUT at the target size - true for the built-in leaf and a d-texture at
natural size, under any ancestor transforms or design-size fits (the hit
test undoes them; localX/localY arrive in the leaf's layout frame). A
leaf laid out at a different size (the supersampling pattern) uses
`scene.handlersFor(() => ({ width, height }))` with its layout size.
checks/dispatch-check.ts pins the walk, claiming, capture, hover, wheel
and tap rules headless; examples/pick.tsx is the live guard.

### Geometry generators

Geometry generators take ONE options object, every field optional with
a default, named as Three names them: `box({ width, height, depth })`
(1x1x1); `plane({ width, height })`, `circle({ radius, segments })` and
`ring({ innerRadius, outerRadius, segments })` (XY, facing +z - rotate
`[-Math.PI/2, 0, 0]` for a floor); `sphere({ radius, widthSegments,
heightSegments })`; `cylinder({ radiusTop, radiusBottom, height,
radialSegments, heightSegments })` (y axis, capped; unequal radii taper
it) and `cone({ radius, height, radialSegments, heightSegments })`;
`capsule({ radius, height, capSegments, radialSegments, heightSegments
})` (y axis, `height` the TOTAL extent like
cylinder's and Godot's/Unity's where Three's is the middle section
only, so `height: 2 * radius` is a sphere and less throws; its collision
volume is `{ a: [0, -(height / 2 - radius), 0], b: [0, height / 2 -
radius, 0], radius }`); `torus({ radius, tube,
radialSegments, tubularSegments })` (lying flat, hole on the y axis) and
`torusKnot({ radius, tube, tubularSegments, radialSegments, p, q })`
(standing y-up) - both oriented for the y-up world, unlike Three's z-up.
No positional form: `box()` is the default cube, `box({ label: "rock" })`
names it. The polyhedra: `tetrahedron`, `octahedron`, `icosahedron` and
`dodecahedron` (`{ radius, detail }`, radius 0.5 like sphere where
Three's is 1) and the generic `polyhedron(vertices, indices, { radius,
detail })` over Three's data form (flat xyz list, CCW triangle list, the
data first like `box3Helper`), each solid projected onto its
circumsphere. `detail` splits every edge `detail + 1` ways first, so
`detail: 0` is the flat-shaded solid (face normals) and anything above a
sphere of uniform triangles with radial normals: `icosahedron({ detail:
3 })` is the icosphere, no pole pinch. Non-indexed like Three, every
triangle owning its three vertices; `edgesGeometry` welds by position,
so `edgesGeometry(dodecahedron())` is the twelve pentagons (the fan
diagonals are coplanar and drop out) where `wireframeGeometry` shows the
triangulation. UVs are the spherical map with Three's seam patch,
stretched toward the poles: the textured sphere stays `sphere()`, the
polyhedra are for flat-shaded, low-poly and procedurally shaded looks
(examples/polyhedra.tsx). Every options object (the profile kit's `extrude`/`lathe`/
`sweep`/`tube` too) also takes `label` and `layout` - `layout` makes the
generator emit that layout in one pass (base channels written, the
extra slots zero), so `box({ layout: "colored" })` then
`fillColors(g, fill)` builds colored geometry without the
generate-then-repack copy; the result is byte-identical to
`withColors(box(), fill)`. `packGeometry(verts, indices, options?)` is
the tail every generator ends in, for your own generators.
`withAttribute(geometry, attr, fill, label?)` derives a copy of any
geometry (generator or hand-built) with one more channel after its
current layout; the source is untouched. `withColors(geometry, fill,
label?)` is the aColor float32x4 case, keeping the "colored" preset
name (`withAttribute` with `unorm8x4` is the 4-byte color).
`fillAttribute(geometry, name, fill, first?, count?)` is the in-place
primitive under both: overwrites one channel the geometry's layout
already carries (withAttribute ADDS one), reading pos/normal/uv from the
buffer itself - so a builder that bakes transforms while writing hands
the baker world-space vertices. `fill` indexes relative to `first`.
`fillColors(geometry, fill, first?, count?)` is its aColor spelling.

### Geometry as data

Geometry as data: `transformGeometry(geometry, { position?, rotation?,
quaternion?, scale? }, label?)` bakes a placement into a copy (the
setTransform shape: Euler XYZ radians or a quaternion, number = uniform
scale), positions through the matrix and normals through its
inverse-transpose, renormalized - correct under non-uniform scale; uvs,
colors, indices and layout copy through. `mergeGeometries(parts, label?)`
concatenates parts into one geometry with offset indices (uint32 past 64k
vertices); parts must share one layout, a mixed list throws. Together
they collapse a static scene to one mesh per material - transform each
part into place, merge, draw once - so only what actually moves keeps a
node, a draw entry and a per-frame `uModel` write of its own. Merging
does not tax picking: a merged geometry's raycast narrowphase runs
through its triangle BVH (built on the first ray), so merge for draw
count without giving up ground queries. Both are
pure array math (Three's `applyMatrix4` + `mergeGeometries`), no GPU
call, and the source geometries are untouched. `wireframeGeometry(geometry,
label?)` and `edgesGeometry(geometry, thresholdAngle?, label?)` (Three's
WireframeGeometry/EdgesGeometry) build `"lines"` geometry over a triangle
geometry's OWN vertex array and layout, shared by reference: every edge
once for the wireframe, for the edges only those where two faces meet at
the threshold (degrees, default 1) or more plus the open borders. Edges
are matched by position, so a uv seam or a per-face normal split draws
one line; a generated round shape is faceted and keeps its facet lines
until the threshold passes 360 / radialSegments. The wireframe of a
"skinned" part draws in its pose under `unlit({ skinned: true })` with
the mesh's palette, and the swap onto a live mesh is
`setGeometry` + `setMaterial` (`examples/wireframe.tsx`). Lines are one
pixel wide on GL ES; thick lines are quad geometry. The debug helpers are
`"lines"` builders too, Three's helper classes camelCased:
`gridHelper({ size?, divisions?, color?, centerColor? })` is the XZ grid
at y 0 (defaults 10 by 10, Three's grays; the two lines through the
origin take `centerColor` and exist only for an even `divisions`),
`axesHelper({ size? })` the X red, Y green, Z blue triad from the origin,
`box3Helper(bounds, options?)` the twelve edges of a
`[minX, minY, minZ, maxX, maxY, maxZ]` box in local space, and
`planeHelper({ size? })` a square outline with its diagonals and a unit
normal tick in the XY plane facing +z, placed like `plane()`, and
`arrowHelper({ length?, headLength?, headWidth? })` an arrow along +y
with a pyramid-outline head, aimed by the node's rotation
(`quatFromTo(out, [0, 1, 0], direction)`), and `capsuleHelper(volume,
{ segments? })` the outline of a `{ a, b, radius }` collision capsule
(rings at both ends, four lines, two half circles per cap; `a == b`
draws a sphere's three great circles) in the volume's own space, the
gizmo Unity and Godot draw for a capsule collider. Godot and
Unity keep these in the editor and Three makes them scene objects; here
they are plain geometry a node places and a material draws. The grid and
the triad are "colored" layout (sRGB colors in, premultiplied linear
aColor out, the material contract) drawn by `unlit({ vertexColors:
true })`; a material that reads no aColor draws them in its own color.
The box, the plane, the arrow and the capsule are base layout. For a bounds box that moves
every frame, draw `edgesGeometry(box())` on a node whose position is the
box center and whose scale is its size and update the transform (Three's
Box3Helper does exactly that) instead of rebuilding.
`examples/wireframe.tsx` draws a grid, the triad and the rover's bounds
box. `geometryBounds(geometry)`
returns the cached local AABB `[minX, minY, minZ, maxX, maxY, maxZ]`, and
`rayBoxDistance(ox, oy, oz, dx, dy, dz, minX, .., maxZ)` is the picking
slab test (entry t >= 0 in units of the direction's length, 0 from
inside, -1 for a miss) - for ray-testing boxes you keep yourself
(triggers, collision volumes) without meshes you do not want to draw.

Normals as data: two ops, split by whether the vertex count survives.
`computeVertexNormals(geometry)` recomputes a triangle geometry's
normals IN PLACE from the faces that name each vertex by index (Three's
`computeVertexNormals`, Unity's `RecalculateNormals`, Godot's
`generate_normals`, all in place), each face weighted by its corner
angle so the result does not depend on how a surface was triangulated.
The math runs in the core in one call over the vertex floats (so aNormal
must be float32x3; a packed normal is an authoring format, pack after
withNormals), and it returns the stream carrying aNormal, which
`updateVertices` re-uploads: the CPU deformation loop is
`fillAttribute(g, "aPos", ...)`, `computeVertexNormals(g)`,
`updateVertices(g)`. That loop is for data that genuinely changes on the
CPU - a streamed cloud, a cloth solved in JS, an editor edit. A
per-frame ripple, sway or wave belongs in a vertex shader: displace and
derive the normal in the vertex stage (`shaderMaterial`), and no
per-vertex work touches the CPU at all; the fillAttribute callback alone
is an interpreter call per vertex per frame. A vertex shared across faces
shades smooth and a split one per face, so a merged or deformed
generator geometry comes out right; a uv seam's copies each see one side
and the seam shows (Three's artifact too). `withNormals(
geometry, creaseAngle = 60, label?)` is the authoring path and a COPY:
faces are matched by position, so a triangle soup, a hand-written face
list, a merged result or a geometry with no aNormal channel at all (one
is added, float32x3) all work and a seam shades smooth across it; 0 is
flat shading, 180 smooths everything, 60 (Three's `toCreasedNormals`
and Unity's import smoothing angle) keeps hard edges hard. It is
`toNonIndexed` + per-corner normals + `mergeVertices`, so the result is
indexed and split only where a crease or a seam needs it (a box stays
24 vertices under 90 degrees). Both pieces are exported:
`toNonIndexed(geometry, label?)` gives every index its own vertex
(identity indices; per-face colors via fillAttribute, flat shading via
computeVertexNormals), `mergeVertices(geometry, tolerance = 1e-4,
label?)` welds vertices equal in every channel back together, the exact
inverse (a normal or uv that differs keeps them apart). Neither takes a
morphed geometry: split or weld before `withMorphTargets`.
`normalsHelper(geometry, { size?, color? })` draws one line per vertex
along its normal (Three's VertexNormalsHelper as a static builder,
"colored" layout, red by default, drawn by `unlit({ vertexColors: true
})` under the mesh's node), the way to look at any of this;
`examples/normals.tsx` shows the three crease angles on one soup, a
rippling sphere on the per-frame loop, and the helper refreshed in place
with the same loop. Three's `center()` is `transformGeometry(g, {
position: [-(b[0] + b[3]) / 2, -(b[1] + b[4]) / 2, -(b[2] + b[5]) / 2]
})` over `geometryBounds`; there is no bounding sphere op, the box is
what every query and the LOD read.

### Profile kit

Profile kit (2D outlines to solids, real texture UVs): a `Profile` is a
closed XY polygon, bare `[x, y]` points crease, `{ p, smooth }` points
share an averaged normal - `fillet(points, radius, segs?)` and
`roundRect(w?, h?, radius?, segs?)` emit those (arc corners smooth).
Winding is normalized, so either authoring direction works.
`extrude(profile, { depth, bevel, bevelSegments })` sweeps along z,
centered, with a quarter-round bevel at both rims; `lathe(profile, {
segments, angle, start })` revolves a CLOSED (x = radius, y = height) profile about the y
axis - watertight by construction, flat caps on partial sweeps;
`sweep(profile, path, options?)` runs the profile along an open 3D polyline with
MITRED joints (each cross-section sits on its bend's bisector plane, so
bends never gape or overlap) and flat caps at both ends. The path
mirrors the profile convention: bare `[x, y, z]` points crease (a strap
folding over an edge), `{ p, smooth }` points shade continuous (tag a
sampled curve's points); the profile's y starts as close to world up as
the first segment allows, then parallel-transports without spinning.
Closed loops are NOT supported yet - overlap the ends by a segment to
fake one. `tube(path, { radius, radialSegments })` is the round-profile
shorthand (wire, rope, pipe), and `pathFrames(path)` exports the
per-segment frames (tangents, cross-section axes, arc lengths) for
custom work along a path. `polygon(profile, options?)` fills one flat (facing
+z, like circle); `triangulate(points)` is the ear-clipping core (fan
fallback, never drops a cap), exported for custom flat work. These pick
uint16/uint32 indices by vertex count automatically.

### Materials

#### unlit

`unlit({ color?, map?, vertexColors?, transparent?, blend?, cull?, alphaTest?, fog? })` -
straight `[r, g, b, a?]` 0..1 sRGB (decoded to linear light, see Color
below), premultiplied internally; `vertexColors: true` multiplies by the
"colored" layout's aColor (the geometry must carry it: withColors, a
gridHelper or axesHelper); `blend` the factors of a transparent draw,
"alpha" when absent, `"add"` for a glow (any mode but "none" implies
`transparent` unless told `transparent: false`, the shaderMaterial
rule); `cull` and
`alphaTest` as on phong (a mapped cutout casts its cutout); `fog: false`
opts out of the scene's fog (all four library materials take it).

#### sprite

`sprite({ color?, map?, transparent?, blend?, billboard? })` - unlit on a quad
that turns to face the camera IN THE VERTEX STAGE (off the shared
uCamRight/uCamUp, or uCamPos for `billboard: "fixed-y"`, which yaws
only and stays upright on world y - Godot's BILLBOARD_FIXED_Y, the
tree/character sprite; the default `"full"` is Three's Sprite, flat to
the screen). No per-frame JS however many sprites. `transparent`
defaults to TRUE here (cutouts; Three's SpriteMaterial default), cull is
off. Draw with `createSprite(material)` / `<Sprite>`: a Mesh over a
shared unit plane, no geometry argument, `scale` = world size, rotation
ignored. Picks by a unit box around its center (its reach at any
facing), so hits carry no normal/face/uv. `examples/sprites.tsx`.

#### shaderMaterial

`shaderMaterial({ vertex, fragment, params?, textures?, depth?,
depthWrite?, blend?, cull?, label? })` - your own GLSL, the
custom-look escape hatch. The STANDARD UNIFORM SET: the vertex stage
MUST declare and use `uniform mat4 uModel` (the mesh's world matrix,
per entry) and `uniform mat4 uViewProj` (the camera, shared
target-level params) - transform with
`uViewProj * uModel * vec4(aPos, 1.0)`; a source missing either throws
at shaderMaterial() creation. The rest is opt-in by declare-and-use:
`uniform vec3 uCamPos` (the camera's world position, shared and written
with uViewProj - the specular/fresnel view vector is
`normalize(uCamPos - worldPos)`), `uniform vec3 uCamRight` / `uCamUp`
(the camera's world-space view axes, shared likewise - a billboard is
`center + uCamRight * x + uCamUp * y`; do NOT rebuild them from
uViewProj rows, that carries the clip flip), `uniform mat4
uInvViewProj` (the camera's inverse view-projection, shared likewise -
a clip position back to world, the world-space ray through a pixel
without knowing the projection) and `uniform mat4 uNormal` (the world
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

#### shaderMaterialClass

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

#### Instanced materials

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
(the layout, a byte mirror, a mesh-owned GPU buffer), written through
`instanceAttribute(mesh, name)` - the accessor `geometryAttribute`
returns, record index in, values as the shader sees them, the codec
packing the bytes - and published with `updateRecords(mesh, { stream?,
first?, count? })`, which is `updateVertices` for records: one coalesced
buffer write per dirty stream at the scene's sync, so ten moved records
of ten thousand cost ten. Its meshes come from `createInstancedMesh`
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

### Instancing

Instancing - one draw entry covering a population, in two forms. The
axis between them is WHERE MOTION IS COMPUTED, the same split as
@solidrt/2d's sprite and record layers.

FIRST, whether to instance at all. An instance costs the GPU a fixed
per-instance setup, so records buy cheap MOTION, not cheap VERTICES:
below roughly a hundred vertices per instance that setup dominates, and
a plain geometry with a bigger vertex buffer draws faster. A point cloud
is the extreme case - one vertex per point in an ordinary indexed
`topology: "points"` geometry, no instancing, measured at 4x the frame
rate of the same points as records. Packing more points per record does
not recover it: every vertex of an instance fetches ALL of that
instance's attributes, so attribute bandwidth grows with the packing.
Reach for instancing when each instance is a real object (a ship, a
tree, a crowd member), not to submit a lot of vertices. The tell in
`/gpu` is a large `instanceCount` beside a tiny `indexCount`.

#### createInstancedMesh

`createInstancedMesh(geometry, material, { capacity?, bounds?, label? })`
draws the geometry once per instance NODE: `addInstance(mesh,
transform?, parent?)` returns a scene node (kind "instance") placed
inside the mesh - setTransform/setTransition/setVisible, lookAt,
worldPosition, pointer handlers and children of its own all apply - whose
matrix RELATIVE to the mesh the spatial core writes into the record
buffer (`bindMatrixRecord` anchored on the mesh node: one coalesced
buffer write per flush however many instances moved, so native
transitions and clip players move instances with zero per-frame JS).
The material declares `INSTANCE_MATRIX_ATTRIBUTES` as its first instance
buffer (four vec4 columns, 16 floats; anything else throws at creation)
and its vertex
stage splices `INSTANCE_MATRIX` from `@solidrt/3d/glsl`: `uModel *
instanceMatrix() * vec4(aPos, 1.0)`, normals through
`instanceNormalMatrix()` (Three's derivation, exact for any rotation and
scale at no per-instance data; a sheared hierarchy - a non-uniformly
scaled group above a rotated instance - takes
`transpose(inverse(mat3(instanceMatrix())))` instead) - or is a stock
material with `instanced`. A second instance buffer, when the material
declares one, gives every instance a STYLE record beside its matrix
(`MeshInstances.streams[0]`): an app-owned stream in the material's
layout, written per instance with `setInstanceStyle(instance, values)`
(one value per attribute component, encoded by the codecs) or by name
through `instanceAttribute` + `updateRecords`, into a JS mirror and
published as ONE coalesced buffer write per stream at the scene's sync
(a frame-rate path, like setTransform); a fresh or recycled slot starts
from the material's `instanceStyle` (white under `instanceColors`, zeros
otherwise); a third buffer is a further stream, reached by name only;
under `alphaTest` the cutout shadow
multiplies by the same color, so an instance faded below the cutoff
casts nothing, like its pixels (Godot's alpha scissor holds in its
shadow pass too). The core's pose buffer and the app's style buffer are
@solidrt/2d's split one dimension up. Slots are fixed
for an instance's life and recycle on `destroy` (at the free: an
instance cannot exist outside its mesh, so the generic add/remove throw
on one), the drawn count is the slot high-water mark,
and past `capacity` (default 64) every buffer doubles into a
replacement, with the live matrix records retargeted in one core call
and each stream's mirror republished. `parent` may be a group inside the mesh's subtree (a squad
in a fleet): the record stays mesh-relative through it. Picking is per
instance: every instance node carries the geometry's bounds and
triangle shape, so `Hit`, `Overlap` and `Impact` name the `instance` and
pointer events bubble from it (`event.instance`) through the mesh to its
ancestors. `bounds` on the mesh is optional here: without it the mesh
culls by the union of its live instances' boxes (following them, hidden
ones included) and sorts by its node position; with it the explicit
box does both. Three's InstancedMesh count
constructor over Unity's one-transform-per-instance model; Godot's
MultiMesh is the record form. The components: `<InstancedMesh capacity>`
with `<Instance position style transition onPointerDown>` children
(`<Group>` children are squads, an `<Instance>` is a parent too);
`examples/fleet.tsx` is a thousand of them springing between formations
with tap-to-tint.
Mount cost is the one difference between the faces: an `<Instance>` row
costs about eight times an `addInstance` call (the component machinery -
a Solid row, two effects, the style effect; a node component's context
provider is built only when it has children, the biggest single
saving), so a few thousand
mount in tens of milliseconds either way; populations spawned per frame,
or in five figures, belong to the function face.

#### createRecordMesh

`createRecordMesh(geometry, material, records, count?, { bounds?,
label? })` is the raw form: `records` is the per-instance data laid out
in the material's first instance layout - a Float32Array over an
all-float layout, otherwise bytes built with `vertexView(layout, buffer)`
and `attributeAccess` like a geometry stream (a length that is not whole
records throws) - copied into the first stream's mirror, whose capacity
starts at the records given; every further instance buffer the material
declares is a zeroed stream of the same capacity, reached by name
through `instanceAttribute`. `count` picks how many draw (default all).
`setRecords(mesh, records, count?)` rewrites from the start (count
defaults to the records written; more than capacity GROWS: capacity
doubles into replacement buffers, the entry is re-pointed via
`setDrawBuffers`, the old ones are freed), `instanceAttribute` +
`updateRecords(mesh, { first, count })` rewrites a few (the range is
against capacity, so write ahead and dial after), and
`setRecordCount(mesh, n)` is the population dial (clamped to capacity;
frame-rate-safe). Records are opaque data (position/yaw/tint/whatever
your shader reads), so the
library cannot know where they place instances: a record mesh has NO
picking leaf unless you pass `bounds` (local, covering the population) -
then it picks and transparent-sorts conservatively as one box. The
escape hatch for motion only JS can compute at scale (a particle sim, a
crowd stepped in a worker).

#### Common to both

Everything mesh works on both: setTransform moves the whole population
through one uModel, setVisible zeroes the drawn count and restores it on
unhide, renderOrder/params/geometry/material swaps apply, and
`disposeInstances(mesh)` detaches and frees the record buffers - the one
explicit free, geometry-buffer rule. `examples/fleet.tsx` (instances,
components) and `examples/instanced.tsx` (records) are the live proofs.

### Background

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

### Cube-map convention

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

### Environment

Environment: `scene.setEnvironment({ cube, intensity?, rotation? } |
null)`, the `environment` option on createScene and the reactive `Scene`
prop - the cube map every `standard` material is lit by (always: the
split sum `envRadiance` at its roughness times PBR's `envBrdf` for the
specular, `envIrradiance` - the fully rough sample along the normal, as
Three's getIBLIrradiance and Godot read it - added to the hemisphere
for the diffuse) and every `phong({ reflectivity })` material mirrors,
typically the skybox's own cube turned with it. The cube to use is a
BAKED one: `bunx srt tool 3d/environment sky.hdr -o assets/sky.srte`
turns an equirectangular Radiance .hdr (Poly Haven's are CC0) into the
six faces plus the GGX-prefiltered mip chain in linear float, and
`await loadEnvironment("assets/sky.srte")` uploads it as an explicit
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
A 128 environment (2 MiB, the default) lights any surface; for a
mirror-finish showpiece bake at 256, and for a crisp backdrop pair it
with a separate hi-res LDR skybox (a 2k panorama through equirectToCube)
as `background` while the .srte stays the `environment`. Scene-level like Three's
`scene.environment`, Unity's environment reflections and Godot's
sky-lit reflections: ONE `uEnv` samplerCube bound on every target the
scene draws into (a 1x1 black placeholder while unset) and one
shared-params write (`uEnvIntensity`, `uEnvRotation`, `uEnvOn`), however
many meshes reflect; no per-material envMap (Three's Basic/Phong
`envMap`) - a custom material composes ENVIRONMENT from
`@solidrt/3d/glsl`. `reflectivity` 0..1 is the
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

### Reflection probes

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
with the same roughness-to-level rule as a .srte chain), so `standard`
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

### Baked sky

Baked sky: `scene.bakeBackground(size?)` is a reflection probe at the
origin that sees no mesh (layer mask 0): the scene's background - the
GLSL sky or the skybox - alone on its six faces, LINEAR (a sky fragment writes light, so the bake stores exactly
what the backdrop draws), prefilters it like a probe and returns the chain's
TextureId for `environment={{ cube }}`: Godot's sky-to-radiance bake, so
a procedural sky lights the scene with no light nodes, and the runtime
way to turn a hi-res LDR skybox into a properly convolved environment
(the `mipmap: true` box chain above is the sharper, cheaper
alternative). A snapshot at the probe format (half float where
renderable; default size 128): bake again when the sky changes, and
destroy the old cube - it is not auto-freed (an environment normally
lives as long as the app). A buffer is UNCLAMPED (the clamp sits in the
resolve's encode), so a sky's sun disc of 40.0
bakes as 40.0 and shows as the broad bright highlight on rough metal
that HDR is for. A sky that reads
`uCamPos` bakes from the origin; scene params (an app clock) are seen as
of the call. `examples/sky-lit.tsx`.

### Panoramas

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
decodeHdr, panoramaToCube, prefilterCube, the .srte encode/decode) is
pure TypeScript and bun-tested - there is no runtime .hdr decoder.

### Fog

Fog: `scene.setFog(fog | null)`, the `fog` option on createScene and
the reactive `Scene` prop, in Three's two shapes: linear `{ color, near,
far }` (`Fog`; fades from near to far, fully fogged past far) or exp2
`{ color, density }` (`FogExp2`, Unity's default; `1 - exp(-(d *
density)^2)`, no start band, never quite opaque - 0.01 is ~63% at 100
units). Either form takes `height` + `heightFalloff` (Godot's fog
height, Unreal's height falloff): full fog at and below `height` (world
y, default 0), thinning by `exp(-(y - height) * heightFalloff)` above -
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

### Color

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

### Lighting GLSL

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

### Custom looks: three tiers

A custom look is a citizen of the scene - lit by its lights, shadowed
and fogged like the stock materials, exposed and tone mapped by the same
resolve - at one of three tiers, top first:

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
2. A SURFACE FUNCTION inside the stock fragment. `phongFragment({ surface,
   prelude })`: `prelude` is file scope (uniforms and helpers; a uniform
   it declares is an ordinary `instance()` param), `surface` declares
   `void surface(inout Surface s)`, called once the program has filled
   the Surface struct from its options (base from uColor, the map and
   the vertex color; the normal, bent by the normal map; emissive,
   ambient, the light model's fields) and before it shades. Rewrite any
   field or `discard`; it reads the varyings, the declared uniforms and
   prelude's names, and runs in the shadow twin too, so what it discards
   casts no shadow. The struct is the contract, no local of the
   generated program is; colors are linear light, premultiplied
   throughout, `Surface.base` included. The material describes the
   surface, the package shades it (Godot's fragment(), Filament's
   material()). This is the tier for anything whose geometry is not a
   surface: a point splat discarding outside `gl_PointCoord`'s inscribed
   circle and flipping a fitted normal towards the viewer keeps the
   scene's whole light model in about fifteen lines, where hand-rolling
   the light loop (tier 3) would have to match every light's falloff,
   cone and shadow by hand.
3. A FRAGMENT OF YOUR OWN over the scene set. Compose `SCENE` (or
   `sceneSource({ lights, receiveShadow, env, fog })`, each flag leaving a
   declaration out): it declares uCamPos, uHemiSky/uHemiGround, the
   light list, the shadow set, the environment and fog - declare none
   of them yourself. Build a `Surface` with `surfaceOf(base,
   normal)`, set the fields you mean, call `shadeBlinn(s, position)` or
   `shadePbr(s, position)` (premultiplied rgb back: hemisphere, every
   light with its shadow, the environment term, the emissive), add your
   own terms times the alpha, and end with `sceneOutput(rgb, alpha,
   position)` (fog; the pixel is linear light, the resolve exposes, tone
   maps and encodes it). A custom LIGHT MODEL
   loops `sceneLight(i, position, normal)` to `uLightCount` instead of a
   shade function: light i's direction and its color already attenuated,
   cone-faded and shadowed (zero when it cannot reach). The stock
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

### Lights

Lights and `phong`: lights are graph NODES, like Three. `createDirectionalLight({
direction?, color?, intensity? })` / `<DirectionalLight>` is parallel light
travelling along `direction` in the node's LOCAL space (default `[0, -1,
0]`, a sun overhead; length ignored), so a parent Group's rotation turns it
and position/scale do not matter - deliberately a direction, not Three's
position-minus-target. `createSpotLight({ direction?, color?, intensity?,
distance?, angle?, penumbra?, decay? })` / `<SpotLight>` is a cone from
the node's WORLD position along that same LOCAL `direction` (aim by
`direction` or a parent's rotation; place by setTransform): `angle` is
the cone half-angle in DEGREES ((0, 90], default 60 - degrees like
camera fov and like Unity/Godot; Three's radians convert as
`angle * 180 / PI`), `penumbra` the
0..1 fraction of it fading to the rim (default 0, a hard edge), and the
strength falls off as `1 / d^decay` (default 2) windowed to zero at
`distance` (0 = no cutoff) - Three's SpotLight semantics minus the
target object. `createPointLight({ color?, intensity?, distance?,
decay? })` / `<PointLight>` is the omnidirectional version: position
only, same falloff, no cone. `createHemisphereLight({ sky?, ground?, intensity?
})` / `<HemisphereLight>` is the ambient term, a gradient by the WORLD
normal's tilt (fixed to world up, the node's transform is ignored); one per
scene, the last attached wins. Placement goes through setTransform, the
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
`uLightCount` (int), `uLightType[N]` (LIGHT_DIRECTIONAL | LIGHT_SPOT |
LIGHT_POINT), `uLightDir[N]`/`uLightPos[N]`/`uLightColor[N]` (intensity
folded into the color) and `uLightParams[N]` (cosInner, cosOuter,
distance, decay) - so a custom fragment composing `LIGHT_SLOTS` +
`LIGHT_LOOKUP` from `/glsl` reads the same list through `lightVector(i,
worldPos, out l)` (returns the attenuation, 0 = skip the light; `phong`
is the shape), and a light change costs one write however many meshes.
A custom fragment that declares only the old directional subset
(`uLightCount`/`uLightDir`/`uLightColor`) still works - it just shades
every light as directional, so keep such materials to directional-only
scenes. Everything starts black: a lit scene with no light shows
nothing, on purpose, like Three. `examples/lamps.tsx` is the spot/point
shape (soft vs hard cone, casting spots, an orbiting bulb).

### phong

`phong(opts)` is the Blinn-Phong material (Three's MeshPhongMaterial, Unity
URP's Simple Lit) beside `unlit`: hemisphere ambient plus the light list, Lambert diffuse, Blinn-Phong highlight when
`specular` (0..1 strength) is set with `shininess` (default 30), a
mirror of the scene's environment when `reflectivity` (0..1, the
face-on weight; see Environment above) is set, blurred by the same
`shininess`, the same `color`/`map`/`transparent`/`blend`/`vertexColors` as unlit,
`triplanar: n` to sample `map` by world position at `n` repeats per
world unit, blended across the three axis planes by the normal, and
`alphaTest: t` for a cutout (a fragment whose final alpha is below `t`
is discarded; Three's alphaTest, glTF MASK): opaque, depth-written, no
sorting, usually with `cull: "none"` for cards, and `instanced: true` /
`instanceColors: true` for an instanced mesh's population (see
Instancing: the placement, the depth pass and a per-instance tint with
no GLSL; the material then draws populated meshes only). Triplanar
is an OPTION, not the default: generators emit 0..1 UVs per face, so a
map on a plane is a decal (UV) while a map on generated scenery wants one
density across parts of any size (triplanar); the map must be created
with `wrap: "repeat"`. Any `map` on a surface seen at distance also wants
`mipmap: true` at creation, or it aliases as it recedes, and a tiled
surface seen at a grazing angle (a floor, a road) wants `anisotropy: 4`
or more beside it, or trilinear smears the far half into the mip its long
axis picked (`createModel` uploads its images with both; the device clamps
the level, `limits.maxAnisotropy` reports it).

#### Surface maps

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

### standard

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

## Models

### Three layers

Authored models come in as glTF 2.0 (.gltf with its .bin and image files
next to it, or single-file .glb) and become a Group carrying the file's
node hierarchy, Three's `gltf.scene`. Three layers, use the lowest that
fits:

#### parseGltf

`parseGltf(bytes, resolve?)` - the pure parser (no engine, runs under
bun and on flux): `ModelData` = `nodes` (the retained hierarchy in
pre-order - name, parent index, local TRS; matrix-form nodes are
TRS-decomposed, shear dropped; nodes that carry no part, joint or
animation target anywhere - cameras, lights, unused empties - are
pruned), `parts` (one per mesh primitive, its node's NAME kept, `node`
index, vertices in the base layout LOCAL to the node - except
skinned parts: "skinned" layout, model-space bind pose, `skin` index),
`skins` (joint node indices + inverse binds), `clips` (the animations
as baked channel buffers: node/path/interpolation, times, values),
`materials` (base color factor, `map` =
index into `images`, `doubleSided`, `transparent` = alphaMode BLEND,
`alphaMode` as written and `alphaCutoff`, spec default 0.5, the
normal and emissive slots, `metalness`/`roughness` factors and the
packed `metalnessRoughnessMap` - standard's inputs), `images`
(the encoded PNG/JPEG bytes, undecoded) and `bounds` (world-space rest
pose: a part's box through its node, a skinned part's per-joint boxes
through the joints' rest transforms - where the skin places it, armature
scale included; conservative under rotation). External
files come through `resolve(uri)` (uri as written, still
percent-encoded; `gltfExternalUris(bytes)` lists them so an async
caller can read them first) - for a .gltf AND for a .glb, which is
usually self-contained but may legally reference external images
(real exporters do); data: uris need no resolver. Missing
normals produce FLAT shading (the spec's rule): the primitive is
un-indexed, one vertex per corner. A mirroring node chain (negative
rest-pose world determinant) flips the part's index winding so
`cull: "back"` still keeps the outside. Non-triangle primitives are
skipped; a required extension the parser does not implement throws
naming it, and Draco or meshopt compression throws "re-export without
mesh compression" - Blender exports Draco by DEFAULT, so that is the
first error a real file hits.

#### createModel

`createModel(data, { material?, label?, autoFree? })` - uploads the images (repeat
wrap, mipmapped, 4x anisotropic), makes one material per glTF material (default `standard`
with the file's color, maps, normal scale, metalness/roughness and
packed map, emissive and transparency - the glTF material model, so a
scene showing a model wants an `environment` (a glTF metal in a scene
with none renders near black); `material: (m, maps, skinned) =>
phong({ color: m.color, map: maps.map ?? undefined, skinned })` for the
Blinn-Phong look, or any other material; it is called once per material and shared), the node table as nested
Groups with the file's local TRS, and one mesh per part under its node,
all inside the returned `Model` (a Group): `add(scene.root, model)`,
place it with `setTransform`, find parts by name in `model.parts`
(`{ name, mesh }`), spin a wheel relative to its axle through
`model.nodes` (`{ name, node }` in table order, parents first; names
repeat when the file's do - `.find()` yours), `model.bounds` for
framing a camera. Skinned parts get the `skinned: true` material
variant and hang off the model ROOT (the spec ignores their node's
transform; the palette places them - see the mixer below). `dispose()`
detaches it and frees the geometry buffers and textures - the model owns
them, nothing else frees them.

#### loadGltf and loadModel

`loadGltf(path)` / `loadModel(path)` - read from `assets/` with flux:fs
and build. Like every resource creator, a model frees with the owning
reactive scope (`dispose()` at its cleanup; `autoFree: false` opts out):
for the loaders that scope is the caller's at the call, captured before
the await, so a model loaded inside a memo goes with the memo's rerun
and one loaded in a component with its unmount. A load that settles
after its scope is gone (unmounted mid-load) is freed on arrival and
resolves already disposed - nothing owns it any more. `loadModel` reads the baked `.srtm` written by `srt tool
3d/model <in.gltf|glb> -o assets/<name>.srtm`: the same parse run once
under bun, stored in the GPU layout, so loading is views onto the file's
bytes plus the image decodes. Numbers from a 32k-vertex, 6-texture model
on a release client: `parseGltf` 124 ms on flux (22 ms under bun) against
40 ms for the whole baked load - the runtime parse is fine for small
models and a binary import (`import bytes from "./x.glb" with { type:
"binary" }` then `createModel(parseGltf(bytes))`, see
`examples/model.tsx`); bake anything big.

### Async loading

Loading is async everywhere but the binary import: loadGltf/loadModel
return promises, and the async value must be read the way Solid 2 async
works - inside a tracking scope whose result the JSX reads back, under a
`<Loading>` boundary. The worked shape is `examples/model-load.tsx`: the
component keeps the async read in a memo (`let loaded = createMemo(() =>
loadModel(path))`), derives everything - framing, mounting - in a second
memo that reads `loaded()` FIRST and returns the scene JSX, and returns
only that memo read; the window/view shell lives in the parent, above the
boundary. Reading the value in the component body instead throws
PENDING_ASYNC_UNTRACKED_READ, and any element the component builds before
the suspending read is orphaned on the boundary's retry and never freed
(the dev leak sentinel reports it) - so the suspending component creates
no elements of its own. Async here means the file read: the parse and
createModel run synchronously on main. Bake anything big to .srtm; when
a source glTF must be parsed at runtime, do the parse in an isolate
(parseGltf's result is plain data and copies across) and keep
createModel on main.

### Placement and sockets

Placement: pieces of one authored set (a body and its fitted cosmetics)
export in one world space, so composing them is `add(group, model)` per
piece and nothing else - no placement math. SOCKETED items (a weapon in
a hand) are different: they bind to a joint, so they only land once a
skeleton exists (a rig-less export cannot place them at all). The joint
is an ordinary Group in `model.nodes` - find it by name and `add()` the
item under it; it then follows the pose, mixer-driven or hand-posed,
like any child transform. Two authoring cases: an item authored about
its own socket
origin needs the plain `add()` and nothing more; one authored in the
RIG'S model space needs a socket Group between joint and item carrying
the joint's rest-pose inverse (at rest the item then sits exactly where
authored, posed it follows), since parenting stacks the joint's
transform on top of the authored placement. Skinned PARTS are the one
thing that never needs this: they hang off the model root and the
palette places them.

### Wardrobe pieces

Wardrobe pieces (a hood, a cape, cuffs) are the third case: exported
WITH a skin over the body's joints and WITHOUT clips, so beside an
animated body they hold their bind pose. `bindSkeleton(body, piece)`
drives them from the body's skeleton - Three's SkinnedMesh.bind, Unity's
`bones =`, Godot's shared Skeleton3D. The piece's palette rows re-bind
onto the body's joint nodes (matched by name, case-insensitive) with
the piece's own inverse binds, so the flush writes the body's pose into
the piece's skin and no per-frame code exists; joints with no body
counterpart (a hat's internal bones) and rigid parts hanging off a
matched node are grafted under the matched body joint and ride it. The
piece then hangs under the body at the body's placement (its skinned
vertices are in the body's model space), and its own joint nodes are no
longer posed by anything: read poses from and socket items on the
BODY's joints. The piece's node tree is never composed - exporters
truncate it above the spine or skip an ancestor in the middle, and
copying locals through such a tree hangs joints off the wrong place
while looking right for every piece whose tree happens to match; the
body's world matrices are what a bone matrix needs. A shared joint
whose bind pose differs (a piece exported against another rest pose or
scale) throws at bind instead of rendering wrong. A piece comes off by
disposing it; disposing a body disposes what it wears. Culling follows
the wear: a body joint's box is the union of every skin reaching it,
its own and the pieces'.

### Applied glTF material fields

Applied: `doubleSided` (the default material draws it with `cull:
"none"`), alphaMode MASK (`alphaTest: alphaCutoff`), `normalTexture`
(+ scale; the derivative frame needs no tangents), `emissiveFactor` x
`emissiveTexture` with KHR_materials_emissive_strength folded into the
factor (a zero factor skips the map too - glTF's product rule, emission
off), `pbrMetallicRoughness` factors as `m.metalness`/`m.roughness` and
its packed texture as BOTH `maps.metalnessMap` and `maps.roughnessMap`
(standard's channel-select options; the default `phong` ignores them).
The `material(m, maps, skinned, vertexColors)` callback receives every
uploaded texture by phong()/standard() option name (`maps.map`/
`maps.normalMap`/`maps.emissiveMap`/`maps.metalnessMap`/
`maps.roughnessMap`); `data.materials` is in file order, so the calls
arrive in file order. A primitive's `COLOR_0` lands in its geometry's
aColor (the "colored" layout, or the skinned list plus aColor for a
rigged one; linear and premultiplied as glTF stores it) and the default
material takes `vertexColors: true` for such parts - the callback's
fourth argument says so, and a material shared by painted and unpainted
parts is made once per variant, like the skinned split.
A `.srtm` baked before the material records carried the PBR fields
(file version 3), or before the vertex formats took the WebGPU spelling
and byte counts (file version 5), is rejected by loadModel - re-bake
with `srt tool 3d/model`. A baked part keeps a quantized export's
bytes: u8 colors, u8/u16 joints and normalized weights, u16 uvs land in
their own formats, positions and normals as floats.

### Animation

Animation: `createMixer(model)` plays `model.clips` by name -
`mixer.play(name, { loop?, speed?, fadeMs? })` (fadeMs crossfades: the
named clip fades in, everything else fades out - Unity's CrossFade,
Godot's play-with-blend), `mixer.stop({ fadeMs? })`, `mixer.playing()`,
`mixer.onFinish` for `loop: false` clips (the pose holds at the end).
Playback is CORE-DRIVEN: there is no update() and no frame loop to
register - clips are registered with the spatial core once and players
sample, weight-blend and write joint TRS natively each frame, so a
playing character costs zero JS per frame (gate other work on
`mixer.playing()`). play() requires the model to be IN a scene (players
bind live arena nodes; removing the model drops them - a re-added model
plays again from play()). Three traps that follow from core ownership:
(1) players advance BEFORE your onFrame, which is therefore the
post-animation hook - read a freshly posed joint and overwrite it
(root-motion strips, skeleton copies) in plain setTransform, last write
wins, all published by the same flush; (2) the JS
`position`/`quaternion` fields of player-animated joints (and of the
model node under root motion "apply") go STALE (they hold the last JS
write) - read poses with `getTransform(node)`, which reads the core;
writes to such a node always go through (setTransform skips its
usual equal-value short-circuit there, so a teleport back to the last
JS-written spot is not lost); (3) a channel nothing plays leaves the node's pose
alone; the draw sort follows native pose writes like any move (the core
re-keys what it recomputed), so an animated transparent sorts as it
animates.

### Root motion

Root motion: `play(name, { inPlace? })` strips a clip's root travel
(Unity's applyRootMotion off, Godot's root_motion_track): the root's
x/z hold at the clip's first key and its height rebases onto the root's
rest position, the vertical bob intact. `createMixer(model, {
rootHeight })` rebases EVERY clip onto that height, pinned or not, for
an export whose clips ride above its rest pose. Unset, it is decided per clip by NET DRIFT of the
root's position track - last key minus first, past a fraction of the
model's height - so run cycles play in place and a taunt that roams and
returns does not (pinning THOSE pushes the slide into the feet);
`mixer.travels(name)` reads the verdict. The root is the topmost node
any position channel of the clip targets. The strip is baked into a
second core clip on first in-place play, so it costs no per-frame JS and
crossfades like any other clip. A GAME wants the travel kept and moved
onto the character: `createMixer(model, { rootMotion: "apply" |
"report" })` plays every clip fully pinned (all three root axes held)
and its yaw held too (the turn about +y stripped key by key, so the
lean and pitch of the pose survive), while the core samples the
authored root tracks at each player's time and hands the per-frame
delta on - a translation in the model's local frame and a yaw in
radians. "apply" adds both to the model node itself (Unity's
applyRootMotion: the character walks and turns where its clip says),
"report" accumulates them until `mixer.rootDelta()` takes them
(`{ position, yaw }`), for a controller to spend through its own
movement (Godot's get_root_motion_position). Zero per-frame JS in
"apply"; one read per frame in "report". Loop wraps are continuous
(the clip's net drift is added across the wrap), crossfades weight the
deltas like the poses, and a clip's travel is given in the root's
CURRENT facing (its own turn so far undone), so a turn that wanders
out and back ends where the clip says and two blending clips agree on
the frame. The object form `{ mode, up?, vertical? }` names the root's
parent-space up axis (default +y; the turn and height axis) and
`vertical: "pose"` keeps the height in the pose, delivering only the
horizontal travel (Unity's bake-into-pose Y - for a controller that
owns gravity). `inPlace` is ignored while rootMotion is set. Yaw is
the swing-twist about up (exact under any lean); a cubic rotation
track is linearized at 60 keys/s before its yaw is held. Verified on
Mixamo's standing turns (external/mixamo-turn).

### Skins

Skins need nothing further: each skin's uBones palette
(model-local jointWorld x inverseBind, sized to the RIG - an rgba32f
float texture, 4 texels wide, one row per joint, sampled in the vertex
stage via texelFetch, so there is no joint cap) is composed by the
spatial core at the frame's flush from the joint nodes themselves, in
any write order, and identical skins (a body/legs split, LODs) share one
computed-once texture. `sampleChannel` (pure, from the root) stays the
JS sampling core for checks and custom drivers;
okf/done/animation-core.md records the design.

### Morph targets

Morph targets (blend shapes): a geometry carries named per-vertex
deltas - `withMorphTargets(geometry, [{ name, position, normal? }])`
for a hand-built shape, the loader's primitive targets for a glTF (its
`extras.targetNames`, else `target<i>`; sparse accessors read) - packed
SPARSE BY VERTEX on `geometry.morphs` (a header texel per vertex, then
only the entries of the targets that move it; memory follows the
authored deltas, never vertices x targets) and uploaded once per
geometry as one rgba32f texture. A `morph: true` material (`phong`,
`unlit`, `standard`; a custom class splices MORPH_DECLS and MORPH_APPLY
from `@solidrt/3d/glsl` before its skin math) walks each vertex's
entries in the vertex stage, before any skinning; a mesh under it
whose geometry carries no targets is rejected at add(), and a plain
material over morphed geometry draws the base shape. WEIGHTS belong to
a NODE, glTF's `node.weights` model: a standalone mesh owns its own; a
model's parts follow their glTF node (`model.nodes`), so a face split
into skin, eyes and teeth parts takes one write. `setMorphWeights(node,
{ smile: 0.7 })` writes by name (the others untouched), an array writes
every target in order, the `<Mesh morphWeights>` prop is the
declarative form; `getMorphWeights(node)` reads them back (from the
core while in a scene), `getMorphNames(node)` lists the targets. The
write lands in the spatial core's register (a row of a small weights
texture, published at the flush like a skin palette - Three's
morphTargetInfluences, Godot's set_blend_shape_value); a file's
`mesh.weights` seed it. Weights ANIMATE two ways, both native: a glTF
`weights` animation channel plays through the mixer like a TRS
channel (into the owning node's register, crossfading across players
by the same weighted average), and a write is a TARGET like a
transform write - `setTransition(node, { weights: { duration: 300 } })`
makes every setMorphWeights animate (springs by default; `from`/`exit`
endpoints are one number per target, `delay` and `stagger` apply), and
`setMorphWeights(node, { smile: 1 }, { duration: 300, bounce: 0.2 })`
animates that one write on its own motion, declaration or not; a
settle calls `onTransitionEnd` with component "weights". A playing
weights track and a JS write are both producers of the register: the
players advance before the frame's JS, so a write from onFrame wins
that frame. `getMorphWeights` reads the core, so it shows a playing
track and a mid-flight transition; the by-name partial write merges
into the last JS-WRITTEN set, not the core's - write every target
while a clip plays. An INSTANCED mesh morphs per instance: the
population owns one weights texture with a row per record slot, each
`<Instance>` (or addInstance node) owns a register published into its
slot's row, and the shader reads the row by `gl_InstanceID` - so
`setMorphWeights(instance, { smile: 1 })`, the `<Instance morphWeights>`
prop, a `transition.weights` spring and a clip's weights track all work
per copy, the shadow variants included, with no per-record data
(Three's InstancedMesh morphs). Skinned plus morphed plus instanced
composes. A custom class splicing MORPH_DECLS reads the row as
`uMorphRow + gl_InstanceID`, which the stock decls already do. A record
mesh (JS-written records, no nodes) cannot morph: rejected at add().

### Not in the subset

Not in the subset, dropped: tangents and further UV sets; samplers are
ignored (every texture repeats); additive blending draws as base color.
The follow-ups are filed in okf/backlog/3d-model-loader.md. The `.srtm`
container is VERSION 8 (node table in the header, node-local vertices,
skins, clips, packed morph targets); older bakes are rejected - re-bake
with `srt tool 3d/model`.

## Traps

### Models and skinning

- A model's vertices are LOCAL to their node; the file's placement lives
  in the node-table TRS, composed by the scene like any Group chain. So
  a part's `geometry` alone is at the origin, `model.bounds` (rest-pose
  world) is what frames a camera, and moving a named node moves its
  subtree. The winding flip for a mirroring node chain is baked into the
  index order from the REST pose - re-scaling a node across zero at
  runtime shows mesh interiors, so do not do that.
- A worn piece's joints are dead after `bindSkeleton`: `piece.nodes`
  still lists them, but nothing poses them and their palette rows sit
  on the body's nodes now. Pose, read and socket on the body.
- Skinning is a VERTEX-STAGE effect: everything that runs off the
  retained tree sees the bind pose. A skinned mesh picks by its
  bind-pose triangles at the model root and its transparent sort key is
  the bind-pose box; moving the JOINTS never moves either, moving the
  MODEL moves both. Shadows are the exception: the shadow variants
  (depth and cutout) skin by the same uBones palette, so a caster casts
  its pose.
- Morphing is the same kind of effect: picking and the transparent sort
  see the base shape, the shadow variants morph. The one thing the
  retained side knows is the box: `geometryBounds` (and a model's
  bounds) grow by the targets' extent, so a morphed shape is never
  culled while its weights stay in 0..1 - a weight past 1 can leave it.
  Normal deltas blend linearly like positions (Three's, Godot's and
  glTF's rule): exact at a target, an approximation between two, so a
  half-blend of EXTREME targets can show a ragged self-shadow
  terminator where the lit and shadowed halves disagree; a modeller's
  blend shapes are far too small for it to show.

### Materials and color

- A `standard` metal in a scene with no environment renders near black:
  its diffuse is zero and the black placeholder cube is all there is to
  reflect (Three and Godot render the same; Unity falls back to an
  ambient probe, this does not). glTF's default metallic factor is 1,
  so an untextured asset is all metal, and createModel's default is
  `standard`: give a model scene an `environment` (loadEnvironment's
  baked .srte, or the skybox's cube), or pass `phong` as the material.
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

### Environment and background

- A reflection probe is a mirrored render (x-flipped projection, winding
  inverted by the engine): anything built from screen-space derivatives
  flips with it, so a normal-mapped surface shows its bumps INVERTED in
  a probe's reflection. Known and shared with every engine's mirrored
  views; a `uMirror` sign on the derivative frame is the fix when it
  matters.
- A generated cube chain (`mipmap: true` from six faces) is a box
  filter, not the GGX convolution the roughness-to-level rule assumes:
  rough reflections read too sharp and the diffuse `envIrradiance` is a
  4x4 average. Bake with `srt tool 3d/environment` for anything
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

### Coordinates and rotation

- The y-down clip flip is baked into `perspective()`; scene code and
  geometry are plain y-up right-handed, and CCW-outward winding culls
  correctly with `cull: "back"`. Do NOT negate y anywhere else, and do not
  "fix" the negated row of `perspective()` - both would mirror the winding
  and show mesh interiors.
- Rotation is stored as a QUATERNION (`node.quaternion`, `[x, y, z, w]`,
  always unit). There is exactly one rotation field: no `node.rotation`
  shadowing it, because a second field is a second thing to go stale (an
  aimed node whose Euler triple still reads as the old pose is the bug
  this model deletes). Euler triples are a boundary format only -
  `setTransform({ rotation })` and the `rotation` prop convert in,
  `getRotation(node, out?)` converts out.
- Euler triples are XYZ order (x applied first: `R = Rx * Ry * Rz`),
  Three's `Euler` default, so a triple copied from a Three scene means the
  same thing here. This CHANGED 2026-08-11: the old `compose()` built
  `Rz * Ry * Rx` (Three's `'ZYX'`) while its comment claimed XYZ. Every
  rotation triple then in the repo, examples, demos and projects was
  single-axis, which is order-independent, so the fix moved no pixels -
  verified, not assumed. There is ONE order and no order argument: a
  per-call order is how one triple ends up meaning two things.
- `getRotation` cannot recover the triple that was written, only a triple
  meaning the same rotation (and at the poles it pins z to 0 and folds the
  roll into x). It is for reading and debugging; anything composing or
  interpolating rotations works with the quaternion.
- `eulerFromQuat` extracts y with `atan2(m02, cos(y))`, NOT Three's
  `asin(m02)`: asin's derivative blows up at the poles, turning 1e-16 of
  matrix error into 1e-8 of angle. Same reason its pole branch starts at
  `cos(y) < 1e-7` rather than Three's `|m02| > 0.9999999` (which is
  `cos(y) ~ 4.5e-4` - three orders early, and inside that band Three
  silently discards real roll). Do not "restore parity" here.
- Aim with `lookAt(node, target, up?)`, never by extracting angles by
  hand. Three's `Object3D.lookAt` semantics deliberately: `target` and
  `up` are WORLD space (ancestor transforms are undone, and the ancestor
  chain is refreshed on the spot rather than waiting for the sync), and
  local +z ends up pointing at the target. To aim along a DIRECTION, add
  it to `worldPosition(node)` - the same conversion Three asks for.
  +z is the library's own sweep axis, so `extrude`/`sweep`/`tube` output
  needs no correction. For a y-axis solid (`cylinder`, `cone`) use
  `quatFromTo(q, [0, 1, 0], dir)` instead of correcting lookAt's +z.
  Divergences from Three, both deliberate: `up` is an argument, NOT a
  per-node field (Three's `object.up` is hidden state that costs a vector
  on every node), and degenerate frames pick a stable perpendicular
  instead of Three's epsilon nudge of the eye.
  There is no `setTransform(node, { matrix })`, and lookAt is a MUTATOR,
  not a rotation-returning function.
- `quatFromTo` is Three's `setFromUnitVectors`, renamed after Unity's
  `FromToRotation` / glam's `from_rotation_arc`: the Three name states a
  precondition instead of the operation, and ours has no such
  precondition (it normalizes). Check Unity/glam/Godot too before copying
  a Three name that reads as an artifact of its class layout.
- The composition set: `quatFromAxisAngle` (radians; normalizes the axis -
  Three/Unity/glam all require a unit axis and silently corrupt
  otherwise), `quatMultiply` (same order contract as the mat4 `multiply`:
  `a * b`, b applies first; does NOT renormalize - the unit product only
  drifts under long accumulation, and setTransform renormalizes on
  write), `quatSlerp` (shortest path across the double cover, constant
  angular velocity, unit output; the damped follow is
  `quatSlerp(q, q, target, 1 - Math.exp(-k * dt))`). All aim/verb usage
  live in `examples/aim.tsx`.
- `setTransform` NORMALIZES an incoming quaternion, and passing `rotation`
  and `quaternion` in one call throws. A non-unit quaternion scales
  geometry by `|q|^2` through `compose()` - Three leaves that trap open
  and documents it; we close it at the one write path instead of paying
  for a check in every compose.
- `lookAt` is exact for rotation and uniform scale up the chain. A
  non-uniformly scaled ancestor shears the frame, so the aim is
  approximate - Three has the identical limitation (both read the parent's
  upper 3x3 as if it were a rotation), and the fix is not to special-case
  it here but to not shear parents of things you aim.
- The package root's `lookAt` is the scene verb; `@solidrt/3d/math` keeps
  its own `lookAt` (the camera view matrix) on the SUBPATH ONLY, the same
  collision rule the Vec3 helpers follow - and the same Object3D/Matrix4
  split Three makes under one name. Do not re-export math's from the root.
- Transforms have ONE write path: `setTransform`/`lookAt`/`setVisible` (or
  the props that call them). Mutating `node.position` directly does not
  sync. Components have no `lookAt` prop - aim through a `ref`.
- Vec3/Quat arguments are COPIED IN everywhere (`setTransform`, `lookAt`,
  `setCamera`, params), so ONE scratch array reused every frame is safe -
  allocating three arrays per node per frame is pure waste. The node's own
  `position`/`quaternion`/`scale` are the live arrays: read them, do not
  hand them out and do not mutate them (that write does not sync).
- `setTransform` early-outs on an unchanged value (rotation compared AFTER
  euler conversion), so driving every node unconditionally from `onFrame`
  costs only the compare for nodes that did not move. Compares are exact,
  like `setVisible`.

### Visibility and instancing

- `visible: false` keeps the entry, drawn with `instanceCount: 0` (a
  cheap off switch); unhiding writes 1, or the mesh's own record count
  when it is instanced - never a bare 1 into an instanced entry. Hidden
  meshes skip uModel writes; the fresh matrix is
  written on unhide. A freshly attached entry starts off the same way and
  the core's flush turns it on when it writes uModel - never add one
  live: it has no world matrix yet, and drawn before the sync microtask it
  flashes at the world origin for a frame.
- Instancing pairs strictly at add(), like layout: an instanced material
  needs a createInstancedMesh or createRecordMesh mesh and vice versa,
  and every instance buffer the material declares must be a layout the
  mesh carries - the matrix (INSTANCE_MATRIX_ATTRIBUTES, the first
  buffer of an instanced mesh) or a stream with the same attribute names
  AND formats (a byte-equal layout in other formats decodes differently)
  - each mismatch throws there, at creation for the mesh's own material
  and at add() for a swapped one. The instance buffers are MESH-owned
  (unlike shared geometry buffers): `disposeInstances` is their one
  free, and the mesh cannot be re-added afterwards. Capacity grows by
  REPLACEMENT, never resize: an addInstance or setRecords past capacity
  doubles into new buffers and swaps them in (an instanced mesh's live
  matrix records move in one core `retargetRecords`, every stream's
  mirror republishes) - amortized like a dynamic array, same policy as
  @solidrt/2d; size `capacity` or the initial records to skip the
  copies. A mesh's stream layouts come from its material AT CREATION: a
  later setMaterial to a class with another layout throws at the
  rebuild.
- Record writes are mirrored, not immediate: `setInstanceStyle`,
  `setRecords` and an `instanceAttribute` write land in the stream's
  `data` and the scene's sync publishes the `dirty` range (`updateRecords`
  marks it for accessor writes) - so a write on a mesh outside any scene
  shows once it is added (the dirty range waits), and reading the GPU
  buffer back mid-frame can lag the mirror by one sync. Growth replaces
  `data`: hold the accessor, not the view.
- The sugar is for FEW records, the mirror is for MANY. `setInstanceStyle`
  over an all-float layout is indexed stores; over a packed layout (the
  stock `float16x4` color included) it is a codec call per component,
  and so is every accessor write. Measured at ten thousand instances
  restyled every frame on a laptop under QuickJS: about 11 ms of JS per
  frame through setInstanceStyle on a float layout, 19 ms on the half
  color, 9 ms through the accessor, 5 ms through the mirror - and that
  5 ms is the app's own per-instance tint math, the write itself is
  free. A population restyled per frame writes the mirror in bulk:
  a typed view over `stream.data` (a Float32Array as handed out, a
  `Float16Array` over the stock color, indexed by slot times components)
  filled in a plain loop, then ONE `updateRecords(mesh, { first, count
  })` - zero per-component calls, the same shape as `setRecords`.
- Instanced casters: `castShadow` on a populated mesh needs a depth pass
  with the instance placement in it - the stock materials' `instanced`
  carries one, a custom class needs `shadowVertex` (see shadows below);
  a class without one is skipped by shadow views, silently.
- A record mesh without explicit `bounds` has no BVH leaf: it never
  picks, pointer events never target it, and its transparent sort key
  falls back to the node's world position. That is deliberate - records
  are opaque to the library, so any inferred box would be a guess. Supply
  `bounds` for anything pickable or transparent. An instanced mesh picks
  per instance regardless and culls by its instances' union; its
  `bounds` replace that union and give the transparent sort a center.
- An instance node's own fields hold its LOCAL pose (relative to its
  parent, as for any node); the record the core writes is its pose
  relative to the MESH, composed through any group between them. A
  destroyed instance takes its children with it, like any destroyed
  node. `disposeInstances` on an instanced mesh flushes the
  core before freeing the buffer, so the destroyed instances' hiding
  writes land in a live buffer; keep that order if you ever free a
  record buffer by hand.

### Shadows

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
  is a full extra pass over the casters plus a sampler unit on every
  receiving program (MAX_LIGHTS of those are always bound, placeholders
  included), so cast from the lights that matter, not all of them.

### Views and scene params

- A camera change is ONE `setTargetParams` write (uViewProj + uCamPos are
  target state), independent of mesh count - never reintroduce per-mesh
  camera writes (uEye-style per-mesh params are exactly the O(scene) cost
  the shared channel removed). Scene scale honestly: hundreds to a
  few thousand objects, bounded by the interpreter, not the GPU. A view
  is one more such write per camera change and one more entry per mesh
  at attach; a view's per-frame cost is the core's (one params write per
  sink per moved node), never JS.
- A mesh's entries are mirrored into every view at attach and dropped at
  detach; `setGeometry`/`setMaterial` rebuild them everywhere. An
  `overrideMaterial` is validated against every mesh's layout (at
  createView for the meshes present, at add() for later ones) exactly like
  a mesh's own material, so an override reading `aColor` throws for a
  base-layout mesh. An instanced mesh the override cannot place (it
  declares no `instanceBuffers`) is skipped, which the view's output
  cannot show, so the scene warns once per view naming it and the count
  (over the settled set, at the flush after the attach). Views are disposed by the scene; `view.dispose()`
  only for dropping one early.
- SCENE-WIDE uniforms go through that same shared channel via
  `scene.setParams({ uTime })`, and this is the single highest-leverage
  pattern in the library. It merges an app-owned name in beside
  uViewProj/uInvViewProj/uCamPos/uCamRight/uCamUp and the scene's own
  fog, environment and output sets (uFog*, uEnv*, uExposure,
  uToneMapping) - names merge, a target tolerates
  zero coverage, neither side clobbers the other. One write per frame
  however many meshes read it, with the motion itself in vertex shaders
  off that one clock. `params`/`setMeshParams` is the PER-MESH answer and
  is O(meshes) per frame; reach for it only when the value genuinely
  differs per mesh. (`scene.texture` is the RESOLVE's id, not the buffer the meshes
  draw into, so `setTargetParams(scene.texture, ...)` reaches the
  resolve alone - setParams is the only spelling.)

### Shader materials

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
  (the three after the GLSL exports under "The model"): a stock fragment
  on your vertex stage,
  a `surface` function, or `SCENE` with `shadeBlinn`/`shadePbr` or
  `sceneLight` and `sceneOutput` at the end. Composing `FOG` and the
  `SHADOW_*` trio by hand still works and is no longer the shape.

### Picking, pointer and collision

- Picking is triangle-accurate for ordinary meshes (`point` is a surface
  point, hits carry `face`/`uv`/`normal`) but box-only for instanced
  meshes and for lines/points geometry: there `point` is where the ray
  meets the population `bounds` box (the geometry's box for lines),
  `normal` that face's, and `face`/`uv` are absent. Never present an
  instanced hit as a surface hit. Both tiers run in the spatial core (Rust); never
  add a per-triangle path in JS - rays at mesh scale are
  interpreter-hostile, and the core already does it.
- `scene.handlers` vs `handlersFor`: localX/localY arrive in the leaf's
  LAYOUT frame (every ancestor transform and design-size fit is already
  undone by the element hit test). `handlers` therefore assumes leaf
  layout == target pixels; scaling by `getBoundingBox` would be WRONG -
  the box composes transforms, and it would double-correct the built-in
  leaf under a design size. Only a leaf whose layout size deliberately
  differs from the target (supersampling) needs `handlersFor`, fed the
  layout size the app itself set.
- Hover (enter/leave) reacts to pointer MOTION only: a mesh animating
  under a still pointer fires nothing until the next move - the same
  limit the element hit test has (hit-test-per-frame is an open platform
  item). Do not poll pick() per frame to fake it.
- overlap()/sweep() test SURFACES, the trimesh contract everywhere: a
  volume wholly inside a closed mesh with no triangle in reach touches
  nothing, and a body whose center has passed through a wall reports
  the push-out on the side its center is on. Keep a skin (moveAndSlide
  does) and never teleport a body into geometry expecting it to come
  out the far side.
- moveAndSlide owns no gravity and no velocity: fold the fall into
  `motion` every frame and zero it while `floor` is set. A walkable
  floor absorbs the vertical part (a body never creeps down a slope it
  can stand on), and with `floorSnap: 0` a standing body reports no
  floor unless its motion presses into one.

### Geometry and components

- Per-generator conventions - orientation, UV mapping, which axis a solid
  stands on, what a cap looks like - live on each generator's doc comment,
  not here. They are consistent (`plane`/`circle`/`ring` face +z, `torus`
  lies flat with the hole on y, discs and cylinder caps get a PLANAR disc
  map inscribed in the unit square) but the doc comment is the source.
- Entry rebuild order: `setGeometry`/`setMaterial` re-add the entry at the
  list END and rebind it with the new material's key, so the core's next
  flush re-sorts and the mesh keeps its place.
- `lathe` takes a CLOSED profile (a cross-section with thickness, or run
  to the axis at x = 0) - it is a solid of revolution, NOT Three's open
  polyline shell. An "open" outline must be closed by the author;
  otherwise the shape is simply wrong, there is no open-profile mode.
- `useScene()`/`Group`/`Mesh` throw outside `<Scene>` (default-less
  context).
- Geometry local bounds cache on the Geometry (like its GPU buffers and
  the core's picking copy): mutating `vertices` after a mesh used them
  leaves stale bounds, a stale GPU buffer AND a stale picking shape
  until `updateVertices` publishes the range - write through the
  accessor and call it, or make a new Geometry.
