---
title: Alloy architecture review
description: Structural review of the alloy crate; macro-architecture is sound, the recurring debt is policies held by call-site convention, with a ranked list of wins and an explicit do-not-churn list.
created: 2026-09-02
---

# Alloy architecture review

Companion to [alloy-code-quality-review.md](alloy-code-quality-review.md)
(same day, line-level). This one is at module altitude: boundaries,
layering, threading, ownership. Four parallel subsystem reviews
(rendertree; raster+gl; gpu+context; app shell + spatial + media),
synthesized. Line numbers are as of this review.

## Verdict

The macro-architecture is sound and none of the load-bearing decisions
need to change. The crate-level dependency graph is clean: rendertree
depends only on impellers plus two gpu handle types, spatial is
freestanding behind a SinkWriter trait, context/ is the deliberate hub,
and the one gl -> raster edge is a shared vocabulary type (DamageRect),
not a cycle of behavior. The three-thread contract, the single ordered
RasterCmd channel with its fire-and-forget vs blocking-RPC split, the
demand-driven frame loop with one-line load shedding, and the
FrameDriver typestate pipeline are all correct and localized.

The recurring debt, showing up independently in every subsystem, is one
pattern: **a policy that must hold across N sites is enforced by
call-site convention (comments, copy-paste discipline) instead of by a
type or a single owner.** Today's confirmed backdrop bugs (quality
review items 1, 2, 6) are all instances. The wins below are mostly
"give the convention an owner"; almost none add new abstraction.

## Ranked wins

### 1. Give damage/partial-repaint single owners on both threads

The damage hotspot spans both threads and is where the recent bugs
live; display-list-op-cost (formerly partial-repaint-android) is planned work that will touch all of
it, so this pays off before that starts.

- UI side: a `DamageLedger` (rendertree/damage.rs) owning
  RenderTree.damaged/damaged_full/frame_damage/backdrop_regions
  (tree.rs:46-54) plus the resolve logic now split across paint_phase,
  resolve_reuse_damage, expand_damage_for_backdrops
  (composite.rs:73-232). Kills the expand-then-clamp call-site
  convention (quality review item 8) by construction.
- Raster side: a `DamageTracker` owning the seven fields
  pending_damage/damage_ring/damage_ring_size/buffer_age/
  buffer_age_tried/damage_hints/damage_hint_failed
  (raster/mod.rs:384-399) with fold/take_patch/presented/
  not_presented/invalidated. The present-retry path currently updates
  the ring by hand at four sites (mod.rs:1056-1078, 1202-1205).
- Route type: `repaint_patch` returns
  `enum WindowRoute { FastPath, Rig(Option<DamageRect>) }` and
  gl/draw.rs consumes it instead of re-deriving `window_samples >= 2`
  (draw.rs:319). A patch can then only exist paired with the rig; fixes
  the two-spellings hazard (quality review item 6) at the root.
  readback.rs:56 legitimately keeps the raw predicate (glReadPixels
  cannot read multisampled regardless of route).

Done 2026-09-02: DamageLedger in rendertree/damage.rs, DamageTracker and
WindowRoute in raster/repaint.rs, gl/draw.rs consuming the route. Verified
live (spin example: 1094/1097 presents partial, 61 fps, no missed
presents).

### 2. Reify the boundary composite in composite.rs

`snapshot_node_uncalled` (composite.rs:605-918) is a 3x2 decision
matrix where every leg re-spells hoisted transform + backdrop emission
+ effect paint + quad draw; that is the structural cause of
"emit_backdrop pasted into 7 quad closures" and of quality-review bugs
2 and 6. A small `BoundaryComposite { own_matrix, element, frame, src,
dst, quad_paint }` with one draw method, plus a `SnapshotKey`
(width/height/scale/outset, spelled four times today) with one
`matches`. Move Hoist + snapshot_node* + draw_cached_recording + cache
structs into rendertree/boundary.rs; composite.rs keeps phases +
record_node/build_recursive at ~600 lines each. Highest value per line
in the crate; shaded snapshots will keep growing.

Done 2026-09-02: boundary.rs holds Hoist, the cache structs, SnapshotKey
(one matches() for the four storage compares) and BoundaryComposite (one
draw path for all seven quad legs and both inline fallbacks; the shared
src/dst/outset derivation lives in its constructor); painted_box is now
shared with service_captures. Verified: alloy tests, boundary_transform
and boundary_detached GL examples, and a live view-shader-history
dissolve pixel-exact against the expected uMix blend. Surfaced
pre-existing: captures of nodes inside a valid boundary cache failed;
fixed same day (okf/done/capture-inside-valid-boundary-cache.md).

### 3. Split gpu/ into protocol half and GL half

gpu/ is two modules wearing one name: vocab/spec/limits/lease/order/
TextureRegistry run on the UI thread (imported throughout context/),
while ShaderTexture/passes/VAOs/SamplerCache are raster-only GL.
gpu/texture.rs alone spans vocabulary (17-300), GL SamplerCache
(304-390), and the UI-side registry (396+). Nothing but convention
stops context/ from calling a GL-taking export like
gpu::generate_mipmap. Splitting (file moves + re-export shuffling,
start with texture.rs) makes the "srt-ui has zero GL" contract
structural instead of conventional, and makes the mirrored context/X
vs gpu/X naming truthful.

Done 2026-09-02: the GL half moved into gl/ (buffer, program, pass,
target, timing as git mvs; SamplerCache + GpuTexture + generate_mipmap
split out as gl/texture.rs; GpuLimits::query became gl::query_limits;
the prev_* restore helpers moved to gl/mod.rs). gpu/ is now the pure
protocol layer - vocab, spec, limits, lease, order, resources, texture
vocabulary + UI-side TextureRegistry - with zero glow-context use (the
u32 constant mappings on vocab enums stay, they cannot execute GL).
GpuTexture left the public lib.rs surface (no external consumers). The
invariant is greppable: `HasContext` appears only under gl/, raster/,
and threads.rs. Verified: cargo check + 387 alloy tests, alloy
examples check, flux --lib --features gui (59 tests), lattice check.
Sequencing note honored: the gpu/target.rs storage/entry split (item
4) now applies to gl/target.rs, after this move.

### 4. Mechanical file splits (navigational, zero behavior change)

- raster/mod.rs (2468 lines) along its five responsibilities; the
  capture.rs pattern (impl RasterState block in a submodule,
  pub(super)) already proves the shape: frame.rs, repaint.rs (home of
  the DamageTracker), compose.rs, targets.rs, resources.rs; mod.rs
  keeps state + stats + run() at ~800. Rename capture.rs to
  offscreen.rs: it holds snapshot/node-capture rasterization, and the
  name collides with capture_frames (playback mode).
- gpu/target.rs (1732): storage.rs + entry.rs splits, and replace the
  ~17 first-entry accessors (entry0()-based, lines 979-1160) with one
  `entry0_info() -> Option<GpuDrawInfo>` since draw_infos() already
  produces the same data per entry.
- rendertree/tree.rs (1415): impl-block splits into tree/transitions.rs
  (the enter/exit/stagger lifecycle, ~370 lines of animation policy),
  tree/geometry.rs, tree/inspect.rs.

### 5. Extract the vsync frame-release state machine from App::run

The frame-release policy in the 540-line App::run loop is seven
mutable locals (app.rs:296-312) mutated at four mutually-dependent
sites (400-427, 563-603, 642-660, 374-381), invariants held in
comments, testable today only on an Android device. liveness.rs is the
in-crate template (pure state machine, caller performs effects, clock
injected): a `FrameRelease` fed on_present/on_signal/set_pacing,
returning emit/arm decisions, unit-tested next to src/tests/. While in
the loop: fold the four inline platform pollers (screen keyboard,
hardware keyboard hotplug, power, refresh rate; app.rs:452-461,
719-740) into one PlatformWatch that emits events on transitions.

### 6. Small, cheap policy consolidations

Each is under an hour and removes one convention:

- End-of-frame housekeeping (drain content changes + release snapshot
  textures + reclaim destroys) is spelled at three frame producers,
  two with comments admitting it (composite.rs:125-142, 246-250,
  frame.rs:114-123). Two free functions, called from all three.
- Route UpdateShaderParams/UpdateShaderTextures/SetDraw through
  entry_write (raster/mod.rs:2098; three hand-rolled copies at
  784-828); a `timed_pass(target, f)` helper for the pass-accounting
  quadruplet copy-pasted at four sites.
- `RasterCmd::invalidates_resolved_content()` in cmd.rs so the
  clean-tree exemption list (raster/mod.rs:621) lives beside the enum
  it classifies.
- Fold Context's six parallel per-target maps (targets,
  shader_sources, manual_targets, sub_targets, depth_ids, orders;
  context/mod.rs:58-158) into TargetMirror with one insert/remove
  doorway; reclaim_destroyed currently must remember six removals
  (context/texture.rs:515-524).
- Spell out container arms in rendertree kind dispatch instead of
  `_ =>` (mod.rs:183, 197, 226, 256; hit.rs:94) so a new ElementKind
  is compiler-enumerated, not silently no-opped.
- `Element::frame_size(inherited)` for the layout-size-else-inherited
  derivation repeated at 8+ sites, and one shared `painted_box` for the
  detached-node derivation duplicated between snapshot_node_uncalled
  and service_captures.

### 7. Boundary hygiene, opportunistic

- AlloyEvent/AlloyCommand pass raw SDL types for cursor, orientation,
  power, theme (event.rs:26, 234-236, 261, 277) while the same
  boundary scrupulously translates keys (keymap.rs:1-6). Two small
  alloy enums translated where keys already are.
- run_playback_loop calls std::process::exit from library code
  (playback.rs:72-74); return a result and let lattice exit.
- Micro: gpu_resources doc attached to depth_owner
  (context/mod.rs:303-310); context/capture "offscreen" naming above.

## Do not churn (explicit)

- tree.rs/composite.rs seam (mutation+invalidation vs read+emit),
  frame.rs FrameDriver typestate, the Damage closure contract
  (a write cannot compile without reporting scope), cull.rs, the
  hit.rs vs router.rs split, the taffy adapter seam, kinds/ as
  enum+match (right for a closed set; only fix the `_ =>` arms).
- The RasterCmd protocol: flat 41-variant enum with per-variant
  calling-convention docs is the better artifact; no sub-enums. The
  fire-and-forget vs RPC rule is applied uniformly.
- Capture/playback as a bool checked at ~5 genuine divergence points;
  a strategy trait would over-engineer it.
- RasterState registry maps stay flat: they are routinely
  split-borrowed and the free functions exist exactly to dodge those
  borrows. Group only never-overlapping concerns (damage; optionally
  the present-pacing quintet).
- The gpu/context mirror discipline: raster-side dependency edges are
  derived from authoritative bindings at each flush, never stored as a
  second graph; UI allocates all ids, creates reply with adopted
  handles; deferred destruction swept against live references. Keep.
- No generic Registry<T> across resource types: per-type behavior
  dominates, and a new resource type touching cmd.rs/dispatch/gpu/
  context/resources.rs is each layer doing its real job.
- threads.rs, liveness.rs, present.rs, vsync.rs, backend.rs GlBinding,
  spatial/ (clean citizen via SinkWriter, no bypass), the media
  registries (consistent shape; delivery differs by nature).
- audio.rs ramp raw-pointer protocol: fragile but scheduled to die
  with the SDL_mixer replacement (video-playback backlog item 6);
  refactoring it now is effort on code with a planned expiry.
- lib.rs platform-fact statics and the flat gpu re-exports: accretion
  but documented, JNI-constrained, and heavily consumed as alloy::X.
- Engine-independence holds everywhere checked. One borderline:
  Element::from_kind maps JSX tag strings (tree.rs:131-140); renderer
  protocol vocabulary, defensible; if a non-JS embedder appears the
  tag-string factory moves to the plugin layer.
