//! The frame path: draw the display list to the window and hand it on -
//! present in interactive mode, read back in playback mode - plus the
//! present-side pacing policy that surrounds it: the present fence depth
//! gate, missed-present (jank) accounting, the swap itself with its
//! context-loss exit, and the window surface rebind.

use std::sync::atomic::Ordering;

use impellers::{DisplayList, ISize};

use super::repaint::WindowRoute;
use super::{
  RasterState, DamageRect, PresentDamage, PresentRun, FALLBACK_REFRESH_HZ, JANK_JITTER_SLACK,
  PRESENT_FAILURE_EXIT_THRESHOLD, PRESENT_FENCE_DEPTH, PRESENT_FENCE_TIMEOUT_NS,
};
use crate::backend::FrameOutput;
use crate::gl;
use crate::gl::Timed;

impl RasterState {
  /// Draw the frame's display list to the window backbuffer and hand it on:
  /// present in interactive mode, read the pixels back in playback mode. Then
  /// notify the main loop, which only does frame bookkeeping (fps,
  /// FrameRendered) and playback encoding. Err means the main loop is gone
  /// and this thread should exit.
  pub(super) fn frame(&mut self, dl: DisplayList) -> Result<(), ()> {
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
    // Playback captures every pixel and the window shader redraws its whole
    // layer; neither frame kind may be pruned to a patch.
    let fast_path = gl::window_fast_path(&self.gl);
    let patch_barred = self.capture_frames || self.window_shader.is_some();
    let mut route = self.damage.route(own_damage, size, fast_path, patch_barred);
    let wait_start = std::time::Instant::now();
    self.await_present_fence();
    let wait_ms = wait_start.elapsed().as_secs_f32() * 1000.0;
    let draw_start = std::time::Instant::now();
    self.pass_timer.begin(&self.gl);
    let drawn = self.draw_to_window(&dl, size, route);
    self.pass_timer.end(&self.gl, Timed::Frame);
    let draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;
    // The overlay composites over the finished frame (shaded or not),
    // before the capture readback so playback frames carry it too. Excluded
    // from draw_ms: it is diagnostics, not the app's frame cost.
    if drawn {
      self.draw_overlay(size);
    }
    if self.capture_frames {
      let pixels = if drawn { gl::read_fbo0_pixels(&self.gl, size) } else { Vec::new() };
      self.tx.send(FrameOutput::Captured(pixels)).map_err(|_| ())?;
    } else {
      let present_start = std::time::Instant::now();
      let mut presented = false;
      if drawn {
        if self.present() {
          presented = true;
        } else if self.rebind_window_surface() {
          // The failed swap's frame is lost with the dead binding (Android
          // replaces the EGL surface across background/resume, and a frame
          // latched by resize or expose can reach this thread before the
          // event-driven rebind). Redraw against the rebound surface and
          // present again; the retry's outcome feeds the failure threshold
          // honestly (fail, rebind, fail again = confirmed loss). The fresh
          // surface preserves nothing, so the retry draws in full.
          self.damage.invalidated();
          own_damage = PresentDamage::Full;
          route = WindowRoute::whole(fast_path);
          if self.draw_to_window(&dl, size, route) {
            presented = self.present();
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
      let present_ms = present_start.elapsed().as_secs_f32() * 1000.0;
      self.timing.record(wait_ms, draw_ms, present_ms);
      self.record_present_interval(drawn);
      // A frame's native cost beyond ~2 vsync periods means this thread is
      // being stalled in the driver; log which step, rate-limited to one line
      // per second so a sustained stall stays readable. Debug, not warn: a
      // saturated tiled GPU (Android TV) lives here in steady state, and the
      // timing stats carry the numbers - raise SRT_LOG=debug to see these.
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
      self.tx.send(FrameOutput::Presented).map_err(|_| ())?;
    }
    // Wake only after the frame is in the channel, so the woken loop finds it.
    if let Some(wake) = &self.wake {
      wake();
    }
    Ok(())
  }

  /// Missed-present (jank) accounting, run as each interactive present
  /// returns from the swap. A miss is a refresh the screen repeated the old
  /// frame through while a next frame was demanded; the demand gate makes
  /// presents legitimately stop when nothing changes, so only gaps with the
  /// frame-request latch set at present time can count - a gap with no
  /// demand is idle, not jank. Counting compares whole periods elapsed
  /// against presents delivered over each contiguous demanded run (span
  /// first, then divide), because individual swap-return intervals jitter by
  /// over half a period on healthy pacing (see JANK_JITTER_SLACK); `fps` and
  /// the per-second averages cannot see a single repeat, this can.
  fn record_present_interval(&mut self, drawn: bool) {
    // No swap happened (minimized zero-size window, failed draw): presents
    // are not pacing anything, so accounting restarts when they resume.
    if !drawn {
      self.present_run = None;
      return;
    }
    let now = std::time::Instant::now();
    let hz = crate::refresh_rate().unwrap_or(FALLBACK_REFRESH_HZ).max(1.0);
    if self.present_run.as_ref().is_some_and(|run| run.hz != hz) {
      self.present_run = None;
    }
    if let Some(run) = self.present_run.as_mut() {
      run.intervals += 1;
      let span_periods = now.duration_since(run.start).as_secs_f64() * hz as f64;
      let expected = (span_periods - JANK_JITTER_SLACK).round().max(0.0) as u64;
      let new = expected.saturating_sub(run.intervals).saturating_sub(run.reported);
      if new > 0 {
        run.reported += new;
        self.stats.missed_presents.fetch_add(new, Ordering::Relaxed);
      }
    }
    // Sampled, never consumed - the UI thread's draw gate owns take().
    let demanded = self.demand_latch.as_ref().is_some_and(|latch| latch.load(Ordering::Relaxed));
    if !demanded {
      self.present_run = None;
    } else if self.present_run.is_none() {
      self.present_run = Some(PresentRun { start: now, hz, intervals: 0, reported: 0 });
    }
  }

  /// Block until outstanding presents are back under PRESENT_FENCE_DEPTH (or
  /// the timeout passes per fence), consuming the awaited fences. See the
  /// `present_fences` field. A timeout is the "GPU is over budget for a full
  /// refresh period and then some" signal - pacing is lost for this frame
  /// (we draw anyway; hanging the raster thread would be worse). Counted for
  /// get_stats (fenceTimeouts) and logged at debug 1/s: a healthy discrete
  /// GPU never hits this while a saturated tiled one (Android TV) lives near
  /// it in steady state, so the counter is the observability and the log
  /// line is SRT_LOG=debug diagnosis material (see
  /// okf/backlog/idle-tick-gpu-backlog-runaway.md, present-fence finding).
  fn await_present_fence(&mut self) {
    while self.present_fences.len() >= PRESENT_FENCE_DEPTH {
      let fence = self.present_fences.pop_front().expect("len checked above");
      let status = unsafe {
        let status =
          glow::HasContext::client_wait_sync(&self.gl, fence, glow::SYNC_FLUSH_COMMANDS_BIT, PRESENT_FENCE_TIMEOUT_NS);
        glow::HasContext::delete_sync(&self.gl, fence);
        status
      };
      match status {
        glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => {}
        status => {
          if status == glow::TIMEOUT_EXPIRED {
            self.stats.fence_timeouts.fetch_add(1, Ordering::Relaxed);
          }
          if self.fence_wait_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
            self.fence_wait_log = Some(std::time::Instant::now());
            if status == glow::TIMEOUT_EXPIRED {
              log::debug!(
                "[alloy] present fence timed out after {}ms: GPU over budget, pacing lost this frame",
                PRESENT_FENCE_TIMEOUT_NS / 1_000_000
              );
            } else {
              log::warn!("[alloy] present fence wait failed (status {status:#x})");
            }
          }
        }
      }
    }
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
    // Playback keeps the window hidden and never swaps.
    if self.capture_frames {
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

  /// Swap the window's backbuffer; true on success. Without the failure
  /// check a lost context / removed device leaves the app running normally
  /// while nothing reaches the screen (a frozen window with no message). A
  /// failed swap gets one rebind-and-redraw recovery attempt (see `frame`);
  /// a confirmed loss exits instead: see okf/backlog/gpu-context-loss.md.
  fn present(&mut self) -> bool {
    if self.binding.swap() {
      self.present_failures = 0;
      // At most one fence joins per frame (a retried present only follows a
      // failed one, which queued nothing), and `await_present_fence` trimmed
      // to depth-1 before the draw, so the queue never exceeds
      // PRESENT_FENCE_DEPTH. A failed fence_sync just means no pacing this
      // frame: same behavior as before this mechanism existed.
      if let Ok(fence) = unsafe { glow::HasContext::fence_sync(&self.gl, glow::SYNC_GPU_COMMANDS_COMPLETE, 0) } {
        self.present_fences.push_back(fence);
        // Flush the fence into the command stream now. ANGLE/D3D11 defers a
        // post-swap fence's submission (~2 swaps) and its glClientWaitSync
        // never blocks (the flush bit does not rescue it), so without this
        // the wait reads TIMEOUT_EXPIRED on every frame of a healthy GPU and
        // fenceTimeouts counts frames instead of stalls. Free elsewhere: the
        // swap already flushed everything but the fence itself. Measured in
        // alloy/examples/present_fence_probe.rs (phases A vs D).
        unsafe { glow::HasContext::flush(&self.gl) };
      }
      return true;
    }
    self.present_failures += 1;
    if self.present_failures == 1 {
      log::error!("[alloy] present failed: {}", self.binding.error());
    }
    if self.present_failures >= PRESENT_FAILURE_EXIT_THRESHOLD {
      log::error!("[alloy] GPU context lost ({} consecutive failed presents), exiting", self.present_failures);
      std::process::exit(1);
    }
    false
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
    if !self.capture_frames && !self.binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", self.binding.error());
    }
    true
  }
}
