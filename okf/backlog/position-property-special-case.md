---
title: position is the one layout prop handled outside the layout adapter
description: Supporting positioning-context-relative bounding boxes forced position out of the style adapter into a special case at the top of apply_jsx, splitting it from its sibling insets.
created: 2026-08-14
---

# position is the one layout prop handled outside the layout adapter

Every layout prop flows through one uniform path: the style adapter in
[flux/src/alloy_plugins/properties/layout.rs](../../flux/src/alloy_plugins/properties/layout.rs)
decodes it into the taffy style. Except `position`, which is special-cased at
the top of `apply_jsx` ([flux/src/alloy_plugins/properties/mod.rs:62](../../flux/src/alloy_plugins/properties/mod.rs))
and routed through `Element::set_position`
([alloy/src/rendertree/mod.rs:386](../../alloy/src/rendertree/mod.rs)).

Why it ended up there: `set_position` has a second effect beyond the style. It
also sets `positioning_context` on LayoutData, the flag that stops the ancestor
walk for container-relative bounding boxes
([alloy/src/rendertree/tree.rs:365](../../alloy/src/rendertree/tree.rs)). The
style adapter has no way to report a side effect back and no access to the
element, so the prop had to be lifted out of it.

The cost is that `position` is now separated from `top`/`left`/`right`/`bottom`,
which still go through the adapter, and a reader of either path sees only half
of what positioning does. The next prop that needs an element-level side effect
will copy the special case rather than fix it.

Candidate directions, none evaluated:

- let the style adapter return side effects alongside the decoded style, so the
  caller applies them (keeps the adapter pure, adds a return type)
- give the adapter access to the element (simplest, weakens the boundary that
  made the adapter testable in isolation)
- derive `positioning_context` where it is read instead of storing it - the walk
  in tree.rs could ask the style directly, and the flag disappears

The third is worth checking first: if the flag is redundant with
`style.position`, the side effect goes away and `position` returns to the
adapter with no new machinery.

Source: root TODO.md, migrated 2026-08-14.
