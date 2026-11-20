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
//! A backdrop layer starts from the pixels drawn beneath it so far, so it
//! is drawn inline in its parent's pass, as before. A shader blend mode's
//! composite copies its destination where it draws; its content is drawn
//! ahead all the same. A layer that nothing of shows (an empty rect) is
//! dropped with everything inside it, as the executor always skipped it.
//! Pure: no GL, so the tests read a schedule off a walk.

use super::walk::{Batch, BatchProgram, Layer};

/// One step of a draw, by the index of the batch it concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
  /// Bind and clear the target: the main pass starts. Every step before it
  /// draws a layer ahead.
  Target,
  /// Draw the batch at this index into the frame drawing now.
  Batch(usize),
  /// Open the layer whose `LayerBegin` is at this index: take its storage
  /// and draw into it from here on (a backdrop layer's filled from the
  /// frame beneath it).
  Open(usize),
  /// Close the layer whose `LayerEnd` is at this index: finish its storage,
  /// run its blur if it blurs, and hold the image for its composite.
  Close(usize),
  /// Composite the held image of the layer whose `LayerEnd` is at this
  /// index into the frame drawing now.
  Composite(usize),
}

/// Whether `layer` is drawn ahead of the frame it composites into: every
/// layer but a backdrop layer, whose content starts from that frame.
pub(crate) fn draws_ahead(layer: &Layer) -> bool {
  !layer.backdrop
}

/// The steps that draw `batches`, the layer table `layers` beside them:
/// the drawn-ahead layers first (with `ahead` false, none: every layer
/// inline, the walk's order), then `Step::Target`, then the target's own
/// pass.
pub(crate) fn schedule(batches: &[Batch], layers: &[Layer], ahead: bool) -> Vec<Step> {
  let ends = matching_ends(batches);
  let mut drawn_ahead = Vec::new();
  let main = pass(batches, layers, &ends, 0, batches.len(), ahead, &mut drawn_ahead);
  drawn_ahead.push(Step::Target);
  drawn_ahead.extend(main);
  drawn_ahead
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

// The steps of the frame drawing batches `from..to`, a drawn-ahead layer
// inside it replaced by its composite and its own steps pushed onto
// `drawn_ahead`, after those of the layers drawn ahead inside it.
fn pass(
  batches: &[Batch],
  layers: &[Layer],
  ends: &[usize],
  from: usize,
  to: usize,
  ahead: bool,
  drawn_ahead: &mut Vec<Step>,
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
      let inner = pass(batches, layers, ends, i + 1, end, ahead, drawn_ahead);
      let block = if ahead && draws_ahead(layer) { &mut *drawn_ahead } else { &mut steps };
      block.push(Step::Open(i));
      block.extend(inner);
      block.push(Step::Close(end));
      steps.push(Step::Composite(end));
    }
    i = end + 1;
  }
  steps
}
