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
- `@solidrt/2d` `<SpriteLayer>`/`<View2d>` and `@solidrt/3d` `<Scene>`/`<View3d>`: `pointer` given together with `events={false}` silently feeds nothing (the feed listens at a root no event reaches); throw at mount, the dev validation policy.
- `@solidrt/2d` AGENTS.md "30k sprites: 12.9 ms raw records vs 30.8 ms via setSprite" is a write-path comparison that reads as "30k is affordable"; add the clause that it excludes whatever computes the motion, usually the dominant cost (a 24k-particle sim measured ~25 ms, nearly all simulation).
- `@solidrt/3d` bindSkeleton matches joints by case-insensitive name only; a pipeline that differs by prefix or suffix (Mixamo's `mixamorig:`, a one-sided `_JNT`) needs a `match` option mapping a piece name to a body name - Unity matches exact names and leaves the rest to the app, so add it when a consumer shows up.
- `@solidrt/3d` scene.ts: an `overrideMaterial` view silently drops every instanced mesh whose record layout the override does not declare (the documented rule), which is invisible in the output - warn once per view in dev, naming the view label and the count.
- `@solidrt/3d` `unlit()` takes no `blend`, so there is no stock additive material and a glow drops to `shaderMaterial`; `shaderMaterial` already has the option and the fog docs call `blend: "add"` out, so pass it through the unlit class key.
- `@solidrt/3d` a ReflectionProbe's `dispose()` destroys its chain cube while the scene may still be pointing at it as the environment (a subtree that owns the environment leaves the scene sampling a destroyed texture until something re-bakes); clear the environment when the cube it names is the one being destroyed.

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
