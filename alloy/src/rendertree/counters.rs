use std::cell::Cell;

// Layout-activity counters for perf diagnosis (the "one layer below the phase
// timings" instrumentation): how much measuring/shaping a rebuild did and how
// much of the tree was dirtied leading into it. Thread-locals because the
// increment sites (Text::measure, Text::shaped, invalidate_cache) have no
// tree reference in scope, and everything that touches them runs on the one
// UI thread; a counter costs one Cell bump, cheap enough to leave always on.

thread_local! {
  static MEASURE_CALLS: Cell<u32> = const { Cell::new(0) };
  static PARA_SHAPES: Cell<u32> = const { Cell::new(0) };
  static DIRTIED: Cell<u32> = const { Cell::new(0) };
}

/// Counter values accumulated since the previous `take`.
#[derive(Clone, Copy, Default)]
pub struct LayoutCounters {
  /// Text measure invocations (mostly cache hits; cheap).
  pub measure_calls: u32,
  /// Paragraphs actually shaped (cache misses; the expensive signal).
  pub para_shapes: u32,
  /// Taffy layout caches cleared by property writes (invalidate_cache walks;
  /// how much of the tree a write burst dirtied).
  pub dirtied: u32,
}

pub fn note_measure_call() {
  MEASURE_CALLS.with(|c| c.set(c.get() + 1));
}

pub fn note_para_shape() {
  PARA_SHAPES.with(|c| c.set(c.get() + 1));
}

pub fn note_dirtied() {
  DIRTIED.with(|c| c.set(c.get() + 1));
}

/// Read and zero all counters. Called once per rebuilt frame by the draw
/// loop, so the values cover exactly one rebuild plus the writes since the
/// previous one.
pub fn take() -> LayoutCounters {
  LayoutCounters {
    measure_calls: MEASURE_CALLS.with(|c| c.replace(0)),
    para_shapes: PARA_SHAPES.with(|c| c.replace(0)),
    dirtied: DIRTIED.with(|c| c.replace(0)),
  }
}
