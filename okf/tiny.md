# Tiny

One line each, grouped by area. No frontmatter, no ceremony. Work small enough
that nothing has to be decided: the fix is obvious and it just needs someone
who is already in that file. Delete the line when it is done - a tiny item
carries no reasoning worth keeping, so it leaves no `done/` record.

If you need more than one line, or need to explain why, it is not tiny.
Something that needs a decision, a design or an investigation is an idea
([ideas.md](ideas.md)) or a `backlog/` file.

The headings are areas because these get picked up opportunistically, when you
are in that code anyway. File an item where the work happens, not where the
symptom shows. A heading that outgrows this file splits into its own.

## Core

`packages/core` - the renderer and the reactivity surface.

- renderer.ts leak sentinel (scanForOrphans): it warns only when a NEW element type joins the orphans, so a leak that keeps growing at a stable set of types goes silent after the first warning (one was caught only by reading `orphanNodes` in get_stats); warn again when the total crosses an order of magnitude.
- Enter animations: warn (properties/mod.rs, the `transition` branch) when a config declaring `from` lands on a node already entered whose previous config declared none - a `from` that arrives after the mount frame's advance never plays and nothing says so. Not on a from-to-from swap: `closing() ? OUT : IN` toggles are legitimate.
- Layout slide per-axis motion (Reanimated's curved and sequenced presets): `x`/`y` sub-motions on the `layout` entry, additive on okf/done/transition-layout-animations.md.
- Shared-element layout transitions: a `layoutId` key on the `layout` entry; a node mounting with the id of a node exiting this tick inherits its last box, which the exit ghost already knows (Framer `layoutId`, SwiftUI `matchedGeometryEffect`). After the box lane (okf/backlog/transition-layout-size.md).

## Flux

`flux/` - the JavaScript runtime and its plugins.

- `flux:fs` has no `rename`, so an app doing its own atomic write (temp file, then replace) cannot; add `FluxFile.rename(to)` over `std::fs::rename`. Load-bearing now that `onSuspend`/`onQuit` make the app own its persistence.

## Extensions

`@solidrt/2d` and `@solidrt/3d`.

- `@solidrt/2d` `Sprite` exports `_slot`, `_x`, `_y`, `_w`, `_h`, `_rot`, `_flipX`, `_flipY` on the public type; make them genuinely private or readonly accessors, so reading a flip state stops meaning `getSprite()` allocating a full `Required<SpriteOptions>`.
- `@solidrt/2d` tiles.ts: the chunk math (checkCell, chunkOf, slot, the rect slicing in setTiles) is pure but lives beside the GPU imports, so the tile layer has no headless check; move it to a tiles-math.ts and pin it like oversample-math.ts (probes/2d-tiles-bulk-probe.tsx covers it live only).
- `@solidrt/2d` AGENTS.md "30k sprites: 12.9 ms raw records vs 30.8 ms via setSprite" is a write-path comparison that reads as "30k is affordable"; add the clause that it excludes whatever computes the motion, usually the dominant cost (a 24k-particle sim measured ~25 ms, nearly all simulation).
- `@solidrt/3d` bindSkeleton matches joints by case-insensitive name only; a pipeline that differs by prefix or suffix (Mixamo's `mixamorig:`, a one-sided `_JNT`) needs a `match` option mapping a piece name to a body name - Unity matches exact names and leaves the rest to the app, so add it when a consumer shows up.
- `@solidrt/2d` `SpriteTransition.all` is typed `NodeTransitionSpec`, so it accepts `from`/`exit`, and `toNodeTransition` passes `all` through without the unit lift every other lane gets; type it `NodeMotionSpec` like the core and 3d, where `all` is a motion with no endpoints.
- `@solidrt/2d` `<SpriteLayer>` has no `stagger` prop though `SpriteLayerOptions.stagger` and `layer.setStagger()` exist and `<Scene stagger>` does too, so the layer-root stagger is imperative-only.
- `@solidrt/2d` `Camera2d.camera()` is declared `CameraUpdate` (every field optional) but always returns all six, unlike `layer.camera()` and `view.camera()` in the same package; declare it `CameraState` and drop the `??` at every call site.
- `@solidrt/2d` AGENTS.md never states the colour contract: tint multiplies against premultiplied sRGB texels with no decode, which is right for an unlit pipeline but means `[0.5, 0.5, 0.5]` is a different brightness than in 3d, whose color.ts documents sRGB in, linear shading.

## Components

`packages/components`.

## DX

The `srt` CLI, the dev server, MCP, debug commands, examples and probes.

- `srt:dev` registerDebug: an async command's Promise JSON-encodes as `{}` with no warning; reject async commands loudly at registration (async is known-unsupported, okf/done/mcp-debug-commands.md).
- Control API `POST /input` wheel: `deltaY: 300` reaches the app as about 14 (examples/pick.tsx's ring spun 0.14 rad at 0.01 rad per px); find where the synthetic wheel delta is scaled between the server and the client's wheel event and make it mean pixels like a real wheel, or state the unit in debugging.md.
- `srt render` passes a startup failure: a module that throws before `render()` gets the error window, which renders, so the capture completes and the exit-code gate reads success for a broken app; fail the run instead of building the error engine.

## Runtime

alloy, forge, flux, lattice.

- alloy examples: a panic inside the `app.run` closure (srt-ui thread) strands the SDL window black until killed, since main keeps pumping events; `alloy/examples/depth_texture.rs` installs an exiting panic hook locally, lift that into `alloy::setup` for `Mode::Run` so every probe fails fast.
- alloy spatial `bind_texture_slot`: the palette anchor must be an ancestor of every bound node (documented, unchecked; createModel and bindSkeleton both rely on it); add a debug-build ancestry check so a row bound across hierarchies errors instead of posing wrong.
