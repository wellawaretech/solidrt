---
title: Detached-view transform origin pivots around the inherited box
description: Fixed 2026-08-03 - a d-view's unset transform origin now defaults to its local (0,0), the point its children's coordinates are authored against, instead of the inherited box's centre (which made the drawn position depend on window size). Explicit origins are unchanged; drawn-bounds-centre was considered and rejected.
created: 2026-08-02
completed: 2026-08-03
---

# Detached-view transform origin pivots around the inherited box

Source: the animated-explainer demo feedback (2026-08-02), its biggest
single cost. TransformProps documented "Unset defaults to the axis center",
but for a d-view - which has no layout box of its own - the centre used was
the INHERITED box's.

Probed on a 1692x1128 window, three groups each containing a d-rect at
local (0,0,200,100):

| group | result |
| --- | --- |
| `<d-view x={100} y={100}>` | drawn at (100,100), 200x100 - as expected |
| `<d-view x={100} y={300} scale={0.5}>` | drawn at (522,579), 100x50 |
| `<d-view x={100} y={500} scale={0.5} originX={0} originY={0}>` | drawn at (100,500), 100x50 |

The middle row was the trap: with the origin unset, the scale pivoted
around the centre of the inherited layout box (here the window) in the
group's pre-translation space, and x/y applied afterwards. The group landed
~420 px from where its own coordinates said - and because the pivot was the
window centre, the same code put the group somewhere else on a differently
sized window. Cost observed: the demo gave up on group transforms entirely
and lerped every geometry number in JS - about 40 property writes per frame
doing the job of one animated scale.

## Decision and fix (2026-08-03)

A detached view's unset origin now defaults to its LOCAL (0,0) - the middle
probe row behaves like the third. Rationale:

- (0,0) is the one point that is always defined, never moves, and is what
  every other detached construct anchors to: d-rect/d-oval x/y (box
  top-left), d-line endpoints, path `d` coordinates, the d-view's own x/y,
  viewBox design space. SVG groups pivot at the user-space origin for the
  same reason.
- "Own drawn bounds centre" (the item's original preference) was rejected:
  a d-view has no computed own-bounds anywhere (even captures use the
  inherited box), so it would need new subtree-bounds machinery plus
  invalidation coupling (child geometry changes clearing ancestor transform
  caches), and the pivot would drift while content animates - rotation
  around a moving point wobbles the group.
- Centering is one explicit line, since the author wrote the child
  coordinates: `originX={100} originY={50}` for content in a 200x100 local
  space.

Explicit origins are UNCHANGED, including the documented wrinkle that
pct()/keyword origins on a d-view resolve against the inherited box (rarely
wanted; use pixels). Laid-out views keep the box-centre default.

Shipped: `detached` flag on the View kind set in `Element::no_layout`
(covers every construction path), fallback switch in `View::resolve_center`
(alloy/src/rendertree/kinds/view.rs) - build, hit-test and bounding-box
paths all go through the same compose, so they agree for free. Tests in
alloy/src/tests/view.rs. Documented in TransformProps (types.d.ts), core
AGENTS.md (d-view bullet), scaffold AGENTS.md (gotcha 19).
