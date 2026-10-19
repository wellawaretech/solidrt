//! Draw slicing under a submission time budget
//! (okf/plans/splat-adreno-gpu-fault.md, part B).
//!
//! A GPU kernel bounds one SUBMISSION's run, never a frame's: msm KGSL
//! faults a context when 2 s pass without one of its submissions retiring
//! (the clock restarts at each retire), Windows TDR bounds a DMA packet the
//! same way. A submission is the command stream between two flushes - on a
//! tiled GPU the whole frame, since nothing forces the driver to submit
//! earlier - so a draw whose own run is seconds (786k splats on an Adreno
//! 610) faults the context however healthy the rest of the frame is.
//!
//! The engine's invariant: no submission it issues is predicted to run
//! longer than `SUBMISSION_BUDGET_MICROS`. A heavy draw entry is issued as
//! slices, each flushed and waited for on the raster thread (gl::pass),
//! which measures every slice on the clock the kernel keeps; the per-entry
//! `SliceGauge` remembers the measured rate and `SlicePlan` sizes the next
//! render's slices from it, converging to one slice - no flush, no wait,
//! one fence polled later as an upper bound - wherever the draw fits. The
//! planner is pure arithmetic (src/tests/draw_slices.rs).

use std::cell::Cell;
use std::time::Instant;

use crate::gpu::vocab::{DrawRange, Topology};

/// The longest run any one submission may be predicted to take, in
/// microseconds. A quarter of the tightest kernel limit (KGSL's 2000 ms
/// drawobj timeout, Windows TDR's 2 s): room for a clock drop or a camera
/// move between two measurements.
pub(crate) const SUBMISSION_BUDGET_MICROS: u64 = 500_000;
/// What a slice is sized to: half the budget, so a 2x density jump between
/// consecutive slices (a sorted cloud's near splats against its far ones)
/// still lands under it.
const SLICE_TARGET_MICROS: u64 = SUBMISSION_BUDGET_MICROS / 2;
/// A draw predicted at or under this runs as one slice; the gap to the
/// budget is the hysteresis that keeps an entry near the line from
/// alternating between one slice and two from render to render.
const SLICE_MERGE_MICROS: u64 = SUBMISSION_BUDGET_MICROS / 2;
/// A slice is at most this many times the previous one: the rate measured
/// on a light region must not size the next slice for a dense one.
const SLICE_GROWTH_MAX: u64 = 2;
/// Draws under this much work (vertices, or indices on an indexed draw,
/// times instances) are never sliced nor measured: a flush per small draw
/// would cost more than any such draw can run, and the per-vertex rate of
/// a draw this small says nothing.
pub(crate) const SLICE_MIN_WORK: u64 = 1 << 16;
/// The first slice of an entry with no measured rate: small enough to be
/// safe on the slowest GPU this engine has met (the SM-T500 ran 65k
/// vertices of the lizard in about 6 ms), large enough that a fast GPU
/// converges within a handful of slices.
const SLICE_PROBE_WORK: u64 = 1 << 16;
/// One step of a slice wait, in nanoseconds (the GL wait takes an i32 of
/// them, under 2.2 s); the wait loops in steps up to the limit below.
pub(crate) const SLICE_WAIT_STEP_NS: i32 = 1_000_000_000;
/// A slice fence unsignaled this long is a hung GPU: the pass stops
/// waiting and measuring, and the present fence watchdog (the same bound,
/// raster::PRESENT_FENCE_LOSS_SECS) declares the loss.
pub(crate) const SLICE_WAIT_LIMIT_SECS: u64 = 10;
/// A slice must run at least this long for its own rate to stand for a
/// region's density: a submission carries a fixed cost (the flush, the
/// kernel round trip, a tiler's tile store and load, a millisecond or so)
/// that a short slice's rate would attribute to its few vertices - a
/// two-instance tail would then predict the whole draw at seconds. Shorter
/// slices count through the render's total instead.
const SLICE_RATE_MIN_MICROS: u64 = 10_000;

/// What the slices run over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SliceAxis {
  /// Instances `[at, at + take)` of the whole vertex range, the instance
  /// attribute pointers re-pointed per slice (ES 3.0 has no base
  /// instance). `gl_InstanceID` restarts at 0 in every slice, so this axis
  /// is only for a vertex stage that does not read it.
  Instances,
  /// Vertices (indices on an indexed draw) `[at, at + take)` of every
  /// instance; `gl_VertexID` stays the true index.
  Vertices,
}

/// The slicing of one draw: the axis, how many units it has (instances or
/// vertices), the vertices one unit submits (what the rate is per), and
/// the topology's stepping - a slice holds a whole number of primitives, a
/// strip's next slice starts `overlap` vertices back, and a triangle
/// strip's slices are even so every slice starts on the same winding
/// parity as the draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SlicePlan {
  pub axis: SliceAxis,
  pub units: u64,
  pub unit_work: u64,
  /// Slice lengths past the overlap are multiples of this.
  pub step: u64,
  /// Vertices a slice repeats from the previous one.
  pub overlap: u64,
}

/// How a planned draw is issued this render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Issue {
  /// One draw call, no flush, no wait; a fence after it bounds the run.
  Whole,
  /// Slices, each flushed and waited for.
  Sliced,
}

impl SlicePlan {
  /// The plan for a draw, or None when the draw is under the work floor or
  /// cannot be cut into two slices (a single primitive, however many
  /// instances, when the vertex stage reads gl_InstanceID).
  pub(crate) fn plan(range: DrawRange, topology: Topology, reads_instance_id: bool) -> Option<SlicePlan> {
    let vertices = range.vertex_count.max(0) as u64;
    let instances = range.instance_count.max(0) as u64;
    if vertices.saturating_mul(instances) < SLICE_MIN_WORK {
      return None;
    }
    let plan = if instances > 1 && !reads_instance_id {
      SlicePlan { axis: SliceAxis::Instances, units: instances, unit_work: vertices, step: 1, overlap: 0 }
    } else {
      let (step, overlap) = match topology {
        Topology::Points => (1, 0),
        Topology::Lines => (2, 0),
        Topology::Triangles => (3, 0),
        Topology::LineStrip => (1, 1),
        Topology::TriangleStrip => (2, 2),
      };
      SlicePlan { axis: SliceAxis::Vertices, units: vertices, unit_work: instances.max(1), step, overlap }
    };
    plan.sliceable().then_some(plan)
  }

  /// The draw's work in vertices: units times the vertices per unit.
  pub(crate) fn work(&self) -> u64 {
    self.units * self.unit_work
  }

  /// The shortest slice: one primitive, or a strip's overlap plus one step.
  pub(crate) fn min_take(&self) -> u64 {
    self.overlap + self.step
  }

  /// Whether the units hold two slices: the shortest slice, then another
  /// one starting `overlap` back.
  pub(crate) fn sliceable(&self) -> bool {
    self.units >= self.min_take() + self.step
  }

  /// Whole or sliced, from the entry's rate: whole when the draw's
  /// predicted run fits `SLICE_MERGE_MICROS`; sliced without a rate (the
  /// probe) or past it.
  pub(crate) fn issue(&self, rate: Option<f64>) -> Issue {
    match rate {
      Some(r) if self.work() as f64 * r <= SLICE_MERGE_MICROS as f64 => Issue::Whole,
      _ => Issue::Sliced,
    }
  }

  /// The length in units of the slice starting at `at`: sized to
  /// `SLICE_TARGET_MICROS` at `rate` (microseconds per vertex), or the
  /// probe size without one; at most `SLICE_GROWTH_MAX` times `prev`, the
  /// previous slice's length; a whole number of primitives; and the rest
  /// of the draw when what would remain could not hold a slice of its own.
  pub(crate) fn take(&self, at: u64, rate: Option<f64>, prev: Option<u64>) -> u64 {
    let remaining = self.units.saturating_sub(at);
    let wanted_work = match rate {
      Some(r) if r > 0.0 => (SLICE_TARGET_MICROS as f64 / r) as u64,
      _ => SLICE_PROBE_WORK,
    };
    let mut take = (wanted_work / self.unit_work.max(1)).max(self.min_take());
    if let Some(p) = prev {
      take = take.min(p.saturating_mul(SLICE_GROWTH_MAX)).max(self.min_take());
    }
    take = self.overlap + ((take - self.overlap) / self.step).max(1) * self.step;
    if take >= remaining || remaining - (take - self.overlap) < self.min_take() {
      return remaining;
    }
    take
  }

  /// Where the slice after one of `take` units at `at` starts: `overlap`
  /// back from its end, or the end of the draw when it was the last slice
  /// (no overlap is owed past the end, and the loop must terminate).
  pub(crate) fn advance(&self, at: u64, take: u64) -> u64 {
    if at + take >= self.units {
      self.units
    } else {
      at + take - self.overlap
    }
  }
}

/// The rate a sliced render leaves for the next: the slowest slice among
/// those long enough to measure their region (`SLICE_RATE_MIN_MICROS`),
/// and never under the render's total rate, which spreads every slice's
/// fixed cost over all the work.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RateMeter {
  total_micros: u64,
  total_work: u64,
  worst: f64,
}

impl RateMeter {
  /// One measured slice: `micros` for `work` vertices.
  pub(crate) fn add(&mut self, micros: u64, work: u64) {
    self.total_micros += micros;
    self.total_work += work;
    if micros >= SLICE_RATE_MIN_MICROS && work > 0 {
      self.worst = self.worst.max(micros as f64 / work as f64);
    }
  }

  /// Microseconds per vertex for the next render's plan; None before any
  /// slice.
  pub(crate) fn rate(&self) -> Option<f64> {
    (self.total_work > 0).then(|| self.worst.max(self.total_micros as f64 / self.total_work as f64))
  }
}

/// The fence a one-slice render leaves behind, polled without blocking:
/// its signal time, read against `issued`, bounds the draw's run from
/// above (on a tiler it is the frame's), for `work` vertices.
#[derive(Clone, Copy)]
pub(crate) struct Bound {
  pub fence: glow::Fence,
  pub issued: Instant,
  pub work: u64,
}

/// One draw entry's memory of the budget: the measured rate the next
/// render plans from, what the last render issued (the inventory's
/// figures), and the pending bound of a one-slice render. Cells, because
/// renders take the entry by shared reference.
pub(crate) struct SliceGauge {
  /// Microseconds per vertex from the slowest slice of the last exact
  /// measurement, or the bound that sent the entry back to slicing; None
  /// = never measured, the next render probes.
  rate: Cell<Option<f64>>,
  /// The last render's slice count (1 = one draw call); 0 before any.
  slices: Cell<u32>,
  /// The last render's longest submission, microseconds: the slowest
  /// slice, or the bound a one-slice render read.
  submission_micros: Cell<u64>,
  bound: Cell<Option<Bound>>,
  /// Whether the entry's first sliced render was logged.
  logged: Cell<bool>,
}

impl SliceGauge {
  pub(crate) fn new() -> Self {
    SliceGauge {
      rate: Cell::new(None),
      slices: Cell::new(0),
      submission_micros: Cell::new(0),
      bound: Cell::new(None),
      logged: Cell::new(false),
    }
  }

  pub(crate) fn rate(&self) -> Option<f64> {
    self.rate.get()
  }

  /// (slice count, longest submission in microseconds) of the last render.
  pub(crate) fn last_render(&self) -> (u32, u64) {
    (self.slices.get(), self.submission_micros.get())
  }

  /// A sliced render measured: `rate` its meter's verdict, `slices` how
  /// many, `longest` the slowest slice's microseconds.
  pub(crate) fn measured(&self, rate: f64, slices: u32, longest: u64) {
    self.rate.set(Some(rate));
    self.slices.set(slices);
    self.submission_micros.set(longest);
  }

  /// A one-slice render issued, with its bound fence (None when the driver
  /// gave no fence: the render still counts, unmeasured); the previous
  /// bound, if still pending, is handed back for deletion.
  pub(crate) fn whole(&self, bound: Option<Bound>) -> Option<Bound> {
    self.slices.set(1);
    self.bound.replace(bound)
  }

  pub(crate) fn bound(&self) -> Option<Bound> {
    self.bound.get()
  }

  /// Take the pending bound (its fence is the caller's to delete).
  pub(crate) fn take_bound(&self) -> Option<Bound> {
    self.bound.take()
  }

  /// A one-slice render's bound read: `micros` from issue to the fence's
  /// signal, an upper estimate. Past the budget it becomes the rate, so
  /// the next render slices and measures exactly; under it the exact rate
  /// stands.
  pub(crate) fn bound_read(&self, micros: u64, work: u64) {
    self.submission_micros.set(micros);
    if micros > SUBMISSION_BUDGET_MICROS && work > 0 {
      let bound = micros as f64 / work as f64;
      let rate = self.rate.get().map_or(bound, |r| r.max(bound));
      self.rate.set(Some(rate));
    }
  }

  /// Forget the measurement: the draw's content changed in a way the
  /// engine knows about (its instance order's first real publish over the
  /// sentinel seed, when every instance was culled), so the next render
  /// probes again. The pending bound is handed back for deletion.
  pub(crate) fn reset(&self) -> Option<Bound> {
    self.rate.set(None);
    self.slices.set(0);
    self.submission_micros.set(0);
    self.bound.take()
  }

  /// Whether the first sliced render is still to be logged (and marks it).
  pub(crate) fn log_once(&self) -> bool {
    !self.logged.replace(true)
  }
}

/// What a pass's draws did under the budget, for the pass-accounting owner
/// (raster::timed_pass): the submissions issued beyond one per draw, the
/// raster-thread time spent waiting on slice fences, and whether any draw
/// left a bound fence to poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SliceReport {
  pub extra_submissions: u64,
  pub wait_micros: u64,
  pub bounds: u32,
}

thread_local! {
  // Accumulated by the draw loop on this thread, taken around each pass by
  // the owner - the same shape as pass::take_vertices.
  static REPORT: Cell<SliceReport> = const { Cell::new(SliceReport { extra_submissions: 0, wait_micros: 0, bounds: 0 }) };
}

/// Credit a sliced draw: `extra` submissions beyond one and the wait time.
pub(crate) fn note_slices(extra: u64, wait_micros: u64) {
  REPORT.with(|r| {
    let mut v = r.get();
    v.extra_submissions += extra;
    v.wait_micros += wait_micros;
    r.set(v);
  });
}

/// Note a one-slice draw's bound fence, left for the owner to poll.
pub(crate) fn note_bound() {
  REPORT.with(|r| {
    let mut v = r.get();
    v.bounds += 1;
    r.set(v);
  });
}

/// Read and zero this thread's report.
pub(crate) fn take_slice_report() -> SliceReport {
  REPORT.with(|r| r.replace(SliceReport::default()))
}
