---
type: backlog-item
title: parseSvg replaces the svg primitive
description: Remove the <svg>/<d-svg> element in favor of a parseSvg function (forge core, flux:svg module) returning plain draw data that JS maps to d-path subtrees; vector currency becomes path data, matching the texture-id rule that rejected <image>.
status: done
timestamp: 2026-07-30T00:00:00Z
completed: 2026-07-31T00:00:00Z
---

DONE 2026-07-31, as a single sweep rather than the stages below (no
backwards-compatibility requirement, so no coexistence period). Decisions
made at pickup:

- Fluid boxes: option (a) - a `viewBox={[w, h]}` prop on view/d-view, the
  innermost step of the memoized View matrix (paint + hit share it, so
  per-path hit-testing works in design coordinates under fluid scaling).
  Pure fit only, NO intrinsic measure: the box comes from layout props.
- Gradients shipped in the same change (stage 2 folded in): the branded
  gradient wire format gained optional `units: "absolute"`, `spread`, and
  `transform` (SVG matrix sextet) fields, emitted by parseSvg and decoded in
  gui/properties/paint.rs; the JS factories are unchanged.
- The old element's stroke-width bug (group scale baked into geometry but
  not stroke width) is fixed in forge::svg: width scales by sqrt|det|.
- flux:svg takes `color` as a packed u32 (no CSS parsing in Rust); the core
  `parseSvg` re-export accepts any CSS color string via parseColor. Solid
  draw colors come out as `#rrggbbaa` strings, ready for the path color prop.
- An `svg` template tag (String.raw, mirroring `glsl`) ships from core for
  editor highlighting of inline documents.
- No cross-instance parse cache: a memo per call site, revisit if an icon
  grid shows up hot.

Original design below, kept for the rationale and the accepted ceiling.

# parseSvg replaces the svg primitive

Decided 2026-07-30. The `<svg>` element is the outlier primitive: it swallows
a resource, parses markup inside the renderer, and hands back no handle. The
currency rule that rejected an `<image>` primitive (raster currency = texture
id) applies identically here: vector currency = path data. `parseSvg` parses
once and returns data; JS composes ordinary `<d-path>` subtrees from it.

What this buys beyond consistency:

- usvg leaves alloy (27 crates of markup parsing out of the rendering layer).
- One parse per document instead of one per node: `<svg>` keeps its parsed
  tree per element, so N instances of the same icon parse N times; parseSvg
  under a memo parses once.
- Per-path hit-testing and animation become possible; today the svg node is
  one opaque unit by construction.

## API

    // flux:svg, re-exported from core as parseSvg
    parseSvg(src: string, opts?: { color?: Color }): {
      width: number, height: number,      // document/viewBox size
      draws: Array<{ d, color, drawStyle, fillRule?,
                     strokeWidth?, strokeCap?, strokeJoin? }>
    }

Fill and stroke come out as separate entries (mirroring the current
`DrawCmd` flattening in `alloy/src/rendertree/kinds/svg.rs`), so the JS side
is a branch-free map to `<d-path>`. `opts.color` drives `currentColor` at
parse time, exactly as `set_color` does today; note today's element also
re-parses on every color change (set_color invalidates `built`), so a memoed
parseSvg is no regression on recolor. Later refinement if theme-reactive
recoloring shows up hot: mark which draws were currentColor-driven so JS
substitutes colors without re-parsing.

## Layering

The usvg-to-draws conversion (today `kinds/svg.rs` collect/convert_path/
resolve_paint) becomes a forge core owning the usvg dep, returning plain data
only (d strings, rgba, stroke params - no alloy types), same pattern as
`forge/sqlite.rs`. `flux/src/plugins/modules/svg.rs` is the thin marshal.
Sandboxing carries over: both image href resolvers return None, no
resources_dir. `lattice/Cargo.toml`'s comment that resvg rides on alloy's
usvg updates to ride on forge's instead.

## Composition

The consumer wraps draws in a `<d-view>`:

- Scale, known sizes: JS arithmetic on the existing transform prop,
  `transform={scale(size / vbSize)}` - for known sizes this is the correct
  mechanism, not a workaround. Covers every icon usage.
- Scale, fluid boxes: OPEN, choice to be made when picked up. Fluid-sized
  `<svg>` usage is real - emoji faces at `width="95%"` inside percent-sized
  `aspectRatio` cells (`sandbox/emojis/src/index.tsx`), plus Fluent-emoji
  uses in product apps (heroes/animals) - and there JS never knows the pixel
  size, so transform arithmetic cannot replace the element. Options: (a) a
  `viewBox` prop on view/d-view - renderer-side uniform scale-to-fit +
  center + intrinsic measure, essentially old `Svg::build`/`Svg::measure`
  generalized, folded into the shared paint/hit matrix; (b) restructure
  those usages to compute pixel sizes in JS (re-derives layout outside the
  layout engine); (c) treat emoji-scale documents as out of scope for
  d-path subtrees entirely (see ceiling below). Deleting `<svg>` (stage 3)
  is blocked until this is decided.
- Default plain repaintBoundary on the wrapper at the component layer: the
  subtree never changes, so the boundary means it never re-records alongside
  animating siblings. The DL-reuse tier, NOT snapshot (texture per instance,
  blurry under later scaling). parseSvg itself returns data and attaches
  nothing.

`components/Icon` rewrites over this with its public API unchanged.

## Stages

1. forge svg core + `flux:svg` + `parseSvg` re-export from core; rewrite
   `components/Icon` over it. Solid fills/strokes only. `<svg>` untouched.
   Proves the idea end to end with zero renderer edits.
2. Absolute-coordinate gradients exposed to JS (alloy already has
   `GradientUnits::Absolute` internally; the JS factories are box-relative
   0..1 only). Migrate launcher inline icons + app-icon, scaffold gallery,
   examples.
3. Delete: `ElementKind::Svg` + `kinds/svg.rs`, `properties/svg.rs`,
   `SvgProps` + jsx intrinsics, alloy's usvg dep, `alloy/src/tests/svg.rs`.
   Blocked on the fluid-box decision above.

## Parity checklist (same-change items)

- `packages/flux-types` + `docs/flux.md` for the new module (stage 1).
- Doc sweep before deletion: `runtime-modules.d.ts` describes the manifest
  icon as "ready for an `<svg>`"; core examples README + `svg.tsx` example;
  components README/AGENTS Icon sections; `packages/core/AGENTS.md` vector
  guidance.

## Accepted ceiling (unchanged from today)

clipPath, masks, filters, patterns, and SVG text stay unsupported; a
detailed illustration becomes many nodes per instance instead of one.
Concretely for the real usage: a Fluent emoji is 14-21 paths with ~15
gradient defs, so one emoji is ~20-40 d-path nodes and a grid of 30 is
~1000 (no taffy cost - d-paths are no-layout - and a repaintBoundary keeps
re-record at zero, but the node count is real). The emoji usage also means
stage 2 (gradients) is a hard requirement for parity, not a nice-to-have:
emojis render as gray blobs without it.
