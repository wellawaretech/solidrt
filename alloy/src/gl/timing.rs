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
//!
//! Trust: support alone is not enough. Some tiled GPUs defer a pass's tile
//! execution until its result is sampled, and the deferred work is then
//! booked to whichever query happens to be open, so the per-pass/per-frame
//! split follows draw order instead of the work and reads as authoritative
//! nonsense (okf/done/gpu-timer-attribution.md). `new` therefore runs
//! `attribution_probe`, which measures this driver's attribution and
//! disables the timer when it is caught lying - a behavioral verdict per
//! device, never a vendor list. Independently, `poll` bounds every result
//! by wall clock: a span cannot have executed longer than the time between
//! its begin and its harvest, so a larger reading is a bad read and is
//! dropped rather than served.

use glow::HasContext;
use std::collections::VecDeque;
use std::time::Instant;

use super::program::ShaderProgram;

const GPU_DISJOINT_EXT: u32 = 0x8FBB;
/// Queries in flight before new spans go untimed. A frame issues a handful
/// of passes and results retire within a frame or two, so this is headroom,
/// not a budget.
const MAX_PENDING: usize = 64;

/// Attribution probe target edge in pixels; with PROBE_ITERATIONS it sizes
/// the heavy pass to hundreds of microseconds on desktop GPUs and a few
/// milliseconds on low-end mobile - orders of magnitude above the trivial
/// sample pass, negligible against startup.
const PROBE_SIZE: i32 = 256;
/// Sin/cos iterations per fragment of the heavy probe pass (see PROBE_SIZE).
const PROBE_ITERATIONS: u32 = 1024;
/// Probe repetitions; the verdict is the majority, absorbing DVFS clock
/// ramp-up and one-off stalls at startup.
const PROBE_RUNS: u32 = 3;
/// Minimum share of the two-pass total the heavy pass must carry for
/// attribution to count as honest. The heavy pass dwarfs the sample pass by
/// construction, so an honest driver lands near 1.0 and a deferring driver
/// near 0.0; 0.5 splits them with wide margin both ways.
const ATTRIBUTION_MIN_SHARE: f64 = 0.5;

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
  /// In-flight queries in issue order, each with what it timed and when it
  /// began (raster-thread wall clock, the harvest-time sanity bound).
  pending: VecDeque<(glow::Query, Timed, Instant)>,
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
    let mut supported = disjoint_ext || desktop_core || ext.contains("GL_ARB_timer_query");
    if !supported {
      log::info!("[alloy] GPU timer queries unavailable; pass exec time will not be reported");
    } else {
      match attribution_probe(gl, disjoint_ext) {
        Ok(true) => {}
        Ok(false) => {
          supported = false;
          log::info!(
            "[alloy] GPU timer attribution failed its self-test (deferred pass execution is booked to the wrong query); pass exec time will not be reported"
          );
        }
        // No measurement either way; keep the timer rather than punishing a
        // probe failure, but say the numbers are unverified.
        Err(e) => log::warn!("[alloy] GPU timer attribution self-test could not run ({e}); timings are unverified"),
      }
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
    self.pending.push_back((query, Timed::Frame, Instant::now()));
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
    while let Some(&(query, what, begun)) = self.pending.front() {
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
      // A span cannot have executed longer than the wall clock between its
      // begin and its harvest; a larger reading is a bad read (stale query,
      // bogus counter) and no configuration may report more GPU time than
      // wall time (okf/done/gpu-timer-attribution.md, symptom 2).
      let micros = nanos / 1000;
      let wall_micros = begun.elapsed().as_micros() as u64;
      if micros > wall_micros {
        log::debug!("[alloy] GPU timer read {micros}us exceeds its {wall_micros}us wall-clock bound; dropped");
        continue;
      }
      out.push(PassExec { what, micros });
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

/// Offscreen color target for one probe pass, deleted when the probe ends.
struct ProbeTarget {
  tex: glow::Texture,
  fbo: glow::Framebuffer,
}

fn probe_target(gl: &glow::Context) -> Result<ProbeTarget, String> {
  unsafe {
    let tex = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::RGBA8, PROBE_SIZE, PROBE_SIZE);
    // Single-level storage: NEAREST keeps the texture sampling-complete
    // without mips.
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    let fbo = match gl.create_framebuffer() {
      Ok(f) => f,
      Err(e) => {
        gl.delete_texture(tex);
        return Err(format!("glGenFramebuffers failed: {e}"));
      }
    };
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    if status != glow::FRAMEBUFFER_COMPLETE {
      gl.delete_framebuffer(fbo);
      gl.delete_texture(tex);
      return Err(format!("probe framebuffer incomplete: {status:#x}"));
    }
    Ok(ProbeTarget { tex, fbo })
  }
}

fn drop_probe_target(gl: &glow::Context, t: ProbeTarget) {
  unsafe {
    gl.delete_framebuffer(t.fbo);
    gl.delete_texture(t.tex);
  }
}

/// Measure whether this driver books deferred pass execution to the pass's
/// own timer query. Pass A renders a deliberately expensive shader into an
/// offscreen target inside one `TIME_ELAPSED` query; pass B samples that
/// target through a trivial shader inside a second; a `glFinish` closes the
/// run. An honest driver books the heavy work to A's query. A driver that
/// defers tile execution until the result is sampled books it to B's, and
/// A's share of the total collapses - the exact failure that makes the
/// pass/frame split track draw order instead of work. Runs once on the
/// raster thread at startup, before any frame.
///
/// Ok(true): attribution held. Ok(false): caught lying. Err: the probe
/// itself could not run, so nothing was measured either way.
fn attribution_probe(gl: &glow::Context, disjoint_ext: bool) -> Result<bool, String> {
  let heavy_src = format!(
    "void main() {{\n  float acc = 0.0;\n  for (int i = 0; i < {PROBE_ITERATIONS}; i++) {{\n    acc += sin(vUV.x + float(i)) * cos(vUV.y + float(i));\n  }}\n  fragColor = vec4(vec3(acc / float({PROBE_ITERATIONS}) + 0.5), 1.0);\n}}\n"
  );
  // uSrc stays at its GL default, texture unit 0, where the run binds pass
  // A's target.
  let sample_src = "uniform sampler2D uSrc;\nvoid main() { fragColor = texture(uSrc, vUV); }";
  let heavy = ShaderProgram::new_fragment(gl, &heavy_src).map_err(|e| format!("heavy probe shader: {e}"))?;
  let sample = match ShaderProgram::new_fragment(gl, sample_src) {
    Ok(s) => s,
    Err(e) => {
      heavy.delete(gl);
      return Err(format!("sample probe shader: {e}"));
    }
  };
  let a = match probe_target(gl) {
    Ok(t) => t,
    Err(e) => {
      heavy.delete(gl);
      sample.delete(gl);
      return Err(e);
    }
  };
  let b = match probe_target(gl) {
    Ok(t) => t,
    Err(e) => {
      drop_probe_target(gl, a);
      heavy.delete(gl);
      sample.delete(gl);
      return Err(e);
    }
  };

  let verdict = probe_runs(gl, &heavy, &sample, &a, &b, disjoint_ext);
  unsafe {
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    gl.bind_texture(glow::TEXTURE_2D, None);
    gl.use_program(None);
  }
  drop_probe_target(gl, a);
  drop_probe_target(gl, b);
  heavy.delete(gl);
  sample.delete(gl);
  verdict
}

/// The measurement loop of `attribution_probe`: PROBE_RUNS repetitions of
/// heavy-pass-into-A, sample-A-into-B, finish, read both queries. Owns only
/// the two query objects; the caller owns everything else and restores GL
/// state afterwards.
fn probe_runs(
  gl: &glow::Context,
  heavy: &ShaderProgram,
  sample: &ShaderProgram,
  a: &ProbeTarget,
  b: &ProbeTarget,
  disjoint_ext: bool,
) -> Result<bool, String> {
  let mut honest = 0u32;
  let mut broken = 0u32;
  unsafe {
    let qa = gl.create_query().map_err(|e| format!("glGenQueries failed: {e}"))?;
    let qb = match gl.create_query() {
      Ok(q) => q,
      Err(e) => {
        gl.delete_query(qa);
        return Err(format!("glGenQueries failed: {e}"));
      }
    };
    // Reading the disjoint flag clears it, so a disjoint left over from
    // context setup does not void the first run.
    if disjoint_ext {
      gl.get_parameter_i32(GPU_DISJOINT_EXT);
    }
    gl.disable(glow::SCISSOR_TEST);
    gl.disable(glow::DEPTH_TEST);
    gl.disable(glow::BLEND);
    gl.disable(glow::CULL_FACE);
    gl.viewport(0, 0, PROBE_SIZE, PROBE_SIZE);
    gl.bind_vertex_array(None);
    for run in 0..PROBE_RUNS {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(a.fbo));
      gl.use_program(Some(heavy.program));
      gl.begin_query(glow::TIME_ELAPSED, qa);
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      gl.end_query(glow::TIME_ELAPSED);

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(b.fbo));
      gl.use_program(Some(sample.program));
      gl.active_texture(glow::TEXTURE0);
      gl.bind_texture(glow::TEXTURE_2D, Some(a.tex));
      gl.bind_sampler(0, None);
      gl.begin_query(glow::TIME_ELAPSED, qb);
      gl.draw_arrays(glow::TRIANGLES, 0, 3);
      gl.end_query(glow::TIME_ELAPSED);

      // The finish makes both results available and keeps a lying driver's
      // deferral inside this run.
      gl.finish();

      if disjoint_ext && gl.get_parameter_i32(GPU_DISJOINT_EXT) != 0 {
        log::debug!("[alloy] attribution probe run {run} hit a disjoint event; discarded");
        continue;
      }
      let mut heavy_nanos: u64 = 0;
      gl.get_query_parameter_u64_with_offset(qa, glow::QUERY_RESULT, &mut heavy_nanos as *mut u64 as usize);
      let mut sample_nanos: u64 = 0;
      gl.get_query_parameter_u64_with_offset(qb, glow::QUERY_RESULT, &mut sample_nanos as *mut u64 as usize);
      let total = heavy_nanos + sample_nanos;
      // A zero total means the queries cannot see the heavy pass at all:
      // a broken vote, not a discard.
      let share = if total > 0 { heavy_nanos as f64 / total as f64 } else { 0.0 };
      log::debug!("[alloy] attribution probe run {run}: heavy {heavy_nanos}ns, sample {sample_nanos}ns, share {share:.2}");
      if share >= ATTRIBUTION_MIN_SHARE {
        honest += 1;
      } else {
        broken += 1;
      }
    }
    gl.delete_query(qa);
    gl.delete_query(qb);
  }
  if honest + broken == 0 {
    return Err("every probe run hit a disjoint event".to_string());
  }
  Ok(honest > broken)
}