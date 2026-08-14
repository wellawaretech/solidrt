---
title: Children drawn outside their parent's box are not hit-testable
description: A parent's bounds check gates descent into its children as well as its own hit, so a child painted outside the parent's layout box under overflow visible receives no pointer events.
created: 2026-08-14
---

# Children drawn outside their parent's box are not hit-testable

What it looks like when you hit it: a child positioned outside its parent
View's layout box paints correctly (a dropdown hanging below its trigger, a
badge overhanging a corner) but does not respond to taps. Paint honors
`overflow: visible`; hit testing does not. The asymmetry is invisible until
someone tries to click the thing they can see.

Cause: in `hit_recursive` ([alloy/src/rendertree/hit.rs:164](../../alloy/src/rendertree/hit.rs))
the parent's `is_in_bounds` check gates two separate decisions at once - whether
the parent itself is hit, and whether the recursion descends into its children.
A View's bounds is its layout box, so a point outside the box stops the walk
before any child is considered.

## What the fix has to preserve

Splitting the two is the whole job, but the parent's own self-hit must stay
tied to its box. Pointer enter/leave depends on that: `examples/recurse` relies
on the View's box defining its hover region, so making descent unconditional by
widening the parent's bounds would break hover instead.

The shape: descend into children whenever the subtree could contain the point,
gated only by the overflow clip (a clipping parent still stops the walk at its
box, correctly), while the parent's own hit continues to test its box. That
likely means the recursion needs the child extent, not just the parent's box,
to decide descent - so the cost question is whether hit testing needs a union
bound per subtree and where it would be maintained.

Source: root TODO.md, migrated 2026-08-14. Sibling of `done/overflow-viewbox-clip.md`,
which settled the paint side.
