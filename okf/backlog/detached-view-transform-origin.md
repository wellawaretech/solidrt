---
type: backlog-item
title: Detached-view transform origin pivots around the inherited box
description: With originX/originY unset, scale/rotate on a d-view pivots around the centre of the inherited layout box, so the drawn position depends on window size - a silent correctness bug in resizable apps; default the origin to the view's own drawn bounds, or document the rule.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# Detached-view transform origin pivots around the inherited box

Source: the animated-explainer demo feedback (2026-08-02), its biggest
single cost. TransformProps documents "Unset defaults to the axis center",
but for a d-view - which has no layout box of its own - the centre used is
the INHERITED box's.

Probed on a 1692x1128 window, three groups each containing a d-rect at
local (0,0,200,100):

| group | result |
| --- | --- |
| `<d-view x={100} y={100}>` | drawn at (100,100), 200x100 - as expected |
| `<d-view x={100} y={300} scale={0.5}>` | drawn at (522,579), 100x50 |
| `<d-view x={100} y={500} scale={0.5} originX={0} originY={0}>` | drawn at (100,500), 100x50 |

The middle row is the trap: with the origin unset, the scale pivots around
the centre of the inherited layout box (here the window) in the group's
pre-translation space, and x/y apply afterwards. The group lands ~420 px
from where its own coordinates say - and because the pivot is the window
centre, the same code puts the group somewhere else on a differently sized
window. For a resizable, multi-device runtime that is a silent correctness
bug in user code, not just a surprise.

Cost observed: the demo gave up on group transforms entirely and lerped
every geometry number (x, width, height, pitch, font size, radius) in JS -
about 40 property writes per frame doing the job of one animated scale.

Suggestions, in preference order:

1. Default a detached view's transform origin to its own drawn bounds. That
   is the only origin stable under window resize, and it is what "the
   element's centre" means to the person writing it.
2. Failing that, document the actual rule in TransformProps and in the
   detached-positioning example, with the resize consequence spelled out
   ("to scale or rotate a detached group in place, set originX/originY").
