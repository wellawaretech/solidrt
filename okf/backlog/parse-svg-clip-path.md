---
title: Apply SVG clipPath in parseSvg
description: parseSvg drops clipPath (usvg resolves it, forge never reads it) and the render tree has no path clip at all (Impeller's clip_path is never called, overflow clips need a layout box); add a generic clipPath prop on view/d-view/d-path in alloy, emit per-draw clips from forge in flat mode, and carry the same field on groups once tree mode lands.
created: 2026-11-01
---

# Apply SVG clipPath in parseSvg

## Symptom

`parseSvg` ignores `clip-path`: a clipped group draws its full geometry.
Flags, masked avatars and illustrations that crop with a `<clipPath>`
render wrong. The most common case, a Figma export wrapping everything in
`<g clip-path="url(#clip0)">` whose clip is a rect equal to the viewBox,
looks right today only because that clip changes nothing.

The information exists. usvg resolves every clip: `Group::clip_path()`
(usvg sets clip paths on groups only; a `<path clip-path>` becomes a
group) returns a `ClipPath` with `transform()` (objectBoundingBox units
already folded in), `root()` (the clip geometry as a group of paths whose
fill rule is the `clip-rule`) and an optional nested `clip_path()` to
intersect with. `collect()` in `forge/src/svg.rs` never reads it.

The render tree cannot express the result either. Impeller has
`DisplayListBuilder::clip_path(&Path, ClipOperation)`, but alloy only calls
`clip_rect`, `clip_rounded_rect` and `clip_oval` (the overflow clip in
`rendertree/composite.rs` `apply_clip`, the shadow clip-outs). The only clip
an element can carry is `overflow` + `clipRadius`, which needs a layout
box, so a `d-view` cannot clip at all.

## Design

### alloy: a generic path clip

A `clipPath` prop on view, d-view and d-path:

    clipPath?: { d: string; fillRule?: "nonzero" | "evenodd" }[]

A list, not one path: each entry is one successive intersection (SVG
nests clips, and clips on nested groups stack). In node-local content
space, applied after the user chain. Not a parseSvg special: any app can
clip a view to a shape with it.

- Paint: `clip_path(.., Intersect)` per entry under a save; Damage::Paint.
  No layout box needed, so it works on d-views and d-paths.
- Hit test: points outside any entry miss the node and its subtree
  (point-in-path through the lyon code `kinds/path.rs` already uses).
  Clipped-away geometry must not take taps.
- Bounds and cull: intersect with the entries' bounds.
- Boundaries and captures: recorded into content, like the viewBox fit.
- flux `properties/view.rs` (and the path properties) decode; `types.d.ts`;
  one alloy test per walk (paint, hit, bounds).

### forge: clips per draw (flat mode)

The flat draw list has no groups to hang a clip on, so each `SvgDraw`
carries the clip chain of every group above it, baked into document
coordinates: `clip?: { d, fillRule? }[]`, spread onto `<d-path clipPath>`.
The key name on the draw must match the d-path prop so the spread stays
clean. Per entry: map each clip child through the clip's transform and the
group's absolute transform, union the children into one path, then append
the nested `clip_path()` as the next entry.

- Drop a clip whose geometry covers the whole document (the Figma rect).
  Without this every Figma icon pays one stencil clip per draw for nothing.
- Fill rule of a union entry: the shared `clip-rule` of its children.

`Icon` and the player app icon pick it up with no change (they spread the
draws).

### Tree mode

When `parse-svg-tree.md` lands, group nodes carry the same `clipPath`
field in group-local space instead, one clip per group rather than per
draw. That note lists clip paths as staying unsupported; this item
replaces that line.

## Open

- Mixed `clip-rule` inside one `<clipPath>`: one path cannot union
  children with different fill rules. Options: document as a limit (use
  the first child's rule), or convert evenodd children to nonzero
  geometry. Decide at pickup.
- Per-draw cost: a clipped emoji with 20 draws issues 20 clips. Measure on
  Android before shipping; see `rounded-clip-cost-android.md` for how
  clips already hurt there. If it shows, flat mode could merge consecutive
  draws with the same chain behind one clipped d-view, which changes the
  flat shape.

## Out of scope

- `clip-path` on an element inside a `<clipPath>` (rare): skipped,
  documented.
- `<text>` inside a clip: skipped, like text everywhere.
- Masks, filters, patterns: unchanged.

## Stage 1

- alloy: `clipPath` on view/d-view/d-path through paint, hit, bounds,
  boundary; tests.
- flux: property decode; `forge_plugins/svg.rs` marshals `clip`;
  `packages/flux-types/modules/svg.d.ts`.
- forge `svg.rs` + `tests/svg.rs`: chain collection, transform baking,
  nested clip, full-document clip dropped.
- core: `types.d.ts`, `svg.ts` type and doc comment (remove clipPath from
  the unsupported list, also in `forge/src/svg.rs` and the flux types).
- `packages/core/examples/parse-svg.tsx`: one clipped shape.
- Docs: `packages/core/AGENTS.md` / `agents/painting.md` for the prop,
  `docs/reference/elements.md`.
- Verify: `cargo test -p forge -p alloy`, `bun run sol check`, run the
  example under `--project --port 34899` and read hit results back through
  the tree.

## Related

- `done/parse-svg.md`: the flat design and the vector-currency rule.
- `parse-svg-tree.md`: the tree output the group form of this rides on.
- `rounded-clip-cost-android.md`: clip cost on tiled GPUs.
