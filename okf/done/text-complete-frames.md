---
title: A frame never draws text without its glyphs
description: A text layer whose cells are not in the atlas draws without them when the frame's 3 ms cell budget is spent, and the frame presents that way - a size step loses a third of its letters for one frame in a headless render, a snapshot comes back without its text, a title under a per-frame scale flickers live; the warm-up that was meant to prevent it is ASCII-only, keyed on the exact ppem, and its cells are dropped when they land before any text has drawn. The rule becomes the one every renderer uses - a frame that draws text has all of its glyphs, a cold style makes its frame late and never incomplete - and the warm-up is kept as what it is, an optimization.
created: 2026-10-09
completed: 2026-10-09
---

# A frame never draws text without its glyphs

## Symptom

The text atlas (`alloy/src/rendertree/text/glyphs/text_atlas.rs`) makes a
cell a build needs and lacks on the UI thread only while the frame's
`SYNC_CELL_BUDGET` of 3 ms holds, measured from `begin_frame`, so layout
and shaping of a busy frame eat into it. Past the budget the misses go to
the worker, the layer draws without them (`glyph_quads` skips a glyph
with no placement) and the frame presents; the cells land a frame later
and the layer re-rasterizes. A frame's pixels thus depend on the machine's
speed, and every reader of a frame sees the hole:

- A headless render (`sol render`) reads each frame back right after it
  ran: the first frame at a size nothing drew before has a third of its
  letters missing, a step of justified columns has no text at all for one
  frame, and a few large texts whose scale is written every frame lose a
  different subset of letters each frame. The next frame is complete.
- A snapshot taken right after a large tree change paints the shapes and
  no text; the next one is complete.
- Live, letters flicker in and out of a title under an idle scale
  animation.
- A ligature or any non-ASCII glyph draws one frame late on a size or
  content change even when the app warmed every style it uses.

The warm-up was meant to make this rare and cannot: it is ASCII only, by
character (a ligature is a glyph, not a character); it keys on the exact
device ppem, so text under any scale warms nothing; `land` inserts cells
only `if let Some(atlas)`, so a `warmText` whose cells land before the
first text has drawn (the documented use: at startup, under a splash)
drops them, with the style kept in `warmed` so it is never warmed again;
and a warmed style's cells are evictable after 120 unused frames at the
atlas cap while `warmed` still says warmed.

## Decision

A frame that draws text has every glyph it draws. A build makes each cell
it lacks in that frame, however many; a cold style makes its frame late,
as in every browser, and never incomplete. Pixels are a function of the
tree alone, so a headless render is deterministic and a snapshot is
complete by construction. The worker makes warm-ups only, ahead of use,
and a warm-up changes nothing visible when it lands: a build that needs a
cell still queued makes it itself, and the worker's copy lands as a no-op.

The cost is the stall a cold style pays once (from the measurements in
[text-rasterizer-findings](../notes/text-rasterizer-findings.md): about
0.1 ms per cell on the desktop, 0.4 to 0.5 on the tablet, 1 ms on the
armv7 TV, so a cold screen on the TV stalls a few hundred milliseconds
once). Today the same cells take the same time on the worker while the
screen shows part of its text, so the total is equal; the warm-up, once it
works, removes the stall for an app that warms its type scale, which is
its proper job.

Rejected: having `sol render` wait for the cells a frame drew without and
redraw before reading back. It closes one of four exits and leaves the
live client, snapshots and tests with the same frame; and it keeps a
layer-completion machinery whose only purpose is the policy being removed.

## What changes

In the text atlas:

- `ensure` makes every miss now, growing or evicting the atlas as it
  goes; the budget, the deadline and the deferred cells go. A layer asks
  for all of its cells before it reads any placement (two passes in
  `glyph_quads`), so a growth under one of its buckets moves nothing it
  has already used. Layers built earlier in the frame are finished: the
  glyph pass is a blocking round trip, so their pixels are in their own
  textures before the atlas moves.
- The packer exists from construction; only the texture is created at the
  first `flush`. `land` inserts into the packer whether or not the
  texture exists yet, which is the `warmText`-before-first-text fix, and
  makes the atlas testable without a GPU.
- The eviction reports the keys it took, and the atlas forgets a style
  was warmed when its cells go, so a later first sight warms it again.
- Warm jobs no longer hold the engine or request a frame when they
  finish: nothing visible depends on their landing. `request_warm` still
  requests the frame that submits it.
- A cell the atlas cannot place at its cap is logged once; the glyph
  draws as nothing at its advance (the cap itself is
  [glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md)'s subject).

Deleted with the policy: `TextLayer::complete`, `TextImage::complete`,
`layer_incomplete`, the tree's incomplete-text registry and the landed
damage pass in `apply_content_changes`, the HUD's refresh-when-complete,
the atlas's `HoldSource`/`WorkHold` and the `text cells` in-flight kind.

Unchanged: the worker and its priorities (`flux:font` uses both), the
warm-up on first sight of an untransformed style, `warmText` and the
components' type-scale warming, the layer cache and its release.

## Done looks like

A headless render of an app whose text mounts at a size nothing drew
before has the same ink on the first frame with text as on the next (read
2026-10-09, in [text-rasterizer-findings](../notes/text-rasterizer-findings.md),
pinned by `packages/core/tests/text-complete.test.tsx`). A `warmText` at
startup, before any text draws, puts its cells in the atlas, and a style
evicted from a full atlas is warmed again on its next first sight (both
pinned in `alloy/src/tests/glyphs.rs`).

## Left open

- Text under a changing scale: with complete frames the per-frame cells
  of an animated scale cost frame time instead of letters, and the fix is
  raster hysteresis, [text-layer-motion](../backlog/text-layer-motion.md).
- In `okf/tiny.md`: the TV's cold-style first paint read again under the
  rule (the number before was 83 ms with three cells a frame made in time
  and the rest arriving over a few frames), and `warmText` taking strings
  so ligatures and non-ASCII glyphs warm too.

## Findings

What holds without this plan was cut into
[text-rasterizer-findings](../notes/text-rasterizer-findings.md) (the
blocking glyph pass, the desktop read). What stays here is the record:

- 2026-10-09, the dropped landing. `land` inserted only into an existing
  atlas texture, and the texture was created by the first `ensure`, so
  every warm cell finished before the first text drew was removed from
  `pending` and discarded, the style kept in `warmed`. The packer now
  exists from construction and the texture comes at the first `flush`,
  which also made the atlas checkable without a GPU: the warm-up, the
  build and the eviction tests in `alloy/src/tests/glyphs.rs` run on the
  packer alone. The deferred-cell path (a cell that did not fit mid-frame,
  inserted at the next frame start, the frame requested for it) went with
  the budget.
