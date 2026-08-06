---
type: backlog-item
title: "Target verb unification: one target-level family, declarative draw-target params, lifetime-option rename"
description: Pre-release breaking sweep. setShaderParams/setShaderTextures/setShaderSize and setTargetParams/setTargetTextures are two names for one concept - a target's target-level state - split only by target kind; unify on the setTarget* family routing by kind, route the <texture params> prop through the same channel (draw targets currently have NO declarative uniform channel and the prop warn-fails on them), and rename the lifetime `manual` option out of its collision with render: "manual". Cheapest now; more expensive with every app shipped.
status: open
timestamp: 2026-08-06T00:00:00Z
---

# Target verb unification

Approved plan, 2026-08-06. Three coordinated breaking changes plus riders,
staged so each lands green on its own. No deprecation aliases: everything is
pre-release (0.0.0), the whole migration surface is in-repo, and the audit
below names it exhaustively.

## Landed

Stage 1 (A + B + setTargetSize) landed 2026-08-06. `setTargetParams`/
`setTargetTextures` route by kind in alloy Context (single-program arm =
the folded update_shader_* bodies, verbatim; draw arm = the shared path);
`resize_shader_texture` became `resize_target`; the `setShader*` family is
gone from flux, core, flux-types, and docs; the `<texture params>` build
site calls `set_target_params`, so the prop drives draw targets' shared
params. gpu-shared-params.tsx drives its tint via the prop (uView stays
imperative); draw_list.rs replaced the fixed-target-rejection assertion
with a positive routing assertion (create-time red -> routed white ->
strict uNope error). Verified: workspace cargo check (gui feature),
draw_list + shader_uniforms headless assertions, core/3d/trails tsc gates.
Stages 2 (autoFree rename - naming decision still open) and 3 (riders)
remain.

## A. One target-level verb family

Today three families exist: `setShaderParams`/`setShaderTextures`/
`setShaderSize` (fragment textures + fixed pipeline targets),
`setDrawParams`/`setDrawTextures`/`setDrawRange` (per entry), and
`setTargetParams`/`setTargetTextures` (draw-target shared state). The first
and third are the same concept - target-level state - split by target kind:
for a one-draw target, entry-0 params ARE the target-level params (the
gpu-shared-draw-params item said exactly this).

Change:

- `setTargetParams(id, params)` works on EVERY target, routing by kind:
  fragment texture -> pass params; fixed pipeline target -> entry-0 params;
  draw target -> shared params (current behavior). Validation follows the
  kind: single-program targets validate strictly against the one uniform
  table (today's update_shader_params), draw targets use the coverage rule.
- `setTargetTextures(id, textures)` - same routing for sampler bindings.
- `setTargetSize(id, w, h)` - rename of `setShaderSize` (already works on
  every target kind; the old name reads wrong on a draw target, which is
  what the 3d scene resizes).
- REMOVED from flux:gpu, core re-exports, flux-types, docs:
  `setShaderParams`, `setShaderTextures`, `setShaderSize`.
- Unchanged: the per-entry family (`setDrawParams`/`setDrawTextures`/
  `setDrawRange`), `setDraw` (the fixed kinds' range verb), `setDrawOrder`,
  and `resizeTexture` (pixel textures need seed pixels, so forcing one
  resize verb would mean kind-dependent arity; two honest names beat that).

Engine shape: alloy `Context::set_target_params`/`set_target_textures` gain
the kind dispatch (mirror.entries None -> today's update_shader_* path,
Some -> today's shared path). RasterCmds unchanged - the routing picks
between the existing UpdateShaderParams/UpdateTargetParams (and textures
analogs). update_shader_params/update_shader_textures can then lose their
public JS-facing role (kept or folded as internals, implementer's choice).

## B. `<texture params>` drives every target kind

The prop buffers into pending_params and applies at build via
`ctx.alloy.update_shader_params` (alloy/src/rendertree/kinds/texture.rs,
build()), which REJECTS draw targets - so `<texture src={drawTarget}
params={{...}}>` warn-fails today, making draw targets the only target kind
with no declarative uniform channel while the docs call the prop "the
preferred way". Point the build site at the routing method from A; the
prop then means "the target's params" uniformly. Build-time failures stay
warnings (same as today). No new declarative textures prop - bindings stay
imperative-only by design.

## C. Lifetime option rename: `manual` -> `autoFree: false`

`render: "manual"` (who renders) and `manual: true` (who frees) collide so
badly that every doc block spends a sentence disambiguating them. Rename
the lifetime opt-out to `autoFree: false` (default true; most literal
option; alternatives considered: keep/retain - shorter but vaguer).
JS-layer only: the flux plugin never sees the lifetime option. Rename
consistently EVERYWHERE the owner-scoped auto-free opt-out appears - core
gpu.ts CreateOptions and every create helper, @solidrt/3d SceneOptions
(and geometry.ts notes), any other create helper found by a repo-wide
`manual` audit (image/sound/camera wrappers) - or the rename creates a new
inconsistency. `render: "manual"` stays.

## Riders (non-breaking)

- Document the program-state leak publicly (docs/core.md + flux-types): an
  active uniform nothing ever writes reads whatever the shared program
  object last held, from ANY entry or target sharing the program - seed
  everything you declare. No automatic fix exists: zero-filling unset
  uniforms at addDraw would defeat entry-beats-shared, and add-time
  coverage validation breaks the legitimate add-then-set-shared ordering
  the 3d scene itself uses.
- @solidrt/3d: after _attach adds an entry and the camera has synced at
  least once, re-issue `setTargetParams(texture, { uViewProj })` (same
  value, one write). A scene whose ONLY material lacks uViewProj then
  throws at attach with the engine's coverage message instead of at the
  next camera move.
- Internal, optional: shader_sources key enum (Shared vs Entry(id))
  replacing the entry-key-0 sentinel; DrawSpec.instance_buffer 0-sentinel
  -> Option.

## Migration surface (audited 2026-08-06)

setShader* call sites: packages/core/src/gpu.ts (re-exports AND
createShaderTextureMemo internals), packages/3d/src/scene.ts
(setShaderSize), packages/flux-types/gui/gpu.d.ts,
examples/trails/src/index.tsx + index2.tsx, docs/core.md. Engine:
flux/src/plugins/gui/gpu.rs (declare/export/closures),
alloy/src/context.rs (routing), alloy/src/rendertree/kinds/texture.rs
(build call). Lifetime `manual`: ~12 sites, mainly packages/3d
(scene.ts, geometry.ts) + core gpu.ts option types + docs. Also audit
AGENTS.md files (core, scaffold, 3d) and packages/core/examples for both.

## Stages

1. A + B + the setTargetSize rename: one coherent breaking change, every
   call site migrated, cargo check + draw_list.rs example + CI tsc gate
   green, gpu-shared-params.tsx extended to drive its shared tint via the
   `<texture params>` prop (demonstrating B) with uView staying imperative.
2. C: the mechanical lifetime rename, repo-wide, own verification pass.
3. Riders, docs-first; internal enum cleanups last and only if cheap.

## Decisions still open

- `autoFree: false` vs `keep`/`retain` (recommendation: autoFree).
- `setTargetSize` vs `resizeTarget` (recommendation: setTargetSize, the
  set-prefix matches the family it joins).

Related: [gpu-shared-draw-params](gpu-shared-draw-params.md) (the shared
channel this builds on), [gpu-params-positional](gpu-params-positional.md)
(the positional convention, untouched), okf/analysis and the
ffi-write-batching item (orthogonal transport-cost track).
