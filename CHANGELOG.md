<!--
How to write an entry. Newest release first.

Head each release "## <version> - <YYYY-MM-DD>", the date of its tag.

Sections, in this order. Omit none: a section with nothing in it says
"None." so readers can trust that it was considered.

  Fixes                 Things that were broken and now are not. A
                        security-relevant fix leads the list, labelled
                        [security].
  Features              New and changed behavior an app or its users can
                        see. Lead the bullet with the area when there is
                        one: "**GPU: ...**", "**Input: ...**". Performance
                        work belongs here too, not in Various: faster is
                        something users see.
  API                   New or changed public surface. A feature that also
                        adds surface appears twice: Features says what it
                        does, API names the surface.
  Developer experience  What an app developer touches while building: srt
                        commands and their output, dev server behavior,
                        launcher UI, error messages. Contributor-facing
                        build and CI work goes in Various instead.
  Breaking changes      Removals and signature changes. Behavior changes
                        an app can notice without opting in go here too,
                        as a note under "None." So do platform and
                        requirement changes that can strand someone: a
                        dropped target, a minimum OS, Bun or Rust bump.
                        New platform support, which strands nobody, goes
                        in Features.
  Agents                Aimed at coding agents: scaffold AGENTS.md,
                        MCP surfaces, examples (examples exist for agents).
  Various               Everything else: dependency updates, build, CI,
                        backlog notes. Lead the dependency bullet with
                        "Dependencies:", and say so even when there are
                        none.

Rules:
- Roll a cluster of related commits into ONE bullet stating the larger
  goal they served, mentioning the pieces in passing. Not one bullet per
  commit.
- Short descriptions. No docs-were-updated bullets, no individual test
  file names, no rebuilt bundles.
- One line per bullet: the bold lead, a dash, one short clause. A reader
  scans a release, they do not study it - mechanism, rationale and caveats
  belong in the docs. If a bullet needs two clauses to make sense, it is
  probably two bullets or the wrong altitude.
- No per-crate labels. Platform-specific entries DO get a label, leading
  the bullet: [windows], [android], [linux], [macos]. Labels are always
  lowercase, whatever the thing is normally called.
- ASCII only, no em-dashes.
-->

# Changelog

## 0.0.50 - 2026-08-18

### Fixes
- **[windows] `srt mcp` did not find its dev server** - path spelling and live-pid checks were too strict.
- **[windows] `srt render` was never headless** - fell back to a hidden window; now runs without a display.
- **[macos] `srt render` and `srt pack` work** - libimpeller statically linked.

### Features
- **Pretext-based paragraph rendering** - text layout is SolidRT's own; `<span>` runs, inline atoms, floats, wrap controls, app-side line layout.
- **GPU/3D: transparent materials** - plus render order, uniform plumbing and material classes.
- **Audio: PCM sounds** - `createPcmSound` over app-generated samples.

### API
- JSX: `<span>`; `SpanProps`, `TextRunProps`; `float`, `clear` on layout props.
- `TextProps.textOverflow`, `overflowWrap`, `textIndent`, `textWrap`.
- `@solidrt/core`: `prepareText`, `layoutNextLine`, `createPcmSound`; `TextLine`, `PreparedText`, `TextUnit`, `PcmSoundOptions`.
- `flux:rendertree`: `prepareText`. `flux:audio`: `loadPcm`.
- `flux:gpu`: `BlendMode` adds `"multiply"` and `"alpha"`.
- `@solidrt/3d`: `scene.setParams`, `setRenderOrder`, `renderOrder` prop, `shaderMaterialClass`, `Material.transparent`; `ShaderMaterialClass*` types.

### Developer experience
- **`srt init --with <pkg,pkg>`** - pick extensions to include; the picker multi-selects.
- **`srt mcp` explains a miss** - lists the registry's servers when none matches the project.
- **`get_stats` reports `wordHits`** next to `paraShapes`.

### Breaking changes
- **`srt init -t/--template` removed** - use `--with`.

### Agents
- **Scaffold `AGENTS.md`** - rich text and inline atoms; `flux:wasm` documented as interpreted, not a fast lane.
- **`packages/3d/AGENTS.md`** - transparency, render order, uniform plumbing, material classes.

### Various
- Dependencies: none.
- Changelog images saved per release.

## 0.0.49 - 2026-08-16

### Fixes
- **[windows] [macos] self-contained packed executables** - ANGLE libraries embedded.
- **Text input dropped or reordered characters under burst events.**
- **An explicit `undefined` for an optional argument was rejected** - it now means "not provided".
- **`<oval>` never hit-tested.**
- **A new 3D mesh flashed at the origin for one frame.**

### Features
- **Isolates** - a `"use isolate"` module runs on its own thread, called through a typed proxy.
- **FFI: buffers as pointer arguments** - `ArrayBuffer` and views pass as `ptr`.
- **FFI: typed reads and optional symbols.**
- **3D: picking and raycasting** - `scene.pick`, `scene.raycast`, `onPointer*` on meshes.
- **3D: scene background** - a fragment shader inside the scene pass.
- **Faster texture uploads** - staging buffer.
- **Steadier presentation on TVs** - pacing policy follows the input modality.
- **Retained frame re-presents** - changed textures show without a tree change.

### API
- `flux:isolate`: `isolate`; `Isolated`, `IsolateOptions`, `Sendable`.
- `flux:ffi`: `FfiArg`, `SymbolDecl.optional`, `readMemory(ptr, count, type?)`.
- `@solidrt/3d`: `scene.pick`, `scene.raycast`, `scene.setBackground`, `SceneOptions.background`.
- `@solidrt/3d`: `Hit`, `ScenePointerEvent`, `PointerEventProps` and the `onPointer*` props.

### Developer experience
- **Parallel dev servers** - `-s, --session <N>`; `srt mcp` finds its project's server.
- **Dev data under `~/.solidrt/`** - servers and client data trees in one place.
- **`srt client` picks one server** - `--server` wins over `-s`; neither opens the connect screen.
- **Isolate modules ride the toolchain** - served, packed to bytecode, typechecked.

### Breaking changes
- **`-c` now means `--client`** - `--compile` lost its short flag.
- **Dev client data moved to `~/.solidrt/clients/`.**
- **`readMemory` counts elements, not bytes.**
- **REPL `load` is bound to the project root.**
- **Default text weight is Medium** - was Regular.
- **SolidJS 2.0.0-rc.0** - bump peer deps.

### Agents
- **Scaffold `AGENTS.md`** - isolates, sessions, MCP server resolution.
- **`packages/3d/AGENTS.md`** - pick, raycast, BVH, background.
- New 3D examples: `pick`, `scene-background`.

### Various
- Dependencies: SolidJS `2.0.0-rc.0`; `libloading` (win/mac).
- **Frame driver and surface liveness move into alloy.**
- **Launcher lives in `apps/launcher`.**
- OKF restructured; `KNOWN_ISSUES.md` removed.

## 0.0.48 - 2026-08-11

### Fixes
- None.

### Features
- **3D: quaternion rotation** - nodes store a quaternion, so aiming and interpolation are gimbal-free.
- **3D: aiming verbs** - `lookAt` points a node at a world point, `quatFromTo` aims any other axis, `quatSlerp` damps a follow.
- **3D: swept solids** - `sweep` and `tube` run a profile along a polyline with mitred joints and per-point creasing.
- **3D: per-mesh uniforms as a prop** - `<Mesh params>` writes a custom material's uniforms, the declarative face of `setMeshParams`.
- **GPU: array uniforms** - `vec3 uLight[4]` takes one flat array under its bare name, so a light list or palette is one write.

### API
- `@solidrt/3d`: `lookAt`, `worldPosition`, `getRotation`; `TransformProps.quaternion`, `MeshProps.params`.
- `@solidrt/3d`: `sweep`, `tube`, `pathFrames`, and the `PathPoint` / `SweepPath` / `PathFrames` types.
- `@solidrt/3d`: `quat`, `quatFromEuler`, `eulerFromQuat`, `quatFromAxisAngle`, `quatFromTo`, `quatFromFrame`, `quatMultiply`, `quatNormalize`, `quatSlerp`, the `Quat` type.
- `@solidrt/core`: `getOwner`, `runWithOwner`, `createOwner`, `isDisposed` and the `Owner` type, re-exported from SolidJS.

### Developer experience
- **`srt render` fails a short capture** - writing fewer frames than asked for now exits non-zero instead of reporting success.
- **`--duration` takes fractions** - a sub-second render no longer needs a whole second.
- A bad runtime flag reports a usage error instead of panicking.

### Breaking changes
- **`rotation` follows Three's `Euler` XYZ order** - multi-axis triples rotate differently than before.
- **`lookAt` from `@solidrt/3d` is the scene verb** - the view-matrix builder stays on the `/math` subpath.

### Agents
- **`packages/3d/AGENTS.md`** - quaternion storage, the Euler boundary, the sweep path model.
- New examples: `aim`, `sweep-paths`.

### Various
- Dependencies: no updates.
- **alloy owns the platform facts** - input state and the present clock move down out of lattice, leaving the paced timeline above.
- Backlog filed: float texture formats, sampleable depth, alpha translucency, depth func; GPU pipeline extensions closed.

## 0.0.47 - 2026-08-10

### Fixes
- **A JavaScript error no longer takes the app down** - handlers, callbacks and property writes report and carry on.
- **`fetch` silently dropped headers and bodies it could not marshal** - they throw now.
- **Hit testing** - `viewBox` clip and scroll offsets, and padded views, resolved against the wrong box.
- **GPU repaint** - texture content changes count as damage, a `params` write re-baked twice, the stats overlay froze on texture-only frames.

### Features
- **Input: frame-batched multi-pointer delivery** - one same-age batch per frame, ending in a `pointerFrame` event.
- **Gestures: `createTransform`** - pan, pinch and rotate as one recognizer; core now owns it, `createPan` and the arena.
- **3D: geometry and profiles** - circle, cone, ring, torus with real UVs, plus `extrude` / `lathe` / `shape` and `withColors`.
- **3D: custom materials** - a standard uniform set, `@solidrt/3d/glsl` lighting pieces, `<Scene output>`, `scene.project`.
- **GPU: shared target params tolerate zero coverage** - shared state no longer depends on write order.
- **Frame-stepped time** - GUI timers and `performance.now()` run on the paced frame timeline (see Breaking changes).
- **Faster microtasks** - `queueMicrotask` enqueues natively, dropping the promise machinery every reactive flush paid for.

### API
- `@solidrt/core`: `createTransform`; `createPan` and `arena`, moved from `@solidrt/components`; the `Element` type.
- `srt:events`: `pointerFrame`. Also `performance.timeOrigin`.
- `flux:gpu`: `captureSnapshot` resolves `{ width, height, data }` (see Breaking changes).
- `@solidrt/3d`: `circle`, `cone`, `ring`, `torus`, `withColors`, `fillColors`, `VERTEX_LAYOUTS`, `normalMatrix`.
- `@solidrt/3d`: `extrude` / `lathe` / `shape` / `fillet` / `roundRect` / `triangulate`, the `/glsl` subpath, `SceneProps.output`, `scene.project`.

### Developer experience
- **Clock control** - `set_time_scale` and `step_frames` freeze or advance a client, so snapshots stop racing animations.
- **Input injection** - `send_input` drives pointer and key input into a running client.
- **Snapshots crop and scale**, and node properties read back through the render-tree query.
- **`list_clients`** - the server's entry and project dir, per client the platform, version, profile, capabilities and queries.

### Breaking changes
- **`captureSnapshot` returns pixels, not a texture** - nothing to free; upload with `createTexture` to display it.
- **GUI timers are frame-stepped** - one-frame resolution, one fire per interval per frame; `Date.now()` stays wall time.
- **`fetch` and `parseColor` throw** where they used to drop the value or paint black.
- **`VERTEX_LAYOUT` is now `VERTEX_LAYOUTS`** in `@solidrt/3d`, keyed by layout name.
- One behavior change: `atob` tolerates missing `=` padding, per WHATWG.

### Agents
- **Scaffold `AGENTS.md`: what you paint with** - the tiers that replace CSS, and which web reflex maps to which.
- **`packages/3d/AGENTS.md`** - profiles, vertex layouts, the standard uniform set, the `output` prop.
- New example: `scene-post-effect`.

### Various
- Dependencies: flux drops `base64`, since the engine provides `atob` / `btoa`. No other updates.
- `@solidrt/3d` is marked experimental: expect more API churn there than elsewhere.
- Tests added across flux, alloy and lattice.
- Backlog filed: content-damage cost, texture params write path, parallel dev servers, MCP verification surface, and more.

## 0.0.46 - 2026-08-06

### Fixes
- None.

### Features
- **New `@solidrt/3d` package** - the SolidRT variant of Three.js.
- **GPU: shared target params/textures** - values every draw entry reads; one write per camera move.
- **GPU: unified target verbs** - `setTargetParams` / `setTargetTextures` / `setTargetSize` on every target kind.
- **Image encoding** - `encodeImage` reverses `decodeImage`; the codec now lives in `flux:image`.
- **App arguments** - apps read theirs from `flux:process` `argv`; `argv[0]` is the first argument.

### API
- New package `@solidrt/3d`.
- `flux:gpu` and `@solidrt/core/gpu`: `setTargetParams` / `setTargetTextures` / `setTargetSize`.
- `createDrawTarget` gains positional `params` and `opts.textures`.
- `flux:image`: `decodeImage`, `encodeImage`; core re-exports `encodeImage`.
- `CreateOptions.autoFree` replaces `manual` (see Breaking changes).
- `flux:process` `argv`.

### Developer experience
- **`srt render` needs no display** - runs over SSH and in CI; captures are machine-independent.
- Render frames land in the invoking directory; `-o` picks another.
- Everything after a bare `--` reaches the app, on every client of a dev session.
- **`exit()` ends a render run** - `--duration` is now an upper bound.
- Scaffolded `tsconfig.json` allows `.ts`/`.tsx` import extensions.

### Breaking changes
- **`setShader*` removed** - use the `setTarget*` verbs; fails at typecheck (TS) or import (JS).
- **`manual: true` is now `autoFree: false`** - plain JS ignores the old option silently (double-free risk).
- **`createDrawTarget`** - options move behind positional `params`; a `clearColor`-only bag silently reads as params.
- **`argv` reshaped** - no executable/script entries; `argv[0]` is the first app argument.
- **SolidJS peer bump** - to `2.0.0-beta.31`; move your `solid-js` / `@solidjs/*` pins with it.

### Agents
- **`packages/3d/AGENTS.md`** - the scene model, material uniform contract, and traps.
- New examples: `gpu-shared-params`, `scene-basic`.

### Various
- Dependencies: SolidJS family to `2.0.0-beta.31` (see Breaking changes).
- `@solidrt/3d` joins the published package set.
- Changelog screenshot script under `scripts/changelog/`.
- Backlog filed: 3D roadmap, shared draw params, headless render determinism, and more.

## 0.0.45 - 2026-08-04

### Fixes
- **viewBox hit testing** - hits now go through the `viewBox` transform, so hit targets line up with what you see (regression test added).
- [windows] **Present fences** - missing `glFlush` meant post-swap ANGLE/D3D11 fences never signalled; frames could stall.

### Features
- **GPU: the low-level API can now render a 3D scene frame** - N meshes, one material each, one shared depth buffer, one pass. That completes with retained draw lists per target (`createDrawTarget`, add/remove with stable ids), explicit draw order, index buffers, cull mode, and per-instance attributes.
- **GPU: subtree shader effects** - a `<view>` can run a fragment shader over its own rasterization (params, extra textures, outset, and `previous` for source-history effects like cross-dissolve). Hit testing stays on the undistorted geometry.

### API
- `@solidrt/core/gpu`: `createDrawTarget` + `addDraw` / `removeDraw` / `setDrawOrder` / `setDrawParams` / `setDrawTextures` / `setDrawRange`; new `DrawId`, `CullMode`, `IndexBinding`, `IndexFormat`, `IndexRange` types.
- `createRenderPipeline` gains `instanceAttributes`, `instanceBuffer`, `cull`; `setDraw` accepts an index range.
- `<view shader={...}>` with the new `ViewShaderProps`.

### Developer experience
- None.

### Breaking changes
- None. One behavior change: `instanceCount` now defaults to one instance per instance-buffer record (was required).

### Agents
- **Scaffold `AGENTS.md`** - agents must first check for an already-running dev server + client (MCP `list_clients`) and work against it via `reload` / `get_logs` / `get_snapshot`, instead of starting a second `srt run`.
- **New examples** covering the new surfaces: `gpu-draw-list`, `view-shader`, `view-shader-history`, `view-viewbox` (plus README index and a `responsive-grid` touch-up).

### Various
- Dependencies: no updates.
- Tests now run in release CI, with previously-uncommitted lattice and flux tests added.
- [windows] Build: `Makefile.windows` + static-CRT cmake toolchain file.
- Backlog filed: ANGLE present-fence pacing, adaptive fence depth, GPU cube maps, GPU uniform arrays.

---

Releases before 0.0.45 are not covered here; see the git log.
