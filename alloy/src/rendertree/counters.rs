use std::cell::Cell;

// Layout-activity counters for perf diagnosis (the "one layer below the phase
// timings" instrumentation): how much measuring/shaping a rebuild did and how
// much of the tree was dirtied leading into it. Thread-locals because the
// increment sites (Text::measure, Text::shaped, invalidate_cache) have no
// tree reference in scope, and everything that touches them runs on the one
// UI thread.
//
// Always on BY DESIGN, in production builds too (decided 2026-07-17): a
// counter is a branchless thread-local integer bump on paths that each do
// orders of magnitude more work, and these numbers are exactly what a
// production bug report needs. The line to hold: bump-an-integer diagnostics
// stay ungated; anything heavier (formatting, allocating, per-event dumps)
// must be gated or must not ship.

thread_local! {
  static MEASURE_CALLS: Cell<u32> = const { Cell::new(0) };
  static PARA_SHAPES: Cell<u32> = const { Cell::new(0) };
  static WORD_HITS: Cell<u32> = const { Cell::new(0) };
  static DIRTIED: Cell<u32> = const { Cell::new(0) };
  static CACHE_GETS: Cell<u32> = const { Cell::new(0) };
  static CACHE_HITS: Cell<u32> = const { Cell::new(0) };
  static DRAWS: Cell<u32> = const { Cell::new(0) };
  static PARAGRAPHS: Cell<u32> = const { Cell::new(0) };
  static CLIPS: Cell<u32> = const { Cell::new(0) };
  static ROUNDED_CLIPS: Cell<u32> = const { Cell::new(0) };
  static SAVE_LAYERS: Cell<u32> = const { Cell::new(0) };
  static BLENDS: Cell<u32> = const { Cell::new(0) };
  static GRADIENTS: Cell<u32> = const { Cell::new(0) };
}

/// Counter values accumulated since the previous `take`.
#[derive(Clone, Copy, Default)]
pub struct LayoutCounters {
  /// Text measure invocations (mostly cache hits; cheap).
  pub measure_calls: u32,
  /// Paragraphs actually shaped (cache misses; the expensive signal). On the
  /// owned path one per word shaped, i.e. word cache misses.
  pub para_shapes: u32,
  /// Words answered from the shared word cache (see rendertree/text/words.rs).
  pub word_hits: u32,
  /// Taffy layout caches cleared by property writes (invalidate_cache walks;
  /// how much of the tree a write burst dirtied).
  pub dirtied: u32,
  /// Taffy per-node cache lookups (one per compute_child_layout entry).
  pub cache_gets: u32,
  /// Lookups answered from the cache. A hit on a container skips its whole
  /// subtree, so gets minus hits bounds how much of the tree was re-solved.
  pub cache_hits: u32,
  /// Display-list ops the paint walk recorded (the frame's cost on the
  /// raster thread and the GPU scales with these on a slow CPU or a tiled
  /// GPU, okf/backlog/display-list-op-cost.md): draw ops of every kind
  /// (rects, paths, textures, paragraphs, replayed recordings), of which
  /// `paragraphs` are the per-word paragraph draws text emits; clip ops,
  /// of which `rounded_clips` are the rounded (and oval) ones, the kind
  /// that costs a third of a tiled GPU's frame on a resizing box; and
  /// save layers (opacity and filter groups, backdrop filters). Ops inside
  /// a reused repaint-boundary recording are not re-recorded and so not
  /// counted; its replay is one draw.
  pub draws: u32,
  pub paragraphs: u32,
  pub clips: u32,
  pub rounded_clips: u32,
  pub save_layers: u32,
  /// Paints built for those draws with a blend mode other than source-over
  /// (a tiny destination-out draw leaves a tiled GPU's fast blend path) and
  /// with a gradient color source (shaded per pixel per frame).
  pub blends: u32,
  pub gradients: u32,
}

pub fn note_measure_call() {
  MEASURE_CALLS.with(|c| c.set(c.get() + 1));
}

pub fn note_para_shape() {
  PARA_SHAPES.with(|c| c.set(c.get() + 1));
}

pub fn note_word_hit() {
  WORD_HITS.with(|c| c.set(c.get() + 1));
}

pub fn note_dirtied() {
  DIRTIED.with(|c| c.set(c.get() + 1));
}

pub fn note_cache_get(hit: bool) {
  CACHE_GETS.with(|c| c.set(c.get() + 1));
  if hit {
    CACHE_HITS.with(|c| c.set(c.get() + 1));
  }
}

/// One display-list draw op recorded by the paint walk.
pub fn note_draw() {
  DRAWS.with(|c| c.set(c.get() + 1));
}

/// One per-word paragraph draw (also a draw).
pub fn note_paragraph() {
  DRAWS.with(|c| c.set(c.get() + 1));
  PARAGRAPHS.with(|c| c.set(c.get() + 1));
}

/// One clip op; `rounded` for a rounded-rect or oval clip.
pub fn note_clip(rounded: bool) {
  CLIPS.with(|c| c.set(c.get() + 1));
  if rounded {
    ROUNDED_CLIPS.with(|c| c.set(c.get() + 1));
  }
}

/// One save layer.
pub fn note_save_layer() {
  SAVE_LAYERS.with(|c| c.set(c.get() + 1));
}

/// A paint built for a draw: whether it carries a gradient color source
/// and whether its blend mode is other than source-over.
pub fn note_paint(gradient: bool, blend: bool) {
  if gradient {
    GRADIENTS.with(|c| c.set(c.get() + 1));
  }
  if blend {
    BLENDS.with(|c| c.set(c.get() + 1));
  }
}

/// Read and zero all counters. Called once per rebuilt frame by the draw
/// loop, so the values cover exactly one rebuild plus the writes since the
/// previous one.
pub fn take() -> LayoutCounters {
  LayoutCounters {
    measure_calls: MEASURE_CALLS.with(|c| c.replace(0)),
    para_shapes: PARA_SHAPES.with(|c| c.replace(0)),
    word_hits: WORD_HITS.with(|c| c.replace(0)),
    dirtied: DIRTIED.with(|c| c.replace(0)),
    cache_gets: CACHE_GETS.with(|c| c.replace(0)),
    cache_hits: CACHE_HITS.with(|c| c.replace(0)),
    draws: DRAWS.with(|c| c.replace(0)),
    paragraphs: PARAGRAPHS.with(|c| c.replace(0)),
    clips: CLIPS.with(|c| c.replace(0)),
    rounded_clips: ROUNDED_CLIPS.with(|c| c.replace(0)),
    save_layers: SAVE_LAYERS.with(|c| c.replace(0)),
    blends: BLENDS.with(|c| c.replace(0)),
    gradients: GRADIENTS.with(|c| c.replace(0)),
  }
}
