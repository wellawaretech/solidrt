---
title: App-level runtime configuration
description: A start-only `solidrt.runtime` block in package.json for tunables that today are compile-time constants (JS stack size, word cache size, paragraph engine, fetch cache cap, log level), carried by the manifest and applied when an app is activated.
created: 2026-08-17
---

# App-level runtime configuration

## Problem

Every runtime tunable is a `const` in some crate. An app author cannot
change any of them, and the ones that already say they should be tunable
are the ones apps will hit first:

- `JS_STACK_SIZE = 64 MiB` (lattice/src/lib.rs), comment: "tunable down
  per-app later". `EngineConfig.stack_size` is the only knob already
  plumbed to the engine builder.
- Word cache `CAPACITY = 8192` entries (alloy/src/rendertree/text/words.rs).
  Text-heavy apps want more; small devices want less.
- `Text.paragraph_engine` (alloy/src/rendertree/text/mod.rs) is a
  Rust-only bool with no way to flip it from an app. It is the escape hatch
  while the owned engine matures (see [text-layout-owned](../done/text-layout-owned.md)).
- `FETCH_CACHE_MAX_BYTES = 256 MiB` (flux/src/standards_plugins/fetch.rs),
  doc comment: "placeholder cap until a real default is decided".
- Log level exists only as the `SRT_LOG` env var; a packed app cannot ship
  a level.

There is no config file to put these in. The only app-author config
surface is the `solidrt` key of the nearest `package.json` (`fonts`,
`icon`, `appId`, `org`, `displayName`), read by
`packages/cli/src/project.ts` and carried to the runtime by the version
manifest (`lattice/src/manifest.rs`) for both dev pushes and packs.

## Decision

Start-only. Values are read when an app is activated (dev push applies,
packed binary boots) and never change while it runs. No runtime setter, no
hot reload of config without an app reload. This covers everything on the
list and keeps the runtime side a plain struct handed to constructors.

## Shape

`package.json`:

```json
{
  "solidrt": {
    "appId": "com.example.app",
    "runtime": {
      "jsStackSize": 33554432,
      "wordCacheSize": 16384,
      "paragraphEngine": false,
      "fetchCacheBytes": 134217728,
      "logLevel": "warn"
    }
  }
}
```

Every key optional; absent means the current constant. Sizes are bytes or
entry counts as plain integers, no unit suffix parsing. Unknown keys and
wrong types: CLI fails the build (dev/prod validation policy: everything
today is dev, so sites throw). The runtime trusts the manifest and applies
defaults for anything missing, so a manifest from an older CLI still loads.

## Plumbing

1. CLI (`project.ts`): validate `pkg.solidrt.runtime`, copy it verbatim
   into the manifest as `runtime`. One typed object, one validation site.
2. `lattice/src/manifest.rs`: `#[serde(default)] pub runtime:
   RuntimeConfig`, all fields `Option<_>`, `Default` = none set.
3. Application, at the point the app is activated (the reload path in
   `lattice/src/lib.rs` where the engine builder is assembled):
   - `jsStackSize` -> `FluxEngine::builder().stack_size(..)` (exists).
   - `fetchCacheBytes` -> new `EngineConfig` field, read where the fetch
     plugin opens its cache instead of the constant.
   - `wordCacheSize`, `paragraphEngine` -> alloy: the rendertree already
     gets rebuilt/reset per app; give `WordCache::new` a capacity and
     `Text` a default for `paragraph_engine` sourced from a
     `rendertree::TextConfig`, set through the existing alloy command
     channel at activation. Rendertree stays engine-independent: it takes
     the struct, not the manifest.
   - `logLevel` -> `alloy::logging` level, applied at activation; `SRT_LOG`
     env var wins when set (developer override beats app default).
4. Docs: `packages/cli/scaffold/AGENTS.md` (or wherever `solidrt.fonts` is
   documented) gets the key list with defaults.

## Not in this stage

- GPU/window-level knobs (`MSAA_SAMPLES`, swap interval): the go client
  creates its window once and hosts many apps, so these cannot be per-app
  without window recreation. Packed-only would be a target special case.
  Own item if wanted.
- JS memory limit / GC threshold: neither is set today (rquickjs
  defaults). Natural neighbours of `jsStackSize` in the same block; add
  when there is a reason to, not speculatively.
- Layout caches (`MEASURE_ENTRIES`, `MAX_CACHED_WIDTHS`), fetch
  concurrency/retry, websocket backpressure, pacing constants: no app has
  asked. The block is where they go if one does.
- Gesture slop and responsive breakpoints in `packages/core`: theme/design
  level, not runtime config.
- Instance-buffer growth in `@solidrt/2d` and `@solidrt/3d` (candidates,
  raised 2026-08-23 with [2d-layer-capacity-growth](../done/2d-layer-capacity-growth.md)):
  the initial reservation and the growth factor. Today the reservation is
  `createSpriteLayer({ capacity })` (default 1024) in 2d and, in 3d,
  implicit in the records given to `createInstancedMesh` (reserve by
  passing a larger array and a smaller `count`); an explicit `capacity`
  option on `createInstancedMesh` / `<InstancedMesh>` would make the two
  packages speak the same word. The growth factor is 2x in both (amortized
  O(1); waste bounded at 100% of the live size) and hard-coded; 1.5x is the
  lower-waste alternative. Both are per-object library defaults, so the
  natural home is the create options, not the runtime block - listed here
  so they are decided alongside the other tunables rather than ad hoc.
- The scene light cap in `@solidrt/3d` (`MAX_LIGHTS`, 4, raised
  2026-08-23 with the lit material): a GLSL array size baked into every
  lit fragment and into the scene's shared light list, so it is fixed per
  app, not per scene. Four directional lights cover non-PBR scenes; an app
  wanting more would raise it here rather than per material.
- Any runtime-mutable config, env-var mirrors for the new keys, or a
  separate config file.

Neighbour: [shared-config-constants](shared-config-constants.md) collects
the cross-crate NAMES and identities (cache file names, the client
identity) that need one code-level home, not an app surface; the fetch
cache cap is the one value on both lists.

## Done looks like

An app sets `solidrt.runtime.jsStackSize` and `wordCacheSize` in
`package.json`; `srt dev` push and `srt pack` both apply them, visible via
the stats overlay / MCP `get_stats` (word cache capacity) and a deliberate
deep-recursion probe (stack size). A wrong type fails `srt check` with a
sentence-case message naming the key.
