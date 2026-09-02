---
title: Alloy code quality review (effects/backdrop era)
description: Multi-agent review of the alloy crate; three confirmed backdrop rendering bugs, structural hazards, and duplication findings, with verified failure scenarios.
created: 2026-09-02
---

# Alloy code quality review

Reviewed 2026-09-02, centered on the newest code: the backdrop-filter,
filter, and shadow work in `rendertree/` plus the window fast path in
`gl/`. Eight parallel finder passes, deduplicated, with adversarial
verification of every correctness claim. Verdicts: CONFIRMED means the
control flow was traced end to end; PLAUSIBLE means unrefuted but not
fully traced (or a design call). Line numbers are as of this review.

## Confirmed rendering bugs

### 1. Backdrop regions lost inside reused Recording boundaries

`composite.rs`. `build_recursive` pushes a panel's damage-widening
region (~530-541) only for nodes the walk enters. A Recording boundary
with a valid cache early-returns (~561-564) without descending, so a
backdrop panel nested inside it pushes no region on reuse frames.
`paint_phase` rebuilds the list from a fresh `BuildContext` each walk
and overwrites `tree.backdrop_regions` (~115-119), so both walk frames
and `resolve_reuse_damage` frames use the incomplete list.

The baked backdrop save_layer DOES re-filter the live window at replay
(`draw_display_list` copies commands into the enclosing list,
tree.rs ~756), so pixels genuinely change; only the repaint rect fails
to grow. `invalidate_paint` fires only for changes inside the boundary,
so damage beneath the panel but outside the boundary reuses the cache
and nothing compensates. Failure: animated content under a nested glass
panel leaves a stale re-filtered band outside the patch rect on EGL
buffer-age partial repaint.

### 2. Snapshot quad path emits the backdrop without the overflow clip

`composite.rs`. All seven snapshot quad sites (~731, 806, 826, 844,
879, 898, 909) call `emit_backdrop` inside `draw_with_transform`
(~488-501), which only saves and applies the matrix - never
`apply_clip`. The rounded overflow clip runs at ~1062 inside the raster
recording (record_clip under Hoist::Transform), so it exists only in
the texture pixels, not the composite-time clip stack. `emit_backdrop`
clips only a plain rect (~437), while its own doc says the overflow
clip applied above is how rounded glass is spelled. The recording path
(`draw_cached_recording` ~474-477) and the inline path (~1062-1070)
both clip before emitting.

Failure: `backdropFilter` + overflow clip + `clipRadius` +
`repaintBoundary="snapshot"` paints the frosted backdrop with square
corners poking past the rounded content; identical markup without the
snapshot boundary renders rounded.

### 3. Element opacity never applies to the backdrop layer

`composite.rs`. At all three composite paths the backdrop save_layer is
pushed and restored directly into the target before the element's
opacity/filter group opens: `record_node` emits at ~1069 before the
effect save_layer at ~1139; `draw_cached_recording` at ~477 before
`draw_dl_with_effects` at ~479; the quad sites emit before
`draw_texture_rect`, whose `quad_paint` carries opacity/filter for the
content only.

Failure: a glass panel animating opacity 0 -> 1 shows the fully
filtered backdrop rectangle from the first frame; opacity 0 does not
hide the element. CSS composites the filtered backdrop inside the
element's group so it fades with the element. No comment marks the
divergence as intentional.

## Confirmed, low severity

### 4. `service_captures` leaks capture-walk backdrop regions

`composite.rs` ~973-997 saves/restores size, content, cull, and the
boundary stats around the capture's `record_node`, but not
`ctx.backdrop_regions`. The capture walk double-pushes the captured
subtree's regions (harmless over-widening); under a non-2D transform it
pushes a `None` entry, and one `None` degrades
`expand_damage_for_backdrops` to Unbounded -> full damage. Net: one
conservative or full frame during a capture, which is a readback frame
anyway. Fix is adding the field to the save/restore list.

### 5. Panels inside snapshot boundaries widen window damage spuriously

The region push does not distinguish panels whose baked backdrop reads
the boundary's offscreen rather than the window. Their window-space
regions are pushed every re-raster frame (and vanish on reuse), so
window-side damage intersecting the panel rect is widened though it
cannot affect the offscreen; a non-2D ancestor forces full damage every
re-raster frame. Verified consequence is over-repaint and region
flip-flop, not stale pixels. Caveat from verification: the widening
from damage INSIDE the snapshot is load-bearing (the panel re-filters
the changed offscreen at re-raster) and must stay.

## Latent structural risks

### 6. The window fast path predicate has two spellings

`rig.rs` names `window_fast_path()` (`window_samples >= 2`) and
`raster/mod.rs` gates repaint-patch production on it, but
`gl/draw.rs:319` (and `readback.rs:56`) still spell the raw
`window_samples(gl) >= 2`. The patch contract depends on the branch
`repaint_patch` predicts being the branch `draw` takes; today they
agree only by coincidence of spelling. If one site grows a term (a
driver-reject latch, a debug override), the frame arrives root-clipped
to the damage rect while draw full-clears FBO 0 - everything outside
the rect presents stale or black. draw.rs and readback.rs should call
`window_fast_path`.

### 7. `emit_backdrop` is hand-pasted into all seven quad closures

`draw_with_transform` has no other callers, so the emission belongs
inside it (pass element + frame); several pasted lines are misindented.
The next snapshot branch that copies `draw_texture_rect` without the
pasted line silently loses backdrop filters on exactly one cache state.

### 8. Backdrop damage widening is a call-site convention

`expand_damage_for_backdrops` must run immediately before
`clamp_damage`, and both damage-resolution paths (`paint_phase` ~118,
`resolve_reuse_damage` ~213) repeat the pair by hand. Folding the
widening into `clamp_damage` (or one `resolve_frame_damage`) makes
every conversion widened by construction; a third resolution path added
without the expand step reproduces bug 1's artifact.

### 9. Blur reach is maintained at three unlinked sites

Cull-rect inflation in `record_node` (~1105), extent inflation in
`cull.rs compute_envelope` (~264), and the backdrop region outset in
`build_recursive` (~523) all apply `blur_outset()` independently, bound
only by prose comments. A directional filter entry (CSS drop-shadow is
the natural next one) applied at one site but not another culls content
the filter pulls into view, or under-covers damage.

### 10. Backdrop panel bounds derived independently in three places

The layout-size-else-inherited box at zero is computed in
`emit_backdrop` (drawn layer bounds), the region push (tracked bounds),
and `cull.rs own_extent`. A shared `backdrop_bounds(element, inherited)`
keeps drawn and tracked regions provably identical.

## Efficiency

### 11. Filter Impeller objects rebuilt every composited frame

`effect_paint` (~394) and `emit_backdrop` (~425) re-run
`FilterState::normalized_matrix` and re-create native
ColorFilter/ImageFilter/Paint (refcounted Impeller FFI allocations) on
every composite draw, though FilterState changes only via
`set_filter`/`set_backdrop_filter`. Roughly three native alloc/free
pairs per filtered panel per frame at 60-120 Hz on srt-ui; the
color-only backdrop case allocates an identity-matrix ImageFilter each
frame. Cache the fused matrix or finished handles on the View at set
time; composite clones a handle.

### 12. Shadowed dashed path runs the dash walk twice per build

`kinds/path.rs:475` (shadow) and `:494` (main stroke) each call
`self.dashed_path(dash)`; the geometry is identical (the shadow only
translates). One hoisted `let dashed = ...` halves the dominant cost of
a marching-ants-style animated dashed path.

### 13. `expand_damage_for_backdrops` is O(n^2) with no region cap

Fine for the expected 1-3 panels, but panel count is app-controlled and
the neighboring damaged-node accumulation is capped. A
MAX_BACKDROP_REGIONS degrade-to-Full guard bounds the worst case (200
blurred list rows -> up to 40k rect intersections per resolve, twice
per frame).

## Duplication and simplification

- Shadow cast rect (origin + dx/dy - spread, size + 2*spread, clamped)
  is verbatim in `Rectangle::build` and `Oval::build`; ShadowState
  already owns the extent math and should own a `cast_rect`. cull.rs's
  hand-maintained `with_shadow` kind list is a third surface.
- `RoundingRadii` construction from `[tl,tr,br,bl]` is copied three
  times: `rect.rs clip_out` (~26), `rect.rs draw` (~48),
  `composite.rs apply_clip` (~338). One shared helper.
- `to_backdrop_image_filter` re-implements `to_image_filter`'s body
  (`filter.rs` ~87/~113), differing only in TileMode and the identity
  fallback; one private `blur_filter(radius, tile)`.
- The unbounded CLIP_INF save_layer rect is spelled twice
  (`composite.rs` ~441, ~1134); one inline fn next to the constant.
- `BackdropRegion = Option<(Rect, f32)>` encodes one global fact
  ("something was unmappable") per entry; a plain Vec plus one
  `regions_unmappable` bool deletes the alias and both unwrap sites.
- `effect_paint(rgb: f32, ...)` takes a bare 0.0/1.0 mode float at four
  call sites; two named constructors or a two-variant enum.

## Conventions and docs

- `window_fast_path` was inserted under `window_samples`' doc block
  (`rig.rs` ~300), so the "FBO 0's multisample count, queried once per
  process" doc now attaches to the wrong (bool-returning) function and
  `window_samples` is undocumented. Move the block down.
- `paraShapes` silently narrowed: after the paragraph-engine removal,
  `note_para_shape` is called only from word-cache shaping, so the
  counter changed population with no rename or note. Possibly intended;
  a rename (wordShapes) or comment removes the ambiguity.
- `tests/effects.rs`: `let eps = 1e-3;` and ~10 inline `1e-4`
  tolerances violate the ALL_CAPS-epsilon rule, and the file adds the
  third+ verbatim copy of the place/rect/bounded/close helper quartet
  (also in cull.rs; rect/close in damage.rs, line.rs, path.rs). A
  shared pub(crate) test helper module stops the spread.
- `filter.rs:18` over-claims "CSS filter semantics": blur converts with
  BLUR_RADIUS_TO_SIGMA = 0.5 (the box-shadow convention), so
  `filter={blur:10}` is sigma 5 where CSS `blur(10px)` means sigma 10.
  The halving is a documented deliberate unified convention
  (okf/backlog/impeller-effects.md, "CSS-style radius" in types.d.ts);
  only the header's blanket claim is wrong for blur.

## Claims investigated and refuted

Recorded so they are not re-raised:

- Backdrop double-emission on transformless boundaries: impossible.
  `own_matrix` returns Some for EVERY View (identity or not), so a
  Hoist::None boundary is necessarily a non-View and `emit_backdrop`
  no-ops for non-Views. The zero-size fallback records inline into the
  window builder and returns before any quad site.
- Missing AA_OUTSET on backdrop extents: harmless. The backdrop is
  clip_rect + save_layer; a clip bounds coverage and cannot feather
  outward past the box, and PresentDamage pads by DAMAGE_PAD_PX = 1.
- flux decode of the new filter properties is correct (spread rejected
  on path, blur non-negative, hueRotate radians pass-through); the
  sepia/hue-rotate/saturate matrices match the SVG feColorMatrix spec;
  region geometry through own_matrix + to_window is consistent because
  map_translate applies the child's layout location first.
