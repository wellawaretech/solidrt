---
title: Impeller interop GlyphInfo grapheme bounds are unusable
description: ImpellerGlyphInfoGetGraphemeClusterBounds swaps x and y when building the ImpellerRect, and the rect it returns for glyphs of a single-line paragraph carries no position (per-glyph width in the x slot, zero width), so caret geometry cannot be read from it.
project: flutter engine, impeller/toolkit/interop (github.com/flutter/flutter)
versions: impellers 0.4.2 (engine SHA in the crate's ENGINE_SHA)
status: unfiled
link:
created: 2026-08-18
---

# Impeller interop: GlyphInfo grapheme bounds are unusable

Found 2026-08-18 while adding caret stops to `prepareText` (editing needs
the x after each grapheme of a shaped word). Plan A was
`Paragraph::create_glyph_info_at_code_unit_index_utf16(i)` +
`GlyphInfo::get_grapheme_cluster_bounds()`.

Two problems, one visible in the source, one measured:

1. `impeller/toolkit/interop/glyph_info.cc` builds the result as
   `ImpellerRect{ bounds.y(), bounds.x(), bounds.width(), bounds.height() }`:
   x and y swapped.
2. Measured on a single-line paragraph "iWi" (Noto Sans 14 px): the rect's
   x slot holds 5.30 / 14.69 / 5.30 for the three clusters (a per-glyph
   width, not a position; the middle W does not read as ~17), and its width
   is 0. Nothing in it says where the glyph is.

Workaround in `alloy/src/rendertree/text/words.rs` (`WordCache::caret_stops`):
shape every grapheme prefix of the word through the word cache and take its
advance. Kerning across the cut is lost (sub-pixel), complex scripts are
wrong (as everywhere else, see [text-bidi](../backlog/text-bidi.md)).
Revisit if the interop grows glyph positions or `getRectsForRange`.
