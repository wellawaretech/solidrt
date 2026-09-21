//! The frame's GPU time from the compositor stack's per-frame timestamps
//! (EGL_ANDROID_get_frame_timestamps): the window buffer's rendering-complete
//! time, the same fence SurfaceFlinger reports as frameReady. Android's
//! window draw cannot be timed with a TIME_ELAPSED query on every driver: on
//! Adreno the span around the draw reads the frame interval rather than the
//! work (32 ms at a two-refresh cadence, 46-52 at three, for a 15 ms frame),
//! which fed the cadence hold a term that never let it step down
//! (okf/backlog/android-frame-gpu-time-from-egl-timestamps.md). Where the
//! extension is present the frame's GPU term comes from here and the frame's
//! timer query is not issued; the per-pass queries are untouched.
//!
//! The span charged to a frame is `complete - max(begin, previous complete)`:
//! a frame queued behind the previous frame's GPU work is charged for its own
//! execution only (the Frame Pacing library's model), and `begin` is taken
//! ahead of the offscreen pass flush, since on a tiler the passes execute in
//! the same submission as the window draw. Results land a frame or two behind
//! the present they describe, which the controller already windows.
//!
//! Owned by the raster thread, the one thread with the window binding
//! current; every EGL call here runs there. The three entry points the safe
//! khronos-egl API does not expose are resolved through eglGetProcAddress
//! into signatures pinned to the extension's ABI; nothing else is reached
//! beneath the safe API.

use std::collections::VecDeque;

// EGL_ANDROID_get_frame_timestamps tokens (EGL/eglext.h; the compositor
// timing names 0x3431-0x3433 sit between the enable and the frame
// timestamps). The probe-only ones live behind the Android gate with the
// probe.
#[cfg(target_os = "android")]
const EGL_TIMESTAMPS_ANDROID: i32 = 0x3430;
const EGL_RENDERING_COMPLETE_TIME_ANDROID: i32 = 0x3435;
const EGL_TIMESTAMP_PENDING_ANDROID: i64 = -2;
const EGL_TIMESTAMP_INVALID_ANDROID: i64 = -1;
const EGL_TRUE: u32 = 1;
#[cfg(target_os = "android")]
const EXTENSION: &str = "EGL_ANDROID_get_frame_timestamps";

/// Frames whose completion is still awaited. The display stack keeps a short
/// history of its own, so an entry this far back is asked once more and then
/// dropped rather than kept forever.
const PENDING_DEPTH: usize = 4;
/// Queued frames that left the ring without ever completing (the stack
/// answered gone, or never answered) before the source is given up on and
/// the timer query takes the frame back. A stack that works answers within
/// a frame or two; one that does not would otherwise starve the controller
/// of its GPU term silently.
const UNANSWERED_LIMIT: u32 = 8;

type EglDisplay = *mut std::ffi::c_void;
type EglSurface = *mut std::ffi::c_void;
type GetNextFrameId = unsafe extern "system" fn(EglDisplay, EglSurface, *mut u64) -> u32;
#[cfg(target_os = "android")]
type GetFrameTimestampSupported = unsafe extern "system" fn(EglDisplay, EglSurface, i32) -> u32;
type GetFrameTimestamps = unsafe extern "system" fn(EglDisplay, EglSurface, u64, i32, *const i32, *mut i64) -> u32;

/// One queued frame awaiting its rendering-complete time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Entry {
  pub frame_id: u64,
  /// CLOCK_MONOTONIC ns at the frame's first GPU command.
  pub begin_ns: i64,
}

/// What the display stack answers for one frame id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Poll {
  /// The GPU has not finished the frame.
  Pending,
  /// No value will come (the stack did not record one, or the frame left
  /// its history).
  Gone,
  /// Rendering complete at this CLOCK_MONOTONIC ns.
  Complete(i64),
}

/// What one sweep settled: the latest completed frame's span, and how many
/// frames left the ring without completing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Swept {
  pub latest_micros: Option<u64>,
  pub gone: u32,
}

struct Armed {
  // The binding the entry points were resolved against; also answers for
  // the EGL error when a call fails.
  egl: crate::egl_headless::Egl,
  display: EglDisplay,
  surface: EglSurface,
  next_frame_id: GetNextFrameId,
  timestamps: GetFrameTimestamps,
}

enum State {
  /// Not asked yet for the current window surface.
  Unprobed,
  /// No extension, the surface refused it, or the stack never answered: the
  /// timer query path stands.
  Unsupported,
  // Only the Android probe constructs it; the other targets keep the
  // machinery so the raster thread's code is the same everywhere.
  #[cfg_attr(not(target_os = "android"), allow(dead_code))]
  Armed(Armed),
}

/// Per-frame GPU completion from the window surface's frame timestamps.
pub(crate) struct FrameTimestamps {
  state: State,
  /// The frame being drawn: stamped at its first GPU command, given its id
  /// just before the swap, queued once the swap succeeded.
  begin_ns: Option<i64>,
  in_flight: Option<Entry>,
  pending: VecDeque<Entry>,
  /// The previous frame's completion, the floor of the next span.
  last_complete_ns: Option<i64>,
  /// Frames that left the ring without completing since the last one that
  /// did (see UNANSWERED_LIMIT), and whether any frame ever completed.
  unanswered: u32,
  answered: bool,
}

impl FrameTimestamps {
  pub(crate) fn new() -> Self {
    FrameTimestamps {
      state: State::Unprobed,
      begin_ns: None,
      in_flight: None,
      pending: VecDeque::new(),
      last_complete_ns: None,
      unanswered: 0,
      answered: false,
    }
  }

  /// Whether frames are measured here (and the frame timer query is not
  /// issued). False until the first frame after a bind has probed.
  pub(crate) fn armed(&self) -> bool {
    matches!(self.state, State::Armed(_))
  }

  /// The window surface changed (Android replaces it across
  /// background/resume): timestamps must be re-enabled on the new one, and
  /// frames queued on the old one will never answer.
  pub(crate) fn forget(&mut self) {
    self.state = State::Unprobed;
    self.begin_ns = None;
    self.in_flight = None;
    self.pending.clear();
    self.last_complete_ns = None;
    self.unanswered = 0;
    self.answered = false;
  }

  /// The frame's first GPU command is about to be issued. Probes the surface
  /// on the first frame after a bind (the binding is current here).
  pub(crate) fn frame_begin(&mut self) {
    if matches!(self.state, State::Unprobed) {
      self.state = probe();
    }
    if self.armed() {
      self.begin_ns = now_ns();
    }
  }

  /// Just before the swap: the id the swap's buffer will be queued under.
  pub(crate) fn before_present(&mut self) {
    self.in_flight = None;
    let State::Armed(armed) = &self.state else { return };
    let Some(begin_ns) = self.begin_ns.take() else { return };
    let mut frame_id = 0u64;
    let ok = unsafe { (armed.next_frame_id)(armed.display, armed.surface, &mut frame_id) };
    if ok == EGL_TRUE {
      self.in_flight = Some(Entry { frame_id, begin_ns });
    } else {
      let why = format!("eglGetNextFrameIdANDROID failed ({:?})", armed.egl.get_error());
      self.give_up(&why);
    }
  }

  /// The swap succeeded: the in-flight frame is queued and will answer.
  pub(crate) fn presented(&mut self) {
    if let Some(entry) = self.in_flight.take() {
      if self.pending.len() >= PENDING_DEPTH {
        self.pending.pop_front();
        self.note_unanswered(1, "never answered");
      }
      self.pending.push_back(entry);
    }
  }

  /// Ask the display stack for every queued frame still awaited, oldest
  /// first, and return the GPU time (micros) of the latest frame that
  /// completed in this sweep. Non-blocking: a pending frame ends the sweep,
  /// since later frames cannot complete before earlier ones.
  pub(crate) fn harvest(&mut self) -> Option<u64> {
    let State::Armed(armed) = &self.state else { return None };
    let mut last_error = None;
    let poll = |frame_id: u64| -> Poll {
      let names = [EGL_RENDERING_COMPLETE_TIME_ANDROID];
      let mut value: i64 = 0;
      let ok = unsafe { (armed.timestamps)(armed.display, armed.surface, frame_id, 1, names.as_ptr(), &mut value) };
      if ok != EGL_TRUE {
        last_error = Some(armed.egl.get_error());
        Poll::Gone
      } else if value == EGL_TIMESTAMP_INVALID_ANDROID {
        Poll::Gone
      } else if value == EGL_TIMESTAMP_PENDING_ANDROID {
        Poll::Pending
      } else {
        Poll::Complete(value)
      }
    };
    let swept = sweep(&mut self.pending, &mut self.last_complete_ns, poll);
    if let Some(micros) = swept.latest_micros {
      if !self.answered {
        self.answered = true;
        log::info!("[alloy] frame timestamps: first frame completed, {:.1} ms", micros as f32 / 1000.0);
      }
      self.unanswered = 0;
    }
    if swept.gone > 0 {
      let why = match last_error {
        Some(e) => format!("gone, EGL error {e:?}"),
        None => "gone, no timestamp recorded".to_string(),
      };
      self.note_unanswered(swept.gone, &why);
    }
    swept.latest_micros
  }

  // Frames left the ring without completing. Before the first completion
  // ever, a run of them means the stack does not answer for this surface;
  // after it, a run means it stopped, and either way the timer query is the
  // better source from here on.
  fn note_unanswered(&mut self, count: u32, why: &str) {
    self.unanswered = self.unanswered.saturating_add(count);
    if self.unanswered >= UNANSWERED_LIMIT {
      let why = format!("{} frames {why}", self.unanswered);
      self.give_up(&why);
    }
  }

  fn give_up(&mut self, why: &str) {
    log::info!("[alloy] frame timestamps unusable ({why}); frame GPU time from the timer query");
    self.state = State::Unsupported;
    self.begin_ns = None;
    self.in_flight = None;
    self.pending.clear();
  }
}

/// The sweep behind `harvest`, over any answerer: pops every frame the stack
/// has settled (complete or gone), stops at the first pending one, and
/// reports the latest completed frame's span in micros beside the count of
/// frames that went without one.
pub(crate) fn sweep(
  pending: &mut VecDeque<Entry>,
  last_complete_ns: &mut Option<i64>,
  mut poll: impl FnMut(u64) -> Poll,
) -> Swept {
  let mut swept = Swept::default();
  while let Some(entry) = pending.front().copied() {
    match poll(entry.frame_id) {
      Poll::Pending => break,
      Poll::Gone => {
        pending.pop_front();
        swept.gone += 1;
      }
      Poll::Complete(complete_ns) => {
        pending.pop_front();
        if let Some(micros) = span_micros(complete_ns, entry.begin_ns, *last_complete_ns) {
          swept.latest_micros = Some(micros);
        }
        *last_complete_ns = Some(complete_ns);
      }
    }
  }
  swept
}

/// The GPU time charged to a frame: from the later of its first command and
/// the previous frame's completion, to its own completion. None when the
/// stamps disagree (a completion before the frame began is not a reading).
pub(crate) fn span_micros(complete_ns: i64, begin_ns: i64, last_complete_ns: Option<i64>) -> Option<u64> {
  let start = last_complete_ns.map_or(begin_ns, |last| last.max(begin_ns));
  let span = complete_ns - start;
  (span >= 0).then_some(span as u64 / 1000)
}

// Only Android's EGL carries the extension; elsewhere nothing is loaded or
// asked, and the frame keeps its timer query.
#[cfg(target_os = "android")]
fn probe() -> State {
  use khronos_egl as egl;
  let unsupported = |why: &str| {
    log::info!("[alloy] frame GPU time from the timer query ({why})");
    State::Unsupported
  };
  let instance = match crate::egl_headless::load_egl() {
    Ok(i) => i,
    Err(e) => return unsupported(&format!("no libEGL: {e}")),
  };
  let Some(display) = instance.get_current_display() else { return unsupported("no current EGL display") };
  let Some(surface) = instance.get_current_surface(egl::DRAW) else { return unsupported("no current EGL surface") };
  let extensions = instance.query_string(Some(display), egl::EXTENSIONS).map(|s| s.to_string_lossy().into_owned());
  if !extensions.is_ok_and(|s| s.split(' ').any(|e| e == EXTENSION)) {
    return unsupported(&format!("{EXTENSION} absent"));
  }
  let Some(next_frame_id) = instance.get_proc_address("eglGetNextFrameIdANDROID") else {
    return unsupported("eglGetNextFrameIdANDROID missing");
  };
  let Some(supported) = instance.get_proc_address("eglGetFrameTimestampSupportedANDROID") else {
    return unsupported("eglGetFrameTimestampSupportedANDROID missing");
  };
  let Some(timestamps) = instance.get_proc_address("eglGetFrameTimestampsANDROID") else {
    return unsupported("eglGetFrameTimestampsANDROID missing");
  };
  // The extension's ABI, fixed since Android 8: the pointers eglGetProcAddress
  // returns for these names have exactly these signatures.
  let next_frame_id: GetNextFrameId = unsafe { std::mem::transmute(next_frame_id) };
  let supported: GetFrameTimestampSupported = unsafe { std::mem::transmute(supported) };
  let timestamps: GetFrameTimestamps = unsafe { std::mem::transmute(timestamps) };
  if let Err(e) = instance.surface_attrib(display, surface, EGL_TIMESTAMPS_ANDROID, EGL_TRUE as i32) {
    return unsupported(&format!("EGL_TIMESTAMPS_ANDROID refused: {e}"));
  }
  let (display, surface) = (display.as_ptr(), surface.as_ptr());
  if unsafe { supported(display, surface, EGL_RENDERING_COMPLETE_TIME_ANDROID) } != EGL_TRUE {
    return unsupported("rendering-complete time not supported on this surface");
  }
  log::info!("[alloy] frame GPU time from EGL frame timestamps (rendering complete)");
  State::Armed(Armed { egl: instance, display, surface, next_frame_id, timestamps })
}

#[cfg(not(target_os = "android"))]
fn probe() -> State {
  State::Unsupported
}

/// CLOCK_MONOTONIC ns, the clock the frame timestamps are on.
#[cfg(target_os = "android")]
fn now_ns() -> Option<i64> {
  let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
  let ok = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } == 0;
  ok.then(|| now.tv_sec as i64 * 1_000_000_000 + now.tv_nsec as i64)
}

#[cfg(not(target_os = "android"))]
fn now_ns() -> Option<i64> {
  None
}
