//! GPU-side pass and frame duration via timer queries.
//!
//! The raster thread's wall clock around a pass measures issue cost, not
//! execution: GL is asynchronous, so a pass with a heavy fragment shader and
//! a trivial command stream reads as free. `PassTimer` wraps each pass in a
//! `TIME_ELAPSED` query and harvests the results later, never blocking: a
//! query is read only once the driver reports it available, so the numbers
//! lag the pass by a frame or two, which is fine for cumulative counters.
//!
//! Availability: `GL_EXT_disjoint_timer_query` on GLES (Mesa, Android,
//! ANGLE over D3D11) or core timer queries on desktop GL 3.3+. Without
//! either the timer is inert and the counters it feeds stay absent, so
//! "unsupported" and "idle" stay distinguishable upstream.

use glow::HasContext;
use std::collections::VecDeque;

const GPU_DISJOINT_EXT: u32 = 0x8FBB;
/// Queries in flight before new spans go untimed. A frame issues a handful
/// of passes and results retire within a frame or two, so this is headroom,
/// not a budget.
const MAX_PENDING: usize = 64;

/// What a timed span was: a pass into a target (0 for passes with no
/// retained target, e.g. node shaders) or the window draw of one frame.
#[derive(Clone, Copy)]
pub enum Timed {
  Pass { target: u64 },
  Frame,
}

/// One harvested span and its GPU execution time.
pub(crate) struct PassExec {
  pub what: Timed,
  pub micros: u64,
}

pub struct PassTimer {
  supported: bool,
  /// GLES: `GPU_DISJOINT_EXT` must be checked when harvesting; a disjoint
  /// event (clock change, context switch) invalidates every pending result.
  disjoint_ext: bool,
  free: Vec<glow::Query>,
  pending: VecDeque<(glow::Query, Timed)>,
  /// `TIME_ELAPSED` queries cannot nest; the pass inside an active one goes
  /// untimed rather than erroring.
  active: bool,
}

impl PassTimer {
  pub fn new(gl: &glow::Context) -> Self {
    let ext = gl.supported_extensions();
    let disjoint_ext = ext.contains("GL_EXT_disjoint_timer_query");
    let v = gl.version();
    let desktop_core = !v.is_embedded && (v.major > 3 || (v.major == 3 && v.minor >= 3));
    let supported = disjoint_ext || desktop_core || ext.contains("GL_ARB_timer_query");
    if !supported {
      log::info!("[alloy] GPU timer queries unavailable; pass exec time will not be reported");
    }
    PassTimer { supported, disjoint_ext, free: Vec::new(), pending: VecDeque::new(), active: false }
  }

  pub fn supported(&self) -> bool {
    self.supported
  }

  /// Start timing a pass. Returns false when the pass will not be timed
  /// (unsupported, nested, or the pending queue is full); `end` is then a
  /// no-op, so callers pair them unconditionally.
  pub fn begin(&mut self, gl: &glow::Context) -> bool {
    if !self.supported || self.active || self.pending.len() >= MAX_PENDING {
      return false;
    }
    let query = match self.free.pop() {
      Some(q) => q,
      None => match unsafe { gl.create_query() } {
        Ok(q) => q,
        Err(e) => {
          log::warn!("[alloy] GPU timer query creation failed: {e}; disabling pass exec timing");
          self.supported = false;
          return false;
        }
      },
    };
    unsafe { gl.begin_query(glow::TIME_ELAPSED, query) };
    self.pending.push_back((query, Timed::Frame));
    self.active = true;
    true
  }

  /// End the span started by `begin`, attributing it to `what`.
  pub fn end(&mut self, gl: &glow::Context, what: Timed) {
    if !self.active {
      return;
    }
    unsafe { gl.end_query(glow::TIME_ELAPSED) };
    if let Some(back) = self.pending.back_mut() {
      back.1 = what;
    }
    self.active = false;
  }

  /// Harvest every retired query, in issue order, without blocking. Called
  /// once per raster command; a result not yet available ends the sweep
  /// (later queries cannot be available before earlier ones).
  pub fn poll(&mut self, gl: &glow::Context) -> Vec<PassExec> {
    let mut out = Vec::new();
    if self.pending.is_empty() {
      return out;
    }
    // Any pending query is in flight; the one still recording is never
    // available, so it simply stops the sweep like any other.
    while let Some(&(query, what)) = self.pending.front() {
      let mut available: u64 = 0;
      unsafe {
        gl.get_query_parameter_u64_with_offset(query, glow::QUERY_RESULT_AVAILABLE, &mut available as *mut u64 as usize)
      };
      if available == 0 {
        break;
      }
      let mut nanos: u64 = 0;
      unsafe { gl.get_query_parameter_u64_with_offset(query, glow::QUERY_RESULT, &mut nanos as *mut u64 as usize) };
      self.pending.pop_front();
      self.free.push(query);
      out.push(PassExec { what, micros: nanos / 1000 });
    }
    if !out.is_empty() && self.disjoint_ext {
      let disjoint = unsafe { gl.get_parameter_i32(GPU_DISJOINT_EXT) } != 0;
      if disjoint {
        log::debug!("[alloy] GPU timer disjoint; dropping {} pass timings", out.len());
        out.clear();
      }
    }
    out
  }
}
