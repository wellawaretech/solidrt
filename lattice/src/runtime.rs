use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use alloy::rendertree::PlatformContext;
use alloy::resample::SharedResampler;
use alloy::{AlloyEvent, InputState};
use flux::gui::input::InputEvent;
use flux::{emit_event, ExecHandle};

use crate::paced_clock::PacedClock;

/// Dev-tool clock control, shared between the dev-server connection (any
/// thread) and the frame verb: a time scale (0 pauses frame delivery to JS)
/// and a pending single-step count consumed one per frame signal while
/// paused. Cheap to clone: both fields are `Arc`s. Defaults to scale 1 with
/// no steps, which leaves the frame path byte-for-byte on its normal route -
/// builds without a dev connection never see another value.
#[derive(Clone)]
pub struct ClockControl {
  // f64 bits.
  scale: Arc<AtomicU64>,
  steps: Arc<AtomicU64>,
}

// The writer half is only reachable from the dev connection (go builds); the
// runtime builds still construct and read the control.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
impl ClockControl {
  pub fn new() -> Self {
    Self { scale: Arc::new(AtomicU64::new(1.0f64.to_bits())), steps: Arc::new(AtomicU64::new(0)) }
  }

  pub fn scale(&self) -> f64 {
    f64::from_bits(self.scale.load(Ordering::Relaxed))
  }

  /// Set the time scale; negative input is clamped to 0 (paused).
  pub fn set_scale(&self, scale: f64) {
    self.scale.store(scale.max(0.0).to_bits(), Ordering::Relaxed);
  }

  /// Queue `n` single-step frames, delivered while paused.
  pub fn add_steps(&self, n: u64) {
    self.steps.fetch_add(n, Ordering::Relaxed);
  }

  pub fn pending_steps(&self) -> u64 {
    self.steps.load(Ordering::Relaxed)
  }

  /// Consume one pending step; false when none are queued.
  fn take_step(&self) -> bool {
    self.steps.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1)).is_ok()
  }

  /// Back to normal time: scale 1, pending steps dropped. Applied on
  /// reload/stop so no app starts under a stale pause.
  pub fn reset(&self) {
    self.set_scale(1.0);
    self.steps.store(0, Ordering::Relaxed);
  }
}

/// The engine seam: the verbs the runner's event loop drives a UI runtime
/// with. Runner-owned bookkeeping (pointer positions, window facts, fps) has
/// already been applied when a verb is called; implementations only marshal
/// into their engine. Engine lifecycle (build, eval, reload) is deliberately
/// not part of the contract yet: its shape depends on how the engine's async
/// loop and a verb-driven loop reconcile (see REDESIGN.md), and freezing a
/// guess here would break the contract later.
pub trait UiRuntime {
  /// An alloy event arrived. Frame signals (FrameRendered / Tick) never come
  /// through here; they become `frame` calls.
  fn event(&mut self, event: &AlloyEvent);
  /// A frame signal (present or idle tick): run the engine's per-frame work
  /// computing frame `next_frame`.
  fn frame(&mut self, next_frame: u64);
}

/// Drives a flux (QuickJS) engine: events are marshalled onto the JS thread
/// via the engine's ExecHandle. The handle cell is shared with the reload
/// loop, which swaps in each new engine's handle; while no engine is live
/// (startup, mid-reload) both verbs drop their input - it was aimed at an
/// engine that does not exist.
pub struct FluxRuntime {
  exec: Rc<RefCell<Option<ExecHandle>>>,
  // Virtual present counter the playback-mode clock derives time from,
  // published by frame(). Unused in run mode.
  playback_frame: Arc<AtomicU64>,
  // Run-mode pacing for the animation timestamps (see paced_clock). None in
  // playback mode, which uses the deterministic frame/fps clock.
  paced: Option<PacedClock>,
  // Dev-tool pause/step/scale state; permanently scale 1 outside a dev
  // session (and in playback mode, which has no dev connection).
  clock_control: ClockControl,
  // Wall origin for the paced clock's correction target. tokio's Instant so
  // tokio's test clock can drive it.
  wall_start: tokio::time::Instant,
  platform: Arc<PlatformContext>,
  // Sampling handle onto the resampler its producers feed (the alloy pump
  // for real input, the dev connection for synthetic input; see alloy's
  // resample.rs for the slot model and the producer-side rule): moves never
  // arrive as events. frame() drains one resampled move per pointer per
  // frame signal and follows the batch with the "pointerFrame" terminator,
  // so every move a frame delivers is the same age. Idle Ticks keep frame
  // signals coming at refresh cadence, so a buffered move is never more
  // than one period from dispatch even when nothing is painting - hover
  // needs no arrival path anymore. Frame pacing also bounds a device
  // delivering moves faster than the engine drains them (a 1000Hz gaming
  // mouse) to one hit test and JS dispatch per pointer per frame, instead
  // of letting its backlog starve frame signals and replay stale positions.
  // Down/up/wheel dispatch on arrival: they are rare, ordering-sensitive,
  // and (for wheel) carry deltas that must not be dropped. A buffered move
  // consequently dispatches after a later-arriving wheel; wheel deliveries
  // are self-contained (own hit test, own coordinates), so nothing observes
  // the order.
  resampler: SharedResampler,
  // Engine the buffered histories were collected for. The JS listeners (and
  // the node ids a dispatch would route to) die with their engine on reload,
  // so a handle change orphans every history; detect it and clear (see
  // frame()).
  gate_engine: Option<ExecHandle>,
  // Device-fact bookkeeping (the per-frame hover refresh reads last pointer
  // positions): moves surface only as frame()'s samples now, so their
  // positions are recorded there; downs/ups keep updating it from the batch
  // loop on arrival.
  input_state: Arc<InputState>,
  // JS-thread cost of the two per-frame closures (move dispatch, frame
  // work), logged 1/s while input flows. The max column separates a steady
  // dispatch cost from spikes (GC pauses): either can blow the one-period
  // pipeline deadline and slip a frame (see alloy vsync pacing).
  timing: Arc<Mutex<JsTiming>>,
}

#[derive(Default)]
struct JsTiming {
  since: Option<std::time::Instant>,
  moves: u32,
  move_ms: f32,
  move_max: f32,
  frames: u32,
  frame_ms: f32,
  frame_max: f32,
}

impl JsTiming {
  fn record_move(&mut self, ms: f32) {
    self.moves += 1;
    self.move_ms += ms;
    self.move_max = self.move_max.max(ms);
    self.maybe_log();
  }

  fn record_frame(&mut self, ms: f32) {
    self.frames += 1;
    self.frame_ms += ms;
    self.frame_max = self.frame_max.max(ms);
    self.maybe_log();
  }

  fn maybe_log(&mut self) {
    let since = self.since.get_or_insert_with(std::time::Instant::now);
    if since.elapsed().as_secs_f32() < 1.0 {
      return;
    }
    // Idle Ticks keep frame closures running at the refresh cadence, so only
    // input activity makes a second worth reporting.
    if self.moves > 0 {
      log::debug!(
        "[lattice] js: {} moves avg {:.1}ms max {:.1}ms, {} frames avg {:.1}ms max {:.1}ms",
        self.moves,
        self.move_ms / self.moves as f32,
        self.move_max,
        self.frames,
        self.frame_ms / self.frames.max(1) as f32,
        self.frame_max
      );
    }
    *self = JsTiming::default();
  }
}

impl FluxRuntime {
  pub fn new(
    exec: Rc<RefCell<Option<ExecHandle>>>,
    playback_frame: Arc<AtomicU64>,
    paced: Option<PacedClock>,
    clock_control: ClockControl,
    // Raw wall origin for the paced clock: the frame verb feeds ticks from
    // it, and the embedder samples it for the schedule-time timer reading
    // (see lib.rs), so both sides of the timer timeline share one origin.
    wall_start: tokio::time::Instant,
    platform: Arc<PlatformContext>,
    resampler: SharedResampler,
    input_state: Arc<InputState>,
  ) -> Self {
    Self {
      exec,
      playback_frame,
      paced,
      clock_control,
      wall_start,
      platform,
      resampler,
      gate_engine: None,
      input_state,
      timing: Arc::new(Mutex::new(JsTiming::default())),
    }
  }
}

impl UiRuntime for FluxRuntime {
  fn event(&mut self, event: &AlloyEvent) {
    // The pacing clock's refresh rate is this runtime's own bookkeeping (its
    // tick runs inside frame()), tracked whether or not an engine is live.
    if let AlloyEvent::DisplayRefreshRate { hz } = event {
      if let Some(pc) = &self.paced {
        pc.set_hz(*hz);
      }
    }
    let exec = self.exec.borrow();
    let Some(eh) = exec.as_ref() else {
      return;
    };
    // flux marshals the engine-agnostic window / keyboard / device events
    // (including the sticky window facts) directly; pointer events remain
    // because their dispatch is hit-testing, not pure marshalling.
    if flux::gui::events::forward(eh, event) {
      return;
    }
    match event {
      // Downs, ups and wheels dispatch on arrival (hit test against the last
      // computed layout, like Flutter): no frame is needed to deliver them.
      // Handlers that mutate state request the next frame through their ffi
      // calls. Moves never arrive here: their producers consume them into
      // the resampler at emission, and frame() dispatches one position per
      // pointer per frame signal (see `resampler`).
      AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
        // The producer re-seeded the resampler history at the contact (a
        // buffered pre-down move collapsed into the down); here the down
        // just dispatches on arrival as always.
        dispatch(
          eh,
          InputEvent::PointerDown {
            pointer_id: *pointer_id,
            pointer_type: *pointer_type,
            button: *button,
            x: *x,
            y: *y,
            modifiers: *modifiers,
          },
        )
      }
      AlloyEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
        // The producer dropped the pointer's history (a buffered move
        // dispatching after this up would be stale); the up carries the
        // final position.
        dispatch(
          eh,
          InputEvent::PointerUp {
            pointer_id: *pointer_id,
            pointer_type: *pointer_type,
            button: *button,
            x: *x,
            y: *y,
            modifiers: *modifiers,
          },
        )
      }
      AlloyEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => dispatch(
        eh,
        InputEvent::Wheel {
          pointer_id: *pointer_id,
          pointer_type: *pointer_type,
          x: *x,
          y: *y,
          delta_x: *delta_x,
          delta_y: *delta_y,
          modifiers: *modifiers,
        },
      ),
      _ => {}
    }
  }

  /// Run the per-frame JS work for one frame signal (FrameRendered or idle
  /// Tick): publish the frame index, advance the paced clock, pump cameras and
  /// speech, flush rAF callbacks, and emit the "render" event. `next_frame` is
  /// the present index the frame being computed would get.
  fn frame(&mut self, next_frame: u64) {
    let exec = self.exec.borrow();
    let Some(eh) = exec.as_ref() else {
      return;
    };
    // A replaced engine never saw the downs behind the buffered histories;
    // restart them so stale positions cannot dispatch into the new engine.
    if !self.gate_engine.as_ref().is_some_and(|g| g.same_engine(eh)) {
      self.resampler.clear();
      self.gate_engine = Some(eh.clone());
    }
    // Sampled at frame-signal time: the alloy loop feeds the vsync's input
    // into the resampler before emitting the signal, so this frame's touch
    // samples are already in the history.
    let moves = self.resampler.sample();
    // Pointer-fact bookkeeping from the sampled positions (raw moves never
    // leave their producers); the per-frame hover refresh reads these.
    for m in &moves {
      self.input_state.set_pointer_pos((m.pointer_type, m.pointer_id), m.x, m.y);
      self.input_state.set_modifiers(m.modifiers);
    }
    let playback_frame = self.playback_frame.clone();
    let paced = self.paced.clone();
    // The presentation model's period; None in playback mode, which has no
    // presentation model.
    let paced_period_ms = self.paced.as_ref().map(|p| p.period_ms());
    // Display period for video frame scheduling; 0 in playback just disables
    // the selection lookahead.
    #[cfg(feature = "video")]
    let period_us = paced_period_ms.map(|p| (p * 1000.0) as i64).unwrap_or(0);
    // The refresh period a frame's cost is judged against (frame history):
    // the presentation model's in run mode, the capture rate's in playback.
    let judge_period_ms = paced_period_ms.map(|p| p as f32).unwrap_or_else(|| 1000.0 / self.platform.fps().max(1) as f32);
    let clock_control = self.clock_control.clone();
    let wall_start = self.wall_start;
    let platform = self.platform.clone();
    let timing = self.timing.clone();
    eh.exec(move |ctx| {
      // Resampled moves run ahead of the frame work so the frame consumes
      // the state they dirty; timed as moves, not frame cost.
      let has_moves = !moves.is_empty();
      for m in moves {
        let start = std::time::Instant::now();
        flux::gui::input::dispatch(
          &ctx,
          InputEvent::PointerMove {
            pointer_id: m.pointer_id,
            pointer_type: m.pointer_type,
            x: m.x,
            y: m.y,
            dx: m.dx,
            dy: m.dy,
            modifiers: m.modifiers,
          },
        );
        timing.lock().expect("js timing lock poisoned").record_move(start.elapsed().as_secs_f32() * 1000.0);
      }
      if has_moves {
        // The batch terminator: all of this frame's moves have dispatched
        // and every pointer is the same age, so recognizers measure now. It
        // fires even if every move was interest-gated away (harmless), and
        // ahead of the deliver gate below so a paused clock still pairs
        // moves with their terminator.
        flux::gui::input::frame_end(&ctx);
      }
      let start = std::time::Instant::now();
      // Publish the present being computed before reading the clock, so in
      // playback mode the clock reports this frame's virtual time.
      playback_frame.store(next_frame, Ordering::Relaxed);
      // Dev-tool clock control: at scale 0 frame delivery to JS is gated (a
      // true pause: onFrame, rAF and the reactive flush all hang off the
      // render event), except that each queued step lets exactly one frame
      // through at one full period. Everything above and below this gate -
      // touch dispatch, cameras, capture settling, the draw path - keeps
      // running, so the compositor and the capture tools stay alive while
      // app time stands still.
      let scale = clock_control.scale();
      let deliver = scale != 0.0 || clock_control.take_step();
      // rAF and the render event march on the frame timeline (which
      // flux::Timeline also reports): the paced clock in run mode, the
      // frame-derived virtual clock in playback. The virtual timers march on
      // the paced clock's wall-anchored timer reading instead - same
      // pause/step/scale policy, but deadlines stay wall-accurate when slow
      // frames make the smoothed animation reading lag (see paced_clock). In
      // playback both are the deterministic frame clock. performance.now()
      // is on NEITHER - that stays real elapsed time. Idle Ticks arrive at
      // the refresh cadence, so ticking the paced clock for them preserves
      // its one-period-per-call model. Render event carries seconds; JS
      // scales to ms.
      let (ts, timer_ts) = match &paced {
        Some(pc) => {
          // The correction target is wall time. A gated frame ticks at scale
          // 0 (no advance, accrue the offset); a stepped frame advances one
          // exact period.
          let raw = wall_start.elapsed().as_secs_f64() * 1000.0;
          pc.tick(
            raw,
            if !deliver {
              0.0
            } else if scale == 0.0 {
              1.0
            } else {
              scale
            },
          );
          (pc.now_ms(), pc.timer_now_ms())
        }
        None => {
          let t = ctx.userdata::<flux::Timeline>().map(|t| t.now_ms()).unwrap_or(0.0);
          (t, t)
        }
      };
      // Stamp the render tree's animation clock with this frame's app-time
      // before any frame work runs: property writes during the flush start
      // their transition tracks at this time, and the draw path's advance
      // reads the same stamp, so a track's first frame paints its from-value
      // and pause/scale/step semantics ride in with ts.
      if let Some(tree) = ctx.userdata::<flux::gui::tree::SharedRenderTree>() {
        tree.0.borrow_mut().set_transition_now(ts);
      }
      if flux::gui::camera::tick(&ctx) {
        // A camera frame landed in its texture; the screen content changed
        // even though the tree did not.
        platform.request_frame();
      }
      #[cfg(feature = "video")]
      {
        let video = flux::gui::video::tick(&ctx, period_us);
        if video.uploaded || video.playing {
          // Same for a video frame uploaded into its player's texture - and a
          // mid-playback player is standing demand for the next tick, so video
          // rides the frame grid instead of free-running on its own uploads.
          platform.request_frame();
        }
      }
      // Settle any captureSnapshot promises whose captures alloy rendered on the
      // previous paint pass.
      flux::gui::gpu::tick(&ctx);
      #[cfg(feature = "speech")]
      crate::plugins::speech::tick(&ctx);
      if !deliver {
        // Paused: skip rAF and the render event so app time stops, but run
        // the draw path directly - its demand gate decides whether anything
        // needs painting (a queued snapshot capture, a camera frame, a
        // timer-driven tree change), and skips for free otherwise.
        crate::plugins::draw::render_now(&ctx);
        return;
      }
      if scale == 0.0 {
        // A stepped frame must present even when the app requests nothing,
        // so the step is visible to a following snapshot.
        platform.request_frame();
      }
      // Timers fire before the frame callbacks, one task-queue turn per
      // frame (see flux virtual time); the frame then consumes the state
      // they dirtied. They advance on the timer reading, not ts.
      flux::advance_virtual_time(&ctx, timer_ts);
      flux::gui::raf::flush(&ctx, ts);
      let time = ts / 1000.0;
      let obj = flux::rquickjs::Object::new(ctx.clone()).expect("create object");
      obj.set("frame", next_frame).expect("set frame");
      obj.set("time", time).expect("set time");
      // Stamp the frame for draw(): the start instant measures onFrame + flush
      // without any timing call crossing into JS (see frame::RenderFrame).
      crate::frame::RENDER_FRAME.with(|c| {
        c.set(crate::frame::RenderFrame { start: Some(std::time::Instant::now()), frame: next_frame, period_ms: judge_period_ms })
      });
      emit_event(&ctx, "render", obj);
      timing.lock().expect("js timing lock poisoned").record_frame(start.elapsed().as_secs_f32() * 1000.0);
    });
  }
}

// Queue a pointer event for hit-test dispatch on the JS thread (see
// flux::gui::input::dispatch).
fn dispatch(eh: &ExecHandle, event: InputEvent) {
  eh.exec(move |ctx| flux::gui::input::dispatch(&ctx, event));
}
