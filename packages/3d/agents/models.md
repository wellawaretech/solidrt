# @solidrt/3d - models

glTF models: the three layers, async loading, sockets, wardrobe pieces,
applied material fields, skins and morph targets. Read ../AGENTS.md first:
it carries the model this file builds on.

## Three layers

Authored models come in as glTF 2.0 (.gltf with its .bin and image files
next to it, or single-file .glb) and become a Group carrying the file's
node hierarchy, Three's `gltf.scene`. Three layers, use the lowest that
fits:

### parseGltf

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
(each `{ bytes, name, uri? }`: the encoded PNG, JPEG or KTX2 file,
undecoded; its name in the document, else the uri of its file, else
"image<i>"; the `uri` when it is a file of its own) and `bounds` (world-space rest
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

Reuse is kept, not unrolled. A mesh several nodes reference (Blender's
linked duplicates export that way) becomes ONE part per primitive with
`node` = the first node and `placements` = the further ones, and
`EXT_mesh_gpu_instancing` lands the same way: its per-instance TRS
become synthesized child nodes of the referencing node, named
`<node>[i]`, so the hierarchy stays one table and an instance moves
like any node (Three builds an InstancedMesh, Godot a MultiMesh;
Unity's importer shares the mesh and lets its batcher instance).
What cannot share one geometry stays a part per node: a skinned use
(bind-pose model space, placed by its skin), a morphed one (its
weights are the node's), and a placement whose rest-pose world
MIRRORS the first's (opposite determinant sign - the winding flip is
baked into the indices, so one index order cannot serve both). The
file's `extras` ride through where glTF puts them, the way Three fills
`userData` and Godot node metadata: `data.extras` (root), `node.extras`,
`part.extras` (the glTF MESH's) and `material.extras`, each present
only when non-empty - so a collider box authored as a Blender custom
property arrives with the model. Parts come out in WALK order (each
where its first placing node was visited, a node's primitives in
order), and the parts a primitive cannot fold still share its built
geometry: two skinned placements are two parts over ONE `Geometry`
object, a mirrored copy shares the vertex buffer with its own index
order - and the container writes each object and each buffer once
(`decodeModel` restores the identities, so the runtime uploads them
once too). A document with no `scene`/`scenes` draws everything from
its true roots; a hierarchy reaching a node twice (a cycle) throws.

### createModel

`createModel(data, { material?, label?, autoFree? })` - uploads the images (repeat
wrap, mipmapped, at the app quality tier's anisotropy and mip limit, re-sampled in
place when the tier changes - see Quality in large-worlds.md; each texture is labeled `<label>-<image name>`, so the
`/gpu` inventory names the image), makes one material per glTF material (default `standard`
with the file's color, maps, normal scale, metalness/roughness and
packed map, emissive and transparency - the glTF material model, so a
scene showing a model wants an `environment` (a glTF metal in a scene
with none renders near black); `material: (m, maps, skinned) =>
phong({ color: m.color, map: maps.map ?? undefined, skinned })` for the
Blinn-Phong look, or any other material; it is called once per material and shared), the node table as nested
Groups with the file's local TRS, and one mesh per part under its node,
all inside the returned `Model` (a Group): `add(scene.root, model)`,
place it with `setTransform`, find parts by name in `model.parts`
(`{ name, mesh, extras? }`), spin a wheel relative to its axle through
`model.nodes` (`{ name, node, extras? }` in table order, parents first; names
repeat when the file's do - `.find()` yours), `model.bounds` for
framing a camera, `model.extras` and `model.blobs` for the app data a
file or a bake carried. Skinned parts get the `skinned: true` material
variant and hang off the model ROOT (the spec ignores their node's
transform; the palette places them - see the mixer in animation.md). A part with
`placements` is a POPULATION: one InstancedMesh at identity under the
model root (its `anchor`, see Instancing in instancing.md), `instanced: true` material
variant, with an instance node at identity under every placement node
- `part.instances`, in placement order, the part's own node first - so
the copies are one draw entry (`/gpu` shows one entry labelled with the
part's name, `instanceCount` = placements + 1; every mesh's entry
carries its geometry's label there) and still ride the hierarchy: move,
hide or animate the placement node in `model.nodes` and its copy
follows; a pick lands on the copy (`hit.mesh` the part's mesh,
`hit.instance` the instance, whose `parent` is the placement node);
culling is per copy through the mesh's instance union. The one thing a
population cannot do per copy is a material swap - `setMaterial` on the
part's mesh restyles every copy; per-copy tint is `setInstanceStyle`
under a material with `instanceColors`. The `material` callback's
last flag is `instanced` (after `skinned`, `vertexColors`, `morphed`):
pass it through, or the instanced draw has no matrix to read.
`dispose()` detaches it and frees the geometry buffers, a population's
record buffers and the textures - the model owns them, nothing else
frees them.

### loadGltf and loadModel

`loadGltf(path)` / `loadModel(path)` - read from `assets/` with flux:fs
and build. Like every resource creator, a model frees with the owning
reactive scope (`dispose()` at its cleanup; `autoFree: false` opts out):
for the loaders that scope is the caller's at the call, captured before
the await, so a model loaded inside a memo goes with the memo's rerun
and one loaded in a component with its unmount. A load that settles
after its scope is gone (unmounted mid-load) is freed on arrival and
resolves already disposed - nothing owns it any more. `loadModel` reads the baked `.sol3m` written by `sol tool
3d/model <in.gltf|glb> -o assets/<name>.sol3m`: the same parse run once
at build time, stored in the GPU layout, so loading is views onto the
file's bytes plus the image decodes. Numbers from a 32k-vertex, 6-texture model
on a release client: `parseGltf` 124 ms on flux (22 ms under bun) against
40 ms for the whole baked load - the runtime parse is fine for small
models and a binary import (`import bytes from "./x.glb" with { type:
"binary" }` then `createModel(parseGltf(bytes))`, see
`examples/model.tsx`); bake anything big.

### Compressed textures

`sol tool 3d/model <in> -o assets/<name>.sol3m` bakes the images
too: each becomes a KTX2 compressed texture with its mip chain. The file is the
same for every platform; at load each device turns the images into the
block format its GPU samples (BC7 on desktops, ETC2 elsewhere), which
holds them in a quarter of the memory of the decoded image. Measured on
the Khronos Sponza sample (69 images): 363 MB of textures as PNG/JPEG,
91 MB compressed. The encode is slow (a minute for that
model) and runs once, at bake time. Block compression is lossy: judge
the result on your own textures. What it saves is GPU memory; the file
shrinks for color maps and can grow for data maps.

How the images are compressed, and whether, is the app's to say, in the
`textures` group of the `solidrt` key in its package.json (the project
the tool runs in, whose root is where the tool is run). These are the
defaults; name only what differs:

```json
"solidrt": {
  "textures": {
    "compress": true,
    "color": { "codec": "etc1s", "quality": 0.75 },
    "normal": { "codec": "uastc", "quality": 0.9 },
    "data": { "codec": "uastc", "quality": 0.9 },
    "files": []
  }
}
```

Three levels, the narrower one winning: the app (`compress`), a kind
of map (`compress`, `codec`, `quality`), and single files:

```json
"files": [
  { "match": "assets/sponza/textures/lion_*.png", "codec": "uastc", "quality": 0.95 },
  { "match": ["assets/ui/**", "assets/hero.glb#lut*"], "compress": false }
]
```

- `match` is a glob pattern, or a list of them, against the image's
  path FROM THE PROJECT ROOT, written with "/": `*` inside a segment,
  `**` across segments, `?`, `[a-z]`, `[!a-z]` (`matchesGlob` of
  flux:path; no `{a,b}`, list the patterns). Not the uri the glTF
  writes: two models with a `textures/base.png` each are two files.
- An image embedded in the model (a .glb) has no file of its own: the
  model's path names every image in it, and `#name` after it picks
  the ones of that name (the glTF image's `name`, "image<i>" without
  one).
- An entry sets `compress`, `codec`, `quality` or several, over what
  the image's kind says. Where several entries name one image, a
  later entry wins, field by field.
- `"compress": false` keeps the image as its PNG or JPEG. That is for
  what must come back exact (a lookup table, a distance field) and
  for sharp art and text, which block compression damages.
- A pattern that names nothing FAILS the bake, before anything is
  encoded: no file of the project matches it, or the baked model has
  no embedded image of that name. A misspelled pattern would
  otherwise do nothing and say nothing.

`--compress` and `--no-compress` stand in for the app's `compress` for
one run (a quick bake while the model is still changing); what a kind
or an entry says about itself still holds.

The kinds are core's and say what the texels are (`@solidrt/core/textures`).
This package maps a material's slots onto them: base color and emissive
maps are `color`, normal maps are `normal`, metallic-roughness maps
(occlusion rides in the same image) are `data`.
An image several slots share takes the most demanding kind, normal over
color over data, and stays sRGB when one of them is a color slot;
`modelImageUses(data)` returns that per image, the rule the loader and
the bake both follow, for a bake script of your own.
`codec` and `quality` are `encodeTexture`'s: "etc1s" is small (a
quarter of a JPEG), "uastc" is accurate and large (often larger than
the JPEG it replaces), `quality` is 0..1. A normal map steers the
lighting and shows its errors, and a metallic-roughness map packs
unrelated values into its channels, which the small codec handles
badly, so both take the accurate one (what the Khronos KTX guide
recommends). Set `data` to "etc1s" when the download matters more than
roughness detail: on the Khronos Sponza sample that is 9 MB less for
about 2 dB in the views that glossy surfaces fill. An unknown key, codec or field fails the bake naming the key,
before anything is encoded, and the bake prints how many images it
compressed with which codec and quality, and how many it kept. The color space (sRGB for `color`), the mip chain and the
wrap are not settings: they follow how `createModel` samples. The
settings are the app's, not this package's: an app's own bake script,
or another extension's, reads the same group with
`textureSettings(packageJson)` from `@solidrt/core/textures` and asks
`textureSettingFor(settings, kind, { file, name }, matchesGlob)` per
texture (core's AGENTS.md has the rest).

`loadModel` and `loadGltf` handle such a model as they are, and
`loadGltf` reads a glTF that carries KTX2 images itself
(`KHR_texture_basisu`). Only the primitives show the step: transcoding
is asynchronous and `createModel` is not, so bytes obtained another way
go through `transcodeModelImages` between the two:

```ts
let data = await transcodeModelImages(decodeModel(bytes))
let model = createModel(data)
```

`createModel` on data whose KTX2 images were not transcoded throws and
names the function. Data without KTX2 images needs no such step.

### Baking your own geometry

Geometry an app GENERATES (a terrain, a city block, a hundred thousand
vertices of anything) pays the same interpreter cost every start; when
it does not depend on runtime input, bake it once too. The whole data
side of the package - the parser, the container and the pure geometry
kit (generators, `withAttribute`, `mergeGeometries`, `transformGeometry`,
the sweeps and profiles, the math) - is published without the runtime
as `@solidrt/3d/model`, so a bake script runs under `bun` with no
`flux:*` shim:

```ts
import { box, encodeModel, transformGeometry } from "@solidrt/3d/model"
import type { ModelData } from "@solidrt/3d/model"
// nodes in pre-order, one part per drawable (a part several nodes place
// takes `placements`), materials as glTF-shaped records, app data in
// `extras` (JSON) and `blobs` (named bytes, each an aligned block).
let data: ModelData = { nodes, parts, skins: [], clips: [], materials, images: [], bounds, extras: { spawn }, blobs: { grid } }
await Bun.write("assets/level.sol3m", encodeModel(data))
```

`loadModel("assets/level.sol3m")` then builds it like any authored model;
`model.extras` and `model.blobs` hand the app data back (blobs are views
onto the loaded bytes). `sol tool 3d/model` is the same entry applied
to a glTF, so the two bakes share one code path.

## Async loading

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
createModel run synchronously on main. Bake anything big to .sol3m; when
a source glTF must be parsed at runtime, do the parse in an isolate
(parseGltf's result is plain data and copies across) and keep
createModel on main.

## Placement and sockets

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

## Wardrobe pieces

Wardrobe pieces (a hood, a cape, cuffs) are the third case: exported
WITH a skin over the body's joints and WITHOUT clips, so beside an
animated body they hold their bind pose. `bindSkeleton(body, piece)`
drives them from the body's skeleton - Three's SkinnedMesh.bind, Unity's
`bones =`, Godot's shared Skeleton3D. The piece's palette rows re-bind
onto the body's joint nodes (matched by name, case-insensitive; a
pipeline whose names differ by a prefix or suffix - Mixamo's
`mixamorig:`, a one-sided `_JNT` - passes `{ match: pieceName =>
bodyName }`, Unity's leave-it-to-the-app in one option) with
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

## Applied glTF material fields

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
A `.sol3m` baked before the material records carried the PBR fields
(file version 3), or before the vertex formats took the WebGPU spelling
and byte counts (file version 5), is rejected by loadModel - re-bake
with `sol tool 3d/model`. A baked part keeps a quantized export's
bytes: u8 colors, u8/u16 joints and normalized weights, u16 uvs land in
their own formats, positions and normals as floats.

## Skins

Skins need nothing further: each skin's uBones palette
(model-local jointWorld x inverseBind, sized to the RIG - an rgba32f
float texture, 4 texels wide, one row per joint, sampled in the vertex
stage via texelFetch, so there is no joint cap) is composed by the
spatial core at the frame's flush from the joint nodes themselves, in
any write order, and identical skins (a body/legs split, LODs) share one
computed-once texture. `sampleChannel` (pure, from the root) stays the
JS sampling core for checks and custom drivers;
okf/done/animation-core.md records the design.

## Morph targets

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

## Not in the subset

Not in the subset, dropped: tangents and further UV sets; samplers are
ignored (every texture repeats); additive blending draws as base color.
The follow-ups are filed in okf/backlog/3d-model-loader.md. The `.sol3m`
container is VERSION 11 (node table in the header, node-local vertices,
skins, clips, packed morph targets, app data, images under their
names); older bakes are rejected - re-bake with `sol tool 3d/model`.

## Traps

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
- A shared part (`part.instances` set) has ONE mesh for every copy, at
  identity under the model root: hiding it hides all copies, swapping its
  material restyles all, moving it is wrong (its records are anchored on
  the root - see the instancing traps). Per-copy identity lives on the
  placement nodes in `model.nodes` and on `part.instances` - hide, move,
  animate and pick those. A file that references one mesh from nodes on
  both sides of a mirror yields two parts (one per winding sign), so
  `model.parts.find` by name may find the unmirrored one only; the
  mirrored copy carries the mirroring node's name.
- `createModel` refuses a skinned or morphed part with `placements`
  (thrown, not drawn wrong): parseGltf never folds those, so this only
  comes from a hand-built ModelData - give each copy its own part.
- `extras` fields exist only where the file HAD data (an empty object
  is dropped), so read them with `?.` - `model.extras?.spawn`, not
  `model.extras.spawn`.
- An IK solver owns its joints: a `setTransform` on one in the frame is
  solved over (the solve runs after your onFrame), a read of one in the
  frame sees the previous frame's solve, and the solve of a solver at
  weight 0.5 does NOT creep toward the full aim frame by frame - it is
  re-taken from the animated pose every frame. A hand that must stay on
  a handle while the torso turns is a `twoBone` AFTER the `lookAt` in
  the stack, not a second stack.
- A `twoBone` pole decides which way the elbow or knee goes, and a pole
  on the wrong side folds the limb through the body (the forearm sweeps
  across the torso). Put it on the joint's natural side: outward and a
  little back for an elbow, forward for a knee, in the model's space
  so it turns with the character. Mind the rig's handedness: a VRM 0.x
  faces -Z, so the character's LEFT is -x (her left arm's pole is at
  negative x) and a look-at's `forward` is `[0, 0, -1]`. Without a pole
  the animated bend direction holds, which a straight T-pose arm does
  not have, so give arms a pole.
- `feet` needs `layers`: the ground rays run against the scene's picking
  index, which holds the model's own meshes too (a skinned mesh picks by
  its bind-pose box at the model root), so a ray that sees every layer
  lands on the character. Put the ground on a collider layer (the
  physics-collider pattern: a hidden or visible mesh with `layers` set)
  and name it; the model's meshes stay off it. Declare `feet` with the
  model at rest on its ground plane, or give each leg its `height`.
