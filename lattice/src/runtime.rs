use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use alloy::rendertree::PlatformContext;
use alloy::{AlloyEvent, Modifiers, PointerType};
use flux::gui::input::InputEvent;
use flux::{emit_event, ExecHandle};

use crate::paced_clock::PacedClock;
use crate::resample::Resampler;

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
  platform: Arc<PlatformContext>,
  // At most one move dispatch in flight per pointer (mouse and pen; touch
  // goes through the resampler below and never dispatches on arrival). A map
  // entry means a dispatch closure for that pointer is queued on the engine;
  // arrivals overwrite the entry's position and the closure consumes the
  // entry when it runs, dispatching the freshest position. Without this
  // gate, a device delivering moves faster than the engine drains them (a
  // 1000Hz gaming mouse) grows the exec queue without bound: frame signals
  // and queries starve behind it, and stale positions keep replaying long
  // after the input stopped. Down/up/wheel stay ungated: they are rare,
  // ordering-sensitive, and (for wheel) carry deltas that must not be
  // dropped.
  pending_moves: Arc<Mutex<HashMap<(PointerType, u64), PendingMove>>>,
  // Touch moves are consumed by the frame clock instead of dispatching on
  // arrival: frame() drains one resampled move per pointer per frame signal
  // (see resample.rs for the slot model and why). Idle Ticks keep frame
  // signals coming at refresh cadence, so a buffered move is never more than
  // one period from dispatch even when nothing is painting. Mouse and pen
  // keep the arrival path: hover must dispatch without any frame in flight,
  // and desktop delivery has no batching to smooth.
  resampler: Resampler,
  // Engine the pending closures were queued on. Queued closures die with
  // their engine on reload, so a handle change orphans every map entry;
  // detect it and clear (see event()).
  gate_engine: Option<ExecHandle>,
  // JS-thread cost of the two per-frame closures (move dispatch, frame
  // work), logged 1/s while input flows. The max column separates a steady
  // dispatch cost from spikes (GC pauses): either can blow the one-period
  // pipeline deadline and slip a frame (see alloy vsync pacing).
  timing: Arc<Mutex<JsTiming>>,
}

struct PendingMove {
  x: f32,
  y: f32,
  modifiers: Modifiers,
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
    platform: Arc<PlatformContext>,
  ) -> Self {
    Self {
      exec,
      playback_frame,
      paced,
      platform,
      pending_moves: Arc::new(Mutex::new(HashMap::new())),
      resampler: Resampler::new(),
      gate_engine: None,
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
    // A replaced engine took its queued closures with it; every pending-move
    // entry is now an orphan that would gate its pointer forever. Touch
    // histories restart too: the new engine never saw the down.
    if !self.gate_engine.as_ref().is_some_and(|g| g.same_engine(eh)) {
      self.pending_moves.lock().expect("pending moves lock poisoned").clear();
      self.resampler.clear();
      self.gate_engine = Some(eh.clone());
    }
    // flux marshals the engine-agnostic window / keyboard / device events
    // (including the sticky window facts) directly; pointer events remain
    // because their dispatch is hit-testing, not pure marshalling.
    if flux::gui::events::forward(eh, event) {
      return;
    }
    match event {
      // Pointer events dispatch on arrival (hit test against the last
      // computed layout, like Flutter): no frame is needed to deliver them.
      // Handlers that mutate state request the next frame through their ffi
      // calls. Mouse/pen moves go through the per-pointer gate (see
      // `pending_moves`); touch moves feed the resampler and dispatch from
      // frame().
      AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
        let key = (*pointer_type, *pointer_id);
        if *pointer_type == PointerType::Touch {
          self.resampler.push(key, *x, *y, *modifiers);
          return;
        }
        let pending = PendingMove { x: *x, y: *y, modifiers: *modifiers };
        let already_queued =
          self.pending_moves.lock().expect("pending moves lock poisoned").insert(key, pending).is_some();
        if !already_queued {
          let moves = self.pending_moves.clone();
          let timing = self.timing.clone();
          eh.exec(move |ctx| {
            let Some(m) = moves.lock().expect("pending moves lock poisoned").remove(&key) else {
              return;
            };
            let start = std::time::Instant::now();
            flux::gui::input::dispatch(
              &ctx,
              InputEvent::PointerMove {
                pointer_id: key.1,
                pointer_type: key.0,
                x: m.x,
                y: m.y,
                modifiers: m.modifiers,
              },
            );
            timing
              .lock()
              .expect("js timing lock poisoned")
              .record_move(start.elapsed().as_secs_f32() * 1000.0);
          });
        }
      }
      AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
        // Seed the touch history at the contact so the first move already
        // has a velocity; the down itself dispatches on arrival as always.
        if *pointer_type == PointerType::Touch {
          self.resampler.down((*pointer_type, *pointer_id), *x, *y, *modifiers);
        }
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
        // A gated or buffered move dispatching after this up would be stale;
        // the up carries the final position, so drop them.
        self.pending_moves.lock().expect("pending moves lock poisoned").remove(&(*pointer_type, *pointer_id));
        self.resampler.remove((*pointer_type, *pointer_id));
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
    // Sampled at frame-signal time: the alloy loop drains the vsync's input
    // into the event channel before emitting the signal, and the batch loop
    // runs events before the signal, so this frame's touch samples are
    // already in the history.
    let moves = self.resampler.sample();
    let playback_frame = self.playback_frame.clone();
    let paced = self.paced.clone();
    let platform = self.platform.clone();
    let timing = self.timing.clone();
    eh.exec(move |ctx| {
      // Resampled touch moves run ahead of the frame work so the frame
      // consumes the state they dirty; timed as moves, not frame cost.
      for m in moves {
        let start = std::time::Instant::now();
        flux::gui::input::dispatch(
          &ctx,
          InputEvent::PointerMove {
            pointer_id: m.pointer_id,
            pointer_type: m.pointer_type,
            x: m.x,
            y: m.y,
            modifiers: m.modifiers,
          },
        );
        timing.lock().expect("js timing lock poisoned").record_move(start.elapsed().as_secs_f32() * 1000.0);
      }
      let start = std::time::Instant::now();
      // Publish the present being computed before reading the clock, so in
      // playback mode the clock reports this frame's virtual time.
      playback_frame.store(next_frame, Ordering::Relaxed);
      // rAF and the render event use the paced clock in run mode (see
      // paced_clock); playback mode and performance.now() read flux::Clock
      // directly. Idle Ticks arrive at the refresh cadence, so ticking the
      // paced clock for them preserves its one-period-per-call model. Render
      // event carries seconds; JS scales to ms.
      let raw = ctx.userdata::<flux::Clock>().map(|c| c.now_ms()).unwrap_or(0.0);
      let ts = match &paced {
        Some(pc) => {
          pc.tick(raw);
          pc.now_ms()
        }
        None => raw,
      };
      if flux::gui::camera::tick(&ctx) {
        // A camera frame landed in its texture; the screen content changed
        // even though the tree did not.
        platform.request_frame();
      }
      // Settle any captureSnapshot promises whose captures alloy rendered on the
      // previous paint pass.
      flux::gui::gpu::tick(&ctx);
      #[cfg(feature = "speech")]
      crate::plugins::speech::tick(&ctx);
      flux::gui::raf::flush(&ctx, ts);
      let time = ts / 1000.0;
      let obj = flux::rquickjs::Object::new(ctx.clone()).expect("create object");
      obj.set("frame", next_frame).expect("set frame");
      obj.set("time", time).expect("set time");
      // Stamp the start of the JS render handler so draw() can measure onFrame +
      // flush without any timing call crossing into JS (see frame::RENDER_START).
      crate::frame::RENDER_START.with(|c| c.set(Some(std::time::Instant::now())));
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
