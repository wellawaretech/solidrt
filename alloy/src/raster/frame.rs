//! The frame path: draw the display list to the window and hand it on -
//! present in interactive mode, read back in playback mode - plus the
//! present-side pacing policy that surrounds it: the present fence depth
//! gate, missed-present (jank) accounting, the swap itself with its
//! context-loss exit, and the window surface rebind.

use std::sync::atomic::Ordering;

use impellers::{DisplayList, ISize};

use super::repaint::WindowRoute;
use super::{DamageRect, PresentDamage, RasterState, PRESENT_FAILURE_EXIT_THRESHOLD, PRESENT_FENCE_DEPTH, PRESENT_FENCE_TIMEOUT_NS};
use crate::backend::FrameOutput;
use crate::gl;
use crate::gl::Timed;

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

  /// Draw the frame's display list to the window backbuffer and hand it on:
  /// present in interactive mode, read the pixels back in playback mode. Then
  /// notify the main loop, which only does frame bookkeeping (fps,
  /// FrameRendered) and playback encoding. Err means the main loop is gone
  /// and this thread should exit.
  pub(super) fn frame(&mut self, dl: DisplayList, present_at: std::time::Instant) -> Result<(), ()> {
    // The frame's GPU span starts here, ahead of the pass flush: on a tiler
    // the passes execute in the same submission as the window draw.
    self.frame_timestamps.frame_begin();
    // The video frames due for this present go into their textures first,
    // so the flush below converts them and the frame samples the result.
    self.latch_video(present_at);
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
    let drawn = !backgrounded && self.draw_to_window(&dl, size, route);
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
        } else if !crate::window_backgrounded() && self.rebind_window_surface() {
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
      let presented_at = std::time::Instant::now();
      let present_ms = presented_at.duration_since(present_start).as_secs_f32() * 1000.0;
      self.timing.record(wait_ms, draw_ms, present_ms);
      let demanded = self.demand_at_present(drawn);
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
        .map_err(|_| ())?;
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
    // (okf/plans/android-video-punch-through.md).
    if crate::video_plane_active() {
      self.finish_gpu_work();
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
    // The surface went away under this swap (the window was backgrounded
    // while the frame was drawing): expected, not evidence of a loss.
    if crate::window_backgrounded() {
      log::debug!("[alloy] present skipped: window backgrounded ({})", self.binding.error());
      return false;
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

  /// Block until the work drawn into this frame has completed on the GPU, so
  /// the buffer the swap queues carries none of it into the compositor's
  /// commit. Bounded by the same timeout as the pacing wait: a GPU over
  /// budget loses the protection for this frame rather than stalling the
  /// raster thread, and counts as a fence timeout like any other.
  fn finish_gpu_work(&mut self) {
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
    if status == glow::TIMEOUT_EXPIRED {
      self.stats.fence_timeouts.fetch_add(1, Ordering::Relaxed);
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
    if !self.capture_frames && !self.binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", self.binding.error());
    }
    true
  }
}

impl RasterState {
  /// For every YUV output with a latch, take the frame due for the present
  /// at `present_at` (the newest due at or before the deadline plus the
  /// lookahead, see yuv.rs), upload it into the back plane set, flip the set
  /// and rebind the conversion target to it, so the dirty flush re-renders
  /// the output and everything sampling it. In playback the take waits for
  /// the frame that is due, never showing the one that happened to arrive.
  fn latch_video(&mut self, present_at: std::time::Instant) {
    if self.yuv_latches.is_empty() {
      return;
    }
    let deadline_ns = crate::clock::ns(present_at);
    let lookahead_ns = crate::yuv::lookahead_ns();
    let mut taken = Vec::new();
    for (id, entry) in self.yuv_latches.iter_mut() {
      if let Some(frame) = entry.latch.take(deadline_ns, lookahead_ns, self.capture_frames) {
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
