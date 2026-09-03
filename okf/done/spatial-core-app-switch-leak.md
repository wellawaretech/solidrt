---
title: Reset the spatial core on app switch
description: The alloy Context (and with it the spatial core - nodes, sinks, palettes, clip players) is shared across engine rebuilds and never reset, while the dying app's GPU resources are destroyed on engine drop. Leaving an animated 3d app leaves its clip players running against destroyed textures: a warning per frame and a launcher stuck at full frame rate forever.
created: 2026-09-03
completed: 2026-09-03
---

# Reset the spatial core on app switch

## Symptom

Leaving heroes-v2 for the launcher, the log fills with one line per frame,
forever:

```
[spatial] sink write dropped: texture 59 not found
```

The noise is the visible half. The costly half is that the launcher never
goes idle: it runs a full-rate frame loop animating a dead app's skeleton
into a texture that no longer exists.

## Why

`alloy::Context` is created once per process and cloned into every engine
spin (`lattice/src/lib.rs`), so the spatial core - nodes, draw sinks,
shared slots, palettes, instance records, clips and clip players - outlives
the engine that created it. The reload loop closes the dying app's cameras,
microphones and audio, but nothing resets spatial, and
`flux/src/alloy_plugins/spatial.rs` has no `Drop`.

The GPU side does clean up: `TextureInner::drop` in
`flux/src/alloy_plugins/gpu.rs` destroys every texture, buffer, pipeline,
program and stage the dying app created, and the launcher's first composite
reclaims them (a fresh render tree references nothing, so
`reclaim_destroyed` removes the entry).

That asymmetry is the bug. The skin palette texture is gone; the texture
slot sinks bound to it, and the players driving them, are not. Every
launcher frame then:

1. `frame::advance` -> `advance_players` steps the leftover players. They
   keep themselves alive because their clip and target nodes are alive, and
   a looping clip never finishes.
2. The new poses write node TRS, `recompute` restages the palette rows and
   sets the group dirty.
3. `spatial_flush` -> `write_texture` finds no texture entry and warns.
4. `players.active` and the flush's `wrote` both latch `request_frame()`.

Stale `clipEvent` payloads from the dead app's players land in the
launcher's JS the same way.

## What was done

Leaving an app leaves no spatial state behind, and a sink write that
cannot land is attempted once, not once per frame.

- `alloy::Context::reset_spatial` replaces the core with `Spatial::new()`
  (it owns nodes, sinks, groups, clips and players). The lattice reload
  loop calls it beside the camera/microphone/audio closes. At the first
  spin it is a no-op, and the new engine has no nodes yet, so there is no
  ordering hazard.
- `SinkWriter` writes now return whether they landed. The context's writer
  logs the drop and reports false; `Spatial` releases the binding that
  produced it - the node's draw sink in `recompute`/`set_sink_count`, or
  the shared/instance/palette group in `flush`. A slot still naming a
  dropped group stages nothing (the staging paths look the group up and
  skip), so a dead binding costs exactly one warning. This also fixes the
  `wrote` latch: the writer sets it only for a write that went out, so a
  dropped write no longer demands a frame.

## Not done, on purpose

The asymmetry between the GPU plugin's per-engine `Drop` and the spatial
core is left as is. Lattice is the only embedder, and its reload loop is
already where cross-engine state is released (cameras, microphones,
audio); a per-engine record of nodes, clips and players inside the spatial
plugin would duplicate that ownership for an embedder that does not
exist. If a second embedder appears, that is the moment to give the
plugin a `Drop` and move the reset there.
