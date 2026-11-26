//! The order a draw runs its batches in (`exec`). The walk emits a layer
//! inline, between its two marker batches, where it composites. Drawn in
//! that order, every layer leaves the frame it composites into and comes
//! back, and on a tiler each round trip stores and reloads that frame's
//! tiles: twelve fading cards cost twelve stores and reloads of the window
//! per frame. So every layer whose content does not read the pixels beneath
//! it is drawn ahead: into its storage, before the frame it composites into
//! is bound, in the order the layers close (a layer's own drawn-ahead
//! layers before it), its storage held until its composite. Then the
//! target is bound, cleared and drawn once, a drawn-ahead layer contributing
//! only its composite. Its content starts transparent and the clips in
//! effect where it opened apply at the composite alone (`walk::Layer`), so
//! drawing it earlier changes no pixel.
//!
//! A render pass has a fixed cost of its own on a tiler (about 2.5 ms per
//! pass on a Mali-T860, okf/plans/own-renderer.md stage 6 Findings), so the
//! layers drawn ahead do not each take a pass: the plain ones of one
//! nesting level share one atlas storage, each in a region of its own,
//! drawn in one pass under its own viewport offset and scissor, its stencil
//! cleared per region. Each nesting level gets atlases of its own, the
//! deepest level first: a layer samples the atlas its inner layers drew
//! into, and nothing samples the atlas it draws into. A level whose regions
//! do not fit one atlas takes another. A blurred layer keeps storage of
//! its own (its blur passes read that storage whole), a backdrop layer
//! stays inline, and the storage a region draws into is the pool's as any
//! layer's is.
//!
//! A backdrop layer starts from the pixels drawn beneath it so far, so it
//! is drawn inline in its parent's pass, as before. A shader blend mode's
//! composite copies its destination where it draws; its content is drawn
//! ahead all the same. A layer that nothing of shows (an empty rect) is
//! dropped with everything inside it, as the executor always skipped it.
//! Pure: no GL, so the tests read a schedule off a walk.

use super::walk::{Batch, BatchProgram, Layer};

/// Where a layer's content draws: storage of its own, or a region of an
/// atlas shared with the other layers of its nesting level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
  Own,
  /// The region at `offset` of the atlas numbered `atlas` (within a draw):
  /// window pixels of the atlas, x right and y up from its bottom-left
  /// corner, the region's size the layer's content size.
  Region {
    atlas: usize,
    offset: (i32, i32),
  },
}

/// One step of a draw, by the index of the batch it concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
  /// Bind and clear the target: the main pass starts. Every step before it
  /// draws a layer ahead.
  Target,
  /// Draw the batch at this index into the frame drawing now.
  Batch(usize),
  /// Take and clear the atlas numbered `atlas`, of `size` pixels: the
  /// regions opened before its `AtlasEnd` draw into it.
  AtlasBegin { atlas: usize, size: (i32, i32) },
  /// The atlas's regions are drawn: finish its storage, and hold it until
  /// the last of its regions composites.
  AtlasEnd(usize),
  /// Open the layer whose `LayerBegin` is at this index: draw into its place
  /// from here on (a backdrop layer's own storage, filled from the frame
  /// beneath it).
  Open(usize, Place),
  /// Close the layer whose `LayerEnd` is at this index: finish its own
  /// storage and run its blur if it blurs, or leave its region as drawn,
  /// and hold the image for its composite.
  Close(usize, Place),
  /// Composite the held image of the layer whose `LayerEnd` is at this
  /// index into the frame drawing now.
  Composite(usize),
}

/// Whether `layer` is drawn ahead of the frame it composites into: every
/// layer but a backdrop layer, whose content starts from that frame.
pub(crate) fn draws_ahead(layer: &Layer) -> bool {
  !layer.backdrop
}

/// Whether a layer drawn ahead shares an atlas: every one but a blurred
/// layer, whose blur passes read its storage whole.
fn shares_atlas(layer: &Layer) -> bool {
  layer.blur.is_none()
}

/// The steps that draw `batches`, the layer table `layers` beside them:
/// the drawn-ahead layers first (with `ahead` false, none: every layer
/// inline, the walk's order), by nesting level from the deepest, each
/// level's plain layers packed into atlases of `atlas` pixels (None: every
/// layer in storage of its own), then `Step::Target`, then the target's own
/// pass.
pub(crate) fn schedule(batches: &[Batch], layers: &[Layer], ahead: bool, atlas: Option<(i32, i32)>) -> Vec<Step> {
  let ends = matching_ends(batches);
  let mut levels: Vec<Vec<Block>> = Vec::new();
  let main = pass(batches, layers, &ends, 0, batches.len(), ahead, 0, &mut levels);
  let mut steps = Vec::new();
  let mut atlases = 0;
  for blocks in levels.iter().rev() {
    emit_level(blocks, layers, atlas, &mut atlases, &mut steps);
  }
  steps.push(Step::Target);
  steps.extend(main);
  steps
}

// For each `LayerBegin`, the index of its `LayerEnd`; 0 for any other
// batch, which nothing reads.
fn matching_ends(batches: &[Batch]) -> Vec<usize> {
  let mut ends = vec![0; batches.len()];
  let mut open = Vec::new();
  for (i, batch) in batches.iter().enumerate() {
    match batch.program {
      BatchProgram::LayerBegin => open.push(i),
      BatchProgram::LayerEnd => {
        let begin = open.pop().expect("a layer's end marker closes its begin marker");
        ends[begin] = i;
      }
      _ => {}
    }
  }
  debug_assert!(open.is_empty(), "every layer's begin marker has its end");
  ends
}

// A layer drawn ahead: its marker batches, its index in the layer table
// and the steps of its content.
struct Block {
  begin: usize,
  end: usize,
  layer: usize,
  inner: Vec<Step>,
}

// The steps of the frame drawing batches `from..to` at nesting `depth`
// (0: the target's pass), a drawn-ahead layer inside it replaced by its
// composite and its block pushed onto `levels[depth]`, after the blocks of
// the layers inside it (at deeper levels).
#[allow(clippy::too_many_arguments)]
fn pass(
  batches: &[Batch],
  layers: &[Layer],
  ends: &[usize],
  from: usize,
  to: usize,
  ahead: bool,
  depth: usize,
  levels: &mut Vec<Vec<Block>>,
) -> Vec<Step> {
  let mut steps = Vec::new();
  let mut i = from;
  while i < to {
    if batches[i].program != BatchProgram::LayerBegin {
      steps.push(Step::Batch(i));
      i += 1;
      continue;
    }
    let end = ends[i];
    let layer = &layers[batches[i].start];
    if !layer.rect.is_empty() {
      let inner = pass(batches, layers, ends, i + 1, end, ahead, depth + 1, levels);
      if ahead && draws_ahead(layer) {
        if levels.len() <= depth {
          levels.resize_with(depth + 1, Vec::new);
        }
        levels[depth].push(Block { begin: i, end, layer: batches[i].start, inner });
      } else {
        steps.push(Step::Open(i, Place::Own));
        steps.extend(inner);
        steps.push(Step::Close(end, Place::Own));
      }
      steps.push(Step::Composite(end));
    }
    i = end + 1;
  }
  steps
}

// The steps of one nesting level's drawn-ahead layers: the ones that keep
// storage of their own first, each in a pass of its own, then the rest
// packed into atlases of `atlas` pixels (`pack`), one pass each, numbered
// on from `atlases`.
fn emit_level(
  blocks: &[Block],
  layers: &[Layer],
  atlas: Option<(i32, i32)>,
  atlases: &mut usize,
  steps: &mut Vec<Step>,
) {
  let layer = |block: &Block| &layers[block.layer];
  let shared = |block: &Block| atlas.is_some() && shares_atlas(layer(block));
  for block in blocks.iter().filter(|b| !shared(b)) {
    steps.push(Step::Open(block.begin, Place::Own));
    steps.extend(block.inner.iter().copied());
    steps.push(Step::Close(block.end, Place::Own));
  }
  let Some(size) = atlas else { return };
  let regions: Vec<&Block> = blocks.iter().filter(|b| shared(b)).collect();
  if regions.is_empty() {
    return;
  }
  let sizes: Vec<(i32, i32)> = regions.iter().map(|b| (layer(b).rect.width(), layer(b).rect.height())).collect();
  let mut current = None;
  for (block, placed) in regions.iter().zip(pack(&sizes, size)) {
    let atlas_number = *atlases + placed.atlas;
    if current != Some(atlas_number) {
      if let Some(previous) = current {
        steps.push(Step::AtlasEnd(previous));
      }
      steps.push(Step::AtlasBegin { atlas: atlas_number, size });
      current = Some(atlas_number);
    }
    let place = Place::Region { atlas: atlas_number, offset: placed.offset };
    steps.push(Step::Open(block.begin, place));
    steps.extend(block.inner.iter().copied());
    steps.push(Step::Close(block.end, place));
  }
  if let Some(last) = current {
    steps.push(Step::AtlasEnd(last));
    *atlases = last + 1;
  }
}

/// Where one region went: its atlas (numbered from 0 within one `pack`)
/// and its offset in it, window pixels, x right and y up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
  pub atlas: usize,
  pub offset: (i32, i32),
}

/// Where a region of each of `sizes` goes in atlases of `atlas` pixels, in
/// the sizes' order: shelf packing, each region to the right of the one
/// before it along a row as tall as the row's tallest region, a new row
/// above the row when the region does not fit beside it, a new atlas when
/// the row does not fit above. A size the atlas cannot hold at all takes an
/// atlas alone, at its corner (the executor's scissor cuts it; a layer's
/// rect never exceeds the target, which the atlas is at least as large as).
pub(crate) fn pack(sizes: &[(i32, i32)], atlas: (i32, i32)) -> Vec<Placed> {
  let (aw, ah) = atlas;
  let mut out = Vec::with_capacity(sizes.len());
  let mut number = 0;
  // The row's left cursor, its bottom, and its height so far.
  let (mut x, mut y, mut row) = (0, 0, 0);
  let mut empty = true;
  for &(w, h) in sizes {
    debug_assert!(w <= aw && h <= ah, "a region ({w}x{h}) exceeds the atlas ({aw}x{ah})");
    if x + w > aw && x > 0 {
      y += row;
      x = 0;
      row = 0;
    }
    if y + h > ah && !empty {
      number += 1;
      x = 0;
      y = 0;
      row = 0;
    }
    out.push(Placed { atlas: number, offset: (x, y) });
    x += w;
    row = row.max(h);
    empty = false;
  }
  out
}
