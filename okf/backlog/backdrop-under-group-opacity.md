---
title: backdropFilter shows no blur while an ancestor fades
description: A backdropFilter under an opacity-below-1 ancestor stays sharp for the whole fade and snaps to full frost when it ends, because the group-opacity layer becomes the backdrop's root; done means either the blur survives the fade or the containment is documented and warned about in dev.
created: 2026-09-21
---

# backdropFilter shows no blur while an ancestor fades

## Symptom

A tile enters with `from: { opacity: 0 }` and contains a view with
`backdropFilter`. While the opacity is below 1 the content beneath is
sharp; the frost switches on in a single frame when the fade ends. Stepped
with the clock frozen, it stays sharp at nearly full opacity and scale.
Reported in [[quartz-heron]] item 4d.

## Cause (probable)

Opacity below 1 draws the subtree into its own offscreen layer, so the
backdrop reads only that layer, which is empty behind the glass. CSS does
the same (an opacity ancestor is a backdrop root), so the behavior itself
may be intended. But the docs describe that containment only for
`repaintBoundary="snapshot"`, not for `opacity` or `filter` layers, and
nothing warns.

## Done looks like

Pick one, the first being the better experience:

- apply group opacity at composite time for a subtree that contains a
  backdrop, as the runtime already does for a `repaintBoundary` view
  (performance.md rule 5), so the glass keeps its blur while it fades; a
  fading glass pane is the common entrance, and "fade the colours instead"
  is not a workaround that composes;
- or document the containment for `opacity` and `filter` layers next to
  the snapshot one, and warn in dev when a `backdropFilter` sits under an
  `opacity < 1` ancestor.

First check whether `repaintBoundary` on the fading ancestor already keeps
the blur (untested by the reporter); if it does, the fix may be to take
that path automatically.
