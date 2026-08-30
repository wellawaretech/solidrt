# Ideas

One line each. No frontmatter, no ceremony. A shaped idea graduates to a file
in `backlog/` and its line goes: this file holds open ideas only, so it stays
small.

- Evaluate `changesets` for release management: version bumps, changelogs and dist-tag selection across `@solidrt/*`, replacing the hand-rolled bash in release.yml. Only worth it once release cadence picks up or version drift hurts.
- `srt:dev` registerDebug: an async command's Promise JSON-encodes as `{}` with no warning (async is known-unsupported, okf/done/mcp-debug-commands.md) - either await the promise or reject async commands loudly at registration.
- `instances` on an isolate handle: N instances of one module behind one handle, calls spread over them; shape undecided, a pool is userland today (N `isolate()` calls). Left open when okf/done/isolate-follow-ups.md closed.
- Error containment visibility: a native diagnostic flag on a node whose prop or child expression is contained (kept its last value after a throw), so /tree and the inspector show it, plus a dev-only outline. Only if the `Contained error` log line proves insufficient (okf/plans/reactivity-halt-containment.md).
- Focus under `display: "none"`: browsers blur a focused element when an ancestor hides; today a TextInput in a hidden pane keeps focus and keeps receiving keys. Decide whether hiding blurs (and whether re-showing restores), then wire it in the focus session. Left open when okf/done/display-none-subtree.md closed.
- alloy examples: a panic inside the `app.run` closure (srt-ui thread) strands the SDL window black until killed, since main keeps pumping events; `alloy/examples/depth_texture.rs` installs an exiting panic hook locally - lift that into `alloy::setup` for `Mode::Run` so every probe fails fast.
- Slow rebuild frames on the 3d demos: `the-third-dimension` logged paint 70.6 ms for 14 nodes and 85 ms for 8 nodes at startup. Suspicion: a resize frame blocking on raster-thread RPCs (`CreateDrawTarget`, `ResizeShaderTexture`) behind a queue ~10 deep at ~6 ms of GPU work per frame. Unverified. Left open when okf/done/stats-overlay-stale-phase-shares.md closed.
- `@solidrt/2d` `grid()` takes the atlas dimensions separately (`grid(cols, rows, { width, height })`), re-plumbing what `createAtlas` already knows; a `slice(texture, w, h, opts)` helper or letting `Atlas` be built from a `TextureId` closes it for the non-`createAtlas` path.
- `@solidrt/2d` `TileLayer` `clearColor` reads like a background colour but is a per-chunk clear that only touches resident chunks; `chunkClearColor` would be self-documenting.
- `@solidrt/2d` `Sprite` exports `_slot`, `_x`, `_y`, `_w`, `_h`, `_rot`, `_flipX`, `_flipY` on the public type; reading a flip state means `getSprite()` allocating a full `Required<SpriteOptions>`. Either genuinely private or readonly accessors for the cheap reads.
- `@solidrt/2d` AGENTS.md "30k sprites: 12.9 ms raw records vs 30.8 ms via setSprite" is a WRITE-path comparison and reads as "30k is affordable"; add the clause that it excludes whatever computes the motion, which is usually the dominant cost (a 24k-particle sim measured ~25 ms, nearly all simulation, publish nearly free).
- `@solidrt/2d` `layer.records` growth replaces the array; a hoisted reference becomes a detached copy and writes go nowhere silently. The doc states it; a dev-mode generation check on `touch()` would make it loud. Only if it bites again.
