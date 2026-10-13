---
title: Extend Scene/SpriteLayer fill mode with a resolution multiplier and output support
description: Two additive follow-ups to fill-by-default - a dpr/resolution prop for supersampling on top of fill, and fill composed with a custom output leaf.
created: 2026-09-02
completed: 2026-10-13
---

# Extend fill mode: resolution multiplier, fill + output

Fill-by-default landed 2026-09-02: `<Scene>` (3d) and `<SpriteLayer>` (2d)
take `width`/`height` both-or-neither; omitted, the built-in leaf lays out
at 100% and the target (3d) or layer (2d) follows the leaf's box
(getBoundingBoxViewport x displayScale for the 3d target, getLayoutBox for
the 2d layer, with density on the oversample pick). Two asks were
deliberately deferred; both are additive (no shape change to what shipped).

## 1. Resolution multiplier

Problem: fill renders at exactly the on-screen device-pixel size. Apps that
want supersampled 3d (cheap AA beyond `samples`, crisp minified texture
detail) or a cheaper-than-native render (a heavy scene on a weak GPU)
currently have to leave fill for fixed + `output`, giving up everything
fill automates.

Done looks like: a `resolution` prop (default 1) multiplying the tracked
device-pixel size - r3f's `dpr` flattened to one number, Unity's render
scale. `<Scene resolution={2}>` supersamples, `resolution={0.5}` renders at
half density; the leaf layout, events and OrbitCamera viewport are
unaffected (they scale from the layout box, not the target). Reactive.
Validation: finite and > 0, dev-throw.

A budget belongs in the same design: `maxPixels` caps the target's pixel
count, scaling both axes down when the tracked size exceeds it. A fraction
alone cannot hold one cost across devices (a 0.7 that suits a laptop panel
is wasted on a phone and too much on a 4K screen), and fill-rate-bound
scenes are exactly the ones on weak iGPUs at high-DPI panels: a full-pixel
main pass of ~11 ms at 3.4 Mpx dropped to ~5 ms capped at ~1.9 Mpx, with
no visible loss under a HUD. Decide whether both props exist (budget
applied after the multiplier) or one. Without either, the cap means
rebuilding the fill leaf by hand through `output` (a fixed placeholder
size, a 100% `<texture>` with `contain="size"`, `onLayout` plus
`getBoundingBoxViewport` x `displayScale()` into `scene.setSize`, and
`handlersFor` for pointer events), duplicating Scene's own fill code. The
leaf already stretches the texture and the fill handlers already scale
pointer coordinates, so both knobs are a change to the size apply only.

2d symmetry question to settle at
design time: for `<SpriteLayer>` this is arguably just `maxOversample`/
`oversample` already - decide whether 2d gets `resolution` too or the docs
point at the oversample knobs.

Involves: one multiplier in the 3d fill apply (components/scene.tsx), prop
plumbing + docs; a 2d decision. This carries the remainder of stage 3 of
[[2d-layer-display-scale]] (the explicit 3d knob; the auto default half
landed with fill).

## 2. Fill composed with a custom output leaf

Problem: fill currently dev-throws with `output` - the target cannot follow
a leaf it does not own. So a post-effect chain or blendMode composition
still needs the old fixed-size wiring, and those are exactly the apps that
also want density correctness.

Done looks like: an `output` leaf that opts in by reporting its element
back (the callback already runs in scene context, so the natural shape is
spreading `useScene().input.handlersFor(layout)` plus handing the leaf ref
to something like `useScene().follow(el)` - shape to be designed). The
fill machinery then tracks that leaf the same way it tracks the built-in
one. Post-effect targets sampling the scene need to resize with it, so the
contract must say what happens to intermediate targets on resize (the app
owns them; a resize callback or reactive size read).

Involves: API design first (the follow contract), then a small amount of
component code; the AGENTS.md output recipe grows a fill variant.

## Outcome (2026-10-13)

Both halves landed, plus the fill mode of `<View3d>` and `<View2d>`
([3d-scene-views-additive](3d-scene-views-additive.md), the fill bullet
of [2d-layer-views-additive](../backlog/2d-layer-views-additive.md)),
since the follow is one piece of code per package once it exists:
`packages/3d/src/components/fill.ts` (createFill: the on-screen box in
device pixels through the knobs, a reactive size out) and
`packages/2d/src/components/fill.ts` (createViewFill: the view's size
from the layout box, the oversample pick from the on-screen box).

Decisions, with the comparison that drove them:

- The scale is named `renderScale`, not `resolution`: `resolution={0.5}`
  reads like a size. Unity says Render Scale, Unreal Screen Percentage,
  Godot `scaling_3d_scale`. Fill mode only; with width/height either knob
  throws (the fixed contract is that the props ARE the pixels).
- Both knobs exist. `maxPixels` applies after `renderScale`, one factor on
  both axes so the aspect holds; the two answer different questions (how
  much density, what cost on any device). No default budget: Unity and
  Godot default to native, r3f clamps dpr to 2; a cap the runtime picks is
  hidden policy, only the app knows its scene's cost. The docs state the
  cost of that default (a 3x phone renders nine pixels per logical one
  until the app says `maxPixels`).
- 2d symmetry: `maxPixels` exists in 2d too, with the same name and
  meaning, on `<SpriteLayer>`, `<View2d>` and `<TileLayer>` - the auto
  pick already had a budget, hard-coded to the window's device pixels,
  and the prop is that budget made explicit (under the window's own, as
  before). The density knob stays `oversample`, an integer, so the
  resample stays clean; Godot has exactly this split (integer
  `stretch_shrink` for 2d, fractional `scaling_3d_scale` for 3d). The
  2d-layer-display-scale stage 3 is closed by this.
- Fill with `output`: the callback gains a second argument, `leaf`
  (OutputLeaf): `ref` hands the owner the leaf it renders and the target
  follows it as it follows the built-in one; `handlers` are scaled to the
  ref'd leaf's layout box per event in both modes (the silent
  wrong-handlers-on-a-resized-leaf trap of the old recipe is gone);
  `size()` and `deviceSize()` are reactive reads for a post chain,
  `createShaderTextureMemo` consuming them directly, in both packages (in
  2d `size` is view pixels times oversample, so it follows a re-pick
  too). A fill output that
  never refs warns once at the first layout. The "fill needs the
  built-in leaf" throws are gone.
- The resize contract: signal writes apply on the microtask, after the
  frame's paint, and core already flushes pending writes once after all
  onLayout handlers have run (window.ts, runLayoutHandlers), so a post
  chain sized from the reactive reads resizes before the paint with no
  flush of the component's own. A first cut flushed per component; it was
  redundant, and from the effect path a no-op that warns. The first frame
  of the converted example shows the mechanism: its time sits in
  postLayout, not paint.
- The follow is a core primitive, `followBox(what, leaf, apply, { deps,
  missing })` (packages/core/src/follow.ts): the leaf measured after every
  layout and on display-scale changes, in its own units and in device
  pixels, with the empty-leaf and missing-leaf warnings, one stack capture
  per follow. Both packages' fill.ts keep only their size decision; the
  3d decision is pure in fill-math.ts, pinned by tests/fill-math.test.ts
  headless, the way 2d's oversample-math.ts is.
- Owners expose their target size reactively to their children:
  `useScene().viewportSize` (target pixels) and
  `useSpriteLayer().viewportSize` (view pixels), `viewport.size()` as a
  signal. `events` with `output`, and `x`/`y` on a View3d without `into`,
  throw as contradictions.
- Symmetry audit after the first cut: the knob type is `DensityProps` in
  both packages (texels per on-screen pixel, fractional in 3d, integer in
  2d), the 2d prop validation names its component like the 3d one, and
  the fixed-mode knob throw sits in the mount block of Scene and View3d
  alike. What stays different is by design: 2d's knobs act in fixed mode
  too (the pick runs in both), TileLayer has no fill, View2d has no
  `into` yet.
- `examples/scene-post-effect.tsx` converted to fill + `renderScale={2}`
  + `output` with the post target at `leaf.deviceSize()`; scene-views
  stays the fixed-size lane. Verified live on a 1.333 display scale
  window: scene target 3413 x 1920, post target 1707 x 960, the leaf
  draws the post pass.

Tests: packages/3d/tests/fill-knobs.test.tsx (renderScale reactive,
maxPixels factor, output follow with reactive size, View3d fill) and
packages/2d/tests/fill-knobs.test.tsx (output follow, View2d fill,
maxPixels bounding the pick through a designSize fit).

## Non-goals

- TileLayer fill: the tile grid is a creation-fixed world by design.
- Auto-resizing app-owned post-effect chains: the app owns those targets;
  fill can expose the size, not manage the chain.

The shipped fill contract is documented in packages/3d/AGENTS.md (Fill
section) and the SceneProps/SpriteLayerProps doc comments.
