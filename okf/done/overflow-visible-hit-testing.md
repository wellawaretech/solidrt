---
title: Children drawn outside their parent's box are not hit-testable
description: A parent's bounds check gates descent into its children as well as its own hit, so a child painted outside the parent's layout box under overflow visible receives no pointer events.
created: 2026-08-14
completed: 2026-09-24
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

## Done (2026-09-24)

`hit_recursive` splits the two decisions. A node's own hit is still its
box (its hover region does not change); a miss on an Auto node descends
into its children when the subtree's paint envelope - the extent the paint
walk already caches per node for viewport culling (cull.rs, keyed on the
inherited frame the hit walk passes too) - contains the point. The hit
extent never exceeds the paint envelope (every laid-out box is in it), so
a point outside it misses the whole subtree for one rect test and the walk
stays a box test per sibling in the common case; a subtree not painted
yet, or behind a 3D transform, descends. A clipping parent still stops at
its box, and `pointer-events: all` captures nothing outside its box. The
parent joins the path only for a descendant's hit, so the path stays a
root-to-leaf chain (bubbling, `locals_along_path`) and the parent receives
enter/leave through its overflowing child, as the DOM does. No union
bound had to be maintained: the envelope cache was already there.

Tests in `alloy/src/tests/hit.rs`. Verified live with a trigger view and a
dropdown child below its box (`probes/batch3-probe.tsx`): a tap on the
dropdown reached its handler, and a pointer moving onto it entered the
trigger.
