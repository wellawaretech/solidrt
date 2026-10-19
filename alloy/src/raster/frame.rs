//! The frame path: draw the display list to the window and hand it on -
//! present in interactive mode, leave it for a readback headless - plus the
//! present-side pacing policy that surrounds it: the present fence depth
//! gate, missed-present (jank) accounting, the swap itself with its
//! context-loss exit, and the window surface rebind.

use std::sync::atomic::Ordering;

use impellers::{DisplayList, ISize};

use super::repaint::WindowRoute;
use super::{
  DamageRect, FrameSink, PresentDamage, RasterState, PRESENT_FAILURE_EXIT_THRESHOLD, PRESENT_FENCE_DEPTH,
  PRESENT_FENCE_LOSS_SECS, PRESENT_FENCE_TIMEOUT_NS,
};
use crate::backend::FrameOutput;
use crate::gl;
use crate::gl::{ResetStatus, Timed};

/// Why a frame could not complete: the main loop is gone (the thread winds
/// down), or the GL context is lost (the thread recovers, see recover.rs).
pub(super) enum FrameExit {
  MainGone,
  /// The detail for the log, and the reset status the driver gave (None
  /// for a loss found another way: a failed present, the fence watchdog).
  ContextLost {
    detail: String,
    status: ResetStatus,
  },
}

/// What a present did: swapped, failed to swap (the surface is gone or the
/// driver hiccupped; the rebind-and-retry and the failure threshold apply),
/// or found the context lost, with the detail for the log and the status.
enum Presented {
  Yes,
  No,
  Lost { detail: String, status: ResetStatus },
}

/// Whether an SDL error names a lost context: SDL's text carries the EGL
/// error (`EGL_CONTEXT_LOST`, the swap's and make-current's answer once a
/// device is gone under ANGLE or a context was reset under Mesa).
pub(crate) fn is_context_lost_error(error: &str) -> bool {
  error.contains("CONTEXT_LOST")
}

impl RasterState {
  /// Publish the per-target pass counters (RasterStats::targets): one row
  /// per shader target that has rendered, labelled like the GPU inventory
  /// (a sub-target by its region's label), plus the node shader passes
  /// under id 0. Once per presented frame, as a fresh snapshot.
  fn publish_target_counters(&self) {
    let timed = self.stats.timer_queries.load(Ordering::Relaxed);
    let exec = |micros: u64| timed.then_some(micros);
    let mut targets: Vec<super::TargetCounters> = self
      .shaders
      .iter()
      .map(|(id, shader)| {
        let (passes, pass_issue_micros, pass_exec_micros, vertices) = shader.pass_stats();
        super::TargetCounters {
          id: *id,
          label: self
            .textures
            .get(id)
            .and_then(|t| t.label.clone())
            .or_else(|| shader.region().and_then(|r| r.label.clone())),
          passes,
          pass_issue_micros,
          pass_exec_micros: exec(pass_exec_micros),
          vertices,
        }
      })
      .filter(|t| t.passes > 0)
      .collect();
    if self.node_shader_passes > 0 {
      targets.push(super::TargetCounters {
        id: 0,
        label: Some("node shaders".to_string()),
        passes: self.node_shader_passes,
        pass_issue_micros: self.node_shader_issue_micros,
        pass_exec_micros: exec(self.node_shader_exec_micros),
        vertices: self.node_shader_vertices,
      });
    }
    targets.sort_by_key(|t| t.id);
    *self.stats.targets.lock().expect("target counters lock poisoned") = std::sync::Arc::new(targets);
  }

  /// Draw the frame's display list to the window framebuffer and hand it on:
  /// present in interactive mode, leave it in the headless window target in
  /// the headless mode. Then notify the main loop, which only does frame
  /// bookkeeping (fps, FrameRendered). `frame` is the UI frame number the
  /// list was built under, what its GPU completion is published as (see
  /// `RasterStats::retired_frames`). Err means the main loop is gone and
  /// this thread should exit.
  pub(super) fn frame(&mut self, dl: DisplayList, present_at: std::time::Instant, frame: u64) -> Result<(), FrameExit> {
    // A context lost since the last present fails every allocation the
    // frame makes, so the draw never reaches the swap where the loss would
    // otherwise be found: ask first (a flag read).
    let status = self.robustness.reset_status();
    if status.is_reset() {
      return Err(FrameExit::ContextLost { detail: self.loss_detail("before the frame", status), status });
    }
    // The error flag is the second word on it: GL_CONTEXT_LOST
    // (KHR_robustness) is raised once by a lost context, and a driver that
    // never flips the reset status may still raise it. Anything else latched
    // here is some pass's silent no-op, logged for diagnosis (rate-limited
    // like the fence line) and not a loss.
    let error = unsafe { glow::HasContext::get_error(&self.gl) };
    if error == glow::CONTEXT_LOST {
      let status = ResetStatus::Unknown;
      return Err(FrameExit::ContextLost {
        detail: self.loss_detail("GL_CONTEXT_LOST before the frame", status),
        status,
      });
    }
    if error != glow::NO_ERROR && self.gl_error_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
      self.gl_error_log = Some(std::time::Instant::now());
      log::warn!("[alloy] GL error {error:#x} latched before the frame");
    }
    // The frame's GPU span starts here, ahead of the pass flush: on a tiler
    // the passes execute in the same submission as the window draw.
    self.frame_timestamps.frame_begin();
    // The video frames due for this present go into their textures first,
    // so the flush below converts them and the frame samples the result.
    self.latch_video(present_at);
    self.latch_cameras();
    // Last frame's window tap readbacks have had a whole frame to land.
    self.complete_window_taps();
    // The frame samples shader targets (directly via <texture src>, or through
    // the window-shader layer); resolve every pending target write first.
    self.flush_dirty();
    let (width, height) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
    let size = ISize::new(width as i64, height as i64);
    // This frame's content delta: its own damage plus any load-shed frames',
    // plus the overlay's rect while one is active - the overlay is BLENDED
    // over the frame, so the pixels under it must re-raster every frame or
    // last frame's composite would stack.
    let mut own_damage = self.damage.take();
    if let Some(ov) = &self.overlay {
      own_damage = own_damage.union(PresentDamage::Rect(DamageRect {
        x: ov.decl.x,
        y: ov.decl.y,
        width: ov.decl.width as i32,
        height: ov.decl.height as i32,
      }));
    }
    // The window framebuffer: FBO 0, or the headless window target at this
    // frame's size. A target that cannot be allocated (a size past the
    // device's limits) skips the frame like a failed draw, with the reason.
    let window = match self.window_framebuffer(size) {
      Ok(window) => window,
      Err(e) => {
        log::warn!("[alloy] {e}; skipping frame");
        None
      }
    };
    let target_ok = window.is_some() || !self.sink.headless();
    // The in-tile fast path is FBO 0's own: an owned target is single-sample
    // and goes through the rig.
    let fast_path = window.is_none() && gl::window_fast_path(&self.gl);
    // A headless frame is read in full (the window, any node of it) and the
    // window shader redraws its whole layer; neither frame kind may be
    // pruned to a patch.
    let patch_barred = self.sink.headless() || self.window_shader.is_some();
    let mut route = self.damage.route(own_damage, size, fast_path, patch_barred);
    let wait_start = std::time::Instant::now();
    self.await_present_fence()?;
    let wait_ms = wait_start.elapsed().as_secs_f32() * 1000.0;
    let draw_start = std::time::Instant::now();
    // The frame's GPU time comes from the frame timestamps where they are
    // armed; the timer query around the window draw is then not issued, but
    // the span is still held so the offscreen rasters inside the frame stay
    // untimed as they are under the query (`end` releases either).
    if self.frame_timestamps.armed() {
      self.pass_timer.hold();
    } else {
      self.pass_timer.begin(&self.gl);
    }
    // A backgrounded window has no surface to draw to (see WINDOW_BACKGROUNDED
    // in lib.rs): the frame is dropped like an undrawn one, its damage kept
    // for the frame that follows the return-to-visible rebind.
    let backgrounded = crate::window_backgrounded();
    let drawn = !backgrounded && target_ok && self.draw_to_window(&dl, size, route, window);
    self.pass_timer.end(&self.gl, Timed::Frame);
    let draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;
    // The overlay composites over the finished frame (shaded or not), so a
    // headless window readback carries it too. Excluded from draw_ms: it is
    // diagnostics, not the app's frame cost.
    if drawn {
      self.draw_overlay(size, window);
      // The frame as it goes to the window, for the window taps: the read
      // is asynchronous, completed at the next frame.
      self.issue_window_taps(window, (width, height), crate::sdl_utils::ticks_ns());
    }
    // A frame that is not presented has no present fence to retire it: a
    // headless or capture frame is read back, and the readback would sync
    // anyway, so it waits for its own work here (bounded, like the pacing
    // wait) and publishes its completion the way a present's fence does.
    if drawn && self.sink.headless() {
      self.finish_gpu_work(frame);
    }
    if self.sink == FrameSink::Window {
      let present_start = std::time::Instant::now();
      let mut presented = false;
      if drawn {
        match self.present(frame) {
          Presented::Yes => presented = true,
          Presented::Lost { detail, status } => return Err(FrameExit::ContextLost { detail, status }),
          Presented::No => {
            if !crate::window_backgrounded() && self.rebind_window_surface() {
              // The failed swap's frame is lost with the dead binding (Android
              // replaces the EGL surface across background/resume, and a frame
              // latched by resize or expose can reach this thread before the
              // event-driven rebind). Redraw against the rebound surface and
              // present again; the retry's outcome feeds the failure threshold
              // honestly (fail, rebind, fail again = a dead swapchain). The
              // fresh surface preserves nothing, so the retry draws in full.
              self.damage.invalidated();
              own_damage = PresentDamage::Full;
              route = WindowRoute::whole(fast_path);
              if self.draw_to_window(&dl, size, route, window) {
                match self.present(frame) {
                  Presented::Yes => presented = true,
                  Presented::Lost { detail, status } => return Err(FrameExit::ContextLost { detail, status }),
                  Presented::No => {}
                }
              }
            }
          }
        }
      }
      if presented {
        self.damage.presented(own_damage);
        if route.is_patch() {
          self.stats.partial_presents.fetch_add(1, Ordering::Relaxed);
        }
      } else {
        self.damage.not_presented(own_damage);
      }
      let presented_at = std::time::Instant::now();
      let present_ms = presented_at.duration_since(present_start).as_secs_f32() * 1000.0;
      self.timing.record(wait_ms, draw_ms, present_ms);
      let demanded = self.demand_at_present(drawn);
      // A frame's native cost beyond ~2 vsync periods means this thread is
      // being stalled in the driver; log which step, rate-limited to one line
      // per second so a sustained stall stays readable. Debug, not warn: a
      // saturated tiled GPU (Android TV) lives here in steady state, and the
      // timing stats carry the numbers - raise SOLIDRT_LOG=debug to see these.
      if wait_ms + draw_ms + present_ms > 35.0 && self.slow_frame_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
        self.slow_frame_log = Some(std::time::Instant::now());
        log::debug!("[alloy] slow frame: fence wait {wait_ms:.1}ms, draw {draw_ms:.1}ms, present {present_ms:.1}ms");
      }
      // Resize-race diagnostics: the published surface size moved while this
      // frame was drawing, so what just reached the screen already has stale
      // geometry. The resize settle window (lattice) repaints behind it.
      let (now_w, now_h) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
      if (now_w as i64, now_h as i64) != (width as i64, height as i64) {
        log::warn!("[alloy] surface size changed during frame: drew {width}x{height}, now {now_w}x{now_h}");
      }
      // Frame timestamps: the latest queued frame the GPU has finished (this
      // one rarely, usually the one before) becomes the GPU term this
      // notification carries, and the cumulative frame exec the stats read.
      if let Some(micros) = self.frame_timestamps.harvest() {
        self.stats.frame_exec_micros.fetch_add(micros, Ordering::Relaxed);
        self.last_frame_gpu_micros = Some(micros);
      }
      self.publish_target_counters();
      self
        .tx
        .send(FrameOutput::Presented {
          at: presented_at,
          ready_at: present_start,
          gpu_micros: self.last_frame_gpu_micros,
          demanded,
        })
        .map_err(|_| FrameExit::MainGone)?;
    }
    // Wake only after the frame is in the channel, so the woken loop finds it.
    if let Some(wake) = &self.wake {
      wake();
    }
    Ok(())
  }

  /// Whether a next frame was already demanded as this present left the
  /// swap, for the present notification: the main loop's missed-present
  /// accounting (see `present::RefreshCounting::count`) judges only the
  /// intervals a demanded present opens, since the demand gate makes
  /// presents legitimately stop when nothing changes - a gap with no demand
  /// is idle, not jank. A present that swapped nothing (minimized zero-size
  /// window, failed draw) paces nothing and opens no demanded interval.
  fn demand_at_present(&self, drawn: bool) -> bool {
    // Sampled, never consumed - the UI thread's draw gate owns take().
    drawn && self.demand_latch.as_ref().is_some_and(|latch| latch.load(Ordering::Relaxed))
  }

  /// Block until outstanding presents are back under PRESENT_FENCE_DEPTH (or
  /// the timeout passes on the oldest fence), consuming the signaled fences.
  /// See the `present_fences` field. A timeout is the "GPU is over budget for
  /// a full refresh period and then some" signal - pacing is lost for this
  /// frame (we draw anyway; hanging the raster thread would be worse).
  /// Counted for get_stats (fenceTimeouts) and logged at debug 1/s: a healthy
  /// discrete GPU never hits this while a saturated tiled one (Android TV)
  /// lives near it in steady state, so the counter is the observability and
  /// the log line is SOLIDRT_LOG=debug diagnosis material (see
  /// okf/backlog/idle-tick-gpu-backlog-runaway.md, present-fence finding).
  ///
  /// The timed-out fence stays at the head of the queue, so its age is the
  /// watchdog's clock: a frame whose GPU work has not completed within
  /// PRESENT_FENCE_LOSS_SECS is a lost context, on any driver (the GPU hung,
  /// or the kernel reset it and this driver reports no reset status; every
  /// kernel's own hang timeout is shorter). Err is that loss. A wait that
  /// fails outright is a GL error, not a hang; its fence is dropped as
  /// before.
  fn await_present_fence(&mut self) -> Result<(), FrameExit> {
    while self.present_fences.len() >= PRESENT_FENCE_DEPTH {
      let (frame, fence, submitted) = self.present_fences.pop_front().expect("len checked above");
      let status = unsafe {
        glow::HasContext::client_wait_sync(&self.gl, fence, glow::SYNC_FLUSH_COMMANDS_BIT, PRESENT_FENCE_TIMEOUT_NS)
      };
      match status {
        glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => {
          unsafe { glow::HasContext::delete_sync(&self.gl, fence) };
          self.publish_retired(frame);
        }
        glow::TIMEOUT_EXPIRED => {
          self.stats.fence_timeouts.fetch_add(1, Ordering::Relaxed);
          let age = submitted.elapsed();
          if age.as_secs() >= PRESENT_FENCE_LOSS_SECS {
            unsafe { glow::HasContext::delete_sync(&self.gl, fence) };
            let observed = format!("frame {frame}'s GPU work has not completed in {} s", age.as_secs());
            return Err(FrameExit::ContextLost {
              detail: self.loss_detail(&observed, ResetStatus::None),
              status: ResetStatus::None,
            });
          }
          if self.fence_wait_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
            self.fence_wait_log = Some(std::time::Instant::now());
            log::debug!(
              "[alloy] present fence timed out after {}ms: GPU over budget, pacing lost this frame",
              PRESENT_FENCE_TIMEOUT_NS / 1_000_000
            );
          }
          // Back at the head: the frames behind it complete in order, so
          // nothing is awaited before it, and its age keeps counting.
          self.present_fences.push_front((frame, fence, submitted));
          return Ok(());
        }
        status => {
          unsafe { glow::HasContext::delete_sync(&self.gl, fence) };
          if self.fence_wait_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
            self.fence_wait_log = Some(std::time::Instant::now());
            log::warn!("[alloy] present fence wait failed (status {status:#x})");
          }
          return Ok(());
        }
      }
    }
    Ok(())
  }

  /// Publish that UI frame `frame` has completed on the GPU, and with it
  /// every frame before it (the GL stream completes in order): the count of
  /// retired frames becomes at least `frame + 1`. Never moves backwards; a
  /// fence that signals out of number order (a frame resubmitted under an
  /// older number) cannot lower it.
  fn publish_retired(&self, frame: u64) {
    self.stats.retired_frames.fetch_max(frame + 1, Ordering::Release);
  }

  /// Clear the window backbuffer and swap it once, before any frame exists.
  /// Purely so the window becomes visible: on Wayland a surface is not mapped
  /// until its first buffer commit, so an app whose first render blocks (a
  /// synchronous device probe, say) puts nothing on screen at all - no title
  /// bar, nothing for the compositor to show, no way to close it but the pid.
  /// A black window that never fills in is a diagnosable failure; no window is
  /// not.
  ///
  /// Deliberately not a frame: no FrameOutput::Presented, no wake, no present
  /// fence. The main loop's bookkeeping (frame counter, FrameRendered to JS,
  /// vsync arming, pacing samples) must only ever see presents the UI thread
  /// actually built.
  pub(crate) fn prime_window(&self) {
    // A headless surface is never swapped.
    if self.sink.headless() {
      return;
    }
    unsafe {
      glow::HasContext::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, None);
      glow::HasContext::disable(&self.gl, glow::SCISSOR_TEST);
      glow::HasContext::clear_color(&self.gl, 0.0, 0.0, 0.0, 1.0);
      glow::HasContext::clear(&self.gl, glow::COLOR_BUFFER_BIT);
    }
    // Debug, not warn: the first real frame's present judges the surface for
    // real (failure counter, rebind-and-redraw recovery). This one is a
    // courtesy, and a platform that refuses it loses only the empty window.
    if !self.binding.swap() {
      log::debug!("[alloy] priming swap failed: {}", self.binding.error());
    }
  }

  /// Swap the window's backbuffer. Without the failure check a lost context
  /// / removed device leaves the app running normally while nothing reaches
  /// the screen (a frozen window with no message). A failed swap gets one
  /// rebind-and-redraw recovery attempt (see `frame`), and a swapchain that
  /// keeps failing for a reason that is not a loss exits; a lost context -
  /// the swap's EGL error names it, or the reset status does, which is the
  /// only signal when a context-level loss keeps swapping (ANGLE, measured)
  /// - is reported for recovery: see okf/done/gpu-context-loss.md.
  fn present(&mut self, frame: u64) -> Presented {
    // A video plane beneath the window is composited with it in one atomic
    // commit, and that commit waits for every buffer in it to be ready. A
    // window buffer queued with GPU work still outstanding therefore holds
    // the video frame that shares its commit, and the commit behind it -
    // measured on a Samsung SM-T500 (Android 12), where an overlay
    // repainting five times a second cost 40% of the video's frame intervals
    // while the same overlay repainting every frame cost 7%. Finishing here
    // spends that wait on this thread instead: the window frame lands a
    // commit later and the video keeps its slot, which is the trade worth
    // making while a video is on screen. Only while a plane exists; a paused
    // plane pays it for nothing, which is a refinement, not a defect
    // (okf/notes/android-video-plane-compositing.md).
    if crate::video_plane_active() {
      self.finish_gpu_work(frame);
    }
    // The id this swap's buffer is queued under, asked before the swap.
    self.frame_timestamps.before_present();
    if self.binding.swap() {
      self.present_failures = 0;
      self.frame_timestamps.presented();
      // At most one fence joins per frame (a retried present only follows a
      // failed one, which queued nothing), and `await_present_fence` trimmed
      // to depth-1 before the draw, so the queue never exceeds
      // PRESENT_FENCE_DEPTH. A failed fence_sync just means no pacing this
      // frame: same behavior as before this mechanism existed.
      if let Ok(fence) = unsafe { glow::HasContext::fence_sync(&self.gl, glow::SYNC_GPU_COMMANDS_COMPLETE, 0) } {
        self.present_fences.push_back((frame, fence, std::time::Instant::now()));
        // Flush the fence into the command stream now. ANGLE/D3D11 defers a
        // post-swap fence's submission (~2 swaps) and its glClientWaitSync
        // never blocks (the flush bit does not rescue it), so without this
        // the wait reads TIMEOUT_EXPIRED on every frame of a healthy GPU and
        // fenceTimeouts counts frames instead of stalls. Free elsewhere: the
        // swap already flushed everything but the fence itself. Measured in
        // alloy/examples/present_fence_probe.rs (phases A vs D).
        unsafe { glow::HasContext::flush(&self.gl) };
      }
      // A context-level loss keeps swapping: the reset status is its only
      // signal, asked once per presented frame (a flag read).
      let status = self.robustness.reset_status();
      if status.is_reset() {
        return Presented::Lost { detail: self.loss_detail("the swap succeeded", status), status };
      }
      return Presented::Yes;
    }
    // The surface went away under this swap (the window was backgrounded
    // while the frame was drawing): expected, not evidence of a loss.
    if crate::window_backgrounded() {
      log::debug!("[alloy] present skipped: window backgrounded ({})", self.binding.error());
      return Presented::No;
    }
    let error = self.binding.error();
    let status = self.robustness.reset_status();
    if status.is_reset() || is_context_lost_error(&error) {
      return Presented::Lost { detail: self.loss_detail(&format!("present failed: {error}"), status), status };
    }
    self.present_failures += 1;
    if self.present_failures == 1 {
      log::error!("[alloy] present failed: {error}");
    }
    if self.present_failures >= PRESENT_FAILURE_EXIT_THRESHOLD {
      log::error!("[alloy] GPU presents keep failing ({} in a row: {error}), exiting", self.present_failures);
      std::process::exit(1);
    }
    Presented::No
  }

  /// The loss as the log reports it: what was observed, the reset status,
  /// and on Windows why the device went away.
  fn loss_detail(&self, observed: &str, status: ResetStatus) -> String {
    let mut detail = format!("{observed}; {}", status.name());
    if let Some(reason) = self.device.removed_reason() {
      detail.push_str("; ");
      detail.push_str(&reason);
    }
    detail
  }

  /// Block until the work drawn into this frame has completed on the GPU, so
  /// the buffer the swap queues carries none of it into the compositor's
  /// commit. Bounded by the same timeout as the pacing wait: a GPU over
  /// budget loses the protection for this frame rather than stalling the
  /// raster thread, and counts as a fence timeout like any other. On
  /// completion UI frame `frame`, whose work this is, is published retired.
  fn finish_gpu_work(&mut self, frame: u64) {
    let fence = match unsafe { glow::HasContext::fence_sync(&self.gl, glow::SYNC_GPU_COMMANDS_COMPLETE, 0) } {
      Ok(fence) => fence,
      Err(_) => return,
    };
    let status = unsafe {
      let status =
        glow::HasContext::client_wait_sync(&self.gl, fence, glow::SYNC_FLUSH_COMMANDS_BIT, PRESENT_FENCE_TIMEOUT_NS);
      glow::HasContext::delete_sync(&self.gl, fence);
      status
    };
    match status {
      glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => self.publish_retired(frame),
      glow::TIMEOUT_EXPIRED => {
        self.stats.fence_timeouts.fetch_add(1, Ordering::Relaxed);
      }
      _ => {}
    }
  }

  /// Rebind the context to the window's current EGL surface (see the
  /// RasterCmd doc); true on success. Must run on this thread: the context is
  /// current here and SDL_GL_MakeCurrent operates on the calling thread's
  /// binding. The swap interval is per-surface EGL state, so re-assert vsync.
  /// The failure counter is deliberately NOT touched here: the recovery path
  /// in `frame` judges the retry present on its own, and only the
  /// event-driven command resets stale evidence.
  pub(super) fn rebind_window_surface(&mut self) -> bool {
    if !self.binding.bind() {
      log::warn!("[alloy] rebind window surface failed: {}", self.binding.error());
      return false;
    }
    // FBO 0 is a different surface now; its sample count (the fast-path
    // decision) must be asked again, not trusted from before the bind, and
    // the frame timestamps must be enabled on it afresh.
    gl::forget_window_samples();
    self.frame_timestamps.forget();
    if !self.sink.headless() && !self.binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", self.binding.error());
    }
    true
  }
}

impl RasterState {
  /// For every camera stream texture with a tap, upload the slot's newest
  /// frame when it is newer than the one the texture holds. A frame of
  /// another size is the UI side's to act on (it recreates the texture at
  /// the new size and re-attaches the tap), so it is left for that.
  fn latch_cameras(&mut self) {
    if self.camera_taps.is_empty() {
      return;
    }
    let mut due = Vec::new();
    for (id, entry) in self.camera_taps.iter() {
      if let Some(frame) = entry.tap.latest() {
        if frame.seq > entry.uploaded_seq {
          due.push((*id, frame));
        }
      }
    }
    for (id, frame) in due {
      let fits = self.textures.get(&id).is_some_and(|gpu| gpu.width == frame.width && gpu.height == frame.height);
      if !fits {
        continue;
      }
      if let Err(e) = self.update_texture(id, &frame.pixels) {
        log::warn!("[alloy] camera frame upload failed: {e}");
        continue;
      }
      if let Some(entry) = self.camera_taps.get_mut(&id) {
        self.stats.camera_uploaded.fetch_add(1, Ordering::Relaxed);
        self.stats.camera_skipped.fetch_add(frame.seq - entry.uploaded_seq - 1, Ordering::Relaxed);
        entry.uploaded_seq = frame.seq;
      }
      self.content_dirty = true;
    }
  }

  /// For every YUV output with a latch, take the frame due for the present
  /// at `present_at` (the newest due at or before the deadline plus the
  /// lookahead, see yuv.rs), upload it into the back plane set, flip the set
  /// and rebind the conversion target to it, so the dirty flush re-renders
  /// the output and everything sampling it. Under a stepped clock the take
  /// waits for the frame that is due, never showing the one that happened
  /// to arrive.
  fn latch_video(&mut self, present_at: std::time::Instant) {
    if self.yuv_latches.is_empty() {
      return;
    }
    let deadline_ns = crate::clock::ns(present_at);
    let lookahead_ns = crate::yuv::lookahead_ns();
    let mut taken = Vec::new();
    for (id, entry) in self.yuv_latches.iter_mut() {
      if let Some(frame) = entry.latch.take(deadline_ns, lookahead_ns, self.sink.headless()) {
        let back = 1 - entry.front;
        entry.front = back;
        taken.push((*id, back, frame));
      }
    }
    for (id, set, frame) in taken {
      self.stats.video_latched.fetch_add(1, Ordering::Relaxed);
      self.stats.video_skipped.fetch_add(frame.skipped as u64, Ordering::Relaxed);
      if frame.late {
        self.stats.video_late.fetch_add(1, Ordering::Relaxed);
      }
      let Some(entry) = self.yuv_latches.get(&id) else { continue };
      let data = frame.frame.data;
      if data.len() < entry.frame_size {
        log::warn!("[alloy] yuv frame for {id} is {} bytes, needs {}", data.len(), entry.frame_size);
        continue;
      }
      let planes = entry.sets[set].clone();
      for &(_, plane, offset) in &planes {
        let len = match self.textures.get(&plane) {
          Some(gpu) => gpu.format.byte_len(gpu.width, gpu.height),
          None => {
            log::warn!("[alloy] yuv plane {plane} not found");
            continue;
          }
        };
        match data.get(offset..offset.saturating_add(len)) {
          Some(bytes) => {
            if let Err(e) = self.update_texture(plane, bytes) {
              log::warn!("[alloy] yuv plane update failed: {e}");
            }
          }
          None => {
            log::warn!("[alloy] yuv plane {plane} needs {len} bytes at offset {offset}, frame has {}", data.len())
          }
        }
      }
      let bindings: Vec<crate::gpu::TextureBinding> =
        planes.iter().map(|&(name, plane, _)| crate::gpu::TextureBinding::new(name, plane)).collect();
      self.entry_write(id, "yuv latch rebind", |_, shader| shader.set_sampler_bindings(&bindings));
      self.content_dirty = true;
    }
  }
}
