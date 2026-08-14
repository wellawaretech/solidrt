---
title: Line vs path - the segment primitive
description: Decided 2026-08-02 - line stays, as the primitive whose geometry is numbers (animatable endpoints, dash) against path's DSL string; a laid-out line is a rule (thin box; in general the box's top-left-to-bottom-right diagonal), no mirror prop; growth direction is caps and arrowhead markers, added when a design asks.
created: 2026-08-02
completed: 2026-08-02
---

# Line vs path - the segment primitive

Started as "what is a laid-out line for?" (the animated-explainer demo
feedback, 2026-08-02: a stale not-registered note cost the demo its callout
lines) and widened into "why have `<line>` at all when `<path>` exists?".
Decided 2026-08-02.

## The decision

Line stays, and its identity is: the segment whose geometry is NUMBERS.
`path` is the general instrument (any geometry, via the `d` DSL string);
`line` earns its place by being writable and animatable as data - each
endpoint is one typed property write that diffs per-prop, where the "same"
segment as a path re-parses a rebuilt string every frame. It also already
carries dashing (onLength/offLength), which PathProps does not.

The layout form is a rule: give it a thin box (length x strokeWidth), which
falls out of the general definition - endpoints default to
`(0,0) -> (box_w, box_h)` (alloy/src/rendertree/kinds/line.rs), so a
zero-height box is a horizontal rule and a zero-width box a vertical one.
The general diagonal is the engine's no-special-case rule, not a feature
anyone targets; genuinely angled lines are `d-line`'s job (endpoints, per
the detached-only geometry decision 2026-08-01) or a path's. No mirror /
anti-diagonal prop: the one hypothetical user (an X over a layout cell) is
rare and expressible with two d-lines.

Documented 2026-08-02: LineProps doc comment in types.d.ts, plus the
intrinsics bullet in core AGENTS.md.

## Recorded growth direction (open when a design asks)

Line's natural extensions are exactly what path strings are bad at, and all
stay numeric/enum props:

- Stroke caps (butt/round/square) - line ends are where caps matter.
- Arrowhead/marker ends (capStart/capEnd) - one concrete design ask
  already: the explainer demo's worst shipped defect was two hand-built
  arrowhead constructions, built from rects and radius tricks because
  nothing provides them.
- Possibly a polyline points array - the numbers-not-string middle ground.

Per minimal-first these are not being built speculatively; open a fresh
item when the next design asks (the arrowhead one is closest).
