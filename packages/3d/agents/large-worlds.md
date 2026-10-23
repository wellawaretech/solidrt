# @solidrt/3d - large worlds

Culling, the quality tier, level of detail and residency. Read
../AGENTS.md first: it carries the model this file builds on.

## Culling

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
cull by their quad's reach at any facing; a sprite whose material (or
`params`) floors its screen size exceeds that reach far away, so the
floor coming on switches the mesh's culling off, one way. A SKINNED
part is culled by
the union of its joints' boxes (the bake computes each joint's
influence box in joint space, `ModelSkin.jointBounds`, .sol3m VERSION
5; the joint nodes carry them as culling-only bounds, outside the
picking index), so the box follows the pose with no per-frame JS -
Unity's bone bounds, Godot's per-bone AABBs; no `updateWhenOffscreen`
knob is needed. Probe faces set no frustum (six cameras, one target).
Shadow tiles cull casters against their light frustum, which is what
makes `shadow.distance` and cascades cheaper.

OCCLUSION culling is the second gate, after the frustum: what the
nearest walls cover is not drawn either (Godot's OccluderInstance3D,
Unity's occludees). Occluders are AUTHORED, on a Mesh or a Group:
`occluder` (`setOccluder`) takes `"geometry"` (the mesh's own
triangles - exact, since a triangle occludes exactly what lies behind
it, holes included; the shape for walls, floors and buildings),
`"bounds"` (the mesh's box as a solid, twelve triangles, for a crate
that fills its box), or an explicit shape in node space - `{ box }`,
`{ quad: [width, height] }` (a two-sided wall in the node's XY plane,
Godot's QuadOccluder3D), `{ sphere: { center, radius } }`, `{ geometry }`
(a low-poly proxy). Box and sphere stand for SOLIDS; a mesh whose
triangles move in the vertex stage (skinned, morphing, a sprite, an
instanced population) refuses `"geometry"` and takes a proxy instead.
`occludersFromMeshes(scene, { minSize })` marks every plain mesh whose
box's longest edge reaches `minSize` by its geometry (Unity's static
flag by size) and returns them. The core rasterizes the SHOWN occluders
into a small CPU depth buffer per target (128 texels wide, the target's
aspect) only when the camera or an occluder moved, and switches off
every mesh whose cull box - the frustum test's, `cullMargin` included -
lies wholly behind it; a still scene costs nothing and a scene with no
occluder costs nothing. Conservative to the buffer's texel: a mesh at an
occluder's edge, or straddling the near plane, stays drawn; a crack
narrower than a texel between two occluders occludes as if closed; a
node never occludes itself; an occluder can itself be occluded. A hidden
occluder occludes nothing (an open door). The same occluders block
SOUND (Positional sound in sound.md): the object forms take `audio` - absent
is the default blocking, `false` sight only (glass), `{ gain?, cutoff? }`
its own level and low-pass - and `{ shape: "geometry" | "bounds", audio }`
is the object form of the two shortcuts. Per target: `occlusion` on
createScene and createView (default true; `setOcclusion` live), never
on shadow tiles (a caster behind a wall still casts) nor probe faces;
the quality row has no say, since the fill-bound low row gains most.
The budget is a constant: past 4096 occluder triangles per target per
build the rest are skipped that build (less culling, never a wrong
cull), and `/stats` carries `occludedDraws`, `occluders`,
`occluderTriangles`, `occluderTrianglesDrawn` (the ones that reached
the raster: zero with every occluder behind the camera),
`occluderTrianglesSkipped`, `occlusionBuilds` and `occlusionBuildMs`. WHY a mesh is not drawn is `scene.cullState(mesh)`
(`view.cullState` through a view's camera): "on", or the first of
"hidden", "frustum", "occluded", "lod" that applies, as the core last
decided it (after a settle, what the next frame draws); null while the
mesh has no entry on that target. `examples/occlusion.tsx` is the
shape; `probes/3d-occlusion-probe.tsx` the cost.

## Quality

One tier, every knob. `setQuality("low" | "medium" | "high")` in
@solidrt/core is the app's statement of the device's budget - Unity's
quality level and Godot's `.mobile` overrides as one reactive value
(`quality()`), default "medium", and nothing picks it for you: a GPU
name is not a budget, and a measured default waits on a startup cost
probe. The package maps the tier to a ROW of `QUALITY_TIERS`
(quality.ts), and every quality knob defaults from the row; each knob's
own option stays the override, so what an app sets explicitly never
moves:

| knob | low | medium | high | override |
|---|---|---|---|---|
| shadow kernel (`shadows.quality`) | hard | medium | high | the Scene option |
| light `mapSize` | 512 | 1024 | 2048 | the light's `shadow.mapSize` |
| directional `cascades` | 1 | 2 | 2 | `shadow.cascades`; a light that authored a `camera` box stays one map |
| `lodBias` | 0.7 | 1 | 1.5 | the Scene option |
| LOD cross-fade | off | on | on | none - the row's call |
| `samples` (scene and views) | 1 | 1 | 4 | the Scene / View option, live |
| fill `maxPixels` | 1 Mpx | none | none | the Scene / View3d prop |
| model texture anisotropy | 1 | 4 | 16, device-clamped | none - id state every scene shares |
| model texture mip limit (`baseLevel`) | 1 | 0 | 0 | none |
| probe `size` | 64 | 128 | 256 | the probe option; fixed at creation |

Low is the Adreno 610 tablet and Mali-T860 TV class (the soft kernel
is half a 2 Mpx frame there, MSAA 4x ~9 ms, the fill cap the last
lever), high the laptop class. A tier is a speed choice, never a safety
one: the engine keeps every GPU submission under its own budget (a heavy
draw, a million-splat cloud at the window size on the tablet, is issued
as several submissions sized from what it measures), so a row too rich
for a device is slow, not black (okf/done/splat-adreno-gpu-fault.md). Multisampling is high's alone: on a
Mali phone (Pixel 7) any count above 1 on the half-float buffer costs a
flat ~54 ms per 2.6 Mpx frame, 2x the same as 4x, while rgba8 4x costs
~40% (okf/done/mali-linear-scene-path-doubled.md); okf/done/3d-quality-tier.md
has the rows and cites the measurements behind them. A scene may say its
own: `<Scene quality="low">` is that row whatever the app tier (a
thumbnail scene), `quality={{ samples: 8 }}` a row object over the app
tier; spread a row for a custom level, `{ ...QUALITY_TIERS.high,
samples: 8 }`. `scene.quality()` is the row in force,
`useScene().quality()` its reactive form under the scene, for the app's
own work to scale by. Everything follows live: a tier change rewrites
the light set (the kernel), the LOD bias and fade (target state),
reallocates the multisample storage of the scene and of the views that
set none under their ids (`setTargetSamples`; `scene.setSamples` /
`view.setSamples` are the explicit forms), re-settles the shadow atlas
(map sizes, cascade counts) and re-samples every model texture in place
(`setTextureSampling`); probes keep the size they were baked at (a cube
is create-once; bake again to follow). A `createScene` outside a
reactive owner takes the tier at creation and follows it through its
own `setQuality`. Text, the 2d layers and the UI do not read the tier
yet.

## Level of detail

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
below 1 hands over sooner; default the quality row's (0.7 / 1 / 1.5,
see Quality), which at low also switches every band hard (no
cross-fade: inside a band both levels draw). Picking and collision see the level the queried target draws
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
the shape.

## Residency

RESIDENCY is the core's third gate beside culling and LOD, and the one
that gates MEMORY instead of drawing: which regions of a world should
EXIST right now, given every viewer's view volume, told only when that
changes (`alloy/src/spatial/residency.rs`, the contract in
okf/done/spatial-residency-gate.md, the 3d form in
okf/plans/3d-world-residency.md). Culling and LOD leave everything
resident; a world larger than memory - an open map, a long track, a
city - streams by this gate instead of by camera distances recomputed
in `onFrame`. Two region forms, both group nodes. EXPLICIT:
`createRegion({ bounds, enter, leave, ...content })` / `<Region>` is a
bounded group you place like any node (a level section, a building
interior; Unreal's level streaming volume), its content loaded and
built while a viewer is within `enter` of its box and kept until beyond
`leave`. GRID: `createRegionGrid({ cell, enter, leave, origin?, bounds?,
...content })` / `<RegionGrid>` is a root group you add once; every
cell the gate wants becomes a child RegionNode at the cell's world
corner for as long as it is held (Unreal's World Partition, Minecraft's
chunk columns), so a cell's content is cell-local and float precision
is per cell; `cell` is `[w, h, d]` with 0 meaning unbounded along that
axis (`[64, 0, 64]` a column grid over XZ, `[32, 32, 32]` a voxel
world), `bounds` an optional cell-index box outside which no cell
exists. A load distance is a RADIUS around the view volume, not a view
(Unreal's streaming sources load a sphere): the cells behind and beside
a viewer within `enter` are wanted too, so the world is there when the
camera turns. A world of one layer is the column grid (0 on the up
axis); finite cells on the up axis stack, and the column above and below
the eye is wanted like the ring around it. A 3d grid has no `wrap`: a
3d scene has one world space. VIEWERS are what the scene already pushes:
every camera write (`setView`) is a viewer, so the scene, each view and
each shadow tile want regions, and a region is wanted if ANY of them
wants it (a minimap beside the main view, a shadow camera; a region
beyond its load distance that still casts into view does so through
its proxy); `createStreamingSource(scene, { position, radius })` is a viewer
beyond the cameras, Unreal's streaming source - the world ahead of a
fast vehicle, a teleport destination loaded before the cut - moved
with `setPosition`. The test is the cull test with hysteresis: wanted
within `enter` of the view volume (the frustum planes pushed out by
`enter`, as `cullMargin` widens the cull test), kept until beyond
`leave` (`leave >= enter`), so a camera on a border never thrashes; an
explicit region tests the full frustum, a grid cell the side and near
planes plus a depth cap of the margin along the view direction, the
far plane ignored, so for a grid `enter`/`leave` read as LOAD DISTANCES.

What a region HOLDS is the app's, through three callbacks and a
proxy (`RegionContent`): `load(region, signal)` prepares the data off
the thread - a file read, `decodeModel`, a glTF parse in a `"use
isolate"` module - and may be async; `build(region, data)` makes the
content under `region.content`, and when it is a GENERATOR its steps
run one per slice of the frame budget (core's `slice`, `setSliceBudget`:
milliseconds per frame, default 2, Unity's asyncUploadTimeSlice and
Unreal's time-sliced actor registration in one shape, every job in the
app spending the one budget), so `yield* buildModel(region, data)` runs
createModel's creates one per step (an image upload, the skin palettes,
a part each, then each part's buffers warmed) and a `yield` after each
create of your own spreads a region over frames instead of landing its
creates in one; `unload(region, data)` runs at leave BEFORE the content
goes, with the data, for the app's own state (an editable world reads
its cell back here); `proxy(region)` makes the node drawn while the
region is NOT shown - a merged low-poly stand-in, a card, a slab at the
terrain's mean height - hidden once the content shows, casting shadows
meanwhile (the HLOD rule). An explicit region's proxy stands from its
creation and comes back at every leave; a grid cell exists only from
its enter, so its proxy covers the load and the build, never the cells
still pending or stalled beyond the budget: a stand-in for the far ring
is a COARSER grid with wider margins whose cell content is the finer
cells' proxy (Unreal's HLOD layer), two grids beside each other.
The content is HIDDEN until the build completes (its entries exist at
instance count 0, picks skip it), then shown in the frame the last step
ran in; a leave aborts a load in flight (the signal), cancels a build
under way (the generator closes, its `finally` runs), runs `unload`,
removes and disposes the content (never `destroy` with an exit: a region
leaves off screen) and, for a grid cell, the node; `region.own(x)`
registers anything else to dispose at leave (buildModel registers its
model; a population's record buffers are mesh-owned, so
`region.own({ dispose: () => disposeInstances(mesh) })`), `region.warm(geometry)`
acquires a geometry's buffers now (one create-sized step) so the add
that follows uploads nothing. `region.state` is the gate's "pending" or
"stalled" until the enter, then "loading", "building", "shown" or
"failed" (a load or build that threw: reported once, `onError` on the
node or grid, empty until the next enter retries); `region.data` the
loaded data while held; `onStateChange` on the node or grid. `retain`
keeps the data of left regions (a flag on an explicit region, an LRU
count on a grid) so a re-enter skips `load`. The component form takes
`build` and `proxy` as RENDER PROPS returning JSX, mounted under the
cell's content node when its data arrives: one slice step, so a build
whose creates must spread over frames is the generator's.

The gate PACES: leaves are immediate, enters are delivered under one
scene-wide `setResidencyBudget(n)` (default 16 regions started per
frame), the highest `priority` (an integer on a region or grid: a
collision grid before decorative scatter) then the nearest to a
viewer's eye first, and a region still pending when the camera turns
away never enters. Memory is BOUNDED in BYTES: a region's `weight` (a
grid's per cell) is the estimate the limits admit it by, and the
package MEASURES the content as it shows - every geometry under it
once, every population's buffers, every owned model's textures
(`region.measure()` re-measures after content added later) - so the
grid's own `limit` and the scene-wide `setResidencyLimit(bytes)`
(from @solidrt/core or @solidrt/3d; Unreal's streaming pool size) count content: past a
limit the most needed pending region displaces the least needed
resident ones (lower priority, then further), the eviction's leave
before the enter it made room for; a region nothing can make room for
is wanted and STALLED, no event and no frame demanded until a view
moves. The 2d tile layer weighs in bytes too, so a layer and a grid
share one scene limit. Freed geometry buffers and decoded model
textures go through the package's RESOURCE POOL (`createResourcePool`
in @solidrt/core/gpu, installed at the first region; `setResourcePool`
to share one with the tile layers) and the next acquire of the same
byte or format class takes them back - a write into a retired id, no
create; block-compressed textures are create-once by the GPU contract
and are not pooled. `/stats` reports the gate (`regionsWanted`,
`regionsResident`, `regionsPending`, `regionsStalled`, cumulative
`regionEnters`/`regionLeaves` and the window's pair, `residentWeight`
in bytes), the lifecycle (`app.regionsLoading`, `app.regionsBuilding`,
`app.regionsShown`, `app.regionsFailed`), the slice queue
(`app.sliceMs`, `app.sliceQueued`, `app.sliceSteps`) and the pools
(`app.poolHits`/`poolMisses`/`poolHeld`); `/gpu` groups a region's
resources by its label. A world that declares no region loads and
draws exactly as now. `examples/streaming.tsx` is the shape: a terrain
grid whose cells are generated in an isolate, built a step at a time,
proxied by slabs, bounded by a byte limit, wanted by the main camera
and a minimap alike.
