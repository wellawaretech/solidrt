---
title: Extend Scene/SpriteLayer fill mode with a resolution multiplier and output support
description: Two additive follow-ups to fill-by-default - a dpr/resolution prop for supersampling on top of fill, and fill composed with a custom output leaf.
created: 2026-09-02
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
Validation: finite and > 0, dev-throw. 2d symmetry question to settle at
design time: for `<SpriteLayer>` this is arguably just `maxOversample`/
`oversample` already - decide whether 2d gets `resolution` too or the docs
point at the oversample knobs.

Involves: one multiplier in the 3d fill apply (components.tsx), prop
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

## Non-goals

- TileLayer fill: the tile grid is a creation-fixed world by design.
- Auto-resizing app-owned post-effect chains: the app owns those targets;
  fill can expose the size, not manage the chain.

The shipped fill contract is documented in packages/3d/AGENTS.md (Fill
section) and the SceneProps/SpriteLayerProps doc comments.
