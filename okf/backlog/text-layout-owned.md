---
title: Own the text layout, demote drawParagraph to a shaper
description: Impeller's paragraph is a black box for line breaking, so inline elements, exclusions, custom breaking and cheap re-layout are unreachable; experiment with a pretext-style split where every paragraph is a single-line single-style run we measure once and place ourselves.
created: 2026-08-16
---

# Own the text layout, demote drawParagraph to a shaper

Experimental. If it does not pan out, [text-inline-spans](text-inline-spans.md)
is the fallback: same `<span>` API on top of Impeller's style stack. The API
is engine-neutral; this item is about what sits under it.

## Problem

`<text>` hands the whole string to one Impeller `Paragraph`, which shapes,
breaks lines, aligns, ellipsizes and draws. Everything above shaping is a
black box, and Impeller's C API has no placeholders, no exclusion shapes,
no custom breaking, and re-lays out from scratch per width (hence the
16-slot width cache in text.rs). Structurally unreachable today:

- inline elements: an icon, chip, badge, texture or `<pressable>` link mid
  sentence,
- variable-width lines: text around a floated image, in a circle, drop
  caps, per-line indent,
- custom breaking: balanced or pretty last lines, hyphenation, hanging
  punctuation, orphan control,
- cheap animated width (resizable panels, zoom): re-layout as arithmetic
  instead of reshaping,
- streaming and huge text: append segments and re-break only the tail,
  measure height without materializing paragraphs, virtualize by line.

And one that is about pixels, not layout: Impeller's glyph path is
grayscale-only antialiasing with no text gamma or contrast compensation, so
thin light-on-dark stems bleed into the background and read as hairlines.
The `FontWeight::Medium` default in text.rs is a workaround for that (see
[dpi-aware-default-font-weight](dpi-aware-default-font-weight.md)). Nothing
in the C API lets us tune it. Owning layout is what makes the rasterizer
replaceable, so text quality on dark backgrounds is a concrete driver here,
not a side effect.

## Approach

The pretext idea (chenglou/pretext): decouple line breaking from both
shaping and rendering. Segment once, measure each segment once with the
platform shaper (canvas `measureText` there), then layout is pure arithmetic
over cached widths and the caller draws per line.

Our shaper and rasterizer is the Impeller paragraph, and it is the only text
drawing primitive the C API has (`DrawParagraph`; no draw-glyphs or
draw-run). So it stays, but shrinks to `measureText` + `fillText`: every
paragraph we build is single-line, single-style, effectively infinite width,
no maxLines/alignment/ellipsis. It shapes one run and tells us width,
ascent, descent and glyph positions. Everything above lives in alloy:

- segmentation and break opportunities (`unicode-segmentation`,
  `unicode-linebreak` for UAX #14) - never below word level, so joining
  scripts keep shaping correctly inside a run,
- a (segment, style) -> run paragraph cache; the cache entries are the
  draw objects, so drawing a line is placing cached paragraphs at offsets,
- line breaking, alignment, justify, ellipsis, max lines, indent,
  exclusions, in the Taffy measure func,
- inline atoms: any element measured by Taffy is a segment,
- baseline placement of runs on a line from the metrics we read.

Rich text costs nothing extra: a run boundary is a segment boundary with a
different style key, so `<span>` falls out of the same mechanism.

Two things stay the paragraph's job on purpose: shaping and reordering
inside a run, and glyph-level queries (caret x for an index, hit x -> index)
via the run's glyph info plus the run's offset. No glyph math of our own.

## Shaper is a trait

Under this split the engine's contract collapses to two calls on one run:

- `shape(text, style) -> {width, ascent, descent, glyph positions}`
- `draw(run, x, y)`

Segmentation, breaking, alignment, ellipsis, spans, inline atoms and
selection sit above that trait and never see the engine. First
implementation: single-line SkParagraph via Impeller, as described above.
Candidate second implementation: cosmic-text (rustybuzz shaping + swash
rasterization, pure Rust), which also brings variation axes and letter
spacing that the Impeller C surface will never expose. Caveat: Impeller has
no draw-glyphs primitive, so a non-Skia shaper must bring its own
rasterization: a glyph atlas texture drawn with `DrawTextureRect` (or
outlines as `draw_path`), plus font discovery and fallback (system fonts,
packaged Noto, emoji), which today ride on the typography context. That is
where quality lives - AA, gamma-corrected blending or stem darkening for
light-on-dark, subpixel positioning, optional LCD AA on desktop, hinting
policy per DPI - and it is why the swap is worth having.

Evidence it renders acceptably: `scripts/changelog/changelog-shot.tsx` is
this architecture done crudely (per-word paragraphs, flexbox as the breaker)
and looks fine, so per-segment shaping is not a visible fidelity problem for
Latin text.

## Bidi

Bidirectional text (UAX #9) is deliberately out of the first stages: runs
are placed on a line in logical order and "start" means left. Because a run
is a real Impeller paragraph, an RTL or mixed word, or a whole RTL sentence
in one style, still shapes and reorders correctly inside its run. What
regresses relative to today is confined to RTL rich text spanning styled runs
on one line, RTL paragraph alignment, and break positions in mixed-direction
lines. Adding it later means feeding `unicode-bidi` levels to the breaker
and the placer; it is an input, not a redesign. Word-level segmentation from
day one keeps that door open.

## Costs to measure, not guess

- Paragraph objects alive = distinct (segment, style) pairs across the tree,
  plus per-run draws per line. A counter next to `note_para_shape` says
  whether a per-node cache is enough or a shared LRU is needed.
- Impeller paragraphs are SkParagraph underneath; building one per segment
  is not free. Prepare cost vs today's one paragraph per width.
- Justify is ours (Impeller does not justify a one-line paragraph, it is
  the last line): per-word placement.
- Ellipsis, max lines, letter spacing: ours (letter spacing is not in the C
  API on either path).

## Stage 1 findings (2026-08-16)

Implemented behind `<text textLayout="owned">` (default stayed
`"paragraph"` until stage 6 made owned the only engine apps see). Code, as
of the 2026-08-17 reorganization, `alloy/src/rendertree/text/`: `layout.rs`
(segmenter via unicode-linebreak, breaker, baseline placement, intrinsic
widths; pure, no font or engine types), `shape.rs` (`Text::prepare_owned`
/`owned_layout`: one Impeller paragraph per wrap unit, ParaKey cache, the
grapheme re-split), `paragraph.rs` (the paragraph engine), `runs.rs` (the
run/span model), `mod.rs` (the `Text` element); unit tests in
`alloy/src/tests/text_layout.rs`. Probe: `text-layout-probe.tsx` renders
the newest changelog bullets plus Latin+CJK, hard-break and unbreakable
samples twice side by side; `alloy/examples/text_layout_bench.rs` times the
two paths; `alloy/examples/para_metrics_probe.rs` shows what a single-line
paragraph reports.

- Pixels: identical to drawParagraph on every changelog bullet and the mixed
  Latin/CJK sample (0 differing channels over the compared columns). One
  paragraph per wrap unit gives advance (`get_max_intrinsic_width`, includes
  trailing whitespace) and ink width (`get_longest_line_width`, excludes it)
  in one build, and the summed advances match Impeller's own placement to
  the sub-pixel.
- Known difference: a word wider than the wrap width overflows on the owned
  path; Impeller breaks inside it at grapheme level. Stage 2 work (a
  grapheme fallback for oversized units).
- Cost, release, 463-byte paragraph = 73 wrap units. Cold (text SkParagraph
  has never seen, i.e. first render and every real text change): paragraph
  0.83 ms vs owned prepare 0.79 ms, a wash, because SkParagraph's cost is
  per glyph run either way. Re-layout at a new width from prepared runs:
  0.6 us vs a paragraph rebuild of 8 us (a SkParagraph global-cache hit; a
  cold rebuild is the 0.83 ms). Owned's warm prepare of unchanged text is
  ~200 us for the 73 objects; not a text-change number, since a changed
  string misses SkParagraph's cache on both paths. A per-segment cache
  (word text + style) would make an edit re-shape one word instead of 73;
  not built in stage 1, so no claim there. Objects alive per paragraph:
  one per wrap unit; the number to watch.

## Stages

1. DONE (see findings above): spike in alloy behind a flag on `<text>`:
   Rust segmenter, measure via single-line paragraph, greedy breaker,
   per-line placement, LTR only. Compare pixels, shape counts and timing
   against the drawParagraph path on the changelog shot and a Latin plus
   CJK sample. This is the go/no-go for the whole item; on no-go, close it
   and proceed with [text-inline-spans](text-inline-spans.md).
2. Parity, in sub-steps:
   a. DONE (2026-08-16): `<span>` with style overrides (color as paint,
      fontFamily, fontSize, fontWeight, fontStyle, lineHeight), nested
      spans, run collection with cascade in Rust (`RenderTree::sync_text`
      walks the span subtree, `Text::runs` holds `TextRun`s whose overrides
      resolve against the text at shape time); a wrap unit that straddles
      runs is split into glued pieces the breaker fits as one. Spans work
      on both paths while the flag exists. Probe columns stay
      pixel-identical with mixed families, sizes and colors on one line;
      reactive span text and color updates reflow on both paths. Removed
      the layout pass's direct-children-only text aggregation, which had
      overwritten the eager sync.
   b. DONE (2026-08-16): owned-path paragraph parity: maxLines +
      textOverflow (ellipsis is ours: the last line is trimmed to ink until
      the ellipsis run fits, drawn in the paragraph default style), justify
      (slack over unit gaps of wrapped lines only), and `overflowWrap:
      "normal" | "anywhere"` for units wider than the line (a matter of
      taste and context, so a prop: `anywhere` re-splits the unit at
      grapheme boundaries and lays out again, Impeller's behavior and the
      default; `normal` keeps the unit whole and overflows, CSS's default;
      the paragraph path cannot honor `normal`). Justify, center+clip and
      ellipsis are pixel-identical to Impeller in the probe. Two known
      deltas: Impeller keeps the trailing space before an ellipsis, ours
      trims it; a grapheme-split word loses kerning, so a character or so
      fewer fits on the line than Impeller's in-word break.
   c. DONE (2026-08-16): the changelog shot renders a bullet as one
      `<text textLayout="owned">` with span children (`toWords`, the
      wrapping row and `Word` are gone, `inlineRuns` stays); same size,
      real word spacing instead of a fixed gap. The probe carries styled
      runs, the paragraph options and a live-update sample.
   d. DONE (2026-08-17): shared word cache, `text/words.rs`: one
      `WordCache` on `PlatformContext` (UI thread, cleared on
      `reset_fonts`), an `lru::LruCache` of 8192 (word text, resolved
      `RunStyle`) -> (paragraph, metrics); `RunStyle`/`PaintState` gained
      Hash/Eq (floats by bits, zero canonicalized) for the key. Every
      owned-path piece goes through `get_or_shape`; `paragraph_style` is
      built only on a miss. Counter `word_hits` next to `para_shapes`
      (`wordHits` in get_stats). Bench (release, 73 units): warm prepare
      13 us vs 230 us through SkParagraph's own cache; one word edited 32
      us vs 264 us for the paragraph rebuild; cold unchanged (~1.1 ms,
      shaping is shaping). Not an LRU of our own: ordering and eviction
      are the crate's, the wrapper picks key, value and counters. The
      key carries the whole paint because Impeller bakes the foreground
      into the paragraph object; a color change therefore re-shapes its
      words (warm shapes). A metrics tier keyed on font fields only would
      decouple layout from paint; worth it only once something measures
      without drawing (the primitives item's `prepare`), so left for then.
3. DONE (2026-08-16), the first exclusive features, both owned-path only:
   a. Span hit testing: `TextRun` names its leaf span node, `Text::hit_run`
      finds the placed piece under a text-local point (rectangle arithmetic
      over the cached line layout, no shaper query) and hit.rs pushes the
      span chain after the text, so a `<span>` takes the pointer props and a
      link wrapping across two lines is one span with a hit box per line;
      events bubble span -> span -> text. Verified live: both halves of a
      wrapped link fire the span handler and bubble.
   b. Inline atoms: any laid-out element child of `<text>` becomes an
      `ATOM_CHAR` (U+FFFC) run in the paragraph text - a wrap unit of its
      own with a break opportunity before and after, never inside - sized by
      the layout pass as a shrink-to-fit root (max-content, margins included
      in the advance) before the text measures itself, placed bottom on the
      baseline (HTML's inline-block default; the line grows for a taller
      atom), then positioned by writing its computed location from the line
      layout; composite and hit testing descend into a text's laid-out
      children. `overflowWrap: anywhere` never splits an atom; the paragraph
      path ignores atoms; a detached `d-text` cannot host one (nothing lays
      out under it). No `verticalAlign` yet. `computed_text` carries the
      U+FFFC, so tree snapshots show it. Verified: chip, swatch, tall badge
      and a wider-than-column box in the probe, and an atom's own handler
      fires and bubbles to the text.
   Shared word cache (2d) deliberately deferred until the functionality is
   complete.
4. Variable-width lines and custom breaking, pure text_layout work, in
   sub-steps (bare minimum first):
   a. DONE (2026-08-16): extent-per-line hook. `layout` takes
      `&dyn Fn(LineCursor) -> Vec<LineExtent>`: asked once per line as its
      first unit arrives, with the line's index, top y and the opening
      run's height, it returns the line's spans (start x + width each) in
      left to right order: one for a plain column (`vec![LineExtent::
      full(w)]`), two around an exclusion in the middle, N+1 around N; an
      empty answer means no room, the breaker moves down by the cursor's
      height and asks again (a hook must leave room below every
      exclusion). The breaker fills a line's segments left to right at one
      y (shared height and baseline), a unit that does not fit moves to the
      next segment, then the next line; only an empty line takes an
      overflowing unit. Alignment and justify work per segment, the ellipsis
      trims the last segment only. `Line` records `segments`
      (`LineSegment`: extent, ink used, run range); `Layout.width` is the
      widest segment's right ink edge; the intrinsic widths ignore the hook.
      Not re-cut: a line that grows taller than the cursor's height after
      it opened (CSS re-evaluates; we do not, unless it shows in practice).
      First prop on it: `textIndent` (pixels; negative hangs: first line at
      0, the rest indented by the magnitude; a hard break does not start a
      new first line; owned only, the paragraph path ignores it). measure
      adds |indent| to both intrinsic widths so shrink-to-fit text does not
      wrap where it need not. Verified by line-start geometry against a
      guide rule in the probe; segments by unit tests only until 4b.
   b. DONE (2026-08-17): `float="left" | "right"` on an atom (element-level
      prop, `Element::float`, copied into `TextRun.float` and the atom's
      `Run.float`; a write on an atom resyncs the owning text like a span
      write). In text_layout the float is out of the flow: placed at the top
      of the line where it occurs (this line if still empty, else the next),
      against that side of the hook's outer extents, beside same-side floats
      its top band overlaps; its box (advance by height) is an exclusion the
      breaker subtracts from every line's extents whose top band overlaps it
      (`Breaker::cut`, so N floats give N+1 segments across a line for
      free), `Layout.floats` carries the positions, `Layout.height` grows to
      the lowest float, floats after the last text land below it, floats
      past a truncation are cut. `clear="left" | "right" | "both"` on an
      atom (`Run.clear`): closes the open line if it has content, flushes
      pending floats, moves y below the matching exclusions; a cleared float
      stacks below instead of beside. Kept because it is the CSS float
      model's other half, and the cross-paragraph case needs nothing (the
      text box already includes its floats). A float and a hard break on an
      empty line do not make a blank line; the intrinsic widths still count
      a float as an inline unit (max-content overestimates by its width).
      Covers image runaround and drop caps; verified in the probe (drop cap,
      left swatch mid-text, right float, box growing to the float).
      Middle exclusions, obstacle lists and the like are deliberately NOT
      props: `float` is where the shape props stop; anything further is app
      composition on [text-layout-primitives](text-layout-primitives.md).
   c. DONE (2026-08-17): `textWrap="wrap" | "balance" | "pretty"`
      (`text_layout::layout_wrap`, `Wrap`): post-passes over greedy via a
      break cap `(from_line, x)` that only affects fitting (alignment and
      justify still use the real extents, so a balanced right-aligned
      heading still ends at the box edge). Balance: repeatedly cap under
      the widest line's ink edge while the line count and overflow set
      hold (exact, terminates at the widest unit). Pretty: when the last
      line is a lone unit, cap the line above (only) under its ink edge so
      its last unit drops; accept if the count holds and the last line has
      two or more units. Neither runs when truncated by maxLines. Under
      justify, balance changes breaks but lines still fill the width, as in
      CSS. Owned only. Verified: unit test and the probe's balanced heading.
   Later, as apps ask: hyphenation (`hyphenation` crate, needs re-shaping
   of the halves, fits the overflow re-split machinery), hanging
   punctuation, text inside a shape (the hook exists once a. lands).
5. Bidi (POSTPONED, user decision 2026-08-16): levels into breaker and
   placer, RTL alignment, RTL samples.
6. DONE (2026-08-17): owned is the engine. The `textLayout` prop, the
   `TextLayoutMode` enum and every "owned only" caveat in types/docs are
   gone; the probe and the changelog shot are single-engine. The
   drawParagraph-as-layout path itself stays in `text/paragraph.rs`
   (`Text::shaped`, `ParaCache`) behind a Rust-only
   `Text.paragraph_engine: bool` (default false, no prop): a reference
   and fallback engine, ~100 lines, worth keeping until the rasterizer
   (7) lands. Spans, atoms, floats, indent and wrap do nothing on it.
   Sanity: gallery example and probe render unchanged.
7. Own rasterizer: second shaper implementation behind the trait with our
   own glyph atlas, aimed first at light-on-dark quality; retire the
   Medium-weight workaround when it lands.

## Related

- [text-inline-spans](text-inline-spans.md): the fallback and the API.
- [text-layout-primitives](text-layout-primitives.md): the blocks this
  engine should expose to apps once stages 4c, 6 and 2d have landed.
- [scoped-text-defaults](scoped-text-defaults.md),
  [dpi-aware-default-font-weight](dpi-aware-default-font-weight.md): the
  default paragraph style is the base every run's style key layers on.
- [app-wide-zoom](app-wide-zoom.md): cheap re-layout at scale.
