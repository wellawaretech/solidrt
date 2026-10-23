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

The files in agents/ carry the depth this one leaves out, each topic with
its own traps; read the one that matches the work before starting it:
- agents/cameras.md - Camera input, the orbit and first-person cameras,
  shots and blends, and overlay projection. Read before adding a camera or
  camera controls, or pinning UI to a 3D point.
- agents/interaction.md - Picking, raycasts, collision and pointer events
  on meshes. Read before making anything in the scene respond to a tap, a
  ray or a collision.
- agents/geometry.md - Vertex layouts, indices and topology, the geometry
  generators, geometry as data and the profile kit. Read before building,
  generating or editing a mesh's geometry.
- agents/materials.md - The built-in materials (unlit, sprite, phong,
  standard, shaderMaterial), instanced materials and color handling. Read
  before choosing or tuning a material, transparency or color.
- agents/shaders.md - The lighting GLSL and the three tiers of custom
  looks. Read before writing a shaderMaterial or any custom GLSL look.
- agents/lighting.md - Lights and shadows. Read before adding a light or
  turning on shadows.
- agents/environment.md - Background, cube maps, environment lighting,
  reflection probes, baked sky, panoramas and fog. Read before setting a
  background or sky, image-based lighting, reflections or fog.
- agents/models.md - glTF models: the three layers, async loading,
  sockets, wardrobe pieces, applied material fields, skins and morph
  targets. Read before loading or placing a model.
- agents/animation.md - Retargeted motion, animation clips and the mixer,
  root motion, spring bones and inverse kinematics. Read before animating
  anything: a transition, a clip, a rig.
- agents/sound.md - Positional sound attached to scene nodes. Read before
  playing a sound from a place in the scene.
- agents/instancing.md - Instanced and record meshes, instance order and
  visibility. Read before drawing many copies of a mesh.
- agents/large-worlds.md - Culling, the quality tier, level of detail and
  residency. Read before building a scene too big to draw or hold whole,
  or tuning for a device's budget.
- agents/splats.md - Captured Gaussian splat scenes: parsing, loading and
  drawing. Read before showing a splat capture.

Contents:

- [The model](#the-model)
  - [Two layers: core and components](#two-layers-core-and-components)
  - [Node lifecycle](#node-lifecycle)
  - [Rendering is the runtime's](#rendering-is-the-runtimes)
  - [Views and layers](#views-and-layers)
  - [Material classes](#material-classes)
  - [Pure pieces and checks](#pure-pieces-and-checks)
- [Components](#components)
  - [Component props](#component-props)
  - [Output composition](#output-composition)
  - [Fill and fixed sizes](#fill-and-fixed-sizes)
- [Traps](#traps)
  - [Coordinates and rotation](#coordinates-and-rotation)
  - [Views and scene params](#views-and-scene-params)

## The model

### Two layers: core and components

Two layers. The imperative core is Solid-free: `createScene`,
`createMesh(geometry, material)`, `add`/`remove`, `setTransform`,
`lookAt`, `getRotation`, `setVisible`, `setRenderOrder` - plain objects
over the spatial core (`flux:spatial`, `alloy/src/spatial/`): every node
in a scene has a core node, JS keeps the LOCAL position/quaternion/scale
as the readable truth and forwards each write, and the core's flush
(one call per frame, in the publish pass of core's `onBeforeRender`
ahead of the paint; at the microtask for a write made outside a frame)
recomputes only the moved subtrees and writes
each entry's uModel (plus uNormal for materials declaring it) and its
visibility switch itself - a move costs its subtree, never the scene.
ONE `setTargetParams` (the shared uViewProj + uCamPos) per camera
change, however many meshes. World matrices live in the core only:
`worldPosition`/`worldInto` (the full matrix)/`lookAt`/picking read them
back (`worldMatrix`, pending writes included). See okf/backlog/spatial-core.md for what still runs
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
is the removal an `exit` rides on (see Retargeted motion in agents/animation.md). The components
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
`view.setResolve` - see Color in agents/materials.md); a tiled view (`into`) renders LINEAR
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
the map view whose mask admits them. LIGHTS carry a `layers` mask too
(all bits by default, see Lights in agents/lighting.md): a light lights and shadows
only the meshes its mask intersects. Shadow views follow the SCENE's
mask (what the scene cannot see must not darken it) narrowed by their
light's own `layers`, and
pick()/raycast()/overlap()/sweep() skip scene-masked-out meshes like
invisible ones - unless the query passes its own `{ layers }`, which is
how a low-poly collision mesh lives undrawn in the scene yet answers
ground and collision queries (the physics-collider pattern).
SEALED INTERIORS are the other manual use: from outside, a closed shell
hides its whole interior, but frustum culling cannot know that.
OCCLUSION culling knows (see Culling in agents/large-worlds.md: `occluder="geometry"` on the
shell's walls, and what they cover is not drawn, from any camera
position, a street of shells included); the layer switch remains for
the deliberate sealed interior and for the LIGHTS, which occlusion does
not mask. Put the interior, the shell
and an opening "plug" on their own layers and switch the scene mask by
the camera's zone (inside, outside, in the doorway) - the reactive
`<Scene layers>` makes it one signal - and mask each light (and the
hemisphere) to its side the same way, so the sun does not light the
room through its shell nor the room's bulb the street. Light `layers`
cannot mask the ENVIRONMENT term (image lighting is per target, not
per light): the interior meshes take `environment={null}` - or an
interior probe's cube - instead (Mesh.environment, see Environment in agents/environment.md).
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

### Material classes

Materials dedupe hard: one program + one pipeline per material CLASS
(a `shaderMaterialClass` per option combination for unlit, lit and
sprite alike: map x transparent x cull x alphaTest, phong's extras on
top), `depth: true` + `cull: "back"` unless the material says otherwise
(`cull: "none"` for double-sided geometry; phong flips the normal on back
faces); an instance is just per-entry uniforms (`uColor`) and bindings
(`uMap`).

### Pure pieces and checks

The pure pieces (`math.ts`, `color.ts`, `geometry.ts`,
`profile.ts`, `sweep.ts`, `gltf.ts`, `model-file.ts`, `splat-data.ts`)
are Solid-free and
GPU-free BY DESIGN so they can be checked headless and run under bun:
`src/model-data.ts` re-exports exactly this set as the published
`@solidrt/3d/model` entry (the bake tool and app bake scripts), and
`tests/model-data.test.ts` imports it under `sol test`, on the bare flux
binary, so a gui or `sol:` import creeping into any of them fails there
(a headless `flux:` import would not); keep them that way.
`src/splat-data.ts` is its own published entry the same way
(`@solidrt/3d/splat`, guarded by `tests/splat-data.test.ts`).
The tests under `tests/` run with `bun run sol test packages/3d` from the
repo root, or one file, or `--filter <text>` for the tests whose name
contains it: the pure modules on the bare flux binary, and the files
that need a GPU (`.test.tsx`, importing `@solidrt/test`) on the
headless dev client. Run them after a change. `raycast.test.tsx` and
`collision.test.tsx` assert the documented picking contract (triangle
accuracy, the box tier, pick/raycast parity, layer masks, the
`{ meshes }` filter) and collision contract (exact sweep times, the
surface rule, the slide filter, layers and meshes on overlap/sweep,
moveAndSlide's landing) against a real scene; run them whenever a doc
edit touches picking or collision claims - two copies of this contract
have drifted before. `fog.test.tsx` reads the fog example's pixels
through its debug commands, and `camera-components.test.tsx` drives
`<OrbitCamera>` and `<FirstPersonCamera>` with a real drag and key.

## Components

### Component props

| Component | Props |
| --- | --- |
| `Scene` | `width?`, `height?` (target pixels - both, or neither = FILL, below), `renderScale?`, `maxPixels?` (fill-mode density: a scale over the tracked device pixels and a cap on their count, below; the cap defaults to the quality row's), `clearColor?`, `camera?` (partial CameraUpdate, `ortho` included - the declarative scene.setCamera; same state as `PerspectiveCamera`, use one form; the camera CONTROLS are not a third form - `OrbitCamera`/`FirstPersonCamera` drive position and target only, so `fov`/`near`/`far` come from here even while a control moves the camera, and the default `far` of 100 is what clips a scene in metres), `background?` (fragment GLSL, or a skybox `{ cube, intensity?, rotation? }`), `environment?` (`{ cube, intensity?, rotation? }`, the cube reflective materials mirror), `fog?` (FogOptions: linear `{ color, near, far }` or exp2 `{ color, density }`, either plus `height`/`heightFalloff` - see Fog in agents/environment.md), `toneMapping?` (`"none"` default, `"aces"`, `"agx"` or `"neutral"`), `exposure?` (default 1), `quality?` (a tier name or a row object over the app tier, every knob's default - see Quality in agents/large-worlds.md; reactive, the app tier too), `lodBias?` (reactive; default the row's), `occlusion?` (reactive; default true, see Culling in agents/large-worlds.md), `shadows?` (`{ quality? }`: "hard" | "low" | "medium" | "high", the soft shadow kernel's tap tier, default the row's, reactive), `bloom?` (`{ threshold?, intensity?, radius? }`, the stock bloom on the resolve, reactive), `layers?` (target mask, default 1), `depth?` (`"texture"` exposes scene.depthTexture; not with samples), `samples?` (1/2/4/8 MSAA; default the row's, live - the storage is reallocated in place), `label?`, `ref?(scene)`, `output?(texture, leaf)` (compose the leaf yourself; `leaf.ref`, `leaf.handlers`, `leaf.size()`, `leaf.deviceSize()` - see Output composition; `events` then throws), `resolve?` (the source, `{ source, textures }`, or a function of the buffer id returning either; fixed at creation - see Color in agents/materials.md), `events?` (pointer events, default on), `pointer?` (the leaf's pointer feed, fed from the scene's root), `onPointerDown/Move/Up?`, `onWheel?`, `onTap?` (the scene's own handlers, the last stop of the walk - `event.mesh` null over empty space) |
| `View3d` | a Scene child rendering the scene again from a camera of its own (scene.createView as a component): `width?`, `height?` (target pixels, live - both, or neither = FILL exactly as Scene's: the leaf fills its sized parent and the view follows it; a tile of `into` gives both), `renderScale?`, `maxPixels?` (fill-mode density, as Scene's), `x?`, `y?` (the tile's top-left in `into`, live; throw without `into`), `into?` (tile an app-owned draw target - one pass for every view into it; fixed at creation), `camera?` (partial CameraUpdate on the view's camera, live; same state as a `PerspectiveCamera` child), `layers?` (the view's mask, live), `occlusion?` (live; default true, see Culling in agents/large-worlds.md), `clearColor?`, `label?`, `overrideMaterial?`, `fog?` (FogOptions, or null for none), `depth?`, `filter?`, `wrap?` (createView's, fixed), `samples?` (default the scene's quality row's; live), `ref?(view)`, `output?(texture, leaf)` (as Scene's; else a built-in `<texture>` leaf at the target size, a tile shown through srcX/srcY), `resolve?` (as Scene's; not with `into`), `bloom?` (BloomOptions overrides the scene's, null turns it off in this view, absent follows the scene; not with `into`), `events?`, `pointer?` (the view leaf's feed, fed from the view's root), `onPointerDown/Move/Up?`, `onWheel?`, `onTap?` (the view's own handlers); camera-control children drive the VIEW (inside, `useScene()` reports the view as `viewport` and the view's feed as `pointer`); node children mount to the scene as outside, and under the view's leaf get their ordinary pointer handlers, picked through the view's camera (`view.pick`), the view as the root of that walk |
| `Group` | `position?`, `rotation?` (Euler radians, XYZ order), `quaternion?` (either, not both), `scale?` (number = uniform), `visible?`, `occluder?` (an explicit occluder shape at the node: `{ quad }`, `{ box }`, `{ sphere }` or `{ geometry }`, see Culling in agents/large-worlds.md), the bubbling pointer events (below: down/move/up/wheel/tap from a hit descendant; a group is never the struck node, so it takes no `onPointerEnter`/`onPointerLeave` - `Lod` likewise), `ref?(node)` |
| `Lod` | a Group whose direct children carrying `lodSize` (a prop every node component takes: the projected size below which that child hands over, see Level of detail in agents/large-worlds.md) are its levels, sorted by size, never by JSX position; plus `fade?` (the cross-fade band fraction, default 0); a child without `lodSize` is drawn always |
| `InstancedLod` | as InstancedMesh minus `material`, plus `levels` (`[{ geometry, material, size }]` nearest first, fixed at creation; instanced materials as InstancedMesh's), `castShadow?` (every level); `<Instance>` children populate it as under `InstancedMesh`, each drawing the level its own projected size picks |
| `Mesh` | `geometry`, `material`, transforms as Group, `params?` (per-mesh uniforms, merge semantics - no unset), `renderOrder?`, `castShadow?`, `layers?` (membership bitmask, default 1), `frustumCulled?` (default true; false for geometry a vertex stage moves beyond the node's box - a billboard, a fullscreen quad; see Culling in agents/large-worlds.md), `cullMargin?` (world units of slack around the box for bounded displacement), `occluder?` (`"geometry"`, `"bounds"` or a shape; what the mesh hides, see Culling in agents/large-worlds.md), pointer events (below), `ref?(mesh)` |
| `Sprite` | as Mesh minus `geometry`: a camera-facing unit quad, `scale` is its world size, rotation is ignored; pair with a `sprite()` material |
| `InstancedMesh` | as Mesh (an instanced `material`: a stock one with `instanced`/`instanceColors`, or a class declaring INSTANCE_MATRIX_ATTRIBUTES), plus `capacity?` (instance slots, default 64; the buffers double past it), `bounds?` (an optional cull box, never a hit: instances pick by themselves, and without it the mesh culls by their union), `instanceOrder?` (draw the records in key order, fixed at creation - see "Instance order" in agents/instancing.md), `label?`; a PARENT: `<Instance>` children populate it, `<Group>` children are squads; the record buffers are component-owned and freed on unmount, and `anchor?` (a SceneNode, fixed at creation: the ancestor the records are relative to, so `<Instance mesh>` children may sit anywhere under it - see createInstancedMesh) |
| `Instance` | one instance of the enclosing `InstancedMesh`: transforms, `transition`, pointer events as Group, plus `style?` (the material's style record, one value per component - `[r, g, b, a]` under `instanceColors`), `ref?(instance)`; a parent too (a `<Mesh>` under an instance rides with it), and `mesh?` (the population, fixed at creation, when the instance is placed outside its `<InstancedMesh>` - inside that mesh's `anchor` subtree) |
| `RecordMesh` | as Mesh, plus `records` (the per-instance records in the material's first instance layout, an ArrayBufferView; buffer capacity starts at the first value and grows on larger rewrites), `count?` (records drawn, default all), `bounds?` (local [minX..maxZ] over the population - without it the mesh never picks), `instanceOrder?` (draw the records in key order, fixed at creation - see "Instance order" in agents/instancing.md), `transfer?` (hand the records to the engine at creation, the write-once form - a later `records` value then throws; see "Instance order" in agents/instancing.md); the record buffers are component-owned and freed on unmount |
| `SplatMesh` | as Mesh, plus `data` (a baked SplatData from loadSplat; write-once under the default transfer - swap clouds by remounting), `count?` (the most splats drawn, default all: a budget the engine's selection fits by thinning every chunk in view by importance), `lodThreshold?` (pixels of contribution below which a chunk's least important splats are not drawn, default SPLAT_LOD_THRESHOLD, 0 for the budget alone), `material?` (a fork of the stock splat class); bounds, the chunk culling, the back-to-front instance order and the stock material come from the bake (see "Splats") |
| `PerspectiveCamera` | `fov?` (vertical DEGREES, default 60), `near?`, `far?`, `position?`, `lookAt?`, `up?` - or the Scene `camera` prop, the same state (last write wins) |
| `SpotLight` | transforms as Group, `direction?` (local aim, default [0, -1, 0]), `color?`, `intensity?`, `distance?` (falloff cutoff, 0 = none), `angle?` (cone half-angle DEGREES, default 60), `penumbra?` (0..1 rim fade, default 0), `decay?` (falloff exponent, default 2), `sourceRadius?` (world units, default 0 = a point source; above 0 contact-hardening soft shadows), `castShadow?`, `shadow?` (mapSize - default the quality row's -, bias, normalBias, radius, near), `ref?(light)` |
| `PointLight` | transforms as Group (position is what matters), `color?`, `intensity?`, `distance?`, `decay?`, `sourceRadius?` (as SpotLight), `castShadow?` (six face maps, six shadow slots), `shadow?` (mapSize - default the quality row's -, bias, normalBias, radius, near), `ref?(light)` |

### Output composition

Output composition: without `output`, `Scene` emits a minimal
`<texture>` leaf and nothing else is forwarded - anything more goes
through `output(texture, leaf)`, which renders in place of that leaf: a
`<d-texture>`, a leaf with blendMode/fit/pointer/layout props, or a
post-effect chain (`createShaderTarget` sampling the id with a
covering-triangle pass; created in the callback it disposes with the
Scene). Return null for no leaf at all and compose `scene.texture`
elsewhere. Called once, untracked, inside the scene context. `output`
composes the DISPLAYED texture, the resolve's output: an LDR effect
lives here; one that needs the scene's radiance is a `resolve` (Color in agents/materials.md).

`leaf` (OutputLeaf) is the contract between your leaf and the scene:
`ref={leaf.ref}` hands the scene the leaf you render, so a fill-mode
target follows it exactly as it follows the built-in one (give it the
built-in leaf's layout, `width/height={pct(100)}` and `contain="size"`,
or any layout you want followed); `{...leaf.handlers}` routes pointer
events through it, scaled to the ref'd leaf's layout box per event, so
one spread carries the node events, the scene's own and the `pointer`
feed whatever size the leaf lays out at; `leaf.size()` is the target's
size and `leaf.deviceSize()` the leaf's on-screen size in device pixels,
both reactive and applied before the frame that follows a resize paints
(core flushes pending writes once the layout handlers have run), so a
post chain sizes its own targets from them - `createShaderTextureMemo`
takes the read directly and resizes in place. A post pass that should
run at display density (its sampler doubling as the downsample of a
supersampled scene) sizes from `deviceSize`; one that works in scene
pixels (a depth-reading effect) from `size`. `examples/scene-post-effect.tsx`
is the shape: a fill scene at `renderScale={2}` under an aberration pass
at display density. The fixed form still composes by hand: Scene
`width`/`height` are target pixels and the leaf's own width/height are
layout, so render and display size separate; `ref` then only scales
`leaf.handlers` to the leaf's layout. A fill `output` that never refs a
leaf leaves the target at its 1 x 1 creation size, and the first layout
warns once.

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
and the camera aspect follows the box. Fill or fixed is decided at
mount, and giving exactly one of width/height throws. `<View3d>` fills
by the same rule (both or neither; a tile of `into` has its rect, so it
gives both), so a split-screen pane or a picture-in-picture follows its
box at device density like the scene does; the fill branch is one piece
of code for both (components/fill.ts over core's `followBox`, the size
decision in fill-math.ts, pure and pinned headless). `x`/`y` place a
tile in `into` and throw without one.

Two knobs scale the tracked size, reactive, fill mode only (with
`width`/`height` the target IS the pixels, and either throws):
`renderScale` (default 1) multiplies it on each axis - 2 renders four
times the pixels and the leaf's linear sampler box-averages them down
(supersampling, cheap anti-aliasing beyond `samples`), 0.5 renders a
quarter (a heavy scene on a weak GPU); Unity's render scale, Godot's
`scaling_3d_scale`, r3f's `dpr`. `maxPixels` caps the target's pixel
count after the scale: both axes shrink by one factor, so the aspect
holds, and one number holds one cost on every device, which a scale
cannot (a 0.7 that suits a laptop panel is wasted on a phone and too
much on a 4K screen); a fill-rate-bound main pass of ~11 ms at 3.4 Mpx
on an iGPU drops to ~5 ms capped at ~1.9 Mpx with no visible loss under
a HUD. The default cap is the quality row's: none at medium and high
(native density, as Unity and Godot default; r3f clamps its dpr to 2),
1 Mpx at low - the 720p-class count games render at on that GPU class;
`maxPixels` on the Scene or View3d overrides it, since beyond the tier
only the app knows its scene's cost. Neither knob
touches the leaf's layout, pointer events or the camera controls:
`handlersFor` scales events by target over layout box and the orbit
control divides by `size().height`, both invariant under the scale. The
2d packages' knobs are `oversample` (an integer, so the resample stays
clean) and the same `maxPixels`.

With `output`, the target follows the leaf you `ref` (Output composition
above); without a ref it stays at its creation size and the first
layout warns once. The `output` leaf is yours to lay out: the built-in
leaf's `contain="size"` keeps the target's pixels out of the flex
automatic minimum (below), and a leaf you compose wants the same.

The fill leaf is size-contained (`contain="size"`, the CSS layout prop
every element takes): it adds nothing to its ancestors' sizes, exactly as
an empty `<view>` at 100% would. So a column of header, `<Scene>` pane
(`flexGrow: 1`) and footer lays out at the window's height with no
`minHeight={0}` anywhere, and a pane with no definite height does not
come out square. The price is that the leaf has no size of its own at
all: a parent with no size leaves it empty, the scene is invisible, and
the component warns once with the stack of the component that built it,
naming the fix (a size, or flexGrow in a sized column).

Fixed sizes still matter where the target is a measured quantity: probes
and checks that snapshot exact pixels, supersampling via `output`, or a
scene composited at a size unrelated to its layout. `width`/`height` are
DEVICE pixels and the leaf's layout is LOGICAL - a fixed 720-pixel scene
under a HiDPI `designSize` fit is stretched across ~1100 device pixels
and looks soft, and nothing warns; that trap is what fill removes (the
examples fill; scene-views keeps fixed sizes to show multi-view
composition, and supersampling is `renderScale`). A custom `output` leaf
spreads `leaf.handlers`, scaled to its layout box per event, so a leaf
laid out at any size routes right; `useScene()` works inside `output`
because it runs in the scene context. One spread carries everything:
the `pointer` feed listens at the scene's root, so `leaf.handlers` feeds
the nodes, the scene's own handlers and the feed alike (a detached leaf
gives its feed a `layout`, it has no layout box to normalize by).
`events` wires the built-in leaf only: given together with `output` it
throws, the leaf and its spread being yours. `useScene().viewportSize`
is the owner's target size as a signal, for a HUD or a per-pixel
material under it.

## Traps

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
  A directional or spot light aims its `direction` instead of +z, so the
  beam points at the target, the way Unity, Godot and Three all aim a
  light; `up` still chooses the roll. The aim holds the `direction` of
  the call: a later setLight `direction` turns the beam off the target.
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
- RANGE: positions are f64 end to end - the JS numbers, the FFI carriers
  (Float64Array), the spatial arena - so a scene a million units from
  zero picks, collides, streams and animates as exactly as one at zero
  (Godot's double-precision build, on by default). The GPU takes f32, so
  what reaches it is RELATIVE to each target's RENDER ORIGIN: a point the
  camera snaps near its eye on a power-of-two cell 64 near planes wide
  (camera.ts; the core's `set_origin`), the core writing every entry's
  uModel and the light positions relative to it and `cameraParams` the
  view, uCamPos and the shadow matrices (HDRP's camera-relative
  rendering, re-based per cell instead of per frame so a camera move
  stays ONE shared write). Within reach of the origin the f32 error at
  the near plane is a few hundredths of a pixel whatever the distance
  from zero; a camera crossing a cell re-bases the target in one core
  shift write, never a params write per entry. Nothing in the API
  changes shape for this; only a shader that reads `vWorldPos` as an
  ABSOLUTE coordinate adds `uOrigin` (the traps in agents/shaders.md). Not
  origin-relative, by design: instance records, splat centres and record
  meshes are local to their mesh (or anchor) and ride ITS precision, so a
  population spread over a million units of one mesh's local space keeps
  f32 steps - place such populations by their nodes, or in regions.

### Views and scene params

- A camera change is ONE `setTargetParams` write (uViewProj + uCamPos are
  target state, both relative to the target's render origin, uOrigin,
  so the write stays one however far out the camera is), independent of
  mesh count - never reintroduce per-mesh
  camera writes (uEye-style per-mesh params are exactly the O(scene) cost
  the shared channel removed). Scene scale honestly: hundreds to a
  few thousand objects, bounded by the interpreter, not the GPU. A view
  is one more such write per camera change and one more entry per mesh
  at attach; a view's per-frame cost is the core's (one placement record
  per sink per moved node, sent as one command per target per flush),
  never JS.
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
