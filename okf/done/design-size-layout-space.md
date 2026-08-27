---
title: designSize as layout space
description: Children of a design-size view are laid out at the design size, not the real box, so a laid-out subtree scales into any box without reflowing; from the outside the view sizes like a replaced element (the texture's rules, but compressible) with the design size as its intrinsic size. Decided 2026-08-27 as one rule, no opt-in.
created: 2026-08-27
completed: 2026-08-27
---

# designSize as layout space

Graduated from `ideas.md` 2026-08-27; the console tile probe
(`apps/console/SPEC.md`) was the first user.

## Symptom

`designSize` only "just worked" for `d-*` content. Ordinary flex children under
a design-size view were laid out against the REAL box and then scaled by the fit:
a `flex={1}` child covered a fraction `s` of the box when the fit minified and
overflowed it when it magnified; only `s = 1` looked right. Every other path
already handed the children the design size - paint (`composite.rs` child
walk), culling (`cull::child_frame`), hit testing (`hit.rs`) and bounding
boxes (`tree.rs` ancestor walk); taffy was the one hole. Apps faked the rule
with a fixed inner box of the design size plus a `flexShrink={0}` that is
mandatory and non-obvious.

## Decisions

- **One rule, no opt-in.** Children of a design-size view live in design space
  for layout too. The old layout behaviour was never useful (a double
  transform), and no existing user had laid-out children under a design size.
- **Replaced element from the outside.** The view's intrinsic size is the
  design size, with the texture's `<img>` rules (one known axis derives the
  other from the design aspect), shared as `rendertree::replaced_size`. One
  deliberate difference from a texture: the view is compressible - a
  min-content query gets zero - because a design has no size it cannot scale
  below, and the canonical `<view flex={1} designSize>` must fit a window
  smaller than its design instead of overflowing it.
- Padding on a design-size view insets the children in design units (the inner
  pass resolves it); its border still paints in box space. Documented, not
  engineered.

## Naming

Shipped as `designSize`, not `viewBox`. The prop was implemented under SVG's
name and renamed the same day, once the generalization was clear: it is not a
box (there is no min-x/min-y - a transform already translates, so the design
size is the only irreducible half), and `<view>` is a layout element, so it
should speak the layout platform's vocabulary rather than the graphics
format's. Same reason a future fit control would be `fit="cover"` on the
texture's `object-fit` enum (`types.d.ts`), never `preserveAspectRatio`. SVG's
viewBox stays the lineage, documented on the prop; it is not the name.

## Mechanics

`LayoutContext::compute_child_layout` treats a View with a non-degenerate
`design_size` (`View::design_space`) as a layout boundary
(`LayoutContext::design_size_layout`):

- outside: `compute_leaf_layout` with the replaced-element measure;
- inside, on `PerformLayout` only: the container algorithm for the view's
  display runs with a constant synthetic `LayoutInput` (known and parent
  size = design, available = Definite(design), `ContentSize` so the view's
  own size styles stay with the outer box). Its output is discarded; the
  children's placements are the point.

`View::set_design_size` reports `Damage::Layout` (was Paint): the design size is
now a layout input. Paint, hit, cull and bounding boxes needed no change.

What falls out: the inner input is constant, so a resize re-solves nothing
below the view (the children's caches hit) and a repaint boundary below the
view keeps its recording; a boundary AT the view re-records, since the fit is
content. Nesting composes: an inner design-size view is laid out in the outer's
design space, its children in its own.

## Done looks like

- `alloy/src/tests/layout.rs`: children at the design size in both fit
  directions; replaced-element sizing (unsized in a column, one sized axis);
  compression below the design; a resize keeps the children cached; laid-out
  children hit in design space, both directions.
- `flux/src/tests/properties.rs`: designSize damage is Layout.
- Docs rewritten off "never sizes the element": `types.d.ts`,
  `docs/reference/elements.md`, `examples/README.md`, `view-design-size.tsx`
  (with a laid-out row), `packages/core/AGENTS.md`.

## Dropped: a style aspect ratio derived from the designSize

Proposed as stage 2 so a width-only design-size view in a flex row would take
the design height instead of the line's. Tested before building
(`design_size_view_in_a_row_stretches_unless_aligned`): taffy's stretch step
reads the RAW style cross size (`child_style.size().cross(dir).is_auto()`,
flexbox.rs), so a style `aspect_ratio` - derived or the explicit
`aspectRatio` prop - never stops the stretch; the item is 100x300 either
way and 100x50 only under `alignSelf: flex-start`. That is CSS's rule for
an `<img>` in a flex row too, so the answer is documentation (the designSize
prop doc and packages/core/AGENTS.md name the lever: non-stretch
`alignSelf`/`alignItems`, or size both axes) plus the pinning test, not
runtime plumbing. Wrapping tile grids were never affected: their lines size
to content, so stretch lands on the aspect height.

## Findings

- taffy 0.12 passes the flex automatic-minimum-size query to a leaf's measure
  as `AvailableSpace::MinContent` on the main axis (`flexbox.rs`,
  `resolved_minimum_main_size`), so compressibility is a measure-closure
  concern: the design-size view returns zero under any MinContent query and
  `<view flex={1} designSize>` shrinks into a window smaller than its design.
  A texture never sees this (its measure ignores `available`), which is why
  it stays incompressible, like an `<img>`.
- A container's own `known_dimensions` dominate its style sizes in taffy's
  flexbox/block/grid entry (`styled_based_known_dimensions =
  known_dimensions.or(...)`), and `SizingMode::ContentSize` drops the style
  size/min/max entirely; the inner pass therefore needs no style surgery to
  pin the design box.
- A window resize only clears the root's taffy cache
  (`layout_phase` -> `invalidate_cache(root)` walks UP), so the children of a
  design-size view answer from cache across resizes with no further work -
  pinned by `design_size_children_survive_a_resize_from_cache`.
- Verified end-to-end on Linux GL (2026-08-27) with `view-design-size.tsx` on a
  827x1077 logical window: fit 1.292, and the laid-out row reads back from
  `/tree` at x 51.69 = 40 * 1.292, y 750.42 = 280.06 (letterbox) + 364 *
  1.292, width 723.63 = (640 - 80) * 1.292 - design units through the fit,
  where box-space layout would have given (827 - 80) * 1.292 = 965.
- Verified on Android (2026-08-27, SM-T500 tablet, Adreno 610, client built
  from this tree) against the same server over `--lan`: 1333x728 logical,
  fit 1.82 with side letterbox (tx 84.1); the row reads back at x 156.9 =
  84.1 + 40 * 1.82, y 662.48 = 364 * 1.82, width 1019.2 = 560 * 1.82, and
  the text measures 242.6 design px on both devices - one layout, no reflow
  across form factors. Launched with the `srt android` intent extra only
  (`am start ... --es srt_dev_server host:port` after `force-stop`), since
  the command's install step would replace a locally built APK with the
  published one.
- Degenerate designSize values (zero, negative, NaN, infinite) used to leave
  the tree inconsistent: no fit scale, yet paint, hit testing, culling and
  bounding boxes read the raw `design_size` and handed the children a zero
  frame - and the layout boundary made it worse by ignoring such a designSize
  altogether. Fixed 2026-08-27: the property layer throws on a non-positive
  or non-finite extent (`design_size_rejects_a_degenerate_design_space`), and
  every reader in the tree goes through `View::design_space()`, the one
  definition of "the design size".
